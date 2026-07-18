use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_OUTPUT_CHUNK_BYTES: usize = 1024;
const CLIENT_IO_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_CONCURRENT_CLIENTS: usize = 8;
const MAX_CONCURRENT_PROCESSES: usize = 8;
const PIPE_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);

static SHUTDOWN_PIPE_WRITE_FD: AtomicI32 = AtomicI32::new(-1);

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
struct RunOutputResponse<'a> {
    version: u32,
    #[serde(rename = "type")]
    response_type: &'static str,
    run_id: &'a str,
    stream: &'static str,
    output: String,
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
    runs: HashMap<String, Arc<AtomicBool>>,
    shutting_down: bool,
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
            })),
        }
    }

    fn register(&self, run_id: String) -> Result<RunRegistration, RegisterRunError> {
        let cancelled = Arc::new(AtomicBool::new(false));
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
        state.runs.insert(run_id.clone(), Arc::clone(&cancelled));
        Ok(RunRegistration {
            state: Arc::clone(&self.state),
            run_id,
            cancelled,
        })
    }

    fn cancel(&self, run_id: &str) -> bool {
        let cancelled = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .runs
            .get(run_id)
            .cloned();
        let Some(cancelled) = cancelled else {
            return false;
        };
        cancelled.store(true, Ordering::Release);
        true
    }

    fn begin_shutdown(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.shutting_down = true;
        for cancelled in state.runs.values() {
            cancelled.store(true, Ordering::Release);
        }
    }

    fn wait_until_empty(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .runs
                .is_empty()
            {
                return true;
            }
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
    cancelled: Arc<AtomicBool>,
}

impl Drop for RunRegistration {
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .runs
            .get(&self.run_id)
            .is_some_and(|cancelled| Arc::ptr_eq(cancelled, &self.cancelled))
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

pub fn run(socket_path: &Path) -> io::Result<()> {
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

    remove_stale_socket(socket_path)?;
    let listener = UnixListener::bind(socket_path)?;
    let bound_metadata = fs::symlink_metadata(socket_path)?;
    let bound_identity = SocketIdentity::from_metadata(&bound_metadata);
    let _cleanup = SocketCleanup {
        path: socket_path.to_path_buf(),
        identity: bound_identity,
    };
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;
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
        if let Err(error) = thread::Builder::new()
            .name("capture-delegate-ipc".to_owned())
            .spawn(move || {
                let _worker_slot = worker_slot;
                let _ = handle_connection(stream, process_slots, active_runs);
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
) -> io::Result<()> {
    stream.set_read_timeout(Some(CLIENT_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(CLIENT_IO_TIMEOUT))?;

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
    } else {
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
            "start_process" => {
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
                stream.set_write_timeout(None)?;
                if let Err(error) = thread::Builder::new()
                    .name("capture-delegate-process".to_owned())
                    .spawn(move || {
                        let _ = run_process(
                            stream,
                            run_id,
                            executable,
                            arguments,
                            timeout,
                            process_slot,
                            registration,
                        );
                    })
                {
                    return Err(io::Error::other(format!(
                        "process worker spawn error: {error}"
                    )));
                }
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
            _ => write_protocol_error(&mut stream, "unknown_request_type")?,
        }
    }

    Ok(())
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

fn run_process(
    stream: UnixStream,
    run_id: String,
    executable: String,
    arguments: Vec<String>,
    timeout: Duration,
    _process_slot: WorkerSlot,
    registration: RunRegistration,
) -> io::Result<()> {
    let stream = Arc::new(Mutex::new(stream));
    let timeout_started = Instant::now();
    let mut child = match Command::new(executable)
        .args(arguments)
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            drop(registration);
            return write_json_frame(
                &stream,
                &RunExitResponse {
                    version: PROTOCOL_VERSION,
                    response_type: "run_exit",
                    run_id: &run_id,
                    exit_code: None,
                    error_code: Some("spawn_failed"),
                },
            );
        }
    };
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

    let cancelled = Arc::clone(&registration.cancelled);
    let stdout_writer = Arc::clone(&stream);
    let stdout_run_id = run_id.clone();
    let stdout_cancelled = Arc::clone(&cancelled);
    let stdout_drain = match thread::Builder::new()
        .name("capture-delegate-stdout".to_owned())
        .spawn(move || {
            drain_process_output_until_cancelled(
                stdout,
                stdout_writer,
                stdout_run_id,
                "stdout",
                stdout_cancelled,
            )
        }) {
        Ok(drain) => drain,
        Err(error) => {
            kill_process_group(&mut child);
            let _ = child.wait();
            return Err(error);
        }
    };
    let stderr_writer = Arc::clone(&stream);
    let stderr_run_id = run_id.clone();
    let stderr_cancelled = Arc::clone(&cancelled);
    let stderr_drain = match thread::Builder::new()
        .name("capture-delegate-stderr".to_owned())
        .spawn(move || {
            drain_process_output_until_cancelled(
                stderr,
                stderr_writer,
                stderr_run_id,
                "stderr",
                stderr_cancelled,
            )
        }) {
        Ok(drain) => drain,
        Err(error) => {
            cancelled.store(true, Ordering::Release);
            kill_process_group(&mut child);
            let _ = child.wait();
            let _ = stdout_drain.join();
            return Err(error);
        }
    };

    let (exit_code, error_code) = loop {
        let child_status = child.try_wait()?;
        match classify_process_supervision(
            child_status.is_some(),
            timeout_started.elapsed() >= timeout,
            cancelled.load(Ordering::Acquire),
        ) {
            ProcessSupervision::Exited => {
                // A leader that exits while descendants remain (for example
                // `sh -c 'sleep 30 & exit 0'`) must not leak them past the
                // run's terminal frame, including after an accepted cancel.
                kill_process_group(&mut child);
                break (
                    child_status
                        .expect("completed child should have status")
                        .code(),
                    None,
                );
            }
            ProcessSupervision::TimedOut => {
                kill_process_group(&mut child);
                break (child.wait()?.code(), Some("timed_out"));
            }
            ProcessSupervision::Cancelled => {
                kill_process_group(&mut child);
                break (child.wait()?.code(), Some("cancelled"));
            }
            ProcessSupervision::Running => thread::sleep(PIPE_DRAIN_POLL_INTERVAL),
        }
    };
    cancelled.store(true, Ordering::Release);
    let stdout_result = stdout_drain
        .join()
        .map_err(|_| io::Error::other("stdout drain thread panicked"))?;
    let stderr_result = stderr_drain
        .join()
        .map_err(|_| io::Error::other("stderr drain thread panicked"))?;
    // The run id must be released before the terminal frame is written so a
    // client that observes run_exit can immediately reuse the id.
    drop(registration);
    write_json_frame(
        &stream,
        &RunExitResponse {
            version: PROTOCOL_VERSION,
            response_type: "run_exit",
            run_id: &run_id,
            exit_code: if error_code.is_some() {
                None
            } else {
                exit_code
            },
            error_code,
        },
    )?;
    stdout_result?;
    stderr_result
}

#[cfg(test)]
fn drain_process_output<R: Read>(
    reader: R,
    stream: Arc<Mutex<UnixStream>>,
    run_id: String,
    output_stream: &'static str,
) -> io::Result<()> {
    drain_process_output_until_cancelled(
        reader,
        stream,
        run_id,
        output_stream,
        Arc::new(AtomicBool::new(false)),
    )
}

fn drain_process_output_until_cancelled<R: Read>(
    mut reader: R,
    stream: Arc<Mutex<UnixStream>>,
    run_id: String,
    output_stream: &'static str,
    cancelled: Arc<AtomicBool>,
) -> io::Result<()> {
    let mut buffer = [0_u8; MAX_OUTPUT_CHUNK_BYTES];
    let mut utf8_carry = Vec::with_capacity(3);
    loop {
        let bytes_read = match reader.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if cancelled.load(Ordering::Acquire) {
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
            let response = RunOutputResponse {
                version: PROTOCOL_VERSION,
                response_type: "run_output",
                run_id: &run_id,
                stream: output_stream,
                output,
            };
            if let Err(error) = write_json_frame(&stream, &response) {
                cancelled.store(true, Ordering::Release);
                return Err(error);
            }
        }
    }
    if !cancelled.load(Ordering::Acquire) && !utf8_carry.is_empty() {
        let response = RunOutputResponse {
            version: PROTOCOL_VERSION,
            response_type: "run_output",
            run_id: &run_id,
            stream: output_stream,
            output: String::from_utf8_lossy(&utf8_carry).into_owned(),
        };
        write_json_frame(&stream, &response).inspect_err(|_| {
            cancelled.store(true, Ordering::Release);
        })?;
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

    // SAFETY: `reader` supplies a valid open pipe descriptor, and fcntl neither
    // retains it nor accesses Rust-managed memory.
    let fd = reader.as_raw_fd();
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above; this only adds O_NONBLOCK to the pipe descriptor flags.
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

fn write_json_frame<T: Serialize>(stream: &Arc<Mutex<UnixStream>>, response: &T) -> io::Result<()> {
    let mut frame = serde_json::to_vec(response)?;
    frame.push(b'\n');
    if frame.len() >= MAX_REQUEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "response must be a bounded newline-delimited frame",
        ));
    }
    let mut stream = stream
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    stream.write_all(&frame)?;
    stream.flush()
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
        ActiveRuns, MAX_CONCURRENT_PROCESSES, ProcessSupervision, RegisterRunError, SocketCleanup,
        SocketIdentity, WorkerSlots, classify_process_supervision, drain_process_output,
        handle_connection,
    };
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn shutdown_cancels_existing_runs_and_rejects_new_registrations() {
        let runs = ActiveRuns::new();
        let registration = runs
            .register("existing-run".to_owned())
            .expect("registration should succeed before shutdown");

        runs.begin_shutdown();

        assert!(registration.cancelled.load(Ordering::Acquire));
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
        let stream = Arc::new(Mutex::new(client));

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
        let worker =
            std::thread::spawn(move || handle_connection(server, process_slots, ActiveRuns::new()));
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
