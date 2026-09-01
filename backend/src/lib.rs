mod store;

use crate::store::{Marker, RunRecord, Session, Source, Store};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_OUTPUT_CHUNK_BYTES: usize = 1024;
/// Maximum stdin bytes queued for a run but not yet written to its child.
const MAX_PENDING_STDIN_BYTES: usize = 1_048_576;
const CLIENT_IO_TIMEOUT: Duration = Duration::from_millis(250);
const OUTPUT_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONCURRENT_CLIENTS: usize = 8;
const MAX_CONCURRENT_PROCESSES: usize = 8;
const PIPE_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
const INPUT_WAIT_CPU_QUIET_NANOSECONDS: u64 = 30_000_000;
const WORKTREE_ROOT_DIRECTORY_NAME: &str = "capture-delegate-worktrees";
const MAX_SANITIZED_RUN_ID_BYTES: usize = 48;
const WORKTREE_ADD_TIMEOUT: Duration = Duration::from_secs(30);
const WORKTREE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SESSION_TITLE_BYTES: usize = 4096;
/// Categorization is optional, but a stated kind must be one the app knows how to present.
const SESSION_KINDS: [&str; 6] = [
    "meeting",
    "conversation",
    "presentation",
    "pair_work",
    "personal_note",
    "imported_audio",
];
const LIST_SESSIONS_LIMIT: usize = 50;
const MAX_SOURCE_TEXT_BYTES: usize = 4096;
const MAX_SOURCE_SPEAKER_BYTES: usize = 256;
const LIST_SOURCES_LIMIT: usize = 50;
/// A marker always states the kind of attention it deserves, and it must be one the app
/// knows how to present.
const MARKER_KINDS: [&str; 6] = [
    "important",
    "decision",
    "action",
    "question",
    "delegate",
    "research",
];
const MAX_MARKER_NOTE_BYTES: usize = 4096;
const LIST_MARKERS_LIMIT: usize = 50;
const LIST_RUNS_LIMIT: usize = 50;

static SHUTDOWN_PIPE_WRITE_FD: AtomicI32 = AtomicI32::new(-1);
static WORKTREE_NONCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

extern "C" fn handle_shutdown_signal(_signal: libc::c_int) {
    let write_fd = SHUTDOWN_PIPE_WRITE_FD.load(Ordering::Relaxed);
    if write_fd >= 0 {
        let byte = [1_u8];
        // SAFETY: the handler only writes one byte to the self-pipe descriptor;
        // write is async-signal-safe and does not retain the buffer pointer.
        let _ = unsafe { libc::write(write_fd, byte.as_ptr().cast(), byte.len()) };
    }
}

#[derive(Deserialize)]
struct Request {
    version: u32,
    #[serde(rename = "type")]
    request_type: String,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    executable: Option<String>,
    #[serde(default)]
    arguments: Option<Vec<String>>,
    #[serde(default)]
    timeout_milliseconds: Option<serde_json::Value>,
    #[serde(default)]
    input_wait_detect_milliseconds: Option<serde_json::Value>,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    pty: Option<bool>,
    #[serde(default)]
    worktree_repository: Option<serde_json::Value>,
    #[serde(default)]
    title: Option<String>,
    // Double option so an explicit `"kind": null` stays distinguishable from an
    // omitted field: only omission means uncategorized.
    #[serde(default, deserialize_with = "deserialize_present")]
    kind: Option<Option<String>>,
    // Double option as with `kind`: for a run link, only an omitted field means unlinked, so an
    // explicit null has to stay distinguishable.
    #[serde(default, deserialize_with = "deserialize_present")]
    session_id: Option<Option<String>>,
    // Untyped so a float, a negative, or a string is an error frame instead of a
    // dropped connection, matching timeout_milliseconds.
    #[serde(default)]
    start_ms: Option<serde_json::Value>,
    #[serde(default)]
    end_ms: Option<serde_json::Value>,
    #[serde(default)]
    at_ms: Option<serde_json::Value>,
    // Double option as with `kind`: only an omitted speaker means unattributed.
    #[serde(default, deserialize_with = "deserialize_present")]
    speaker: Option<Option<String>>,
    #[serde(default)]
    text: Option<String>,
    // Double option as with `kind`: only an omitted note means a marker without one.
    #[serde(default, deserialize_with = "deserialize_present")]
    note: Option<Option<String>>,
}

fn deserialize_present<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Serialize)]
struct HealthResponse {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    status: &'static str,
}

#[derive(Serialize)]
struct ProtocolErrorResponse {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    code: &'static str,
}

#[derive(Serialize)]
struct CreateSessionResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    session: &'a Session,
}

#[derive(Serialize)]
struct ListSessionsResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    sessions: &'a [Session],
    truncated: bool,
}

#[derive(Serialize)]
struct AddSourceResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    source: &'a Source,
}

#[derive(Serialize)]
struct ListSourcesResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    sources: &'a [Source],
    truncated: bool,
}

#[derive(Serialize)]
struct AddMarkerResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    marker: &'a Marker,
}

#[derive(Serialize)]
struct ListMarkersResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    markers: &'a [Marker],
    truncated: bool,
}

#[derive(Serialize)]
struct ListRunsResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    runs: &'a [RunRecord],
    truncated: bool,
}

#[derive(Serialize)]
struct RunOutputResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    run_id: &'a str,
    stream: &'static str,
    output: String,
}

#[derive(Serialize)]
struct RunInputWaitingResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    run_id: &'a str,
    quiet_for_milliseconds: u64,
}

#[derive(Serialize)]
struct RunMetadataResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    run_id: &'a str,
    pid: u32,
    pgid: u32,
    executable: &'a str,
    arguments: &'a [String],
    working_directory: String,
    started_at: String,
    finished_at: String,
    duration_ms: u64,
    environment_variable_names: Vec<String>,
    redactions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_branch: Option<String>,
}

#[derive(Serialize)]
struct RunExitResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    run_id: &'a str,
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<&'static str>,
}

struct ClientWriter {
    stream: Mutex<UnixStream>,
    dead: AtomicBool,
}

impl ClientWriter {
    fn new(stream: UnixStream) -> Self {
        Self {
            stream: Mutex::new(stream),
            dead: AtomicBool::new(false),
        }
    }

    fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Acquire)
    }

    fn mark_dead(&self) {
        self.dead.store(true, Ordering::Release);
    }

    fn dead_error() -> io::Error {
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            "client connection is unusable after a frame write failure",
        )
    }

    fn write_frame_bytes(&self, frame: &[u8]) -> io::Result<()> {
        if self.is_dead() {
            return Err(Self::dead_error());
        }
        let mut stream = self
            .stream
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.write_frame_bytes_locked(&mut stream, frame)
    }

    fn write_frame_bytes_locked(&self, stream: &mut UnixStream, frame: &[u8]) -> io::Result<()> {
        if self.is_dead() {
            return Err(Self::dead_error());
        }
        write_serialized_frame(stream, frame).inspect_err(|_| self.mark_dead())
    }
}

#[derive(Serialize)]
struct CancelResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    run_id: &'a str,
    status: &'static str,
}

#[derive(Clone)]
struct ActiveRuns {
    state: Arc<Mutex<ActiveRunsState>>,
}

struct ActiveRunsState {
    runs: HashMap<String, Arc<RunControl>>,
    shutting_down: bool,
    cleanup_blockers: usize,
}

struct RunStdinState {
    buffer: Vec<u8>,
    closed: bool,
    handle: Option<RunStdinHandle>,
    veof_sent: bool,
    last_pty_byte_was_newline: bool,
}

enum RunStdinHandle {
    Pipe(std::process::ChildStdin),
    Pty(Arc<File>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunInputStatus {
    Accepted,
    Closed,
    CapacityExhausted,
}

impl RunStdinHandle {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Pipe(stdin) => stdin.write(buffer),
            Self::Pty(master) => master.as_ref().write(buffer),
        }
    }
}

struct RunControl {
    cancelled: AtomicBool,
    paused: AtomicBool,
    pgid: AtomicI32,
    stdin: Mutex<RunStdinState>,
}

impl RunControl {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            pgid: AtomicI32::new(0),
            stdin: Mutex::new(RunStdinState {
                buffer: Vec::new(),
                closed: false,
                handle: None,
                veof_sent: false,
                last_pty_byte_was_newline: true,
            }),
        }
    }

    fn send_input(&self, data: &[u8]) -> RunInputStatus {
        let mut state = self
            .stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.closed {
            return RunInputStatus::Closed;
        }
        if state.buffer.len().saturating_add(data.len()) > MAX_PENDING_STDIN_BYTES {
            return RunInputStatus::CapacityExhausted;
        }
        state.buffer.extend_from_slice(data);
        RunInputStatus::Accepted
    }

    fn close_stdin(&self) {
        self.stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .closed = true;
    }

    fn publish_stdin(&self, child_stdin: std::process::ChildStdin) -> io::Result<()> {
        let fd = child_stdin.as_raw_fd();
        // SAFETY: ChildStdin owns a valid open pipe descriptor; fcntl changes only its status
        // flags and does not retain the descriptor or access Rust-managed memory.
        if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } == -1 {
            return Err(io::Error::last_os_error());
        }
        self.stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .handle = Some(RunStdinHandle::Pipe(child_stdin));
        Ok(())
    }

    fn publish_pty_master(&self, master: Arc<File>) {
        self.stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .handle = Some(RunStdinHandle::Pty(master));
    }

    fn drain_stdin(&self, activity: Option<&ActivityClock>) {
        let mut state = self
            .stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        while state.handle.is_some() && !state.buffer.is_empty() {
            let write_result = {
                let RunStdinState { buffer, handle, .. } = &mut *state;
                handle
                    .as_mut()
                    .expect("published stdin handle should be present")
                    .write(buffer)
            };
            match write_result {
                Ok(0) => {
                    state.closed = true;
                    state.buffer.clear();
                    state.handle.take();
                }
                Ok(written) => {
                    if matches!(state.handle, Some(RunStdinHandle::Pty(_))) {
                        state.last_pty_byte_was_newline = state.buffer[written - 1] == b'\n';
                    }
                    state.buffer.drain(..written);
                    if let Some(activity) = activity {
                        activity.record();
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    state.closed = true;
                    state.buffer.clear();
                    state.handle.take();
                }
            }
        }

        if state.closed && state.buffer.is_empty() {
            if matches!(state.handle, Some(RunStdinHandle::Pty(_))) && !state.veof_sent {
                let veof_count = if state.last_pty_byte_was_newline {
                    1
                } else {
                    2
                };
                state.buffer.extend(std::iter::repeat_n(0x04, veof_count));
                state.veof_sent = true;
                // A PTY has no half-close: if the child disables ICANON, VEOF cannot signal EOF,
                // so close_stdin only establishes the closed send boundary.
            } else {
                state.handle.take();
            }
        }
    }

    fn stdin_is_idle(&self) -> bool {
        match self.stdin.try_lock() {
            Ok(state) => state.buffer.is_empty(),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                poisoned.into_inner().buffer.is_empty()
            }
            Err(std::sync::TryLockError::WouldBlock) => false,
        }
    }

    fn teardown_stdin(&self) {
        let mut state = self
            .stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        state.buffer.clear();
        state.handle.take();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SendInputStatus {
    Accepted,
    NotFound,
    Closed,
    CapacityExhausted,
}

#[derive(Debug, Eq, PartialEq)]
enum RegisterRunError {
    Duplicate,
    ShuttingDown,
}

impl ActiveRuns {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ActiveRunsState {
                runs: HashMap::new(),
                shutting_down: false,
                cleanup_blockers: 0,
            })),
        }
    }

    fn register(&self, run_id: String) -> Result<RunRegistration, RegisterRunError> {
        let control = Arc::new(RunControl::new());
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.shutting_down {
            return Err(RegisterRunError::ShuttingDown);
        }
        if state.runs.contains_key(&run_id) {
            return Err(RegisterRunError::Duplicate);
        }
        state.runs.insert(run_id.clone(), Arc::clone(&control));
        Ok(RunRegistration {
            state: Arc::clone(&self.state),
            run_id,
            control,
        })
    }

    fn cancel(&self, run_id: &str) -> bool {
        let control = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .runs
            .get(run_id)
            .cloned();
        let Some(control) = control else {
            return false;
        };
        control.cancelled.store(true, Ordering::Release);
        true
    }

    fn pause(&self, run_id: &str) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let control = state.runs.get(run_id);
        let Some(control) = control else {
            return false;
        };
        control.paused.store(true, Ordering::SeqCst);
        let pgid = control.pgid.load(Ordering::SeqCst);
        if pgid != 0 {
            signal_process_group(pgid, libc::SIGSTOP);
        }
        true
    }

    fn resume(&self, run_id: &str) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let control = state.runs.get(run_id);
        let Some(control) = control else {
            return false;
        };
        control.paused.store(false, Ordering::SeqCst);
        let pgid = control.pgid.load(Ordering::SeqCst);
        if pgid != 0 {
            signal_process_group(pgid, libc::SIGCONT);
        }
        true
    }

    fn send_input(&self, run_id: &str, data: &str) -> SendInputStatus {
        let control = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .runs
            .get(run_id)
            .cloned();
        let Some(control) = control else {
            return SendInputStatus::NotFound;
        };

        match control.send_input(data.as_bytes()) {
            RunInputStatus::Accepted => SendInputStatus::Accepted,
            RunInputStatus::Closed => SendInputStatus::Closed,
            RunInputStatus::CapacityExhausted => SendInputStatus::CapacityExhausted,
        }
    }

    fn close_stdin(&self, run_id: &str) -> bool {
        let control = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .runs
            .get(run_id)
            .cloned();
        let Some(control) = control else {
            return false;
        };
        control.close_stdin();
        true
    }

    fn begin_shutdown(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.shutting_down = true;
        for control in state.runs.values() {
            control.cancelled.store(true, Ordering::Release);
        }
    }

    fn wait_until_empty(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.runs.is_empty() && state.cleanup_blockers == 0 {
                return true;
            }
            drop(state);
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(PIPE_DRAIN_POLL_INTERVAL);
        }
    }
}

struct RunRegistration {
    state: Arc<Mutex<ActiveRunsState>>,
    run_id: String,
    control: Arc<RunControl>,
}

struct CleanupBlocker {
    state: Arc<Mutex<ActiveRunsState>>,
}

impl RunRegistration {
    fn acquire_cleanup_blocker(&self) -> CleanupBlocker {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.cleanup_blockers += 1;
        CleanupBlocker {
            state: Arc::clone(&self.state),
        }
    }

    fn publish_stdin(&self, stdin: std::process::ChildStdin) -> io::Result<()> {
        self.control.publish_stdin(stdin)
    }

    fn publish_pty_master(&self, master: Arc<File>) {
        self.control.publish_pty_master(master);
    }

    fn drain_stdin(&self) {
        self.control.drain_stdin(None);
    }

    fn drain_stdin_with_activity(&self, activity: &ActivityClock) {
        self.control.drain_stdin(Some(activity));
    }

    fn publish_pgid(&self, pgid: libc::pid_t) {
        let _state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // All pgid/pause transitions and their signals serialize under the registry mutex.
        self.control.pgid.store(pgid, Ordering::SeqCst);
        if self.control.paused.load(Ordering::SeqCst) {
            signal_process_group(pgid, libc::SIGSTOP);
        }
    }

    fn retire(&self) {
        self.control.teardown_stdin();
        let _state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.control.pgid.store(0, Ordering::SeqCst);
    }
}

impl Drop for CleanupBlocker {
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert!(state.cleanup_blockers > 0);
        state.cleanup_blockers = state.cleanup_blockers.saturating_sub(1);
    }
}

impl Drop for RunRegistration {
    fn drop(&mut self) {
        self.control.teardown_stdin();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .runs
            .get(&self.run_id)
            .is_some_and(|control| Arc::ptr_eq(control, &self.control))
        {
            state.runs.remove(&self.run_id);
        }
    }
}

#[derive(Clone)]
struct WorkerSlots {
    available: Arc<(Mutex<usize>, Condvar)>,
}

impl WorkerSlots {
    fn new(count: usize) -> Self {
        Self {
            available: Arc::new((Mutex::new(count), Condvar::new())),
        }
    }

    fn acquire(&self) -> WorkerSlot {
        let (available, wake_worker) = &*self.available;
        let mut available = available
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *available == 0 {
            available = wake_worker
                .wait(available)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *available -= 1;
        WorkerSlot {
            available: Arc::clone(&self.available),
        }
    }

    fn try_acquire(&self) -> Option<WorkerSlot> {
        let (available, _) = &*self.available;
        let mut available = available
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *available == 0 {
            return None;
        }
        *available -= 1;
        Some(WorkerSlot {
            available: Arc::clone(&self.available),
        })
    }
}

struct WorkerSlot {
    available: Arc<(Mutex<usize>, Condvar)>,
}

impl Drop for WorkerSlot {
    fn drop(&mut self) {
        let (available, wake_worker) = &*self.available;
        let mut available = available
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *available += 1;
        wake_worker.notify_one();
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct SocketIdentity {
    device: u64,
    inode: u64,
}

impl SocketIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

struct SocketCleanup {
    path: PathBuf,
    identity: SocketIdentity,
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if SocketIdentity::from_metadata(&metadata) == self.identity {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn run(socket_path: &Path, store_path: &Path) -> io::Result<()> {
    // SAFETY: a zeroed signal set is initialized before it is used.
    let mut shutdown_signals: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: shutdown_signals owns valid storage for a signal set.
    if unsafe { libc::sigemptyset(&mut shutdown_signals) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: shutdown_signals is initialized and SIGTERM is a valid signal.
    if unsafe { libc::sigaddset(&mut shutdown_signals, libc::SIGTERM) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: shutdown_signals is initialized and SIGINT is a valid signal.
    if unsafe { libc::sigaddset(&mut shutdown_signals, libc::SIGINT) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: shutdown_signals is initialized and the previous mask is not requested.
    let mask_result =
        unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &shutdown_signals, std::ptr::null_mut()) };
    if mask_result != 0 {
        return Err(io::Error::from_raw_os_error(mask_result));
    }

    // Durable state must be usable before the socket advertises the service, and this
    // backend must be the store's only owner before it may rewrite lifecycle state.
    let store = store::open_store(store_path)?;
    let _store_owner = store::acquire_store_ownership(store_path)?;
    remove_stale_socket(socket_path)?;
    cleanup_orphaned_worktrees();
    let listener = UnixListener::bind(socket_path)?;
    let bound_metadata = fs::symlink_metadata(socket_path)?;
    let bound_identity = SocketIdentity::from_metadata(&bound_metadata);
    let _cleanup = SocketCleanup {
        path: socket_path.to_path_buf(),
        identity: bound_identity,
    };
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;
    // Runs still marked running belong to a backend that died mid-run. The store-ownership
    // lock held above proves no live backend can be rewritten by this sweep, and running it
    // before the accept loop means no client can read a page that claims a dead run is live.
    // Startup recovery is mandatory: if the sweep cannot run, the backend must not serve
    // stale lifecycle state.
    store.mark_dangling_runs_interrupted()?;
    let worker_slots = WorkerSlots::new(MAX_CONCURRENT_CLIENTS);
    let process_slots = WorkerSlots::new(MAX_CONCURRENT_PROCESSES);
    let active_runs = ActiveRuns::new();
    install_shutdown_handler(
        active_runs.clone(),
        socket_path.to_path_buf(),
        bound_identity,
    )?;
    // The shutdown thread inherits this blocked mask; only the main thread is unblocked here.
    // SAFETY: shutdown_signals is initialized and the previous mask is not requested.
    let mask_result = unsafe {
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &shutdown_signals, std::ptr::null_mut())
    };
    if mask_result != 0 {
        return Err(io::Error::from_raw_os_error(mask_result));
    }

    loop {
        let worker_slot = worker_slots.acquire();
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) => {
                eprintln!("IPC accept error: {error}");
                continue;
            }
        };
        let process_slots = process_slots.clone();
        let active_runs = active_runs.clone();
        let store = store.clone();
        if let Err(error) = thread::Builder::new()
            .name("capture-delegate-ipc".to_owned())
            .spawn(move || {
                let _worker_slot = worker_slot;
                let _ = handle_connection(stream, process_slots, active_runs, store);
            })
        {
            eprintln!("IPC worker spawn error: {error}");
        }
    }
}

fn install_shutdown_handler(
    active_runs: ActiveRuns,
    socket_path: PathBuf,
    bound_identity: SocketIdentity,
) -> io::Result<()> {
    let mut pipe_fds = [-1; 2];
    // SAFETY: pipe receives storage for exactly two file descriptors.
    if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    let read_fd = pipe_fds[0];
    let write_fd = pipe_fds[1];
    if let Err(error) = configure_shutdown_pipe(read_fd, write_fd) {
        close_shutdown_pipe(read_fd, write_fd);
        return Err(error);
    }
    SHUTDOWN_PIPE_WRITE_FD.store(write_fd, Ordering::Relaxed);

    // SAFETY: a zeroed sigaction is initialized below before being installed.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = handle_shutdown_signal as *const () as usize;
    action.sa_flags = 0;
    // SAFETY: action owns valid storage for a signal mask.
    if unsafe { libc::sigemptyset(&mut action.sa_mask) } == -1 {
        close_shutdown_pipe(read_fd, write_fd);
        return Err(io::Error::last_os_error());
    }

    // SAFETY: action is fully initialized, and the old action is not requested.
    if unsafe { libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut()) } == -1 {
        close_shutdown_pipe(read_fd, write_fd);
        return Err(io::Error::last_os_error());
    }
    // SAFETY: action is fully initialized, and the old action is not requested.
    if unsafe { libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut()) } == -1 {
        close_shutdown_pipe(read_fd, write_fd);
        return Err(io::Error::last_os_error());
    }

    if let Err(error) = thread::Builder::new()
        .name("capture-delegate-shutdown".to_owned())
        .spawn(move || {
            let mut byte = [0_u8];
            loop {
                // SAFETY: read_fd is the open read end of the self-pipe and byte
                // provides writable storage for the requested single byte.
                let bytes_read =
                    unsafe { libc::read(read_fd, byte.as_mut_ptr().cast(), byte.len()) };
                if bytes_read > 0 {
                    break;
                }
                if bytes_read == -1
                    && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted
                {
                    continue;
                }
                return;
            }

            active_runs.begin_shutdown();
            let _ = active_runs.wait_until_empty(Duration::from_secs(5));
            if let Ok(metadata) = fs::symlink_metadata(&socket_path)
                && SocketIdentity::from_metadata(&metadata) == bound_identity
            {
                let _ = fs::remove_file(&socket_path);
            }
            std::process::exit(0);
        })
    {
        close_shutdown_pipe(read_fd, write_fd);
        return Err(error);
    }

    Ok(())
}

fn configure_shutdown_pipe(read_fd: libc::c_int, write_fd: libc::c_int) -> io::Result<()> {
    for fd in [read_fd, write_fd] {
        // SAFETY: fd is an open descriptor returned by pipe.
        let descriptor_flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if descriptor_flags == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fd is open and descriptor_flags preserves its existing descriptor flags.
        if unsafe { libc::fcntl(fd, libc::F_SETFD, descriptor_flags | libc::FD_CLOEXEC) } == -1 {
            return Err(io::Error::last_os_error());
        }
    }

    // SAFETY: write_fd is the open write descriptor returned by pipe.
    let status_flags = unsafe { libc::fcntl(write_fd, libc::F_GETFL) };
    if status_flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: write_fd is open and status_flags preserves its existing status flags.
    if unsafe { libc::fcntl(write_fd, libc::F_SETFL, status_flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

fn close_shutdown_pipe(read_fd: libc::c_int, write_fd: libc::c_int) {
    SHUTDOWN_PIPE_WRITE_FD.store(-1, Ordering::Relaxed);
    // SAFETY: both descriptors were returned by pipe and are closed at most once
    // along these setup-error paths.
    let _ = unsafe { libc::close(read_fd) };
    // SAFETY: as above, this is the matching write descriptor from pipe.
    let _ = unsafe { libc::close(write_fd) };
}

fn remove_stale_socket(socket_path: &Path) -> io::Result<()> {
    if !socket_path.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(socket_path)?;
    if !metadata.file_type().is_socket() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "socket path exists and is not a socket",
        ));
    }

    match UnixStream::connect(socket_path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "socket is already in use",
        )),
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            fs::remove_file(socket_path)
        }
        Err(error) => Err(error),
    }
}

fn handle_connection(
    mut stream: UnixStream,
    process_slots: WorkerSlots,
    active_runs: ActiveRuns,
    store: Store,
) -> io::Result<()> {
    stream.set_read_timeout(Some(CLIENT_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(CLIENT_IO_TIMEOUT))?;

    loop {
        let frame_deadline = Instant::now() + CLIENT_IO_TIMEOUT;
        let mut request_frame = Vec::new();
        loop {
            let remaining = frame_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "request frame deadline elapsed",
                ));
            }
            stream.set_read_timeout(Some(remaining))?;

            let mut byte = [0_u8; 1];
            match stream.read(&mut byte)? {
                0 => break,
                _ => {
                    request_frame.push(byte[0]);
                    if request_frame.len() > MAX_REQUEST_BYTES || byte[0] == b'\n' {
                        break;
                    }
                }
            }
        }

        if request_frame.is_empty() {
            return Ok(());
        }
        if request_frame.len() > MAX_REQUEST_BYTES || request_frame.last() != Some(&b'\n') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request must be a bounded newline-delimited frame",
            ));
        }

        let request: Request = match serde_json::from_slice(&request_frame) {
            Ok(request) => request,
            Err(_) => return Ok(()),
        };

        if request.version != PROTOCOL_VERSION {
            write_protocol_error(&mut stream, "incompatible_version")?;
            return Ok(());
        }
        match request.request_type.as_str() {
            "health" => {
                let response = HealthResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "health_response",
                    status: "ok",
                };
                serde_json::to_writer(&mut stream, &response)?;
                stream.write_all(b"\n")?;
                stream.flush()?;
            }
            "create_session" => {
                let Some(title) = request
                    .title
                    .map(|title| title.trim().to_owned())
                    .filter(|title| !title.is_empty() && title.len() <= MAX_SESSION_TITLE_BYTES)
                else {
                    return write_protocol_error(&mut stream, "invalid_create_session");
                };
                let kind = match request.kind {
                    None => None,
                    Some(Some(kind)) if SESSION_KINDS.contains(&kind.as_str()) => Some(kind),
                    // Unknown kinds and explicit null are both rejected; only an
                    // omitted field states an uncategorized session.
                    Some(_) => return write_protocol_error(&mut stream, "invalid_create_session"),
                };
                let session = Session::draft(&title, kind.as_deref());
                let response = CreateSessionResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "create_session_response",
                    session: &session,
                };
                // Escape-heavy titles can pass the raw byte check yet serialize past the
                // frame bound; reject those before anything is persisted. Admission is
                // checked against the single-item LIST envelope, the larger of the two
                // frames, so an accepted session can never persist yet be unlistable.
                let list_probe = ListSessionsResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "list_sessions_response",
                    sessions: std::slice::from_ref(&session),
                    truncated: false,
                };
                if serialize_json_frame(&list_probe).is_err() {
                    return write_protocol_error(&mut stream, "invalid_create_session");
                }
                let Ok(frame) = serialize_json_frame(&response) else {
                    return write_protocol_error(&mut stream, "invalid_create_session");
                };
                if let Err(error) = store.insert_session(&session) {
                    eprintln!("store write error: {error}");
                    return write_protocol_error(&mut stream, "store_unavailable");
                }
                write_serialized_frame(&mut stream, &frame)?;
            }
            "list_sessions" => {
                // Fetch one past the page cap so truncation is observable without a count query.
                let mut sessions = match store.list_sessions(LIST_SESSIONS_LIMIT + 1) {
                    Ok(sessions) => sessions,
                    Err(error) => {
                        eprintln!("store read error: {error}");
                        return write_protocol_error(&mut stream, "store_unavailable");
                    }
                };
                let mut truncated = sessions.len() > LIST_SESSIONS_LIMIT;
                sessions.truncate(LIST_SESSIONS_LIMIT);
                let frame = loop {
                    let response = ListSessionsResponse {
                        version: PROTOCOL_VERSION,
                        response_type: "list_sessions_response",
                        sessions: &sessions,
                        truncated,
                    };
                    match serialize_json_frame(&response) {
                        Ok(frame) => break frame,
                        Err(_) if !sessions.is_empty() => {
                            sessions.pop();
                            truncated = true;
                        }
                        Err(error) => return Err(error),
                    }
                };
                write_serialized_frame(&mut stream, &frame)?;
            }
            "add_source" => {
                // A span is stated in whole non-negative milliseconds; a float, a negative,
                // or a string is a rejected field, not a rejected connection.
                let (Some(session_id), Some(start_ms), Some(end_ms)) = (
                    // A source always names its session, so an omitted and an explicitly null
                    // session_id are the same missing field.
                    request.session_id.flatten(),
                    request.start_ms.as_ref().and_then(milliseconds_field),
                    request.end_ms.as_ref().and_then(milliseconds_field),
                ) else {
                    return write_protocol_error(&mut stream, "invalid_add_source");
                };
                if end_ms < start_ms {
                    return write_protocol_error(&mut stream, "invalid_add_source");
                }
                let Some(text) = request
                    .text
                    .map(|text| text.trim().to_owned())
                    .filter(|text| !text.is_empty() && text.len() <= MAX_SOURCE_TEXT_BYTES)
                else {
                    return write_protocol_error(&mut stream, "invalid_add_source");
                };
                let speaker = match request.speaker {
                    None => None,
                    // A blank or explicit-null speaker is not how an unattributed source is
                    // stated; only omitting the field is.
                    Some(Some(speaker)) => {
                        let speaker = speaker.trim().to_owned();
                        if speaker.is_empty() || speaker.len() > MAX_SOURCE_SPEAKER_BYTES {
                            return write_protocol_error(&mut stream, "invalid_add_source");
                        }
                        Some(speaker)
                    }
                    Some(None) => return write_protocol_error(&mut stream, "invalid_add_source"),
                };
                match store.session_exists(&session_id) {
                    Ok(true) => {}
                    Ok(false) => return write_protocol_error(&mut stream, "unknown_session"),
                    Err(error) => {
                        eprintln!("store read error: {error}");
                        return write_protocol_error(&mut stream, "store_unavailable");
                    }
                }
                let source =
                    Source::draft(&session_id, start_ms, end_ms, speaker.as_deref(), &text);
                let response = AddSourceResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "add_source_response",
                    source: &source,
                };
                // Escape-heavy text can pass the raw byte check yet serialize past the
                // frame bound; reject those before anything is persisted. Admission is
                // checked against the single-item LIST envelope, the larger of the two
                // frames, so an accepted source can never persist yet be unlistable.
                let list_probe = ListSourcesResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "list_sources_response",
                    sources: std::slice::from_ref(&source),
                    truncated: false,
                };
                if serialize_json_frame(&list_probe).is_err() {
                    return write_protocol_error(&mut stream, "invalid_add_source");
                }
                let Ok(frame) = serialize_json_frame(&response) else {
                    return write_protocol_error(&mut stream, "invalid_add_source");
                };
                if let Err(error) = store.insert_source(&source) {
                    eprintln!("store write error: {error}");
                    return write_protocol_error(&mut stream, "store_unavailable");
                }
                write_serialized_frame(&mut stream, &frame)?;
            }
            "list_sources" => {
                // A page can only be scoped to a session that exists, so a missing and an
                // unknown session_id are the same answer, and so is an explicitly null one.
                let Some(session_id) = request.session_id.flatten() else {
                    return write_protocol_error(&mut stream, "unknown_session");
                };
                match store.session_exists(&session_id) {
                    Ok(true) => {}
                    Ok(false) => return write_protocol_error(&mut stream, "unknown_session"),
                    Err(error) => {
                        eprintln!("store read error: {error}");
                        return write_protocol_error(&mut stream, "store_unavailable");
                    }
                }
                // Fetch one past the page cap so truncation is observable without a count query.
                let mut sources = match store.list_sources(&session_id, LIST_SOURCES_LIMIT + 1) {
                    Ok(sources) => sources,
                    Err(error) => {
                        eprintln!("store read error: {error}");
                        return write_protocol_error(&mut stream, "store_unavailable");
                    }
                };
                let mut truncated = sources.len() > LIST_SOURCES_LIMIT;
                sources.truncate(LIST_SOURCES_LIMIT);
                let frame = loop {
                    let response = ListSourcesResponse {
                        version: PROTOCOL_VERSION,
                        response_type: "list_sources_response",
                        sources: &sources,
                        truncated,
                    };
                    match serialize_json_frame(&response) {
                        Ok(frame) => break frame,
                        // Dropping from the end keeps the chronological start of the page.
                        Err(_) if !sources.is_empty() => {
                            sources.pop();
                            truncated = true;
                        }
                        Err(error) => return Err(error),
                    }
                };
                write_serialized_frame(&mut stream, &frame)?;
            }
            "add_marker" => {
                // A marker sits at a whole non-negative millisecond offset; a float, a
                // negative, or a string is a rejected field, not a rejected connection.
                let (Some(session_id), Some(at_ms)) = (
                    // A marker always names its session, so an omitted and an explicitly null
                    // session_id are the same missing field.
                    request.session_id.flatten(),
                    request.at_ms.as_ref().and_then(milliseconds_field),
                ) else {
                    return write_protocol_error(&mut stream, "invalid_add_marker");
                };
                let kind = match request.kind {
                    // A marker always states its kind, so an omitted, an unknown, and an
                    // explicitly null kind are all rejected.
                    Some(Some(kind)) if MARKER_KINDS.contains(&kind.as_str()) => kind,
                    _ => return write_protocol_error(&mut stream, "invalid_add_marker"),
                };
                let note = match request.note {
                    None => None,
                    // A blank or explicit-null note is not how a note-free marker is stated;
                    // only omitting the field is.
                    Some(Some(note)) => {
                        let note = note.trim().to_owned();
                        if note.is_empty() || note.len() > MAX_MARKER_NOTE_BYTES {
                            return write_protocol_error(&mut stream, "invalid_add_marker");
                        }
                        Some(note)
                    }
                    Some(None) => return write_protocol_error(&mut stream, "invalid_add_marker"),
                };
                match store.session_exists(&session_id) {
                    Ok(true) => {}
                    Ok(false) => return write_protocol_error(&mut stream, "unknown_session"),
                    Err(error) => {
                        eprintln!("store read error: {error}");
                        return write_protocol_error(&mut stream, "store_unavailable");
                    }
                }
                let marker = Marker::draft(&session_id, at_ms, &kind, note.as_deref());
                let response = AddMarkerResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "add_marker_response",
                    marker: &marker,
                };
                // Escape-heavy notes can pass the raw byte check yet serialize past the
                // frame bound; reject those before anything is persisted. Admission is
                // checked against the single-item LIST envelope, the larger of the two
                // frames, so an accepted marker can never persist yet be unlistable.
                let list_probe = ListMarkersResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "list_markers_response",
                    markers: std::slice::from_ref(&marker),
                    truncated: false,
                };
                if serialize_json_frame(&list_probe).is_err() {
                    return write_protocol_error(&mut stream, "invalid_add_marker");
                }
                let Ok(frame) = serialize_json_frame(&response) else {
                    return write_protocol_error(&mut stream, "invalid_add_marker");
                };
                if let Err(error) = store.insert_marker(&marker) {
                    eprintln!("store write error: {error}");
                    return write_protocol_error(&mut stream, "store_unavailable");
                }
                write_serialized_frame(&mut stream, &frame)?;
            }
            "list_markers" => {
                // A page can only be scoped to a session that exists, so a missing and an
                // unknown session_id are the same answer, and so is an explicitly null one.
                let Some(session_id) = request.session_id.flatten() else {
                    return write_protocol_error(&mut stream, "unknown_session");
                };
                match store.session_exists(&session_id) {
                    Ok(true) => {}
                    Ok(false) => return write_protocol_error(&mut stream, "unknown_session"),
                    Err(error) => {
                        eprintln!("store read error: {error}");
                        return write_protocol_error(&mut stream, "store_unavailable");
                    }
                }
                // Fetch one past the page cap so truncation is observable without a count query.
                let mut markers = match store.list_markers(&session_id, LIST_MARKERS_LIMIT + 1) {
                    Ok(markers) => markers,
                    Err(error) => {
                        eprintln!("store read error: {error}");
                        return write_protocol_error(&mut stream, "store_unavailable");
                    }
                };
                let mut truncated = markers.len() > LIST_MARKERS_LIMIT;
                markers.truncate(LIST_MARKERS_LIMIT);
                let frame = loop {
                    let response = ListMarkersResponse {
                        version: PROTOCOL_VERSION,
                        response_type: "list_markers_response",
                        markers: &markers,
                        truncated,
                    };
                    match serialize_json_frame(&response) {
                        Ok(frame) => break frame,
                        // Dropping from the end keeps the chronological start of the page.
                        Err(_) if !markers.is_empty() => {
                            markers.pop();
                            truncated = true;
                        }
                        Err(error) => return Err(error),
                    }
                };
                write_serialized_frame(&mut stream, &frame)?;
            }
            "list_runs" => {
                // Fetch one past the page cap so truncation is observable without a count query.
                let mut runs = match store.list_runs(LIST_RUNS_LIMIT + 1) {
                    Ok(runs) => runs,
                    Err(error) => {
                        eprintln!("store read error: {error}");
                        return write_protocol_error(&mut stream, "store_unavailable");
                    }
                };
                let mut truncated = runs.len() > LIST_RUNS_LIMIT;
                runs.truncate(LIST_RUNS_LIMIT);
                let frame = loop {
                    let response = ListRunsResponse {
                        version: PROTOCOL_VERSION,
                        response_type: "list_runs_response",
                        runs: &runs,
                        truncated,
                    };
                    match serialize_json_frame(&response) {
                        Ok(frame) => break frame,
                        // The page is newest-first, so dropping from the end keeps the newest runs.
                        Err(_) if !runs.is_empty() => {
                            runs.pop();
                            truncated = true;
                        }
                        Err(error) => return Err(error),
                    }
                };
                write_serialized_frame(&mut stream, &frame)?;
            }
            "start_process" => {
                let pty = request.pty.unwrap_or(false);
                let worktree_repository = match request.worktree_repository {
                    None => None,
                    Some(value) => {
                        let Some(repository) = value
                            .as_str()
                            .filter(|path| !path.is_empty() && Path::new(path).is_absolute())
                        else {
                            return write_protocol_error(&mut stream, "invalid_start_process");
                        };
                        Some(PathBuf::from(repository))
                    }
                };
                let input_wait_detect_milliseconds = match request.input_wait_detect_milliseconds {
                    None => None,
                    Some(value) => {
                        let Some(milliseconds) =
                            value.as_u64().filter(|milliseconds| *milliseconds > 0)
                        else {
                            return write_protocol_error(&mut stream, "invalid_start_process");
                        };
                        Some(milliseconds)
                    }
                };
                let (Some(run_id), Some(executable), Some(arguments), Some(timeout_milliseconds)) = (
                    request.run_id,
                    request.executable,
                    request.arguments,
                    request
                        .timeout_milliseconds
                        .and_then(|value| value.as_u64())
                        .filter(|timeout_milliseconds| *timeout_milliseconds > 0),
                ) else {
                    return write_protocol_error(&mut stream, "invalid_start_process");
                };
                let timeout = Duration::from_millis(timeout_milliseconds);
                // A run may stand alone, but a stated link must name a session that exists, and
                // that is settled before the run id is admitted so a rejected run never occupies
                // one. As with `kind`, only an omitted field states "no link".
                let linked_session_id = match request.session_id {
                    None => None,
                    Some(None) => {
                        return write_protocol_error(&mut stream, "invalid_start_process");
                    }
                    Some(Some(session_id)) => match store.session_exists(&session_id) {
                        Ok(true) => Some(session_id),
                        Ok(false) => return write_protocol_error(&mut stream, "unknown_session"),
                        Err(error) => {
                            eprintln!("store read error: {error}");
                            return write_protocol_error(&mut stream, "store_unavailable");
                        }
                    },
                };
                let registration = match active_runs.register(run_id.clone()) {
                    Ok(registration) => registration,
                    Err(RegisterRunError::Duplicate) => {
                        return write_protocol_error(&mut stream, "duplicate_run_id");
                    }
                    Err(RegisterRunError::ShuttingDown) => {
                        let stream = Arc::new(Mutex::new(stream));
                        return write_json_frame(
                            &stream,
                            &RunExitResponse {
                                version: PROTOCOL_VERSION,
                                response_type: "run_exit",
                                run_id: &run_id,
                                exit_code: None,
                                error_code: Some("cancelled"),
                            },
                        );
                    }
                };
                let Some(process_slot) = process_slots.try_acquire() else {
                    drop(registration);
                    let stream = Arc::new(Mutex::new(stream));
                    return write_json_frame(
                        &stream,
                        &RunExitResponse {
                            version: PROTOCOL_VERSION,
                            response_type: "run_exit",
                            run_id: &run_id,
                            exit_code: None,
                            error_code: Some("capacity_exhausted"),
                        },
                    );
                };
                if let Err(error) = stream.set_write_timeout(Some(OUTPUT_WRITE_TIMEOUT)) {
                    drop(registration);
                    drop(process_slot);
                    let writer = Arc::new(ClientWriter::new(stream));
                    let _ = write_client_json_frame(
                        &writer,
                        &RunExitResponse {
                            version: PROTOCOL_VERSION,
                            response_type: "run_exit",
                            run_id: &run_id,
                            exit_code: None,
                            error_code: Some("internal_error"),
                        },
                    );
                    return Err(error);
                }
                // The record is written while the id is admitted and before the child exists, so a
                // backend that dies mid-run leaves a record to mark interrupted.
                let record = RunRecord::draft(&run_id, linked_session_id.as_deref(), &executable);
                // A run record grows at termination, so admission must bound the WORST-CASE
                // terminal single-item list frame — an accepted run stays listable for its
                // whole lifecycle. "interrupted" is the longest status; the error code gets
                // headroom from the longest code in the protocol even though it never
                // persists today; exit codes are at worst a full negative i32.
                let mut probe = record.clone();
                probe.status = "interrupted".to_owned();
                probe.exit_code = Some(i64::from(i32::MIN));
                probe.error_code = Some("capacity_exhausted".to_owned());
                probe.ended_at_ms = Some(record.started_at_ms);
                let list_probe = ListRunsResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "list_runs_response",
                    runs: std::slice::from_ref(&probe),
                    truncated: false,
                };
                if serialize_json_frame(&list_probe).is_err() {
                    drop(registration);
                    drop(process_slot);
                    return write_protocol_error(&mut stream, "invalid_start_process");
                }
                if let Err(error) = store.insert_run(&record) {
                    eprintln!("store write error: {error}");
                    drop(registration);
                    drop(process_slot);
                    return write_protocol_error(&mut stream, "store_unavailable");
                }
                let record_key = RunRecordKey {
                    store: store.clone(),
                    id: record.id,
                };
                let writer = Arc::new(ClientWriter::new(stream));
                let terminal = RunTerminal::new(
                    Arc::clone(&writer),
                    run_id.clone(),
                    registration,
                    Some(record_key.clone()),
                );
                if let Err(error) = thread::Builder::new()
                    .name("capture-delegate-process".to_owned())
                    .spawn(move || {
                        let _ = run_process(
                            terminal,
                            executable,
                            arguments,
                            StartProcessOptions {
                                timeout,
                                pty,
                                input_wait_detect_milliseconds,
                                worktree_repository,
                            },
                            process_slot,
                        );
                    })
                {
                    // No worker will ever reach a terminal frame, so this record is closed here
                    // instead of being left to look live until the next restart.
                    if let Err(error) =
                        record_key
                            .store
                            .finish_run(&record_key.id, None, Some("internal_error"))
                    {
                        eprintln!("store write error: {error}");
                    }
                    let _ = write_client_json_frame(
                        &writer,
                        &RunExitResponse {
                            version: PROTOCOL_VERSION,
                            response_type: "run_exit",
                            run_id: &run_id,
                            exit_code: None,
                            error_code: Some("internal_error"),
                        },
                    );
                    return Err(io::Error::other(format!(
                        "process worker spawn error: {error}"
                    )));
                }
                return Ok(());
            }
            "cancel_process" => {
                let Some(run_id) = request.run_id else {
                    return write_protocol_error(&mut stream, "invalid_cancel_process");
                };
                let status = if active_runs.cancel(&run_id) {
                    "accepted"
                } else {
                    "not_found"
                };
                let response = CancelResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "cancel_response",
                    run_id: &run_id,
                    status,
                };
                serde_json::to_writer(&mut stream, &response)?;
                stream.write_all(b"\n")?;
                stream.flush()?;
            }
            "pause_process" => {
                let Some(run_id) = request.run_id else {
                    return write_protocol_error(&mut stream, "invalid_pause_process");
                };
                let status = if active_runs.pause(&run_id) {
                    "accepted"
                } else {
                    "not_found"
                };
                let response = CancelResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "pause_response",
                    run_id: &run_id,
                    status,
                };
                serde_json::to_writer(&mut stream, &response)?;
                stream.write_all(b"\n")?;
                stream.flush()?;
            }
            "resume_process" => {
                let Some(run_id) = request.run_id else {
                    return write_protocol_error(&mut stream, "invalid_resume_process");
                };
                let status = if active_runs.resume(&run_id) {
                    "accepted"
                } else {
                    "not_found"
                };
                let response = CancelResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "resume_response",
                    run_id: &run_id,
                    status,
                };
                serde_json::to_writer(&mut stream, &response)?;
                stream.write_all(b"\n")?;
                stream.flush()?;
            }
            "send_input" => {
                let (Some(run_id), Some(data)) = (
                    request.run_id.filter(|run_id| !run_id.is_empty()),
                    request.data,
                ) else {
                    return write_protocol_error(&mut stream, "invalid_send_input");
                };
                let status = match active_runs.send_input(&run_id, &data) {
                    SendInputStatus::Accepted => "accepted",
                    SendInputStatus::NotFound => "not_found",
                    SendInputStatus::Closed => "closed",
                    SendInputStatus::CapacityExhausted => "capacity_exhausted",
                };
                let response = CancelResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "input_response",
                    run_id: &run_id,
                    status,
                };
                serde_json::to_writer(&mut stream, &response)?;
                stream.write_all(b"\n")?;
                stream.flush()?;
            }
            "close_stdin" => {
                let Some(run_id) = request.run_id.filter(|run_id| !run_id.is_empty()) else {
                    return write_protocol_error(&mut stream, "invalid_close_stdin");
                };
                let status = if active_runs.close_stdin(&run_id) {
                    "accepted"
                } else {
                    "not_found"
                };
                let response = CancelResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "close_stdin_response",
                    run_id: &run_id,
                    status,
                };
                serde_json::to_writer(&mut stream, &response)?;
                stream.write_all(b"\n")?;
                stream.flush()?;
            }
            _ => write_protocol_error(&mut stream, "unknown_request_type")?,
        }

        if request.request_type == "send_input" {
            continue;
        }
        return Ok(());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessSupervision {
    Exited,
    TimedOut,
    Cancelled,
    Running,
}

fn classify_process_supervision(
    child_exited: bool,
    deadline_elapsed: bool,
    cancelled: bool,
) -> ProcessSupervision {
    if child_exited {
        ProcessSupervision::Exited
    } else if deadline_elapsed {
        ProcessSupervision::TimedOut
    } else if cancelled {
        ProcessSupervision::Cancelled
    } else {
        ProcessSupervision::Running
    }
}

fn kill_process_group(child: &mut std::process::Child) {
    // SAFETY: a negative process ID targets the child's process group; no Rust-managed memory is
    // accessed or retained by kill.
    let _ = unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
    let _ = child.kill();
}

type DrainJoinResult = io::Result<io::Result<()>>;

fn teardown_process_and_drains(
    child: &mut std::process::Child,
    control: &RunControl,
    stdout_drain: thread::JoinHandle<io::Result<()>>,
    stderr_drain: Option<thread::JoinHandle<io::Result<()>>>,
    kill_and_reap: bool,
) -> (DrainJoinResult, Option<DrainJoinResult>) {
    control.cancelled.store(true, Ordering::Release);
    if kill_and_reap {
        kill_process_group(child);
        let _ = child.wait();
    }
    let stdout_result = stdout_drain
        .join()
        .map_err(|_| io::Error::other("stdout drain thread panicked"));
    let stderr_result = stderr_drain.map(|drain| {
        drain
            .join()
            .map_err(|_| io::Error::other("stderr drain thread panicked"))
    });
    (stdout_result, stderr_result)
}

fn signal_process_group(pgid: libc::pid_t, signal: libc::c_int) {
    // SAFETY: a negative process ID targets the registered run's process group; kill retains no
    // Rust-managed pointers.
    let _ = unsafe { libc::kill(-pgid, signal) };
}

struct SecretPatterns {
    complete_pem: Regex,
    unterminated_pem: Regex,
    bearer: Regex,
    assignment: Regex,
    direct: Vec<Regex>,
}

fn secret_patterns() -> &'static SecretPatterns {
    static PATTERNS: OnceLock<SecretPatterns> = OnceLock::new();
    PATTERNS.get_or_init(|| SecretPatterns {
        complete_pem: Regex::new(
            r"(?s)-----BEGIN [^\r\n]*PRIVATE KEY-----.*?-----END [^\r\n]*PRIVATE KEY-----",
        )
        .expect("complete PEM regex should compile"),
        unterminated_pem: Regex::new(r"(?s)-----BEGIN [^\r\n]*PRIVATE KEY-----.*\z")
            .expect("unterminated PEM regex should compile"),
        bearer: Regex::new(r"(?i)(bearer\s+)[A-Za-z0-9._~+/=-]{8,}")
            .expect("bearer regex should compile"),
        assignment: Regex::new(
            r#"(?i)\b(api[_-]?key|secret|token|password|passwd|authorization)\b(\s*[=:]\s*)(?:\"([^\s\"']{4,})\"|([^\s\"']{4,}))"#,
        )
        .expect("assignment regex should compile"),
        direct: [
            r"AKIA[0-9A-Z]{16}",
            r"gh[pousr]_[A-Za-z0-9]{36,}",
            r"github_pat_[A-Za-z0-9_]{22,}",
            r"xox[baprs]-[A-Za-z0-9-]{10,}",
            r"sk-[A-Za-z0-9_-]{20,}",
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("secret regex should compile"))
        .collect(),
    })
}

// Redaction is frame-local: a secret split across two read chunks may escape detection.
fn redact_output(output: &str) -> (String, usize) {
    let patterns = secret_patterns();
    let mut redacted = output.to_owned();
    let mut count = 0;

    for pattern in [&patterns.complete_pem, &patterns.unterminated_pem] {
        let matches = pattern.find_iter(&redacted).count();
        if matches > 0 {
            redacted = pattern.replace_all(&redacted, "[REDACTED]").into_owned();
            count += matches;
        }
    }

    let bearer_matches = patterns.bearer.find_iter(&redacted).count();
    if bearer_matches > 0 {
        redacted = patterns
            .bearer
            .replace_all(&redacted, "${1}[REDACTED]")
            .into_owned();
        count += bearer_matches;
    }

    let assignment_matches = patterns.assignment.find_iter(&redacted).count();
    if assignment_matches > 0 {
        redacted = patterns
            .assignment
            .replace_all(&redacted, |captures: &regex::Captures<'_>| {
                let key = captures
                    .get(1)
                    .expect("assignment key capture should exist")
                    .as_str();
                let separator = captures
                    .get(2)
                    .expect("assignment separator capture should exist")
                    .as_str();
                if captures.get(3).is_some() {
                    format!(r#"{key}{separator}"[REDACTED]""#)
                } else {
                    format!("{key}{separator}[REDACTED]")
                }
            })
            .into_owned();
        count += assignment_matches;
    }

    for pattern in &patterns.direct {
        let matches = pattern.find_iter(&redacted).count();
        if matches > 0 {
            redacted = pattern.replace_all(&redacted, "[REDACTED]").into_owned();
            count += matches;
        }
    }

    (redacted, count)
}

fn system_time_as_rfc3339(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let seconds_in_day = seconds % 86_400;
    let hour = seconds_in_day / 3_600;
    let minute = (seconds_in_day % 3_600) / 60;
    let second = seconds_in_day % 60;
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_date_from_unix_days(days: i64) -> (i64, i64, i64) {
    let shifted_days = days + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

enum ProcessOutput {
    Pipe(std::process::ChildStdout),
    Pty(Arc<File>),
}

struct StartProcessOptions {
    timeout: Duration,
    pty: bool,
    input_wait_detect_milliseconds: Option<u64>,
    worktree_repository: Option<PathBuf>,
}

struct RunWorktree {
    root: PathBuf,
    repository: PathBuf,
    path: PathBuf,
    branch: String,
    delete_branch: bool,
    cleaned: bool,
}

struct WorktreeSetupError {
    worktree: Option<RunWorktree>,
}

/// Where a run's durable record lives, so the run can close it exactly once.
#[derive(Clone)]
struct RunRecordKey {
    store: Store,
    id: String,
}

struct RunTerminal {
    writer: Arc<ClientWriter>,
    run_id: String,
    registration: Option<RunRegistration>,
    /// Taken by the single terminal emission, so the one-shot terminal frame is also a
    /// one-shot durable close.
    record: Option<RunRecordKey>,
    worktree: Option<RunWorktree>,
    cleanup_blocker: Option<CleanupBlocker>,
    emitted: bool,
}

impl RunTerminal {
    fn new(
        writer: Arc<ClientWriter>,
        run_id: String,
        registration: RunRegistration,
        record: Option<RunRecordKey>,
    ) -> Self {
        Self {
            writer,
            run_id,
            registration: Some(registration),
            record,
            worktree: None,
            cleanup_blocker: None,
            emitted: false,
        }
    }

    fn install_worktree(&mut self, worktree: Option<RunWorktree>) {
        self.cleanup_blocker = worktree.as_ref().map(|_| {
            self.registration
                .as_ref()
                .expect("run registration should exist before terminal emission")
                .acquire_cleanup_blocker()
        });
        self.worktree = worktree;
    }

    fn registration(&self) -> &RunRegistration {
        self.registration
            .as_ref()
            .expect("run registration should exist before terminal emission")
    }

    fn emit(&mut self, exit_code: Option<i32>, error_code: Option<&'static str>) -> io::Result<()> {
        if self.emitted {
            return Ok(());
        }
        self.emitted = true;
        // The record is closed before the frame is written — and never while the client stream
        // is locked — so a client that observes run_exit can already read a finished record.
        // Store ownership rules out cross-process contention, so a failure here is local store
        // breakage that retrying cannot fix: it is reported loudly, never withholds the
        // terminal frame, and the row is corrected by the next startup sweep.
        if let Some(record) = self.record.take()
            && let Err(error) =
                record
                    .store
                    .finish_run(&record.id, exit_code.map(i64::from), error_code)
        {
            eprintln!("store write error: run record left open: {error}");
        }
        // The run id must be released before the terminal frame is written so a
        // client that observes run_exit can immediately reuse the id.
        drop(self.registration.take());
        let terminal_result = write_client_json_frame(
            &self.writer,
            &RunExitResponse {
                version: PROTOCOL_VERSION,
                response_type: "run_exit",
                run_id: &self.run_id,
                exit_code,
                error_code,
            },
        );
        if let Some(worktree) = self.worktree.as_mut() {
            worktree.cleanup();
        }
        drop(self.cleanup_blocker.take());
        terminal_result
    }
}

fn run_with_internal_error_terminal<F>(terminal: &mut RunTerminal, inner: F) -> io::Result<()>
where
    F: FnOnce(&mut RunTerminal) -> io::Result<()>,
{
    match inner(terminal) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = terminal.emit(None, Some("internal_error"));
            Err(error)
        }
    }
}

impl RunWorktree {
    fn create(repository: PathBuf, run_id: &str) -> Result<Self, WorktreeSetupError> {
        let repository = repository
            .canonicalize()
            .map_err(|_| WorktreeSetupError { worktree: None })?;
        let root = worktree_root().map_err(|_| WorktreeSetupError { worktree: None })?;
        let sanitized_run_id = sanitize_run_id(run_id);
        let nonce = worktree_nonce();
        let directory_name = format!("run-{sanitized_run_id}-{nonce}");
        let path = root.join(directory_name);
        fs::create_dir(&path).map_err(|_| WorktreeSetupError { worktree: None })?;
        let branch = format!("capture-delegate/run-{sanitized_run_id}-{nonce}");
        let mut worktree = Self {
            root,
            repository,
            path,
            branch,
            delete_branch: false,
            cleaned: false,
        };
        worktree.write_owner_sidecar();
        if fs::set_permissions(&worktree.path, fs::Permissions::from_mode(0o700)).is_err() {
            return Err(WorktreeSetupError {
                worktree: Some(worktree),
            });
        }
        let branch_reference = format!("refs/heads/{}", worktree.branch);
        let mut verify_branch = Command::new("git");
        verify_branch
            .arg("-C")
            .arg(&worktree.repository)
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(&branch_reference)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match run_command_with_timeout(&mut verify_branch, WORKTREE_CLEANUP_TIMEOUT) {
            Ok(status) if status.success() => {
                return Err(WorktreeSetupError {
                    worktree: Some(worktree),
                });
            }
            Ok(status) if status.code() == Some(1) => worktree.delete_branch = true,
            _ => {
                return Err(WorktreeSetupError {
                    worktree: Some(worktree),
                });
            }
        }
        let mut add_worktree = Command::new("git");
        add_worktree
            .arg("-C")
            .arg(&worktree.repository)
            .args(["worktree", "add"])
            .arg(&worktree.path)
            .arg("-b")
            .arg(&worktree.branch)
            .arg("HEAD")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let added = command_succeeds_within(&mut add_worktree, WORKTREE_ADD_TIMEOUT);
        if !added {
            return Err(WorktreeSetupError {
                worktree: Some(worktree),
            });
        }
        Ok(worktree)
    }

    fn cleanup(&mut self) {
        if self.cleaned {
            return;
        }
        self.cleaned = true;

        let mut remove_worktree = Command::new("git");
        remove_worktree
            .arg("-C")
            .arg(&self.repository)
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let removed = command_succeeds_within(&mut remove_worktree, WORKTREE_CLEANUP_TIMEOUT);
        let path_is_managed_directory = self.path.parent() == Some(self.root.as_path())
            && fs::symlink_metadata(&self.path)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink());
        if !removed && path_is_managed_directory {
            let _ = fs::remove_dir_all(&self.path);
            let mut prune_worktrees = Command::new("git");
            prune_worktrees
                .arg("-C")
                .arg(&self.repository)
                .args(["worktree", "prune"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let _ = command_succeeds_within(&mut prune_worktrees, WORKTREE_CLEANUP_TIMEOUT);
        }
        if self.delete_branch {
            let mut delete_branch = Command::new("git");
            delete_branch
                .arg("-C")
                .arg(&self.repository)
                .args(["branch", "-D"])
                .arg(&self.branch)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let _ = command_succeeds_within(&mut delete_branch, WORKTREE_CLEANUP_TIMEOUT);
        }
        let _ = fs::remove_file(self.owner_sidecar_path());
    }

    fn owner_sidecar_path(&self) -> PathBuf {
        self.root.join(".owners").join(
            self.path
                .file_name()
                .expect("managed worktree path should have a directory name"),
        )
    }

    fn write_owner_sidecar(&self) {
        let owners = self.root.join(".owners");
        if fs::create_dir_all(&owners).is_err()
            || !fs::symlink_metadata(&owners)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
            || fs::set_permissions(&owners, fs::Permissions::from_mode(0o700)).is_err()
        {
            return;
        }
        let Ok(mut sidecar) = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.owner_sidecar_path())
        else {
            return;
        };
        let _ = writeln!(sidecar, "{}", std::process::id());
    }
}

fn run_command_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> io::Result<std::process::ExitStatus> {
    let mut child = command.spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::TimedOut, "command timed out"));
        }
        thread::sleep(PIPE_DRAIN_POLL_INTERVAL);
    }
}

fn command_succeeds_within(command: &mut Command, timeout: Duration) -> bool {
    run_command_with_timeout(command, timeout).is_ok_and(|status| status.success())
}

impl Drop for RunWorktree {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn worktree_root() -> io::Result<PathBuf> {
    let root = std::env::temp_dir().join(WORKTREE_ROOT_DIRECTORY_NAME);
    fs::create_dir_all(&root)?;
    let metadata = fs::symlink_metadata(&root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::other("worktree root is not a directory"));
    }
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    root.canonicalize()
}

fn cleanup_orphaned_worktrees() {
    if let Ok(root) = worktree_root() {
        cleanup_orphaned_worktrees_in(&root);
    }
}

fn cleanup_orphaned_worktrees_in(root: &Path) {
    let Ok(root) = root.canonicalize() else {
        return;
    };
    if !is_managed_worktree_directory_root(&root) {
        return;
    }

    let owners = root.join(".owners");
    let owners_are_safe = fs::symlink_metadata(&owners)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink());
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("run-")
            || !is_managed_worktree_directory(&root, &path)
            || !owners_are_safe
        {
            continue;
        }
        let sidecar = owners.join(&name);
        if owner_pid_is_dead(&sidecar) {
            remove_orphaned_worktree(&root, &path, &sidecar);
        }
    }

    if owners_are_safe {
        remove_stale_owner_sidecars(&root, &owners);
    }
}

fn is_managed_worktree_directory_root(root: &Path) -> bool {
    fs::symlink_metadata(root)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn is_managed_worktree_directory(root: &Path, path: &Path) -> bool {
    path.parent() == Some(root)
        && fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn owner_pid_is_dead(sidecar: &Path) -> bool {
    let Ok(owner) = fs::read_to_string(sidecar) else {
        return false;
    };
    let Ok(pid) = owner.trim().parse::<libc::pid_t>() else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: kill with signal 0 only checks whether the PID can be signalled.
    let result = unsafe { libc::kill(pid, 0) };
    result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

fn remove_orphaned_worktree(root: &Path, path: &Path, sidecar: &Path) {
    if let Some((repository, git_directory)) = recover_worktree_repository(path) {
        let branch = worktree_branch(&git_directory);
        let mut remove_worktree = Command::new("git");
        remove_worktree
            .arg("-C")
            .arg(&repository)
            .args(["worktree", "remove", "--force"])
            .arg(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !command_succeeds_within(&mut remove_worktree, WORKTREE_CLEANUP_TIMEOUT) {
            remove_managed_worktree_directory(root, path);
        }

        let mut prune_worktrees = Command::new("git");
        prune_worktrees
            .arg("-C")
            .arg(&repository)
            .args(["worktree", "prune"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ = command_succeeds_within(&mut prune_worktrees, WORKTREE_CLEANUP_TIMEOUT);

        if let Some(branch) = branch.filter(|branch| branch.starts_with("capture-delegate/run-")) {
            let mut delete_branch = Command::new("git");
            delete_branch
                .arg("-C")
                .arg(&repository)
                .args(["branch", "-D"])
                .arg(branch)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let _ = command_succeeds_within(&mut delete_branch, WORKTREE_CLEANUP_TIMEOUT);
        }
    } else {
        remove_managed_worktree_directory(root, path);
    }
    let _ = fs::remove_file(sidecar);
}

fn recover_worktree_repository(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let candidate_git_file = path.join(".git");
    let candidate_metadata = fs::symlink_metadata(&candidate_git_file).ok()?;
    if !candidate_metadata.file_type().is_file() || candidate_metadata.file_type().is_symlink() {
        return None;
    }
    let candidate_git_file = candidate_git_file.canonicalize().ok()?;

    let contents = fs::read_to_string(&candidate_git_file).ok()?;
    let mut lines = contents.lines();
    let git_directory = PathBuf::from(lines.next()?.strip_prefix("gitdir: ")?);
    if lines.next().is_some() {
        return None;
    }
    if !git_directory.is_absolute() {
        return None;
    }
    let git_directory = git_directory.canonicalize().ok()?;
    let worktrees = git_directory.parent()?;
    let dot_git = worktrees.parent()?;
    if worktrees.file_name()? != "worktrees" || dot_git.file_name()? != ".git" {
        return None;
    }

    let backlink_contents = fs::read_to_string(git_directory.join("gitdir")).ok()?;
    let mut backlink_lines = backlink_contents.lines();
    let backlink = PathBuf::from(backlink_lines.next()?);
    if backlink_lines.next().is_some() || !backlink.is_absolute() {
        return None;
    }
    if backlink.canonicalize().ok()? != candidate_git_file {
        return None;
    }

    Some((dot_git.parent()?.canonicalize().ok()?, git_directory))
}

fn worktree_branch(git_directory: &Path) -> Option<String> {
    let contents = fs::read_to_string(git_directory.join("HEAD")).ok()?;
    let mut lines = contents.lines();
    let branch = lines.next()?.strip_prefix("ref: refs/heads/")?.to_owned();
    (lines.next().is_none()).then_some(branch)
}

fn remove_managed_worktree_directory(root: &Path, path: &Path) {
    if is_managed_worktree_directory(root, path) {
        let _ = fs::remove_dir_all(path);
    }
}

fn remove_stale_owner_sidecars(root: &Path, owners: &Path) {
    let Ok(entries) = fs::read_dir(owners) else {
        return;
    };
    for entry in entries.flatten() {
        let sidecar = entry.path();
        if fs::symlink_metadata(&sidecar).is_ok_and(|metadata| metadata.is_file())
            && !is_managed_worktree_directory(root, &root.join(entry.file_name()))
        {
            let _ = fs::remove_file(sidecar);
        }
    }
}

fn sanitize_run_id(run_id: &str) -> String {
    let mut sanitized = String::with_capacity(MAX_SANITIZED_RUN_ID_BYTES);
    let mut previous_was_dot = false;
    for character in run_id.chars() {
        if sanitized.len() >= MAX_SANITIZED_RUN_ID_BYTES {
            break;
        }
        let allowed = character.is_ascii_alphanumeric()
            || matches!(character, '_' | '-')
            || (character == '.' && !previous_was_dot);
        let character = if allowed { character } else { '-' };
        previous_was_dot = character == '.';
        sanitized.push(character);
    }
    if sanitized.is_empty() {
        sanitized.push_str("run");
    }
    sanitized
}

fn worktree_nonce() -> String {
    let nanoseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = WORKTREE_NONCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{nanoseconds:x}-{sequence:x}", std::process::id())
}

struct ActivityClock {
    anchor: Instant,
    last_activity_nanoseconds: AtomicU64,
}

impl ActivityClock {
    fn new() -> Self {
        Self {
            anchor: Instant::now(),
            last_activity_nanoseconds: AtomicU64::new(0),
        }
    }

    fn record(&self) {
        self.last_activity_nanoseconds
            .fetch_max(self.elapsed_nanoseconds(), Ordering::Release);
    }

    fn snapshot(&self) -> u64 {
        self.last_activity_nanoseconds.load(Ordering::Acquire)
    }

    fn quiet_for_milliseconds(
        &self,
        expected_activity: u64,
        quiet_started_nanoseconds: u64,
    ) -> Option<u64> {
        if self.snapshot() != expected_activity {
            return None;
        }
        Some(
            self.elapsed_nanoseconds()
                .saturating_sub(quiet_started_nanoseconds)
                / 1_000_000,
        )
    }

    fn elapsed_nanoseconds(&self) -> u64 {
        u64::try_from(self.anchor.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

struct InputWaitEpisode {
    threshold_milliseconds: u64,
    activity_nanoseconds: u64,
    quiet_started_nanoseconds: u64,
    cpu_at_activity_nanoseconds: Option<u64>,
    fired: bool,
}

impl InputWaitEpisode {
    fn new(threshold_milliseconds: u64, activity: &ActivityClock, pid: libc::pid_t) -> Self {
        let activity_nanoseconds = activity.snapshot();
        Self {
            threshold_milliseconds,
            activity_nanoseconds,
            quiet_started_nanoseconds: activity_nanoseconds,
            cpu_at_activity_nanoseconds: process_cpu_time_nanoseconds(pid),
            fired: false,
        }
    }

    fn refresh_after_activity(&mut self, activity: &ActivityClock, pid: libc::pid_t) {
        let current_activity = activity.snapshot();
        if current_activity == self.activity_nanoseconds {
            return;
        }

        self.fired = false;
        let cpu_at_activity_nanoseconds = process_cpu_time_nanoseconds(pid);
        if activity.snapshot() == current_activity {
            self.activity_nanoseconds = current_activity;
            self.quiet_started_nanoseconds = current_activity;
            self.cpu_at_activity_nanoseconds = cpu_at_activity_nanoseconds;
        } else {
            self.cpu_at_activity_nanoseconds = None;
        }
    }

    fn rebaseline_after_pause(&mut self, activity: &ActivityClock, pid: libc::pid_t) {
        let current_activity = activity.snapshot();
        let cpu_at_activity_nanoseconds = process_cpu_time_nanoseconds(pid);
        let quiet_started_nanoseconds = activity.elapsed_nanoseconds();
        if activity.snapshot() == current_activity {
            self.activity_nanoseconds = current_activity;
            self.quiet_started_nanoseconds = quiet_started_nanoseconds;
            self.cpu_at_activity_nanoseconds = cpu_at_activity_nanoseconds;
        } else {
            self.cpu_at_activity_nanoseconds = None;
        }
    }
}

#[allow(deprecated)]
fn process_cpu_time_nanoseconds(pid: libc::pid_t) -> Option<u64> {
    let task_info_size = std::mem::size_of::<libc::proc_taskinfo>();
    let task_info_size = libc::c_int::try_from(task_info_size).ok()?;
    let mut task_info = std::mem::MaybeUninit::<libc::proc_taskinfo>::uninit();
    // SAFETY: task_info points to writable storage of exactly task_info_size bytes, and
    // proc_pidinfo does not retain the pointer after returning.
    let bytes_written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            task_info.as_mut_ptr().cast(),
            task_info_size,
        )
    };
    if bytes_written != task_info_size {
        return None;
    }
    // SAFETY: proc_pidinfo returned the full proc_taskinfo size and initialized the value.
    let task_info = unsafe { task_info.assume_init() };
    let mach_units = task_info
        .pti_total_user
        .saturating_add(task_info.pti_total_system);

    static MACH_TIMEBASE: OnceLock<Option<(u64, u64)>> = OnceLock::new();
    let &(numerator, denominator) = MACH_TIMEBASE
        .get_or_init(|| {
            let mut timebase = std::mem::MaybeUninit::<libc::mach_timebase_info>::uninit();
            // SAFETY: timebase points to writable storage and mach_timebase_info initializes it.
            if unsafe { libc::mach_timebase_info(timebase.as_mut_ptr()) } != 0 {
                return None;
            }
            // SAFETY: mach_timebase_info returned success and initialized timebase.
            let timebase = unsafe { timebase.assume_init() };
            (timebase.denom != 0).then_some((u64::from(timebase.numer), u64::from(timebase.denom)))
        })
        .as_ref()?;
    let nanoseconds =
        u128::from(mach_units).saturating_mul(u128::from(numerator)) / u128::from(denominator);
    Some(u64::try_from(nanoseconds).unwrap_or(u64::MAX))
}

fn open_pseudo_terminal() -> io::Result<(File, File)> {
    let mut master_fd = -1;
    let mut slave_fd = -1;
    // SAFETY: openpty initializes both descriptor outputs and does not retain any pointers.
    if unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == -1
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openpty returned ownership of two valid descriptors.
    let master = unsafe { File::from_raw_fd(master_fd) };
    // SAFETY: openpty returned ownership of two valid descriptors.
    let slave = unsafe { File::from_raw_fd(slave_fd) };

    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: slave is a valid terminal descriptor and termios points to writable storage.
    if unsafe { libc::tcgetattr(slave.as_raw_fd(), termios.as_mut_ptr()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: tcgetattr succeeded and initialized termios.
    let mut termios = unsafe { termios.assume_init() };
    termios.c_lflag &= !libc::ECHO;
    // SAFETY: slave remains open and termios is fully initialized.
    if unsafe { libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &termios) } == -1 {
        return Err(io::Error::last_os_error());
    }
    set_close_on_exec(&master)?;
    set_nonblocking(&master)?;
    Ok((master, slave))
}

fn configure_pty_child(slave_fd: libc::c_int) -> io::Result<()> {
    // SAFETY: called after fork in pre_exec; these operations only mutate the child process's
    // session and descriptor table using the still-open PTY slave descriptor.
    unsafe {
        if libc::setsid() == -1 {
            return Err(io::Error::last_os_error());
        }
        if libc::ioctl(slave_fd, libc::TIOCSCTTY.into(), 0) == -1 {
            return Err(io::Error::last_os_error());
        }
        for target_fd in 0..=2 {
            if libc::dup2(slave_fd, target_fd) == -1 {
                return Err(io::Error::last_os_error());
            }
        }
        if slave_fd > 2 {
            libc::close(slave_fd);
        }
    }
    Ok(())
}

fn run_process(
    mut terminal: RunTerminal,
    executable: String,
    arguments: Vec<String>,
    options: StartProcessOptions,
    _process_slot: WorkerSlot,
) -> io::Result<()> {
    run_with_internal_error_terminal(&mut terminal, |terminal| {
        run_process_inner(terminal, executable, arguments, options)
    })
}

fn run_process_inner(
    terminal: &mut RunTerminal,
    executable: String,
    arguments: Vec<String>,
    options: StartProcessOptions,
) -> io::Result<()> {
    let StartProcessOptions {
        timeout,
        pty,
        input_wait_detect_milliseconds,
        worktree_repository,
    } = options;
    let run_id = terminal.run_id.clone();
    match worktree_repository {
        Some(repository) => match RunWorktree::create(repository, &run_id) {
            Ok(worktree) => terminal.install_worktree(Some(worktree)),
            Err(error) => {
                terminal.install_worktree(error.worktree);
                return terminal.emit(None, Some("worktree_failed"));
            }
        },
        None => terminal.install_worktree(None),
    }
    // The timeout deadline deliberately keeps ticking while the process group is paused.
    let timeout_started = Instant::now();
    let wall_clock_started = SystemTime::now();
    let working_directory = terminal.worktree.as_ref().map_or_else(
        || {
            std::env::current_dir()
                .map(|directory| directory.to_string_lossy().into_owned())
                .unwrap_or_default()
        },
        |worktree| worktree.path.to_string_lossy().into_owned(),
    );
    let mut environment_variable_names: Vec<String> = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    environment_variable_names.sort();
    let (mut child, pty_master) = if pty {
        let (master, slave) = match open_pseudo_terminal() {
            Ok(terminal) => terminal,
            Err(_) => return terminal.emit(None, Some("spawn_failed")),
        };
        let slave_fd = slave.as_raw_fd();
        let mut command = Command::new(&executable);
        command.args(&arguments);
        if let Some(worktree) = terminal.worktree.as_ref() {
            command.current_dir(&worktree.path);
        }
        // SAFETY: configure_pty_child performs only child-local session and descriptor setup.
        unsafe {
            command.pre_exec(move || configure_pty_child(slave_fd));
        }
        let child = match command.spawn() {
            Ok(child) => child,
            Err(_) => return terminal.emit(None, Some("spawn_failed")),
        };
        drop(slave);
        let master = Arc::new(master);
        terminal
            .registration()
            .publish_pty_master(Arc::clone(&master));
        // A PTY has one merged output stream; child stderr is reported as stdout here.
        (child, Some(master))
    } else {
        let mut command = Command::new(&executable);
        command
            .args(&arguments)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(worktree) = terminal.worktree.as_ref() {
            command.current_dir(&worktree.path);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                // A failed spawn has no process metadata and emits only its terminal frame.
                return terminal.emit(None, Some("spawn_failed"));
            }
        };
        if let Err(error) = terminal
            .registration()
            .publish_stdin(child.stdin.take().expect("piped stdin should be available"))
        {
            kill_process_group(&mut child);
            let _ = child.wait();
            return Err(error);
        }
        (child, None)
    };
    let pgid = child.id() as libc::pid_t;
    let activity = input_wait_detect_milliseconds.map(|_| Arc::new(ActivityClock::new()));
    let mut input_wait_episode = input_wait_detect_milliseconds.map(|milliseconds| {
        InputWaitEpisode::new(
            milliseconds,
            activity
                .as_deref()
                .expect("enabled input wait detection should have an activity clock"),
            pgid,
        )
    });
    if let Some(activity) = activity.as_deref() {
        terminal.registration().drain_stdin_with_activity(activity);
    } else {
        terminal.registration().drain_stdin();
    }
    terminal.registration().publish_pgid(pgid);
    let (process_output, stderr) = if let Some(master) = pty_master {
        (ProcessOutput::Pty(master), None)
    } else {
        let stdout = child
            .stdout
            .take()
            .expect("piped stdout should be available");
        let stderr = child
            .stderr
            .take()
            .expect("piped stderr should be available");
        if let Err(error) = set_nonblocking(&stdout).and_then(|_| set_nonblocking(&stderr)) {
            kill_process_group(&mut child);
            let _ = child.wait();
            return Err(error);
        }
        (ProcessOutput::Pipe(stdout), Some(stderr))
    };

    let control = Arc::clone(&terminal.registration().control);
    let redactions = Arc::new(AtomicUsize::new(0));
    let stdout_writer = Arc::clone(&terminal.writer);
    let stdout_run_id = run_id.clone();
    let stdout_control = Arc::clone(&control);
    let stdout_redactions = Arc::clone(&redactions);
    let stdout_activity = activity.clone();
    let stdout_drain = match thread::Builder::new()
        .name("capture-delegate-stdout".to_owned())
        .spawn(move || match process_output {
            ProcessOutput::Pipe(stdout) => drain_process_output_until_cancelled(
                stdout,
                stdout_writer,
                stdout_run_id,
                "stdout",
                stdout_control,
                stdout_redactions,
                stdout_activity,
            ),
            ProcessOutput::Pty(master) => drain_pty_output_until_cancelled(
                master,
                stdout_writer,
                stdout_run_id,
                stdout_control,
                stdout_redactions,
                stdout_activity,
            ),
        }) {
        Ok(drain) => drain,
        Err(error) => {
            kill_process_group(&mut child);
            let _ = child.wait();
            return Err(error);
        }
    };
    let stderr_drain = if pty {
        None
    } else {
        let stderr_writer = Arc::clone(&terminal.writer);
        let stderr_run_id = run_id.clone();
        let stderr_control = Arc::clone(&control);
        let stderr_redactions = Arc::clone(&redactions);
        let stderr_activity = activity.clone();
        let stderr = stderr.expect("non-PTY run should have piped stderr");
        match thread::Builder::new()
            .name("capture-delegate-stderr".to_owned())
            .spawn(move || {
                drain_process_output_until_cancelled(
                    stderr,
                    stderr_writer,
                    stderr_run_id,
                    "stderr",
                    stderr_control,
                    stderr_redactions,
                    stderr_activity,
                )
            }) {
            Ok(drain) => Some(drain),
            Err(error) => {
                let _ = teardown_process_and_drains(&mut child, &control, stdout_drain, None, true);
                return Err(error);
            }
        }
    };

    let mut input_wait_was_paused = false;
    let (exit_code, error_code) = loop {
        let child_status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let _ = teardown_process_and_drains(
                    &mut child,
                    &control,
                    stdout_drain,
                    stderr_drain,
                    true,
                );
                return Err(error);
            }
        };
        match classify_process_supervision(
            child_status.is_some(),
            timeout_started.elapsed() >= timeout,
            control.cancelled.load(Ordering::Acquire),
        ) {
            ProcessSupervision::Exited => {
                // A leader that exits while descendants remain (for example
                // `sh -c 'sleep 30 & exit 0'`) must not leak them past the
                // run's terminal frame, including after an accepted cancel.
                kill_process_group(&mut child);
                // The try_wait-to-retire window matches the pre-existing kill-after-reap pgid-reuse window.
                terminal.registration().retire();
                break (
                    child_status
                        .expect("completed child should have status")
                        .code(),
                    None,
                );
            }
            ProcessSupervision::TimedOut => {
                kill_process_group(&mut child);
                terminal.registration().retire();
                match child.wait() {
                    Ok(status) => break (status.code(), Some("timed_out")),
                    Err(error) => {
                        let _ = teardown_process_and_drains(
                            &mut child,
                            &control,
                            stdout_drain,
                            stderr_drain,
                            true,
                        );
                        return Err(error);
                    }
                }
            }
            ProcessSupervision::Cancelled => {
                kill_process_group(&mut child);
                terminal.registration().retire();
                match child.wait() {
                    Ok(status) => break (status.code(), Some("cancelled")),
                    Err(error) => {
                        let _ = teardown_process_and_drains(
                            &mut child,
                            &control,
                            stdout_drain,
                            stderr_drain,
                            true,
                        );
                        return Err(error);
                    }
                }
            }
            ProcessSupervision::Running => {
                if let Some(activity) = activity.as_deref() {
                    terminal.registration().drain_stdin_with_activity(activity);
                } else {
                    terminal.registration().drain_stdin();
                }
                let mut disarm_input_wait = false;
                if control.paused.load(Ordering::SeqCst) {
                    input_wait_was_paused = true;
                } else if let (Some(activity), Some(episode)) =
                    (activity.as_deref(), input_wait_episode.as_mut())
                {
                    episode.refresh_after_activity(activity, pgid);
                    if input_wait_was_paused {
                        episode.rebaseline_after_pause(activity, pgid);
                        input_wait_was_paused = false;
                    }
                    if !episode.fired
                        && activity
                            .quiet_for_milliseconds(
                                episode.activity_nanoseconds,
                                episode.quiet_started_nanoseconds,
                            )
                            .is_some_and(|quiet| quiet >= episode.threshold_milliseconds)
                        && control.stdin_is_idle()
                    {
                        match write_input_waiting_frame_if_quiet(
                            &terminal.writer,
                            &run_id,
                            pgid,
                            &control,
                            activity,
                            episode,
                        ) {
                            Ok(true) => episode.fired = true,
                            Ok(false) => {}
                            Err(_) => disarm_input_wait = true,
                        }
                    }
                }
                if disarm_input_wait {
                    input_wait_episode = None;
                }
                thread::sleep(PIPE_DRAIN_POLL_INTERVAL);
            }
        }
    };
    let wall_clock_finished = SystemTime::now();
    let duration_ms = u64::try_from(timeout_started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let (stdout_result, stderr_result) =
        teardown_process_and_drains(&mut child, &control, stdout_drain, stderr_drain, false);
    let stdout_result = stdout_result?;
    let stderr_result = if let Some(stderr_result) = stderr_result {
        stderr_result?
    } else {
        Ok(())
    };
    let mut metadata = RunMetadataResponse {
        version: PROTOCOL_VERSION,
        response_type: "run_metadata",
        run_id: &run_id,
        pid: pgid as u32,
        pgid: pgid as u32,
        executable: &executable,
        arguments: &arguments,
        working_directory,
        started_at: system_time_as_rfc3339(wall_clock_started),
        finished_at: system_time_as_rfc3339(wall_clock_finished),
        duration_ms,
        environment_variable_names,
        redactions: redactions.load(Ordering::Relaxed),
        worktree_path: terminal
            .worktree
            .as_ref()
            .map(|worktree| worktree.path.to_string_lossy().into_owned()),
        worktree_branch: terminal
            .worktree
            .as_ref()
            .map(|worktree| worktree.branch.clone()),
    };
    let metadata_result = (|| {
        let frame_is_too_large = |response: &RunMetadataResponse<'_>| -> io::Result<bool> {
            Ok(serde_json::to_vec(response)?.len() + 1 >= MAX_REQUEST_BYTES)
        };
        if frame_is_too_large(&metadata)? {
            metadata.environment_variable_names.clear();
        }
        if frame_is_too_large(&metadata)? {
            metadata.arguments = &[];
        }
        write_client_json_frame(&terminal.writer, &metadata)
    })();
    let run_exit_result = terminal.emit(
        if error_code.is_some() {
            None
        } else {
            exit_code
        },
        error_code,
    );
    metadata_result?;
    run_exit_result?;
    stdout_result?;
    stderr_result
}

#[cfg(test)]
fn drain_process_output<R: Read>(
    reader: R,
    stream: Arc<ClientWriter>,
    run_id: String,
    output_stream: &'static str,
) -> io::Result<()> {
    drain_process_output_until_cancelled(
        reader,
        stream,
        run_id,
        output_stream,
        Arc::new(RunControl::new()),
        Arc::new(AtomicUsize::new(0)),
        None,
    )
}

fn drain_process_output_until_cancelled<R: Read>(
    mut reader: R,
    stream: Arc<ClientWriter>,
    run_id: String,
    output_stream: &'static str,
    control: Arc<RunControl>,
    redactions: Arc<AtomicUsize>,
    activity: Option<Arc<ActivityClock>>,
) -> io::Result<()> {
    let mut buffer = [0_u8; MAX_OUTPUT_CHUNK_BYTES];
    let mut utf8_carry = Vec::with_capacity(3);
    loop {
        let bytes_read = match reader.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if control.cancelled.load(Ordering::Acquire) {
                    break;
                }
                thread::sleep(PIPE_DRAIN_POLL_INTERVAL);
                continue;
            }
            Err(error) => return Err(error),
        };
        if bytes_read == 0 {
            break;
        }
        let output = decode_output_chunk(&mut utf8_carry, &buffer[..bytes_read]);
        if !output.is_empty() {
            let (output, redaction_count) = redact_output(&output);
            redactions.fetch_add(redaction_count, Ordering::Relaxed);
            let response = RunOutputResponse {
                version: PROTOCOL_VERSION,
                response_type: "run_output",
                run_id: &run_id,
                stream: output_stream,
                output,
            };
            write_client_json_frame_with_activity(&stream, &response, activity.as_deref())?;
        }
    }
    if !control.cancelled.load(Ordering::Acquire) && !utf8_carry.is_empty() {
        let (output, redaction_count) = redact_output(&String::from_utf8_lossy(&utf8_carry));
        redactions.fetch_add(redaction_count, Ordering::Relaxed);
        let response = RunOutputResponse {
            version: PROTOCOL_VERSION,
            response_type: "run_output",
            run_id: &run_id,
            stream: output_stream,
            output,
        };
        write_client_json_frame_with_activity(&stream, &response, activity.as_deref())?;
    }
    Ok(())
}

fn drain_pty_output_until_cancelled(
    master: Arc<File>,
    stream: Arc<ClientWriter>,
    run_id: String,
    control: Arc<RunControl>,
    redactions: Arc<AtomicUsize>,
    activity: Option<Arc<ActivityClock>>,
) -> io::Result<()> {
    let mut buffer = [0_u8; MAX_OUTPUT_CHUNK_BYTES];
    let mut utf8_carry = Vec::with_capacity(3);
    loop {
        let mut descriptor = libc::pollfd {
            fd: master.as_raw_fd(),
            events: libc::POLLIN | libc::POLLHUP,
            revents: 0,
        };
        // The PTY master is nonblocking because reads and input writes share its file
        // description. Poll bounds each wait so cancellation remains observable.
        let poll_result = unsafe { libc::poll(&mut descriptor, 1, 50) };
        if poll_result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if poll_result == 0 {
            if control.cancelled.load(Ordering::Acquire) {
                break;
            }
            continue;
        }

        let bytes_read = match master.as_ref().read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if control.cancelled.load(Ordering::Acquire) {
                    break;
                }
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) => return Err(error),
        };
        if bytes_read == 0 {
            break;
        }
        let output = decode_output_chunk(&mut utf8_carry, &buffer[..bytes_read]);
        if !output.is_empty() {
            let (output, redaction_count) = redact_output(&output);
            redactions.fetch_add(redaction_count, Ordering::Relaxed);
            let response = RunOutputResponse {
                version: PROTOCOL_VERSION,
                response_type: "run_output",
                run_id: &run_id,
                stream: "stdout",
                output,
            };
            write_client_json_frame_with_activity(&stream, &response, activity.as_deref())?;
        }
    }
    if !control.cancelled.load(Ordering::Acquire) && !utf8_carry.is_empty() {
        let (output, redaction_count) = redact_output(&String::from_utf8_lossy(&utf8_carry));
        redactions.fetch_add(redaction_count, Ordering::Relaxed);
        let response = RunOutputResponse {
            version: PROTOCOL_VERSION,
            response_type: "run_output",
            run_id: &run_id,
            stream: "stdout",
            output,
        };
        write_client_json_frame_with_activity(&stream, &response, activity.as_deref())?;
    }
    Ok(())
}

fn set_close_on_exec<R: AsRawFd>(descriptor: &R) -> io::Result<()> {
    // SAFETY: descriptor is open and fcntl only reads or updates descriptor flags.
    let flags = unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_GETFD) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above; this preserves existing flags while adding close-on-exec.
    if unsafe {
        libc::fcntl(
            descriptor.as_raw_fd(),
            libc::F_SETFD,
            flags | libc::FD_CLOEXEC,
        )
    } == -1
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_nonblocking<R: std::os::fd::AsRawFd>(reader: &R) -> io::Result<()> {
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd"
    ))]
    const O_NONBLOCK: std::ffi::c_int = 0x0004;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    const O_NONBLOCK: std::ffi::c_int = 0x0800;
    const F_GETFL: std::ffi::c_int = 3;
    const F_SETFL: std::ffi::c_int = 4;

    unsafe extern "C" {
        fn fcntl(fd: std::ffi::c_int, command: std::ffi::c_int, ...) -> std::ffi::c_int;
    }

    // SAFETY: `reader` supplies a valid open descriptor, and fcntl neither
    // retains it nor accesses Rust-managed memory.
    let fd = reader.as_raw_fd();
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above; this only adds O_NONBLOCK to the descriptor flags.
    if unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn decode_output_chunk(utf8_carry: &mut Vec<u8>, bytes: &[u8]) -> String {
    utf8_carry.extend_from_slice(bytes);
    let mut output = String::new();
    let mut consumed = 0;

    loop {
        match std::str::from_utf8(&utf8_carry[consumed..]) {
            Ok(valid) => {
                output.push_str(valid);
                consumed = utf8_carry.len();
                break;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                let valid_end = consumed + valid_up_to;
                output.push_str(
                    std::str::from_utf8(&utf8_carry[consumed..valid_end])
                        .expect("valid UTF-8 prefix should decode"),
                );
                consumed = valid_end;
                let Some(invalid_length) = error.error_len() else {
                    break;
                };
                output.push('\u{fffd}');
                consumed += invalid_length;
            }
        }
    }

    utf8_carry.drain(..consumed);
    output
}

/// A timeline offset in whole non-negative milliseconds, or `None` for anything else.
fn milliseconds_field(value: &serde_json::Value) -> Option<i64> {
    value.as_u64().and_then(|value| i64::try_from(value).ok())
}

fn serialize_json_frame<T: Serialize>(response: &T) -> io::Result<Vec<u8>> {
    let mut frame = serde_json::to_vec(response)?;
    frame.push(b'\n');
    if frame.len() >= MAX_REQUEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "response must be a bounded newline-delimited frame",
        ));
    }
    Ok(frame)
}

fn write_serialized_frame(stream: &mut UnixStream, frame: &[u8]) -> io::Result<()> {
    stream.write_all(frame)?;
    stream.flush()
}

fn write_json_frame<T: Serialize>(stream: &Arc<Mutex<UnixStream>>, response: &T) -> io::Result<()> {
    let frame = serialize_json_frame(response)?;
    let mut stream = stream
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    write_serialized_frame(&mut stream, &frame)
}

fn write_client_json_frame<T: Serialize>(
    writer: &Arc<ClientWriter>,
    response: &T,
) -> io::Result<()> {
    let frame = serialize_json_frame(response)?;
    writer.write_frame_bytes(&frame)
}

fn write_client_json_frame_with_activity<T: Serialize>(
    writer: &Arc<ClientWriter>,
    response: &T,
    activity: Option<&ActivityClock>,
) -> io::Result<()> {
    let frame = serialize_json_frame(response)?;
    writer.write_frame_bytes(&frame)?;
    if let Some(activity) = activity {
        activity.record();
    }
    Ok(())
}

fn write_input_waiting_frame_if_quiet(
    writer: &Arc<ClientWriter>,
    run_id: &str,
    pid: libc::pid_t,
    control: &RunControl,
    activity: &ActivityClock,
    episode: &InputWaitEpisode,
) -> io::Result<bool> {
    if writer.is_dead() {
        return Err(ClientWriter::dead_error());
    }
    let mut stream = match writer.stream.try_lock() {
        Ok(stream) => stream,
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
    };
    if writer.is_dead() {
        return Err(ClientWriter::dead_error());
    }
    if activity.snapshot() != episode.activity_nanoseconds {
        return Ok(false);
    }
    let stdin = match control.stdin.try_lock() {
        Ok(stdin) => stdin,
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
    };
    if !stdin.buffer.is_empty() {
        return Ok(false);
    }
    let Some(cpu_at_activity_nanoseconds) = episode.cpu_at_activity_nanoseconds else {
        return Ok(false);
    };
    let Some(cpu_now_nanoseconds) = process_cpu_time_nanoseconds(pid) else {
        return Ok(false);
    };
    if cpu_now_nanoseconds.saturating_sub(cpu_at_activity_nanoseconds)
        >= INPUT_WAIT_CPU_QUIET_NANOSECONDS
    {
        return Ok(false);
    }
    let Some(quiet_for_milliseconds) = activity.quiet_for_milliseconds(
        episode.activity_nanoseconds,
        episode.quiet_started_nanoseconds,
    ) else {
        return Ok(false);
    };
    if quiet_for_milliseconds < episode.threshold_milliseconds {
        return Ok(false);
    }

    let response = RunInputWaitingResponse {
        version: PROTOCOL_VERSION,
        response_type: "run_input_waiting",
        run_id,
        quiet_for_milliseconds,
    };
    let frame = serialize_json_frame(&response)?;
    writer.write_frame_bytes_locked(&mut stream, &frame)?;
    drop(stdin);
    Ok(true)
}

fn write_protocol_error(stream: &mut UnixStream, code: &'static str) -> io::Result<()> {
    let response = ProtocolErrorResponse {
        version: PROTOCOL_VERSION,
        response_type: "error",
        code,
    };
    serde_json::to_writer(&mut *stream, &response)?;
    stream.write_all(b"\n")?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveRuns, ClientWriter, MAX_CONCURRENT_PROCESSES, MAX_PENDING_STDIN_BYTES,
        PIPE_DRAIN_POLL_INTERVAL, ProcessSupervision, RegisterRunError, RunTerminal, RunWorktree,
        SendInputStatus, SocketCleanup, SocketIdentity, Store, WorkerSlots,
        classify_process_supervision, cleanup_orphaned_worktrees_in, command_succeeds_within,
        drain_process_output, handle_connection, redact_output, run_with_internal_error_terminal,
        store,
    };
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_store() -> Store {
        store::in_memory_store().expect("test store should open")
    }

    fn test_fixture_root(prefix: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("backend should be inside repository")
            .join("target")
            .join(format!("{prefix}-{}-{nonce}", std::process::id()))
    }

    fn initialize_test_repository(repository: &std::path::Path) {
        fs::create_dir_all(repository).expect("repository directory should be created");
        assert!(
            Command::new("git")
                .arg("init")
                .arg(repository)
                .status()
                .expect("git init should run")
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repository)
                .args([
                    "-c",
                    "user.name=Capture Delegate Tests",
                    "-c",
                    "user.email=capture-delegate@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "initial",
                ])
                .status()
                .expect("git commit should run")
                .success()
        );
    }

    fn add_test_worktree(
        repository: &std::path::Path,
        root: &std::path::Path,
        directory_name: &str,
        branch: &str,
    ) -> std::path::PathBuf {
        let path = root.join(directory_name);
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repository)
                .args(["worktree", "add"])
                .arg(&path)
                .arg("-b")
                .arg(branch)
                .arg("HEAD")
                .status()
                .expect("git worktree add should run")
                .success()
        );
        path
    }

    fn reaped_test_pid() -> libc::pid_t {
        let mut child = Command::new("/usr/bin/true")
            .spawn()
            .expect("true should spawn");
        let pid = child.id() as libc::pid_t;
        assert!(child.wait().expect("true should reap").success());
        // SAFETY: pid came from a child which has already been reaped.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
        pid
    }

    #[test]
    fn client_writer_stops_after_the_first_failed_frame_write() {
        let (stream, mut peer) = UnixStream::pair().expect("socket pair should open");
        stream
            .set_write_timeout(Some(std::time::Duration::from_millis(200)))
            .expect("write timeout should configure");
        let writer = Arc::new(ClientWriter::new(stream));
        let frame = vec![b'x'; 8 * 1024 * 1024];
        let first_writer = Arc::clone(&writer);
        let first_write = thread::spawn(move || first_writer.write_frame_bytes(&frame));

        let lock_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            assert!(
                std::time::Instant::now() < lock_deadline,
                "first writer should acquire the stream lock"
            );
            match writer.stream.try_lock() {
                Ok(stream) => drop(stream),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => drop(poisoned.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => break,
            }
            thread::yield_now();
        }

        let sentinel = b"second-writer-sentinel\n".to_vec();
        let second_writer = Arc::clone(&writer);
        let second_sentinel = sentinel.clone();
        let (second_started_tx, second_started_rx) = std::sync::mpsc::channel();
        let second_write = thread::spawn(move || {
            second_started_tx
                .send(())
                .expect("second writer start should be observable");
            second_writer.write_frame_bytes(&second_sentinel)
        });
        second_started_rx
            .recv()
            .expect("second writer should start while the first holds the lock");
        thread::sleep(std::time::Duration::from_millis(20));
        assert!(
            !second_write.is_finished(),
            "second writer should be waiting on the stream lock"
        );

        let first_error = first_write
            .join()
            .expect("first writer should not panic")
            .expect_err("a full socket should reject a bounded write");
        assert!(matches!(
            first_error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ));
        assert!(writer.is_dead());

        peer.set_nonblocking(true)
            .expect("peer should become nonblocking");
        let mut received = Vec::new();
        let mut buffer = [0_u8; 16 * 1024];
        let drain_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match peer.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes_read) => received.extend_from_slice(&buffer[..bytes_read]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if second_write.is_finished() {
                        break;
                    }
                    assert!(
                        std::time::Instant::now() < drain_deadline,
                        "second writer should finish once the peer drains"
                    );
                    thread::yield_now();
                }
                Err(error) => panic!("buffered bytes should drain: {error}"),
            }
        }

        let second_result = second_write.join().expect("second writer should not panic");
        assert!(
            !received
                .windows(sentinel.len())
                .any(|window| window == sentinel),
            "a writer queued before client death must not write after acquiring the lock"
        );
        assert!(second_result.is_err());
        assert!(writer.write_frame_bytes(b"must-not-be-written\n").is_err());
        assert!(matches!(
            peer.read(&mut buffer),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    }

    #[test]
    fn injected_internal_error_emits_one_terminal_frame_and_releases_the_run_id() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("internal-run".to_owned())
            .expect("run should register");
        let (stream, peer) = UnixStream::pair().expect("socket pair should open");
        peer.set_read_timeout(Some(std::time::Duration::from_millis(20)))
            .expect("read timeout should configure");
        let writer = Arc::new(ClientWriter::new(stream));
        let mut terminal = RunTerminal::new(
            Arc::clone(&writer),
            "internal-run".to_owned(),
            registration,
            None,
        );

        let result = run_with_internal_error_terminal(&mut terminal, |_| {
            Err(std::io::Error::other("injected inner failure"))
        });

        assert!(result.is_err());
        assert!(!runs.cancel("internal-run"));
        let mut reader = BufReader::new(peer);
        let mut frame = String::new();
        reader
            .read_line(&mut frame)
            .expect("internal terminal frame should be readable");
        let frame: serde_json::Value = serde_json::from_str(&frame).expect("frame should be JSON");
        assert_eq!(frame["type"], "run_exit");
        assert_eq!(frame["run_id"], "internal-run");
        assert!(frame["exit_code"].is_null());
        assert_eq!(frame["error_code"], "internal_error");
        let mut extra = String::new();
        assert!(matches!(
            reader.read_line(&mut extra),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                )
        ));
    }

    #[test]
    fn typed_terminal_frame_suppresses_the_internal_error_fallback() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("typed-terminal-run".to_owned())
            .expect("run should register");
        let (stream, peer) = UnixStream::pair().expect("socket pair should open");
        peer.set_read_timeout(Some(std::time::Duration::from_millis(20)))
            .expect("read timeout should configure");
        let writer = Arc::new(ClientWriter::new(stream));
        let mut terminal = RunTerminal::new(
            Arc::clone(&writer),
            "typed-terminal-run".to_owned(),
            registration,
            None,
        );

        let result = run_with_internal_error_terminal(&mut terminal, |terminal| {
            terminal.emit(None, Some("cancelled"))?;
            Err(std::io::Error::other("failure after typed terminal"))
        });

        assert!(result.is_err());
        let mut reader = BufReader::new(peer);
        let mut frame = String::new();
        reader
            .read_line(&mut frame)
            .expect("typed terminal frame should be readable");
        let frame: serde_json::Value =
            serde_json::from_str(&frame).expect("typed terminal frame should be JSON");
        assert_eq!(frame["type"], "run_exit");
        assert_eq!(frame["run_id"], "typed-terminal-run");
        assert_eq!(frame["error_code"], "cancelled");
        let mut extra = String::new();
        assert!(matches!(
            reader.read_line(&mut extra),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                )
        ));
    }

    #[test]
    fn injected_internal_error_writes_nothing_after_client_death() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("dead-client-run".to_owned())
            .expect("run should register");
        let (stream, mut peer) = UnixStream::pair().expect("socket pair should open");
        peer.set_nonblocking(true)
            .expect("peer should become nonblocking");
        let writer = Arc::new(ClientWriter::new(stream));
        writer.mark_dead();
        let mut terminal = RunTerminal::new(
            Arc::clone(&writer),
            "dead-client-run".to_owned(),
            registration,
            None,
        );

        let result = run_with_internal_error_terminal(&mut terminal, |_| {
            Err(std::io::Error::other("injected inner failure"))
        });

        assert!(result.is_err());
        let mut buffer = [0_u8; 1];
        assert!(matches!(
            peer.read(&mut buffer),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    }

    #[test]
    fn completed_child_wins_over_an_elapsed_timeout() {
        assert_eq!(
            classify_process_supervision(true, true, false),
            ProcessSupervision::Exited
        );
    }

    #[test]
    fn completed_child_wins_over_a_pending_cancellation() {
        assert_eq!(
            classify_process_supervision(true, false, true),
            ProcessSupervision::Exited
        );
    }

    #[test]
    fn secret_pattern_classes_are_redacted_and_counted() {
        let input = concat!(
            "AKIAABCDEFGHIJKLMNOP\n",
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789\n",
            "github_pat_abcdefghijklmnopqrstuv\n",
            "xoxb-1234567890\n",
            "sk-abcdefghijklmnopqrst\n",
            "Bearer abcdefgh\n",
            "api_key=abcd1234\n",
            "-----BEGIN RSA PRIVATE KEY-----\nprivate-data\n-----END RSA PRIVATE KEY-----\n",
            "-----BEGIN EC PRIVATE KEY-----\nunterminated-private-data",
        );

        let (redacted, count) = redact_output(input);

        assert_eq!(count, 9);
        assert_eq!(redacted.matches("[REDACTED]").count(), 9);
        assert!(redacted.contains("Bearer [REDACTED]"));
        assert!(redacted.contains("api_key=[REDACTED]"));
        for secret in [
            "AKIAABCDEFGHIJKLMNOP",
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "github_pat_abcdefghijklmnopqrstuv",
            "xoxb-1234567890",
            "sk-abcdefghijklmnopqrst",
            "abcdefgh",
            "abcd1234",
            "private-data",
            "unterminated-private-data",
        ] {
            assert!(!redacted.contains(secret), "secret leaked: {secret}");
        }
    }

    #[test]
    fn benign_output_is_not_changed_by_redaction() {
        let input = concat!(
            "monotonic deadline token bucket\n",
            "https://example.com/releases/v1.2.3\n",
            "commit deadbeef0123456789abcdef0123456789abcdef\n",
        );

        assert_eq!(redact_output(input), (input.to_owned(), 0));
    }

    #[test]
    fn active_run_registry_rejects_duplicates_and_releases_ids_on_teardown() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("run-1".to_owned())
            .expect("first registration should succeed");
        assert!(runs.cancel("run-1"));
        assert!(runs.register("run-1".to_owned()).is_err());
        drop(registration);
        assert!(!runs.cancel("run-1"));
        assert!(runs.register("run-1".to_owned()).is_ok());
    }

    #[test]
    fn cleanup_blocker_keeps_shutdown_waiting_after_run_id_release() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("run-1".to_owned())
            .expect("registration should succeed");
        let cleanup_blocker = registration.acquire_cleanup_blocker();

        drop(registration);

        assert!(!runs.wait_until_empty(std::time::Duration::ZERO));
        drop(cleanup_blocker);
        assert!(runs.wait_until_empty(std::time::Duration::ZERO));
    }

    #[test]
    fn bounded_command_runner_kills_and_reaps_a_hung_child() {
        let started = std::time::Instant::now();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30"]);

        assert!(!command_succeeds_within(
            &mut command,
            std::time::Duration::from_millis(20)
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn cleanup_preserves_a_branch_the_worktree_did_not_create() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("backend should be inside repository")
            .join("target")
            .join(format!("branch-owner-{nonce}"));
        let repository = root.join("repository");
        let managed_root = root.join("managed");
        let path = managed_root.join("run");
        fs::create_dir_all(&path).expect("worktree fixture should be created");
        assert!(
            Command::new("git")
                .arg("init")
                .arg(&repository)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args([
                    "-c",
                    "user.name=Capture Delegate Tests",
                    "-c",
                    "user.email=capture-delegate@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "initial",
                ])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["checkout", "-b", "capture-delegate/preexisting"])
                .status()
                .unwrap()
                .success()
        );
        let mut worktree = RunWorktree {
            root: managed_root.canonicalize().unwrap(),
            repository: repository.canonicalize().unwrap(),
            path: path.canonicalize().unwrap(),
            branch: "capture-delegate/preexisting".to_owned(),
            delete_branch: false,
            cleaned: false,
        };

        worktree.cleanup();

        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args([
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "refs/heads/capture-delegate/preexisting",
                ])
                .status()
                .unwrap()
                .success()
        );
        fs::remove_dir_all(&root).expect("fixture should be removed");
    }

    #[test]
    fn run_worktree_owner_sidecar_tracks_its_lifecycle() {
        let fixture_root = test_fixture_root("worktree-owner");
        let repository = fixture_root.join("repository");
        initialize_test_repository(&repository);
        let mut worktree = match RunWorktree::create(repository.clone(), "owner-lifecycle") {
            Ok(worktree) => worktree,
            Err(_) => panic!("worktree should be created"),
        };
        let sidecar = worktree.root.join(".owners").join(
            worktree
                .path
                .file_name()
                .expect("worktree should have a directory name"),
        );

        assert_eq!(
            fs::read_to_string(&sidecar).expect("owner sidecar should be readable"),
            format!("{}\n", std::process::id())
        );
        assert_eq!(
            fs::metadata(worktree.root.join(".owners"))
                .expect("owners directory should be readable")
                .mode()
                & 0o777,
            0o700,
            "owners directory should be private"
        );

        worktree.cleanup();

        assert!(
            !sidecar.exists(),
            "normal cleanup should remove the sidecar"
        );
        fs::remove_dir_all(&fixture_root).expect("fixture should be removed");
    }

    #[test]
    fn orphan_cleanup_removes_only_dead_managed_worktrees_and_owned_branches() {
        let fixture_root = test_fixture_root("orphan-cleanup");
        let repository = fixture_root.join("repository");
        let root = fixture_root.join("managed-root");
        initialize_test_repository(&repository);
        fs::create_dir_all(root.join(".owners")).expect("owners directory should be created");
        let root = root
            .canonicalize()
            .expect("managed root should canonicalize");
        let dead_pid = reaped_test_pid();

        let orphan_branch = "capture-delegate/run-dead-owner";
        let orphan = add_test_worktree(&repository, &root, "run-dead-owner", orphan_branch);
        fs::write(
            root.join(".owners").join("run-dead-owner"),
            format!("{dead_pid}\n"),
        )
        .expect("orphan owner sidecar should write");

        let live_branch = "capture-delegate/run-live-owner";
        let live = add_test_worktree(&repository, &root, "run-live-owner", live_branch);
        fs::write(
            root.join(".owners").join("run-live-owner"),
            format!("{}\n", std::process::id()),
        )
        .expect("live owner sidecar should write");

        let unsided_branch = "capture-delegate/run-no-sidecar";
        let unsided = add_test_worktree(&repository, &root, "run-no-sidecar", unsided_branch);

        let preserved_branch = "keep-this-branch";
        let preserved =
            add_test_worktree(&repository, &root, "run-preserve-branch", preserved_branch);
        fs::write(
            root.join(".owners").join("run-preserve-branch"),
            format!("{dead_pid}\n"),
        )
        .expect("preserved branch owner sidecar should write");

        let malformed = root.join("run-malformed-git");
        fs::create_dir(&malformed).expect("malformed worktree directory should be created");
        fs::write(malformed.join(".git"), "not a valid gitdir file\n")
            .expect("malformed git file should write");
        fs::write(
            root.join(".owners").join("run-malformed-git"),
            format!("{dead_pid}\n"),
        )
        .expect("malformed worktree owner sidecar should write");
        let malformed_sentinel_branch = "keep-malformed-git-sentinel";
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["branch", malformed_sentinel_branch])
                .status()
                .expect("sentinel branch should be created")
                .success()
        );

        let ignored = root.join("not-a-run");
        fs::create_dir(&ignored).expect("non-run directory should be created");
        let symlink = root.join("run-symlink");
        std::os::unix::fs::symlink(&live, &symlink).expect("symlink should be created");
        fs::write(root.join(".owners").join("stale-run"), "123\n")
            .expect("stale sidecar should write");

        cleanup_orphaned_worktrees_in(&root);

        assert!(!orphan.exists(), "dead owner worktree should be removed");
        assert!(
            !root.join(".owners").join("run-dead-owner").exists(),
            "dead owner sidecar should be removed"
        );
        assert!(
            !Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["show-ref", "--verify", "--quiet"])
                .arg(format!("refs/heads/{orphan_branch}"))
                .status()
                .expect("git show-ref should run")
                .success(),
            "managed orphan branch should be removed"
        );
        assert!(live.exists(), "live owner worktree must remain");
        assert!(unsided.exists(), "worktree without sidecar must remain");
        assert!(!preserved.exists(), "dead owner worktree should be removed");
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["show-ref", "--verify", "--quiet"])
                .arg(format!("refs/heads/{preserved_branch}"))
                .status()
                .expect("git show-ref should run")
                .success(),
            "non-managed branch must be preserved"
        );
        assert!(
            !malformed.exists(),
            "dead malformed worktree directory should be removed"
        );
        assert!(
            !root.join(".owners").join("run-malformed-git").exists(),
            "dead malformed worktree sidecar should be removed"
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["show-ref", "--verify", "--quiet"])
                .arg(format!("refs/heads/{malformed_sentinel_branch}"))
                .status()
                .expect("git show-ref should run")
                .success(),
            "malformed metadata cleanup must not delete an unrelated branch"
        );
        assert!(ignored.exists(), "non-run directory must remain");
        assert!(symlink.exists(), "symlink child must remain");
        assert!(
            !root.join(".owners").join("stale-run").exists(),
            "stale sidecar should be removed"
        );

        Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(["worktree", "remove", "--force"])
            .arg(&live)
            .status()
            .expect("live worktree cleanup should run");
        Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(["worktree", "remove", "--force"])
            .arg(&unsided)
            .status()
            .expect("unsided worktree cleanup should run");
        fs::remove_file(&symlink).expect("symlink should be removed");
        fs::remove_dir_all(&fixture_root).expect("fixture should be removed");
    }

    #[test]
    fn orphan_cleanup_rejects_forged_repository_metadata() {
        let fixture_root = test_fixture_root("orphan-forged-metadata");
        let foreign_repository = fixture_root.join("foreign-repository");
        let foreign_worktree_root = fixture_root.join("foreign-worktrees");
        let managed_root = fixture_root.join("managed-root");
        let foreign_branch = "capture-delegate/run-foreign-target";
        initialize_test_repository(&foreign_repository);
        fs::create_dir_all(managed_root.join(".owners"))
            .expect("managed owners directory should be created");
        let managed_root = managed_root
            .canonicalize()
            .expect("managed root should canonicalize");
        let foreign_worktree = add_test_worktree(
            &foreign_repository,
            &foreign_worktree_root,
            "run-foreign-target",
            foreign_branch,
        );
        let foreign_git_contents = fs::read_to_string(foreign_worktree.join(".git"))
            .expect("foreign git file should read");
        let foreign_admin = std::path::PathBuf::from(
            foreign_git_contents
                .trim_end()
                .strip_prefix("gitdir: ")
                .expect("foreign git file should contain an admin path"),
        );
        assert!(foreign_admin.exists(), "foreign admin entry should exist");
        fs::remove_dir_all(&foreign_worktree)
            .expect("foreign worktree directory should be removed directly");

        let forged = managed_root.join("run-forged-owner");
        fs::create_dir(&forged).expect("forged worktree directory should be created");
        fs::write(forged.join(".git"), &foreign_git_contents)
            .expect("forged git file should be written");
        fs::write(
            managed_root.join(".owners").join("run-forged-owner"),
            format!("{}\n", reaped_test_pid()),
        )
        .expect("forged owner sidecar should be written");

        cleanup_orphaned_worktrees_in(&managed_root);

        let foreign_branch_exists = Command::new("git")
            .arg("-C")
            .arg(&foreign_repository)
            .args(["show-ref", "--verify", "--quiet"])
            .arg(format!("refs/heads/{foreign_branch}"))
            .status()
            .expect("git show-ref should run")
            .success();
        let foreign_admin_exists = foreign_admin.exists();
        let forged_exists = forged.exists();
        let forged_sidecar_exists = managed_root
            .join(".owners")
            .join("run-forged-owner")
            .exists();

        let _ = Command::new("git")
            .arg("-C")
            .arg(&foreign_repository)
            .args(["worktree", "prune"])
            .status();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&foreign_repository)
            .args(["branch", "-D", foreign_branch])
            .status();
        fs::remove_dir_all(&fixture_root).expect("fixture should be removed");

        assert!(
            !forged_exists,
            "forged candidate should be deleted directly"
        );
        assert!(
            !forged_sidecar_exists,
            "forged candidate sidecar should be removed"
        );
        assert!(
            foreign_branch_exists,
            "forged metadata must not delete the foreign managed branch"
        );
        assert!(
            foreign_admin_exists,
            "forged metadata must not prune the foreign admin entry"
        );
    }

    #[test]
    fn pausing_before_the_process_group_is_known_records_the_pause_without_signalling() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("run-1".to_owned())
            .expect("registration should succeed");

        assert!(runs.pause("run-1"));
        assert!(registration.control.paused.load(Ordering::SeqCst));
        assert_eq!(registration.control.pgid.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn input_buffered_before_stdin_is_published_is_delivered_in_order() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("run-1".to_owned())
            .expect("registration should succeed");
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat should spawn");

        assert_eq!(
            runs.send_input("run-1", "first\n"),
            SendInputStatus::Accepted
        );
        assert_eq!(
            runs.send_input("run-1", "second\n"),
            SendInputStatus::Accepted
        );
        assert!(runs.close_stdin("run-1"));
        registration
            .publish_stdin(child.stdin.take().expect("stdin should be piped"))
            .expect("stdin should publish");
        while registration.control.stdin.lock().unwrap().handle.is_some() {
            registration.drain_stdin();
            thread::sleep(PIPE_DRAIN_POLL_INTERVAL);
        }

        let output = child.wait_with_output().expect("cat should exit naturally");
        assert_eq!(output.stdout, b"first\nsecond\n");
    }

    #[test]
    fn closing_stdin_rejects_later_input() {
        let runs = ActiveRuns::new();
        let _registration = runs
            .register("run-1".to_owned())
            .expect("registration should succeed");

        assert!(runs.close_stdin("run-1"));
        assert_eq!(runs.send_input("run-1", "after\n"), SendInputStatus::Closed);
    }

    #[test]
    fn pending_stdin_capacity_rejection_is_atomic_and_closed_takes_precedence() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("run-1".to_owned())
            .expect("registration should succeed");
        let nearly_full = "x".repeat(MAX_PENDING_STDIN_BYTES - 1);

        assert_eq!(
            runs.send_input("run-1", &nearly_full),
            SendInputStatus::Accepted
        );
        assert_eq!(
            runs.send_input("run-1", "yz"),
            SendInputStatus::CapacityExhausted
        );
        assert_eq!(
            registration.control.stdin.lock().unwrap().buffer.len(),
            MAX_PENDING_STDIN_BYTES - 1,
            "a rejected request must not enqueue any bytes"
        );
        assert_eq!(runs.send_input("run-1", "z"), SendInputStatus::Accepted);
        assert_eq!(
            registration.control.stdin.lock().unwrap().buffer.len(),
            MAX_PENDING_STDIN_BYTES
        );

        assert!(runs.close_stdin("run-1"));
        assert_eq!(
            runs.send_input("run-1", "overflow"),
            SendInputStatus::Closed,
            "closed stdin must win over the capacity check"
        );
    }

    #[test]
    fn shutdown_cancels_existing_runs_and_rejects_new_registrations() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("existing-run".to_owned())
            .expect("registration should succeed before shutdown");

        runs.begin_shutdown();

        assert!(registration.control.cancelled.load(Ordering::Acquire));
        assert!(matches!(
            runs.register("new-run".to_owned()),
            Err(RegisterRunError::ShuttingDown)
        ));
    }

    #[test]
    fn cleanup_does_not_unlink_replacement_socket() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("backend should be inside repository")
            .join("target")
            .join(format!("cc-{}-{}", std::process::id(), nonce % 100_000_000));
        let socket_path = directory.join("health.sock");
        fs::create_dir(&directory).expect("test directory should be created");

        let original = UnixListener::bind(&socket_path).expect("original socket should bind");
        let identity = SocketIdentity::from_metadata(
            &fs::symlink_metadata(&socket_path).expect("original identity should be readable"),
        );
        let cleanup = SocketCleanup {
            path: socket_path.clone(),
            identity,
        };
        fs::remove_file(&socket_path).expect("original socket path should unlink");
        let replacement =
            UnixListener::bind(&socket_path).expect("replacement socket should bind at same path");

        drop(cleanup);
        assert!(
            socket_path.exists(),
            "cleanup must preserve a replacement socket"
        );

        drop(replacement);
        drop(original);
        fs::remove_file(&socket_path).expect("replacement socket should be removable");
        fs::remove_dir(&directory).expect("test directory should be removable");
    }

    #[test]
    fn cat_process_outputs_before_one_exit_frame() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let existing_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("backend should be inside repository")
            .join("target")
            .join(format!("cat-output-{}", nonce));
        let missing_file = existing_file.with_extension("missing");
        fs::write(&existing_file, "present output\n").expect("existing fixture should be written");
        let (mut client, server) = UnixStream::pair().expect("socket pair should open");
        let worker = std::thread::spawn(move || {
            handle_connection(
                server,
                WorkerSlots::new(MAX_CONCURRENT_PROCESSES),
                ActiveRuns::new(),
                test_store(),
            )
        });
        let request = serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "unit-run",
            "executable": "/bin/cat",
            "arguments": [existing_file, missing_file],
            "timeout_milliseconds": 2_000,
        });
        client
            .write_all(format!("{request}\n").as_bytes())
            .expect("request should write");
        let frames: Vec<serde_json::Value> = BufReader::new(client)
            .lines()
            .map(|line| {
                serde_json::from_str(&line.expect("frame should read")).expect("frame JSON")
            })
            .collect();

        worker
            .join()
            .expect("connection worker should not panic")
            .expect("connection should succeed");
        fs::remove_file(&existing_file).expect("existing fixture should be removed");
        let terminal_indexes: Vec<_> = frames
            .iter()
            .enumerate()
            .filter_map(|(index, frame)| (frame["type"] == "run_exit").then_some(index))
            .collect();
        assert_eq!(terminal_indexes, vec![frames.len() - 1]);
        assert!(frames.iter().any(|frame| frame["stream"] == "stdout"));
        assert!(frames.iter().any(|frame| frame["stream"] == "stderr"));
        assert!(
            frames.last().expect("exit frame")["exit_code"]
                .as_i64()
                .is_some_and(|exit_code| exit_code != 0)
        );
    }

    #[test]
    fn output_scalar_split_at_read_boundary_is_not_corrupted() {
        struct ChunkedReader {
            chunks: Vec<Vec<u8>>,
        }

        impl Read for ChunkedReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.chunks.is_empty() {
                    return Ok(0);
                }
                let chunk = self.chunks.remove(0);
                buffer[..chunk.len()].copy_from_slice(&chunk);
                Ok(chunk.len())
            }
        }

        let expected = format!("{}€", "a".repeat(1023));
        let bytes = expected.as_bytes();
        let reader = ChunkedReader {
            chunks: vec![bytes[..1024].to_vec(), bytes[1024..].to_vec()],
        };
        let (client, mut server) = UnixStream::pair().expect("socket pair should open");
        let stream = Arc::new(ClientWriter::new(client));

        drain_process_output(reader, Arc::clone(&stream), "unit-run".to_owned(), "stdout")
            .expect("output drain should succeed");
        drop(stream);
        let mut frames = String::new();
        server
            .read_to_string(&mut frames)
            .expect("frames should be readable");
        let output: String = frames
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("frame JSON"))
            .filter_map(|frame| frame["output"].as_str().map(str::to_owned))
            .collect();

        assert_eq!(output, expected);
        assert!(!output.contains('\u{fffd}'));
    }

    #[test]
    fn nonexistent_executable_emits_a_structured_terminal_frame() {
        let (mut client, server) = UnixStream::pair().expect("socket pair should open");
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("read timeout should configure");
        let worker = std::thread::spawn(move || {
            handle_connection(
                server,
                WorkerSlots::new(MAX_CONCURRENT_PROCESSES),
                ActiveRuns::new(),
                test_store(),
            )
        });
        let request = serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "missing-run",
            "executable": "/definitely/does/not/exist/capture-delegate",
            "arguments": [],
            "timeout_milliseconds": 2_000,
        });
        client
            .write_all(format!("{request}\n").as_bytes())
            .expect("request should write");
        let mut frame = String::new();
        BufReader::new(client)
            .read_line(&mut frame)
            .expect("terminal frame should be readable");
        let frame: serde_json::Value =
            serde_json::from_str(&frame).expect("terminal frame should be JSON");

        assert_eq!(frame["type"], "run_exit");
        assert_eq!(frame["run_id"], "missing-run");
        assert!(frame["exit_code"].is_null());
        assert_eq!(frame["error_code"], "spawn_failed");
        worker
            .join()
            .expect("connection worker should not panic")
            .expect("spawn failure should be represented in protocol");
    }

    #[test]
    fn exhausted_process_slots_emit_a_terminal_capacity_failure_without_waiting() {
        let process_slots = WorkerSlots::new(MAX_CONCURRENT_PROCESSES);
        let _occupied_slots: Vec<_> = (0..MAX_CONCURRENT_PROCESSES)
            .map(|_| process_slots.acquire())
            .collect();
        let (mut client, server) = UnixStream::pair().expect("socket pair should open");
        client
            .set_read_timeout(Some(std::time::Duration::from_millis(750)))
            .expect("read timeout should configure");
        let worker = std::thread::spawn(move || {
            handle_connection(server, process_slots, ActiveRuns::new(), test_store())
        });
        let request = serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "capacity-run",
            "executable": "/bin/sleep",
            "arguments": ["5"],
            "timeout_milliseconds": 10_000,
        });
        let started = std::time::Instant::now();
        client
            .write_all(format!("{request}\n").as_bytes())
            .expect("request should write");
        let mut frame = String::new();
        BufReader::new(client)
            .read_line(&mut frame)
            .expect("capacity failure should be readable");

        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        let frame: serde_json::Value = serde_json::from_str(&frame).expect("frame should be JSON");
        assert_eq!(frame["type"], "run_exit");
        assert_eq!(frame["run_id"], "capacity-run");
        assert!(frame["exit_code"].is_null());
        assert_eq!(frame["error_code"], "capacity_exhausted");
        worker
            .join()
            .expect("connection worker should not panic")
            .expect("capacity failure should be represented in protocol");
    }

    #[test]
    fn streaming_write_outlives_the_request_write_timeout() {
        let (mut client, server) = UnixStream::pair().expect("socket pair should open");
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("read timeout should configure");
        let worker = std::thread::spawn(move || {
            handle_connection(
                server,
                WorkerSlots::new(MAX_CONCURRENT_PROCESSES),
                ActiveRuns::new(),
                test_store(),
            )
        });
        let request = serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "backpressure-run",
            "executable": "/usr/bin/seq",
            "arguments": ["1", "500000"],
            "timeout_milliseconds": 10_000,
        });
        client
            .write_all(format!("{request}\n").as_bytes())
            .expect("request should write");

        std::thread::sleep(std::time::Duration::from_millis(600));
        let frames: Vec<serde_json::Value> = BufReader::new(client)
            .lines()
            .map(|line| {
                serde_json::from_str(&line.expect("complete frame should be readable"))
                    .expect("every frame should be complete JSON")
            })
            .collect();

        assert!(frames.iter().any(|frame| frame["type"] == "run_output"));
        assert!(
            frames
                .last()
                .is_some_and(|frame| frame["type"] == "run_exit")
        );
        worker
            .join()
            .expect("connection worker should not panic")
            .expect("stream should survive backpressure");
    }
}
