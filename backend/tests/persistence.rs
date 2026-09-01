use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const IO_DEADLINE: Duration = Duration::from_secs(2);
const STARTUP_DEADLINE: Duration = Duration::from_secs(5);
const MAX_SESSION_TITLE_BYTES: usize = 4096;
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    directory: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

impl Fixture {
    fn new() -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("backend should be inside repository")
            .join("target")
            .join(format!(
                "cp-{}-{}",
                std::process::id(),
                FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&directory).expect("fixture directory should be created");
        Self { directory }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }
}

struct BackendProcess {
    child: Child,
    socket_path: PathBuf,
}

impl Drop for BackendProcess {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_file(&self.socket_path);
    }
}

impl BackendProcess {
    fn start(socket_path: &Path, store_path: Option<&Path>, home: Option<&Path>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_capture-delegate-backend"));
        command.args([
            "--socket",
            socket_path.to_str().expect("socket path is UTF-8"),
        ]);
        if let Some(store_path) = store_path {
            command.args(["--store", store_path.to_str().expect("store path is UTF-8")]);
        }
        if let Some(home) = home {
            command.env("HOME", home);
        }
        let mut child = command.spawn().expect("backend should start");

        let deadline = Instant::now() + STARTUP_DEADLINE;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(None) => {}
                Ok(Some(status)) => panic!("backend exited before binding the socket: {status}"),
                Err(error) => panic!("backend status should be readable: {error}"),
            }
            if UnixStream::connect(socket_path).is_ok() {
                return Self {
                    child,
                    socket_path: socket_path.to_path_buf(),
                };
            }
            thread::sleep(Duration::from_millis(10));
        }

        let _ = child.kill();
        let _ = child.wait();
        panic!("backend did not accept socket connections");
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn exchange(socket_path: &Path, request: serde_json::Value) -> serde_json::Value {
    let mut stream = UnixStream::connect(socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(IO_DEADLINE))
        .expect("read deadline should be configured");
    stream
        .set_write_timeout(Some(IO_DEADLINE))
        .expect("write deadline should be configured");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("response should arrive before the read deadline");
    serde_json::from_str(&response).expect("response should be JSON")
}

fn create_session(socket_path: &Path, title: &str) -> serde_json::Value {
    let response = exchange(
        socket_path,
        serde_json::json!({"version": 1, "type": "create_session", "title": title}),
    );
    assert_eq!(response["version"], 1);
    assert_eq!(response["type"], "create_session_response");
    response["session"].clone()
}

fn list_sessions(socket_path: &Path) -> Vec<serde_json::Value> {
    let response = exchange(
        socket_path,
        serde_json::json!({"version": 1, "type": "list_sessions"}),
    );
    assert_eq!(response["version"], 1);
    assert_eq!(response["type"], "list_sessions_response");
    response["sessions"]
        .as_array()
        .expect("sessions should be an array")
        .clone()
}

const SESSION_KINDS: [&str; 6] = [
    "meeting",
    "conversation",
    "presentation",
    "pair_work",
    "personal_note",
    "imported_audio",
];

fn create_session_with_kind(socket_path: &Path, title: &str, kind: &str) -> serde_json::Value {
    let response = exchange(
        socket_path,
        serde_json::json!({"version": 1, "type": "create_session", "title": title, "kind": kind}),
    );
    assert_eq!(response["version"], 1);
    assert_eq!(
        response["type"], "create_session_response",
        "kind {kind:?} should be accepted, got {response}"
    );
    response["session"].clone()
}

fn add_source(socket_path: &Path, mut request: serde_json::Value) -> serde_json::Value {
    request["version"] = serde_json::json!(1);
    request["type"] = serde_json::json!("add_source");
    let response = exchange(socket_path, request);
    assert_eq!(response["version"], 1);
    assert_eq!(
        response["type"], "add_source_response",
        "source should be accepted, got {response}"
    );
    response["source"].clone()
}

fn list_sources(socket_path: &Path, session_id: &str) -> serde_json::Value {
    let response = exchange(
        socket_path,
        serde_json::json!({"version": 1, "type": "list_sources", "session_id": session_id}),
    );
    assert_eq!(response["version"], 1);
    assert_eq!(response["type"], "list_sources_response");
    response
}

/// Streams a whole process run and returns its terminal run_exit frame.
fn run_process(socket_path: &Path, mut request: serde_json::Value) -> serde_json::Value {
    request["version"] = serde_json::json!(1);
    request["type"] = serde_json::json!("start_process");
    let mut stream = UnixStream::connect(socket_path).expect("run client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read deadline should be configured");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    for line in BufReader::new(stream).lines() {
        let frame: serde_json::Value =
            serde_json::from_str(&line.expect("frame should read")).expect("frame should be JSON");
        if frame["type"] == "run_exit" || frame["type"] == "error" {
            return frame;
        }
    }
    panic!("run should terminate with run_exit or an error frame");
}

fn list_runs(socket_path: &Path) -> serde_json::Value {
    let response = exchange(
        socket_path,
        serde_json::json!({"version": 1, "type": "list_runs"}),
    );
    assert_eq!(response["version"], 1);
    assert_eq!(response["type"], "list_runs_response");
    response
}

fn runs_named<'a>(page: &'a serde_json::Value, run_id: &str) -> Vec<&'a serde_json::Value> {
    page["runs"]
        .as_array()
        .expect("runs should be an array")
        .iter()
        .filter(|run| run["run_id"] == run_id)
        .collect()
}

fn mode_of(path: &Path) -> u32 {
    fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("{} metadata should be readable: {error}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn created_sessions_are_listed_newest_first() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("backend.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let first = create_session(&backend.socket_path, "First session");
    let second = create_session(&backend.socket_path, "Second session");

    assert_eq!(first["title"], "First session");
    assert!(
        first["id"]
            .as_str()
            .is_some_and(|id| !id.trim().is_empty() && id != second["id"]),
        "session ids should be unique and non-empty"
    );
    assert_eq!(first["created_at_ms"], first["updated_at_ms"]);
    assert!(
        first["created_at_ms"].as_i64().unwrap_or_default() > 0,
        "created_at_ms should be a positive epoch timestamp"
    );

    let sessions = list_sessions(&backend.socket_path);
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0], second);
    assert_eq!(sessions[1], first);
}

#[test]
fn sessions_survive_a_backend_restart() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("backend.sock");
    let store_path = fixture.path("store.sqlite");

    let session = {
        let mut backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let session = create_session(&backend.socket_path, "Durable session");
        backend.stop();
        session
    };

    let restarted = BackendProcess::start(&socket_path, Some(&store_path), None);
    let sessions = list_sessions(&restarted.socket_path);
    assert_eq!(sessions, vec![session]);
}

#[test]
fn invalid_session_titles_are_rejected_with_an_error_frame() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("backend.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let oversized = "t".repeat(MAX_SESSION_TITLE_BYTES + 1);
    for title in ["", "   \t\n ", oversized.as_str()] {
        let response = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "create_session", "title": title}),
        );
        assert_eq!(
            response,
            serde_json::json!({
                "version": 1,
                "type": "error",
                "code": "invalid_create_session",
            }),
            "title {title:?} should be rejected"
        );
    }

    let missing_title = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "create_session"}),
    );
    assert_eq!(missing_title["code"], "invalid_create_session");

    assert!(list_sessions(&backend.socket_path).is_empty());
}

#[test]
fn store_and_its_parent_directory_are_private() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("backend.sock");
    let store_directory = fixture.path("state");
    let store_path = store_directory.join("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
    create_session(&backend.socket_path, "Permission session");

    assert_eq!(mode_of(&store_path), 0o600, "store file should be private");
    assert_eq!(
        mode_of(&store_directory),
        0o700,
        "store parent directory should be private"
    );
}

#[test]
fn missing_store_argument_uses_the_default_application_support_path() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("backend.sock");
    let home = fixture.path("home");
    fs::create_dir_all(&home).expect("home directory should be created");
    let backend = BackendProcess::start(&socket_path, None, Some(&home));
    create_session(&backend.socket_path, "Default path session");

    let default_store_path = home
        .join("Library")
        .join("Application Support")
        .join("CaptureDelegate")
        .join("store.sqlite");
    assert!(
        default_store_path.is_file(),
        "default store path {} should exist",
        default_store_path.display()
    );
}

fn raw_exchange(socket_path: &Path, request: serde_json::Value) -> String {
    let mut stream = UnixStream::connect(socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(IO_DEADLINE))
        .expect("read deadline should be configured");
    stream
        .set_write_timeout(Some(IO_DEADLINE))
        .expect("write deadline should be configured");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("response should arrive before the read deadline");
    response
}

#[test]
fn list_sessions_responses_are_bounded_with_truncation_reported() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("bounded.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    for index in 1..=52 {
        create_session(&backend.socket_path, &format!("short-{index:02}"));
    }
    let frame = raw_exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_sessions"}),
    );
    assert!(frame.len() <= 8192, "frame should stay bounded");
    let response: serde_json::Value =
        serde_json::from_str(&frame).expect("response should be JSON");
    assert_eq!(response["type"], "list_sessions_response");
    assert_eq!(
        response["sessions"]
            .as_array()
            .expect("sessions should be an array")
            .len(),
        50,
        "count cap should bound the page"
    );
    assert_eq!(response["truncated"], true);

    for _ in 0..4 {
        create_session(&backend.socket_path, &"x".repeat(4000));
    }
    let frame = raw_exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_sessions"}),
    );
    assert!(
        frame.len() <= 8192,
        "oversized titles must not produce an oversized frame"
    );
    let response: serde_json::Value =
        serde_json::from_str(&frame).expect("response should be JSON");
    assert_eq!(response["type"], "list_sessions_response");
    assert_eq!(response["truncated"], true);
    assert!(
        !response["sessions"]
            .as_array()
            .expect("sessions should be an array")
            .is_empty(),
        "the byte budget should still deliver the newest sessions that fit"
    );
}

#[test]
fn session_kinds_are_persisted_and_survive_a_restart() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("kinds.sock");
    let store_path = fixture.path("store.sqlite");

    let expected = {
        let mut backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let mut created = Vec::new();
        for kind in SESSION_KINDS {
            let session =
                create_session_with_kind(&backend.socket_path, &format!("kind {kind}"), kind);
            assert_eq!(
                session["kind"], kind,
                "created session should echo its kind"
            );
            created.push(session);
        }
        // The spec does not require categorization: an uncategorized session has
        // no kind at all in the protocol.
        let uncategorized = create_session(&backend.socket_path, "No kind chosen");
        assert!(
            uncategorized.get("kind").is_none(),
            "an uncategorized session must omit the kind field, got {uncategorized}"
        );
        created.push(uncategorized);
        created.reverse();
        backend.stop();
        created
    };

    let restarted = BackendProcess::start(&socket_path, Some(&store_path), None);
    assert_eq!(list_sessions(&restarted.socket_path), expected);
}

#[test]
fn unknown_session_kinds_are_rejected_with_an_error_frame() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("badkind.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    for kind in ["keynote", "", "MEETING", "auto"] {
        let response = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "create_session", "title": "t", "kind": kind}),
        );
        assert_eq!(
            response,
            serde_json::json!({
                "version": 1,
                "type": "error",
                "code": "invalid_create_session",
            }),
            "kind {kind:?} should be rejected"
        );
    }

    // An explicit null is not how an uncategorized session is stated; only
    // omitting the field is.
    let null_kind = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "create_session", "title": "t", "kind": null}),
    );
    assert_eq!(
        null_kind["code"], "invalid_create_session",
        "an explicit null kind must be rejected, got {null_kind}"
    );

    assert!(
        list_sessions(&backend.socket_path).is_empty(),
        "rejected kinds must not persist sessions"
    );
}

#[test]
fn source_references_persist_chronologically_and_survive_a_restart() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("sources.sock");
    let store_path = fixture.path("store.sqlite");

    let (session_id, other_id, expected) = {
        let mut backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let session = create_session(&backend.socket_path, "Sprint planning");
        let session_id = session["id"].as_str().expect("session id").to_owned();
        let other = create_session(&backend.socket_path, "Unrelated session");
        let other_id = other["id"].as_str().expect("session id").to_owned();

        // Inserted out of order to pin chronological listing.
        let late = add_source(
            &backend.socket_path,
            serde_json::json!({
                "session_id": session_id,
                "start_ms": 872_000,
                "end_ms": 884_000,
                "speaker": "Sarah",
                "text": "Krish, can you check PR 482 and see whether token refresh breaks?",
            }),
        );
        assert_eq!(late["session_id"], session_id.as_str());
        assert_eq!(late["start_ms"], 872_000);
        assert_eq!(late["end_ms"], 884_000);
        assert_eq!(late["speaker"], "Sarah");
        assert!(
            late["id"].as_str().is_some_and(|id| !id.trim().is_empty()),
            "source ids should be non-empty"
        );
        let early = add_source(
            &backend.socket_path,
            serde_json::json!({
                "session_id": session_id,
                "start_ms": 1_000,
                "end_ms": 1_000,
                "text": "Zero-length span without a speaker",
            }),
        );
        assert!(
            early.get("speaker").is_none(),
            "a source without a speaker must omit the field, got {early}"
        );
        add_source(
            &backend.socket_path,
            serde_json::json!({
                "session_id": other_id,
                "start_ms": 5,
                "end_ms": 6,
                "text": "Belongs to the other session",
            }),
        );
        backend.stop();
        (session_id, other_id, vec![early, late])
    };

    let restarted = BackendProcess::start(&socket_path, Some(&store_path), None);
    let page = list_sources(&restarted.socket_path, &session_id);
    assert_eq!(page["truncated"], false);
    assert_eq!(
        page["sources"].as_array().expect("sources array"),
        &expected,
        "sources should list chronologically for their own session only"
    );
    let other_page = list_sources(&restarted.socket_path, &other_id);
    assert_eq!(
        other_page["sources"]
            .as_array()
            .expect("sources array")
            .len(),
        1
    );
}

#[test]
fn invalid_source_references_are_rejected_before_persisting() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("badsource.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
    let session = create_session(&backend.socket_path, "Validation session");
    let session_id = session["id"].as_str().expect("session id");

    let unknown = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "add_source", "session_id": "ses_missing",
            "start_ms": 0, "end_ms": 1, "text": "orphan"}),
    );
    assert_eq!(
        unknown,
        serde_json::json!({"version": 1, "type": "error", "code": "unknown_session"}),
        "a source for a nonexistent session must be rejected"
    );

    let invalid_bodies = [
        // end before start
        serde_json::json!({"session_id": session_id, "start_ms": 10, "end_ms": 9, "text": "t"}),
        // negative start
        serde_json::json!({"session_id": session_id, "start_ms": -1, "end_ms": 9, "text": "t"}),
        // fractional milliseconds
        serde_json::json!({"session_id": session_id, "start_ms": 1.5, "end_ms": 9, "text": "t"}),
        // missing text
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1}),
        // blank text
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1, "text": " \t "}),
        // oversized text
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1,
            "text": "t".repeat(4097)}),
        // blank speaker: omit the field instead
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1, "text": "t",
            "speaker": "  "}),
        // explicit null speaker: omit the field instead
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1, "text": "t",
            "speaker": null}),
        // oversized speaker
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1, "text": "t",
            "speaker": "s".repeat(257)}),
        // missing timing
        serde_json::json!({"session_id": session_id, "text": "t"}),
    ];
    for mut body in invalid_bodies {
        body["version"] = serde_json::json!(1);
        body["type"] = serde_json::json!("add_source");
        let response = exchange(&backend.socket_path, body.clone());
        assert_eq!(
            response,
            serde_json::json!({"version": 1, "type": "error", "code": "invalid_add_source"}),
            "body {body} should be rejected"
        );
    }

    // Escape-heavy text passes the raw byte check but serializes past the frame bound.
    let escape_heavy = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "add_source", "session_id": session_id,
            "start_ms": 0, "end_ms": 1, "text": "\u{0}".repeat(1345)}),
    );
    assert_eq!(escape_heavy["code"], "invalid_add_source");

    let page = list_sources(&backend.socket_path, session_id);
    assert_eq!(
        page["sources"].as_array().expect("sources array").len(),
        0,
        "rejected sources must not persist"
    );

    let unknown_list = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_sources", "session_id": "ses_missing"}),
    );
    assert_eq!(unknown_list["code"], "unknown_session");
    let missing_list = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_sources"}),
    );
    assert_eq!(missing_list["code"], "unknown_session");
}

#[test]
fn list_sources_responses_are_bounded_with_truncation_reported() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("boundedsrc.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
    let session = create_session(&backend.socket_path, "Long transcript");
    let session_id = session["id"].as_str().expect("session id");

    for index in 0..52 {
        add_source(
            &backend.socket_path,
            serde_json::json!({"session_id": session_id, "start_ms": index * 1000,
                "end_ms": index * 1000 + 500, "text": format!("segment {index:02}")}),
        );
    }
    let frame = raw_exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_sources", "session_id": session_id}),
    );
    assert!(frame.len() <= 8192, "frame should stay bounded");
    let response: serde_json::Value =
        serde_json::from_str(&frame).expect("response should be JSON");
    assert_eq!(response["type"], "list_sources_response");
    assert_eq!(
        response["sources"].as_array().expect("sources array").len(),
        50,
        "count cap should bound the page"
    );
    assert_eq!(response["truncated"], true);
    assert_eq!(
        response["sources"][0]["text"], "segment 00",
        "the page should start at the chronological beginning"
    );

    // A separate session whose page is byte-bounded rather than count-bounded:
    // the budget must pop from the end so the chronological start survives.
    let big = create_session(&backend.socket_path, "Big transcript");
    let big_id = big["id"].as_str().expect("session id");
    for index in 0..4 {
        add_source(
            &backend.socket_path,
            serde_json::json!({"session_id": big_id, "start_ms": index,
                "end_ms": index + 1, "text": "x".repeat(4000)}),
        );
    }
    let frame = raw_exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_sources", "session_id": big_id}),
    );
    assert!(
        frame.len() <= 8192,
        "oversized texts must not produce an oversized frame"
    );
    let response: serde_json::Value =
        serde_json::from_str(&frame).expect("response should be JSON");
    assert_eq!(response["truncated"], true);
    let sources = response["sources"].as_array().expect("sources array");
    assert!(
        !sources.is_empty(),
        "the byte budget should still deliver the earliest sources that fit"
    );
    assert_eq!(
        sources[0]["start_ms"], 0,
        "byte truncation must keep the chronological beginning"
    );
}

#[test]
fn process_runs_persist_terminal_records_and_survive_a_restart() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("runs.sock");
    let store_path = fixture.path("store.sqlite");

    let session_id = {
        let mut backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let session = create_session(&backend.socket_path, "Linked session");
        let session_id = session["id"].as_str().expect("session id").to_owned();

        let linked = run_process(
            &backend.socket_path,
            serde_json::json!({"run_id": "linked-run", "executable": "/usr/bin/true",
                "arguments": [], "timeout_milliseconds": 5_000, "session_id": session_id}),
        );
        assert_eq!(linked["type"], "run_exit", "linked run should be admitted");
        assert_eq!(linked["exit_code"], 0);
        let failing = run_process(
            &backend.socket_path,
            serde_json::json!({"run_id": "plain-run", "executable": "/usr/bin/false",
                "arguments": [], "timeout_milliseconds": 5_000}),
        );
        assert_eq!(failing["exit_code"], 1);
        // Client-chosen run ids are reusable; each use must persist its own record.
        let reused = run_process(
            &backend.socket_path,
            serde_json::json!({"run_id": "plain-run", "executable": "/usr/bin/true",
                "arguments": [], "timeout_milliseconds": 5_000}),
        );
        assert_eq!(reused["exit_code"], 0);
        let timed_out = run_process(
            &backend.socket_path,
            serde_json::json!({"run_id": "late-run", "executable": "/bin/sleep",
                "arguments": ["5"], "timeout_milliseconds": 100}),
        );
        assert_eq!(timed_out["error_code"], "timed_out");

        let unknown = run_process(
            &backend.socket_path,
            serde_json::json!({"run_id": "orphan-run", "executable": "/usr/bin/true",
                "arguments": [], "timeout_milliseconds": 5_000, "session_id": "ses_missing"}),
        );
        assert_eq!(
            unknown,
            serde_json::json!({"version": 1, "type": "error", "code": "unknown_session"}),
            "a run for a nonexistent session must be rejected before admission"
        );
        let null_link = run_process(
            &backend.socket_path,
            serde_json::json!({"run_id": "null-run", "executable": "/usr/bin/true",
                "arguments": [], "timeout_milliseconds": 5_000, "session_id": null}),
        );
        assert_eq!(
            null_link["code"], "invalid_start_process",
            "an explicit null session link must be rejected, got {null_link}"
        );
        backend.stop();
        session_id
    };

    let restarted = BackendProcess::start(&socket_path, Some(&store_path), None);
    let page = list_runs(&restarted.socket_path);
    assert_eq!(page["truncated"], false);
    assert_eq!(
        page["runs"].as_array().expect("runs array").len(),
        4,
        "only admitted runs should persist, got {page}"
    );

    let linked = runs_named(&page, "linked-run");
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0]["executable"], "/usr/bin/true");
    assert_eq!(linked[0]["status"], "exited");
    assert_eq!(linked[0]["exit_code"], 0);
    assert_eq!(linked[0]["session_id"], session_id.as_str());
    assert!(
        linked[0].get("error_code").is_none(),
        "a clean exit must omit error_code, got {}",
        linked[0]
    );
    assert!(
        linked[0]["started_at_ms"].as_i64().unwrap_or_default() > 0,
        "started_at_ms should be a positive epoch timestamp"
    );
    assert!(
        linked[0]["ended_at_ms"].as_i64().unwrap_or_default()
            >= linked[0]["started_at_ms"].as_i64().unwrap_or_default(),
        "ended_at_ms should not precede started_at_ms"
    );

    let reused = runs_named(&page, "plain-run");
    assert_eq!(reused.len(), 2, "each reuse of a run id persists a record");
    assert!(
        reused[0]["id"] != reused[1]["id"],
        "run records need distinct stable ids"
    );
    assert!(
        reused
            .iter()
            .all(|run| run.get("session_id").is_none() && run["status"] == "exited"),
        "unlinked runs must omit session_id, got {reused:?}"
    );

    let late = runs_named(&page, "late-run");
    assert_eq!(late.len(), 1);
    assert_eq!(late[0]["status"], "exited");
    assert_eq!(late[0]["error_code"], "timed_out");
    assert!(
        late[0]["exit_code"].is_null() || late[0]["exit_code"].as_i64().is_some(),
        "exit_code reflects whatever the terminal frame reported"
    );

    assert!(
        runs_named(&page, "orphan-run").is_empty() && runs_named(&page, "null-run").is_empty(),
        "rejected runs must not persist"
    );
}

#[test]
fn dangling_running_records_are_marked_interrupted_on_restart() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("interrupted.sock");
    let store_path = fixture.path("store.sqlite");

    {
        let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let mut stream =
            UnixStream::connect(&backend.socket_path).expect("run client should connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read deadline should be configured");
        let request = serde_json::json!({"version": 1, "type": "start_process",
            "run_id": "doomed-run", "executable": "/bin/sleep", "arguments": ["30"],
            "timeout_milliseconds": 60_000});
        stream
            .write_all(format!("{request}\n").as_bytes())
            .expect("request should write");
        // Wait for admission by observing the persisted record itself: the row
        // is inserted pre-spawn, so it appears as "running" within milliseconds.
        let deadline = Instant::now() + STARTUP_DEADLINE;
        loop {
            let page = list_runs(&backend.socket_path);
            if runs_named(&page, "doomed-run")
                .first()
                .is_some_and(|run| run["status"] == "running")
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the live run should become visible as running, got {page}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        // BackendProcess::stop SIGKILLs the backend: no terminal update can run.
    }

    let restarted = BackendProcess::start(&socket_path, Some(&store_path), None);
    let page = list_runs(&restarted.socket_path);
    let doomed = runs_named(&page, "doomed-run");
    assert_eq!(doomed.len(), 1, "the admitted run should have persisted");
    assert_eq!(
        doomed[0]["status"], "interrupted",
        "a run that was live at crash time must be marked interrupted, got {}",
        doomed[0]
    );
    assert!(
        doomed[0]["ended_at_ms"].as_i64().unwrap_or_default() > 0,
        "interruption should stamp ended_at_ms"
    );
    assert!(
        doomed[0].get("error_code").is_none() || doomed[0]["error_code"] == "interrupted",
        "got {}",
        doomed[0]
    );
}

#[test]
fn a_duplicate_launch_cannot_interrupt_a_live_backends_runs() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("dup.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let mut stream = UnixStream::connect(&backend.socket_path).expect("run client should connect");
    let request = serde_json::json!({"version": 1, "type": "start_process",
        "run_id": "long-run", "executable": "/bin/sleep", "arguments": ["30"],
        "timeout_milliseconds": 60_000});
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    let deadline = Instant::now() + STARTUP_DEADLINE;
    loop {
        let page = list_runs(&backend.socket_path);
        if runs_named(&page, "long-run")
            .first()
            .is_some_and(|run| run["status"] == "running")
        {
            break;
        }
        assert!(Instant::now() < deadline, "run should become visible");
        thread::sleep(Duration::from_millis(10));
    }

    // A second backend on the same socket and store must fail to start without
    // touching the live backend's durable records.
    let duplicate = Command::new(env!("CARGO_BIN_EXE_capture-delegate-backend"))
        .args([
            "--socket",
            socket_path.to_str().expect("socket path is UTF-8"),
            "--store",
            store_path.to_str().expect("store path is UTF-8"),
        ])
        .output()
        .expect("duplicate backend should run to completion");
    assert!(
        !duplicate.status.success(),
        "a duplicate launch must refuse to start"
    );

    let page = list_runs(&backend.socket_path);
    let run = runs_named(&page, "long-run");
    assert_eq!(
        run[0]["status"], "running",
        "a duplicate launch must not corrupt the live backend's records, got {}",
        run[0]
    );

    // Reap the sleep worker while the backend is still its parent; the
    // teardown SIGKILL alone would orphan it for the rest of its timeout.
    let reaped = Command::new("/usr/bin/pkill")
        .args(["-P", &backend.child.id().to_string(), "sleep"])
        .status()
        .expect("pkill should run");
    assert!(reaped.success(), "the sleep worker should be reaped");
}

#[test]
fn a_second_backend_on_the_same_store_cannot_start() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("owner.sock");
    let other_socket = fixture.path("other.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let mut stream = UnixStream::connect(&backend.socket_path).expect("run client should connect");
    let request = serde_json::json!({"version": 1, "type": "start_process",
        "run_id": "owned-run", "executable": "/bin/sleep", "arguments": ["30"],
        "timeout_milliseconds": 60_000});
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    let deadline = Instant::now() + STARTUP_DEADLINE;
    loop {
        let page = list_runs(&backend.socket_path);
        if runs_named(&page, "owned-run")
            .first()
            .is_some_and(|run| run["status"] == "running")
        {
            break;
        }
        assert!(Instant::now() < deadline, "run should become visible");
        thread::sleep(Duration::from_millis(10));
    }

    // Binding a different socket must not grant the store: ownership is per
    // store, not per socket path — and per underlying file, not per spelling,
    // so a symlink alias of the store must contend on the same lock.
    assert_takeover_refused(&store_path, &other_socket);
    let alias_path = fixture.path("alias.sqlite");
    std::os::unix::fs::symlink(&store_path, &alias_path).expect("alias should link");
    assert_takeover_refused(&alias_path, &other_socket);

    let page = list_runs(&backend.socket_path);
    assert_eq!(
        runs_named(&page, "owned-run")[0]["status"],
        "running",
        "a refused takeover must not corrupt the owner's records"
    );

    // Reap the sleep worker while the backend is still its parent; the
    // teardown SIGKILL alone would orphan it for the rest of its timeout.
    let reaped = Command::new("/usr/bin/pkill")
        .args(["-P", &backend.child.id().to_string(), "sleep"])
        .status()
        .expect("pkill should run");
    assert!(reaped.success(), "the sleep worker should be reaped");
}

fn assert_takeover_refused(store_path: &Path, socket_path: &Path) {
    let mut second = Command::new(env!("CARGO_BIN_EXE_capture-delegate-backend"))
        .args([
            "--socket",
            socket_path.to_str().expect("socket path is UTF-8"),
            "--store",
            store_path.to_str().expect("store path is UTF-8"),
        ])
        .stderr(Stdio::piped())
        .spawn()
        .expect("second backend should spawn");
    let deadline = Instant::now() + STARTUP_DEADLINE;
    let status = loop {
        match second.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = second.kill();
                let _ = second.wait();
                panic!("a second backend on an owned store must exit, not serve");
            }
            Err(error) => panic!("backend status should be readable: {error}"),
        }
    };
    assert!(!status.success(), "store takeover must be refused");
    let mut stderr = String::new();
    second
        .stderr
        .take()
        .expect("stderr should be captured")
        .read_to_string(&mut stderr)
        .expect("stderr should be readable");
    assert!(
        stderr.contains("already owned by another backend"),
        "refusal must name store ownership, got: {stderr}"
    );
    let _ = fs::remove_file(socket_path);
}

#[test]
fn a_failing_interruption_sweep_prevents_startup() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("badsweep.sock");
    let store_path = fixture.path("store.sqlite");
    {
        // A store whose runs relation cannot be updated: the sweep must fail
        // and the backend must refuse to serve rather than show stale rows.
        let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        drop(backend);
    }
    let output = Command::new("sqlite3")
        .arg(&store_path)
        .arg("ALTER TABLE runs RENAME TO runs_real; CREATE VIEW runs AS SELECT * FROM runs_real;")
        .output()
        .expect("sqlite3 should rewrite the store");
    assert!(output.status.success(), "store rewrite should succeed");

    let mut crippled = Command::new(env!("CARGO_BIN_EXE_capture-delegate-backend"))
        .args([
            "--socket",
            socket_path.to_str().expect("socket path is UTF-8"),
            "--store",
            store_path.to_str().expect("store path is UTF-8"),
        ])
        .spawn()
        .expect("backend should spawn");
    let deadline = Instant::now() + STARTUP_DEADLINE;
    let status = loop {
        match crippled.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = crippled.kill();
                let _ = crippled.wait();
                panic!("a backend that cannot complete startup recovery must exit, not serve");
            }
            Err(error) => panic!("backend status should be readable: {error}"),
        }
    };
    assert!(
        !status.success(),
        "a failed startup recovery must exit non-zero"
    );
}

#[test]
fn list_runs_responses_are_bounded_with_truncation_reported() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("boundedruns.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    for index in 0..54 {
        let padding = "x".repeat(90);
        let exit = run_process(
            &backend.socket_path,
            serde_json::json!({"run_id": format!("bounded-{index:02}-{padding}"),
                "executable": "/usr/bin/true", "arguments": [],
                "timeout_milliseconds": 5_000}),
        );
        assert_eq!(exit["type"], "run_exit");
    }
    let frame = raw_exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_runs"}),
    );
    assert!(frame.len() <= 8192, "frame should stay bounded");
    let response: serde_json::Value =
        serde_json::from_str(&frame).expect("response should be JSON");
    assert_eq!(response["type"], "list_runs_response");
    assert_eq!(response["truncated"], true);
    let runs = response["runs"].as_array().expect("runs array");
    assert!(
        !runs.is_empty() && runs.len() < 54,
        "the byte budget should deliver a bounded non-empty page, got {}",
        runs.len()
    );
}

#[test]
fn escape_heavy_titles_cannot_produce_an_oversized_create_response() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("escape.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    // 1,345 NULs pass the 4,096-byte raw-title check and fit the 8 KiB request
    // bound, but JSON-escape sixfold in the response.
    let title = "\u{0}".repeat(1345);
    let frame = raw_exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "create_session", "title": title}),
    );
    assert!(
        frame.len() <= 8192,
        "create responses must stay within the frame bound, got {} bytes",
        frame.len()
    );
    let response: serde_json::Value =
        serde_json::from_str(&frame).expect("response should be JSON");
    assert_eq!(
        response["type"], "error",
        "an unrepresentable session must be rejected, got {response}"
    );
    assert_eq!(response["code"], "invalid_create_session");
    assert!(
        list_sessions(&backend.socket_path).is_empty(),
        "a rejected session must not be persisted"
    );
}

fn add_marker(socket_path: &Path, mut request: serde_json::Value) -> serde_json::Value {
    request["version"] = serde_json::json!(1);
    request["type"] = serde_json::json!("add_marker");
    let response = exchange(socket_path, request);
    assert_eq!(response["version"], 1);
    assert_eq!(
        response["type"], "add_marker_response",
        "marker should be accepted, got {response}"
    );
    response["marker"].clone()
}

fn list_markers(socket_path: &Path, session_id: &str) -> serde_json::Value {
    let response = exchange(
        socket_path,
        serde_json::json!({"version": 1, "type": "list_markers", "session_id": session_id}),
    );
    assert_eq!(response["version"], 1);
    assert_eq!(response["type"], "list_markers_response");
    response
}

const MARKER_KINDS: [&str; 6] = [
    "important",
    "decision",
    "action",
    "question",
    "delegate",
    "research",
];

#[test]
fn user_markers_persist_chronologically_and_survive_a_restart() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("markers.sock");
    let store_path = fixture.path("store.sqlite");

    let (session_id, other_id, expected) = {
        let mut backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let session = create_session(&backend.socket_path, "Sprint planning");
        let session_id = session["id"].as_str().expect("session id").to_owned();
        let other = create_session(&backend.socket_path, "Unrelated session");
        let other_id = other["id"].as_str().expect("session id").to_owned();

        // Inserted out of order to pin chronological listing.
        let late = add_marker(
            &backend.socket_path,
            serde_json::json!({
                "session_id": session_id,
                "at_ms": 872_000,
                "kind": "decision",
                "note": "We ship the migration behind a flag",
            }),
        );
        assert_eq!(late["session_id"], session_id.as_str());
        assert_eq!(late["at_ms"], 872_000);
        assert_eq!(late["kind"], "decision");
        assert_eq!(late["note"], "We ship the migration behind a flag");
        assert!(
            late["id"].as_str().is_some_and(|id| !id.trim().is_empty()),
            "marker ids should be non-empty"
        );
        let early = add_marker(
            &backend.socket_path,
            serde_json::json!({
                "session_id": session_id,
                "at_ms": 1_000,
                "kind": "important",
            }),
        );
        assert!(
            early.get("note").is_none(),
            "a marker without a note must omit the field, got {early}"
        );
        // Every spec 6.3 marker kind is a valid wire value.
        for (index, kind) in MARKER_KINDS.iter().enumerate() {
            let marker = add_marker(
                &backend.socket_path,
                serde_json::json!({
                    "session_id": other_id,
                    "at_ms": index,
                    "kind": kind,
                }),
            );
            assert_eq!(&marker["kind"], kind);
        }
        backend.stop();
        (session_id, other_id, vec![early, late])
    };

    let restarted = BackendProcess::start(&socket_path, Some(&store_path), None);
    let page = list_markers(&restarted.socket_path, &session_id);
    assert_eq!(page["truncated"], false);
    assert_eq!(
        page["markers"].as_array().expect("markers array"),
        &expected,
        "markers should list chronologically for their own session only"
    );
    let other_page = list_markers(&restarted.socket_path, &other_id);
    assert_eq!(
        other_page["markers"]
            .as_array()
            .expect("markers array")
            .len(),
        MARKER_KINDS.len()
    );
}

#[test]
fn invalid_markers_are_rejected_before_persisting() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("badmarker.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
    let session = create_session(&backend.socket_path, "Validation session");
    let session_id = session["id"].as_str().expect("session id");

    let unknown = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "add_marker", "session_id": "ses_missing",
            "at_ms": 0, "kind": "important"}),
    );
    assert_eq!(
        unknown,
        serde_json::json!({"version": 1, "type": "error", "code": "unknown_session"}),
        "a marker for a nonexistent session must be rejected"
    );

    let invalid_bodies = [
        // unknown kind
        serde_json::json!({"session_id": session_id, "at_ms": 0, "kind": "highlight"}),
        // kinds are case-sensitive wire values
        serde_json::json!({"session_id": session_id, "at_ms": 0, "kind": "Important"}),
        // blank kind
        serde_json::json!({"session_id": session_id, "at_ms": 0, "kind": ""}),
        // explicit null kind
        serde_json::json!({"session_id": session_id, "at_ms": 0, "kind": null}),
        // missing kind
        serde_json::json!({"session_id": session_id, "at_ms": 0}),
        // negative timestamp
        serde_json::json!({"session_id": session_id, "at_ms": -1, "kind": "important"}),
        // fractional milliseconds
        serde_json::json!({"session_id": session_id, "at_ms": 1.5, "kind": "important"}),
        // missing timestamp
        serde_json::json!({"session_id": session_id, "kind": "important"}),
        // blank note: omit the field instead
        serde_json::json!({"session_id": session_id, "at_ms": 0, "kind": "important",
            "note": " \t "}),
        // explicit null note: omit the field instead
        serde_json::json!({"session_id": session_id, "at_ms": 0, "kind": "important",
            "note": null}),
        // oversized note
        serde_json::json!({"session_id": session_id, "at_ms": 0, "kind": "important",
            "note": "n".repeat(4097)}),
    ];
    for mut body in invalid_bodies {
        body["version"] = serde_json::json!(1);
        body["type"] = serde_json::json!("add_marker");
        let response = exchange(&backend.socket_path, body.clone());
        assert_eq!(
            response,
            serde_json::json!({"version": 1, "type": "error", "code": "invalid_add_marker"}),
            "body {body} should be rejected"
        );
    }

    // Escape-heavy notes pass the raw byte check but serialize past the frame bound.
    let escape_heavy = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "add_marker", "session_id": session_id,
            "at_ms": 0, "kind": "important", "note": "\u{0}".repeat(1345)}),
    );
    assert_eq!(escape_heavy["code"], "invalid_add_marker");

    let page = list_markers(&backend.socket_path, session_id);
    assert_eq!(
        page["markers"].as_array().expect("markers array").len(),
        0,
        "rejected markers must not persist"
    );

    let unknown_list = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_markers", "session_id": "ses_missing"}),
    );
    assert_eq!(unknown_list["code"], "unknown_session");
    let missing_list = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_markers"}),
    );
    assert_eq!(missing_list["code"], "unknown_session");
}

#[test]
fn list_markers_responses_are_bounded_with_truncation_reported() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("boundedmrk.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
    let session = create_session(&backend.socket_path, "Long meeting");
    let session_id = session["id"].as_str().expect("session id");

    for index in 0..52 {
        add_marker(
            &backend.socket_path,
            serde_json::json!({"session_id": session_id, "at_ms": index * 1000,
                "kind": "important", "note": format!("moment {index:02}")}),
        );
    }
    let frame = raw_exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_markers", "session_id": session_id}),
    );
    assert!(frame.len() <= 8192, "frame should stay bounded");
    let response: serde_json::Value =
        serde_json::from_str(&frame).expect("response should be JSON");
    assert_eq!(response["type"], "list_markers_response");
    assert_eq!(
        response["markers"].as_array().expect("markers array").len(),
        50,
        "count cap should bound the page"
    );
    assert_eq!(response["truncated"], true);
    assert_eq!(
        response["markers"][0]["note"], "moment 00",
        "the page should start at the chronological beginning"
    );

    // A separate session whose page is byte-bounded rather than count-bounded:
    // the budget must pop from the end so the chronological start survives.
    let big = create_session(&backend.socket_path, "Note-heavy meeting");
    let big_id = big["id"].as_str().expect("session id");
    for index in 0..4 {
        add_marker(
            &backend.socket_path,
            serde_json::json!({"session_id": big_id, "at_ms": index,
                "kind": "research", "note": "x".repeat(4000)}),
        );
    }
    let frame = raw_exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "list_markers", "session_id": big_id}),
    );
    assert!(
        frame.len() <= 8192,
        "oversized notes must not produce an oversized frame"
    );
    let response: serde_json::Value =
        serde_json::from_str(&frame).expect("response should be JSON");
    assert_eq!(response["truncated"], true);
    let markers = response["markers"].as_array().expect("markers array");
    assert!(
        !markers.is_empty(),
        "the byte budget should still deliver the earliest markers that fit"
    );
    assert_eq!(
        markers[0]["at_ms"], 0,
        "byte truncation must keep the chronological beginning"
    );
}

// The admission frame check must account for the single-item LIST envelope, which is
// larger than the add/create response envelope: a record accepted against the smaller
// frame could persist yet be permanently unreadable, because the list byte budget
// would pop even a lone record from the page. These sweeps cross the escape-heavy
// admission boundary and pin: accepted implies listable alone.

#[test]
fn every_accepted_marker_is_singly_listable() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("mrkbound.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let (mut accepted, mut rejected) = (0, 0);
    for nulls in 1290..1342 {
        let session = create_session(&backend.socket_path, &format!("Boundary {nulls}"));
        let session_id = session["id"].as_str().expect("session id");
        let response = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "add_marker", "session_id": session_id,
                "at_ms": 0, "kind": "important", "note": "\u{0}".repeat(nulls)}),
        );
        if response["type"] == "error" {
            assert_eq!(response["code"], "invalid_add_marker");
            rejected += 1;
            continue;
        }
        accepted += 1;
        let page = list_markers(&backend.socket_path, session_id);
        assert_eq!(
            page["markers"].as_array().expect("markers array").len(),
            1,
            "an admitted marker must be listable alone, nulls={nulls}"
        );
    }
    assert!(accepted > 0, "the sweep must include admittable sizes");
    assert!(rejected > 0, "the sweep must cross the admission boundary");
}

#[test]
fn every_accepted_source_is_singly_listable() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("srcbound.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let (mut accepted, mut rejected) = (0, 0);
    for nulls in 1290..1342 {
        let session = create_session(&backend.socket_path, &format!("Boundary {nulls}"));
        let session_id = session["id"].as_str().expect("session id");
        let response = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "add_source", "session_id": session_id,
                "start_ms": 0, "end_ms": 1, "text": "\u{0}".repeat(nulls)}),
        );
        if response["type"] == "error" {
            assert_eq!(response["code"], "invalid_add_source");
            rejected += 1;
            continue;
        }
        accepted += 1;
        let page = list_sources(&backend.socket_path, session_id);
        assert_eq!(
            page["sources"].as_array().expect("sources array").len(),
            1,
            "an admitted source must be listable alone, nulls={nulls}"
        );
    }
    assert!(accepted > 0, "the sweep must include admittable sizes");
    assert!(rejected > 0, "the sweep must cross the admission boundary");
}

#[test]
fn every_accepted_session_is_singly_listable() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("sesbound.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let (mut accepted, mut rejected) = (0, 0);
    for nulls in 1290..1342 {
        let response = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "create_session",
                "title": "\u{0}".repeat(nulls)}),
        );
        if response["type"] == "error" {
            assert_eq!(response["code"], "invalid_create_session");
            rejected += 1;
            continue;
        }
        accepted += 1;
        let created_id = response["session"]["id"].as_str().expect("session id");
        // Sessions list newest-first and the byte budget pops from the end, so the
        // just-created session must survive as the head of a non-empty page.
        let page = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "list_sessions"}),
        );
        let sessions = page["sessions"].as_array().expect("sessions array");
        assert!(
            sessions.first().is_some_and(|s| s["id"] == created_id),
            "an admitted session must head its own list page, nulls={nulls}, got {page}"
        );
    }
    assert!(accepted > 0, "the sweep must include admittable sizes");
    assert!(rejected > 0, "the sweep must cross the admission boundary");
}

#[test]
fn every_admitted_run_stays_listable_after_termination() {
    // Run records grow when they terminate (status, exit code, error code, end
    // timestamp), so the admission bound must cover the WORST-CASE terminal
    // single-item list frame — otherwise a boundary-sized run_id persists a
    // record that the list byte budget pops even when it is alone.
    let fixture = Fixture::new();
    let socket_path = fixture.path("runbound.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    // Both terminal shapes are swept: a clean exit (exit_code, no error_code)
    // and a spawn failure (null exit_code, persisted error_code) — a probe
    // that stopped budgeting persisted error codes would fail the second.
    // run_id has no raw length bound, so plain single-byte padding reaches the
    // frame bound with ONE-byte granularity: no listability window, however
    // narrow, can be jumped by the sweep.
    let (mut accepted, mut rejected) = (0, 0);
    for pad in 7900..7990 {
        for (prefix, executable, check) in [
            ("e", "/bin/echo", "clean"),
            (
                "s",
                "/nonexistent/capture-delegate-spawn-probe",
                "spawn_failed",
            ),
        ] {
            let run_id = format!("{prefix}{}", "x".repeat(pad));
            let response = run_process(
                &backend.socket_path,
                serde_json::json!({"run_id": run_id, "executable": executable,
                    "arguments": ["boundary"], "timeout_milliseconds": 60_000}),
            );
            if response["type"] == "error" && response["code"] == "invalid_start_process" {
                rejected += 1;
                continue;
            }
            if check == "clean" {
                assert_eq!(
                    response["exit_code"], 0,
                    "the probe run should exit cleanly"
                );
            } else {
                assert_eq!(
                    response["error_code"], "spawn_failed",
                    "the spawn probe should fail to spawn, got {response}"
                );
            }
            accepted += 1;
            // The just-terminated run is the newest record, so it must head the
            // newest-first page; the byte budget pops from the end, never the head.
            let page = list_runs(&backend.socket_path);
            let head = &page["runs"].as_array().expect("runs array")[0];
            assert_eq!(
                head["run_id"], run_id,
                "an admitted run must stay listable after termination, pad={pad} ({check})"
            );
            assert_eq!(head["status"], "exited");
            if check == "spawn_failed" {
                assert_eq!(
                    head["error_code"], "spawn_failed",
                    "the persisted error code must survive listing, pad={pad}"
                );
            }
        }
    }
    assert!(accepted > 0, "the sweep must include admittable sizes");
    assert!(rejected > 0, "the sweep must cross the admission boundary");
}

fn set_session_note(socket_path: &Path, mut request: serde_json::Value) -> serde_json::Value {
    request["version"] = serde_json::json!(1);
    request["type"] = serde_json::json!("set_session_note");
    let response = exchange(socket_path, request);
    assert_eq!(response["version"], 1);
    assert_eq!(
        response["type"], "set_session_note_response",
        "note should be accepted, got {response}"
    );
    response["session"].clone()
}

#[test]
fn session_notes_set_clear_and_survive_a_restart() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("note.sock");
    let store_path = fixture.path("store.sqlite");

    let session_id = {
        let mut backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let session = create_session_with_kind(&backend.socket_path, "Sprint planning", "meeting");
        let session_id = session["id"].as_str().expect("session id").to_owned();
        let created_at = session["created_at_ms"].as_i64().expect("created stamp");

        let updated = set_session_note(
            &backend.socket_path,
            serde_json::json!({"session_id": session_id, "note": "Follow up with Sarah"}),
        );
        assert_eq!(updated["id"], session_id.as_str());
        assert_eq!(updated["note"], "Follow up with Sarah");
        assert_eq!(
            updated["kind"], "meeting",
            "a note must not disturb the kind"
        );
        assert!(
            updated["updated_at_ms"].as_i64().expect("updated stamp") >= created_at,
            "setting a note must touch updated_at_ms"
        );

        // A note is one editable field: setting again replaces it wholesale.
        let replaced = set_session_note(
            &backend.socket_path,
            serde_json::json!({"session_id": session_id, "note": "Revised after standup"}),
        );
        assert_eq!(replaced["note"], "Revised after standup");
        backend.stop();
        session_id
    };

    let restarted = BackendProcess::start(&socket_path, Some(&store_path), None);
    let sessions = list_sessions(&restarted.socket_path);
    assert_eq!(sessions[0]["id"], session_id.as_str());
    assert_eq!(
        sessions[0]["note"], "Revised after standup",
        "the note must survive a restart"
    );

    // Explicit null is the clear operation for this update message — the one
    // place null carries meaning, because an update needs a way to erase.
    let cleared = set_session_note(
        &restarted.socket_path,
        serde_json::json!({"session_id": session_id, "note": null}),
    );
    assert!(
        cleared.get("note").is_none(),
        "a cleared note must omit the field, got {cleared}"
    );
    let sessions = list_sessions(&restarted.socket_path);
    assert!(
        sessions[0].get("note").is_none(),
        "a cleared note must stay cleared in listings"
    );
}

#[test]
fn invalid_session_notes_are_rejected_without_touching_the_note() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("badnote.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
    let session = create_session(&backend.socket_path, "Validation session");
    let session_id = session["id"].as_str().expect("session id");
    set_session_note(
        &backend.socket_path,
        serde_json::json!({"session_id": session_id, "note": "The original note"}),
    );

    let unknown = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "set_session_note",
            "session_id": "ses_missing", "note": "orphan"}),
    );
    assert_eq!(
        unknown,
        serde_json::json!({"version": 1, "type": "error", "code": "unknown_session"}),
    );
    let missing_session = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "set_session_note", "note": "x"}),
    );
    assert_eq!(missing_session["code"], "unknown_session");

    let invalid_bodies = [
        // the note field must be present (null means clear; omission is an error)
        serde_json::json!({"session_id": session_id}),
        // blank note: clear with null instead
        serde_json::json!({"session_id": session_id, "note": " \t "}),
        // oversized note
        serde_json::json!({"session_id": session_id, "note": "n".repeat(4097)}),
        // escape-heavy note within the raw byte cap but past the frame bound
        serde_json::json!({"session_id": session_id, "note": "\u{0}".repeat(1345)}),
    ];
    for mut body in invalid_bodies {
        body["version"] = serde_json::json!(1);
        body["type"] = serde_json::json!("set_session_note");
        let response = exchange(&backend.socket_path, body.clone());
        assert_eq!(
            response,
            serde_json::json!({"version": 1, "type": "error", "code": "invalid_set_session_note"}),
            "body {body} should be rejected"
        );
    }

    let sessions = list_sessions(&backend.socket_path);
    assert_eq!(
        sessions[0]["note"], "The original note",
        "rejected updates must leave the stored note untouched"
    );
}

#[test]
fn every_accepted_note_keeps_its_session_singly_listable() {
    // Notes join the session record that list_sessions serializes, so the
    // update must probe the single-item list envelope: an accepted note must
    // never make its session unlistable.
    let fixture = Fixture::new();
    let socket_path = fixture.path("notebound.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let (mut accepted, mut rejected) = (0, 0);
    for nulls in 1290..1342 {
        let session = create_session(&backend.socket_path, &format!("Boundary {nulls}"));
        let session_id = session["id"].as_str().expect("session id");
        let response = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "set_session_note",
                "session_id": session_id, "note": "\u{0}".repeat(nulls)}),
        );
        if response["type"] == "error" {
            assert_eq!(response["code"], "invalid_set_session_note");
            rejected += 1;
            continue;
        }
        accepted += 1;
        let sessions = list_sessions(&backend.socket_path);
        assert!(
            sessions.first().is_some_and(|s| s["id"] == *session_id),
            "a session with an accepted note must head its own page, nulls={nulls}"
        );
    }
    assert!(accepted > 0, "the sweep must include admittable sizes");
    assert!(rejected > 0, "the sweep must cross the admission boundary");
}

#[test]
fn notes_on_narrow_timestamp_sessions_stay_singly_listable() {
    // The UPDATE stamps a freshly sampled updated_at_ms that can be WIDER than
    // the stored one (legacy rows are not guaranteed 13-digit stamps), so the
    // admission probe must not trust the fetched row's timestamp width.
    let fixture = Fixture::new();
    let socket_path = fixture.path("narrow.sock");
    let store_path = fixture.path("store.sqlite");
    let session_id = {
        let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let session = create_session(&backend.socket_path, "Narrow stamps");
        session["id"].as_str().expect("session id").to_owned()
    };
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
    let (mut accepted, mut rejected) = (0, 0);
    for nulls in 1290..1342 {
        // Every accepted update restamps a 13-digit updated_at_ms, so the row
        // must be re-narrowed before each attempt or only the first one probes
        // a narrow stamp.
        let narrowed = Command::new("sqlite3")
            .arg(&store_path)
            .arg("PRAGMA busy_timeout = 5000; UPDATE sessions SET updated_at_ms = 7")
            .status()
            .expect("sqlite3 should run");
        assert!(narrowed.success(), "fixture narrowing should succeed");
        let response = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "set_session_note",
                "session_id": session_id, "note": "\u{0}".repeat(nulls)}),
        );
        if response["type"] == "error" {
            assert_eq!(response["code"], "invalid_set_session_note");
            rejected += 1;
            continue;
        }
        accepted += 1;
        let sessions = list_sessions(&backend.socket_path);
        assert!(
            sessions.iter().any(|s| s["id"] == *session_id),
            "an accepted note must never delist its session, nulls={nulls}"
        );
    }
    assert!(accepted > 0, "the sweep must include admittable sizes");
    assert!(rejected > 0, "the sweep must cross the admission boundary");
}

fn add_transcript_segment(socket_path: &Path, mut request: serde_json::Value) -> serde_json::Value {
    request["version"] = serde_json::json!(1);
    request["type"] = serde_json::json!("add_transcript_segment");
    let response = exchange(socket_path, request);
    assert_eq!(response["version"], 1);
    assert_eq!(
        response["type"], "add_transcript_segment_response",
        "segment should be accepted, got {response}"
    );
    response["segment"].clone()
}

fn list_transcript(socket_path: &Path, session_id: &str) -> serde_json::Value {
    let response = exchange(
        socket_path,
        serde_json::json!({"version": 1, "type": "list_transcript", "session_id": session_id}),
    );
    assert_eq!(response["version"], 1);
    assert_eq!(response["type"], "list_transcript_response");
    response
}

#[test]
fn transcript_segments_persist_chronologically_and_survive_a_restart() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("transcript.sock");
    let store_path = fixture.path("store.sqlite");

    let (session_id, other_id, expected) = {
        let mut backend = BackendProcess::start(&socket_path, Some(&store_path), None);
        let session = create_session(&backend.socket_path, "Sprint planning");
        let session_id = session["id"].as_str().expect("session id").to_owned();
        let other = create_session(&backend.socket_path, "Unrelated session");
        let other_id = other["id"].as_str().expect("session id").to_owned();

        // Inserted out of order to pin chronological listing.
        let late = add_transcript_segment(
            &backend.socket_path,
            serde_json::json!({
                "session_id": session_id,
                "start_ms": 872_000,
                "end_ms": 884_000,
                "speaker": "Sarah",
                "text": "Krish, can you check PR 482 and see whether token refresh breaks?",
            }),
        );
        assert_eq!(late["session_id"], session_id.as_str());
        assert_eq!(late["start_ms"], 872_000);
        assert_eq!(late["end_ms"], 884_000);
        assert_eq!(late["speaker"], "Sarah");
        assert!(
            late["id"].as_str().is_some_and(|id| !id.trim().is_empty()),
            "segment ids should be non-empty"
        );
        let early = add_transcript_segment(
            &backend.socket_path,
            serde_json::json!({
                "session_id": session_id,
                "start_ms": 1_000,
                "end_ms": 1_000,
                "text": "Zero-length span without an attributed speaker",
            }),
        );
        assert!(
            early.get("speaker").is_none(),
            "a segment without a speaker must omit the field, got {early}"
        );
        add_transcript_segment(
            &backend.socket_path,
            serde_json::json!({
                "session_id": other_id,
                "start_ms": 5,
                "end_ms": 6,
                "text": "Belongs to the other session",
            }),
        );
        backend.stop();
        (session_id, other_id, vec![early, late])
    };

    let restarted = BackendProcess::start(&socket_path, Some(&store_path), None);
    let page = list_transcript(&restarted.socket_path, &session_id);
    assert_eq!(page["truncated"], false);
    assert_eq!(
        page["segments"].as_array().expect("segments array"),
        &expected,
        "segments should list chronologically for their own session only"
    );
    let other_page = list_transcript(&restarted.socket_path, &other_id);
    assert_eq!(
        other_page["segments"]
            .as_array()
            .expect("segments array")
            .len(),
        1
    );
}

#[test]
fn invalid_transcript_segments_are_rejected_before_persisting() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("badsegs.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);
    let session = create_session(&backend.socket_path, "Strict transcript");
    let session_id = session["id"].as_str().expect("session id");

    let cases = [
        // Missing fields.
        serde_json::json!({"start_ms": 0, "end_ms": 1, "text": "no session"}),
        serde_json::json!({"session_id": session_id, "end_ms": 1, "text": "no start"}),
        serde_json::json!({"session_id": session_id, "start_ms": 0, "text": "no end"}),
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1}),
        // Malformed spans.
        serde_json::json!({"session_id": session_id, "start_ms": 2, "end_ms": 1, "text": "inverted"}),
        serde_json::json!({"session_id": session_id, "start_ms": -1, "end_ms": 1, "text": "negative"}),
        serde_json::json!({"session_id": session_id, "start_ms": 0.5, "end_ms": 1, "text": "float"}),
        // Blank or oversized text, blank or explicit-null speaker.
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1, "text": "   "}),
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1,
            "text": "x".repeat(4097)}),
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1, "text": "ok",
            "speaker": "   "}),
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1, "text": "ok",
            "speaker": null}),
        serde_json::json!({"session_id": session_id, "start_ms": 0, "end_ms": 1, "text": "ok",
            "speaker": "s".repeat(257)}),
    ];
    for mut case in cases {
        case["version"] = serde_json::json!(1);
        case["type"] = serde_json::json!("add_transcript_segment");
        let response = exchange(&backend.socket_path, case.clone());
        assert_eq!(response["type"], "error", "case should be rejected: {case}");
        assert_eq!(response["code"], "invalid_add_transcript_segment");
    }
    let response = exchange(
        &backend.socket_path,
        serde_json::json!({"version": 1, "type": "add_transcript_segment",
            "session_id": "ses_missing", "start_ms": 0, "end_ms": 1, "text": "orphan"}),
    );
    assert_eq!(response["type"], "error");
    assert_eq!(response["code"], "unknown_session");

    let page = list_transcript(&backend.socket_path, session_id);
    assert_eq!(
        page["segments"].as_array().expect("segments array").len(),
        0,
        "no rejected segment may persist"
    );
}

#[test]
fn every_accepted_transcript_segment_is_singly_listable() {
    let fixture = Fixture::new();
    let socket_path = fixture.path("segbound.sock");
    let store_path = fixture.path("store.sqlite");
    let backend = BackendProcess::start(&socket_path, Some(&store_path), None);

    let (mut accepted, mut rejected) = (0, 0);
    for nulls in 1290..1342 {
        let session = create_session(&backend.socket_path, &format!("Boundary {nulls}"));
        let session_id = session["id"].as_str().expect("session id");
        let response = exchange(
            &backend.socket_path,
            serde_json::json!({"version": 1, "type": "add_transcript_segment",
                "session_id": session_id, "start_ms": 0, "end_ms": 1,
                "text": "\u{0}".repeat(nulls)}),
        );
        if response["type"] == "error" {
            assert_eq!(response["code"], "invalid_add_transcript_segment");
            rejected += 1;
            continue;
        }
        accepted += 1;
        let page = list_transcript(&backend.socket_path, session_id);
        assert_eq!(
            page["segments"].as_array().expect("segments array").len(),
            1,
            "an admitted segment must be listable alone, nulls={nulls}"
        );
    }
    assert!(accepted > 0, "the sweep must include admittable sizes");
    assert!(rejected > 0, "the sweep must cross the admission boundary");
}
