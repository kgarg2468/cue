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

    assert!(
        list_sessions(&backend.socket_path).is_empty(),
        "rejected kinds must not persist sessions"
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
