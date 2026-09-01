use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
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
