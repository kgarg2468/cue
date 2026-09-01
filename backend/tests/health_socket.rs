use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const IO_DEADLINE: Duration = Duration::from_millis(750);
const DRIBBLE_INTERVAL: Duration = Duration::from_millis(25);
const DRIBBLE_CLOSE_UPPER_BOUND: Duration = Duration::from_secs(1);
const MAX_REQUEST_BYTES: usize = 8 * 1024;
static TEMP_SOCKET_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct BackendProcess {
    child: Child,
    directory: PathBuf,
    socket_path: PathBuf,
}

impl Drop for BackendProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn temporary_socket_directory() -> PathBuf {
    let target_directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend should be inside repository")
        .join("target");

    loop {
        let directory = target_directory.join(format!(
            "ct-{}-{}",
            std::process::id(),
            TEMP_SOCKET_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&directory) {
            Ok(()) => {
                if let Err(error) =
                    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                {
                    let _ = fs::remove_dir_all(&directory);
                    panic!("temporary directory should be private: {error}");
                }
                return directory;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("temporary directory should be created: {error}"),
        }
    }
}

fn start_backend() -> BackendProcess {
    let directory = temporary_socket_directory();
    let socket_path = directory.join("health.sock");
    let store_path = directory.join("store.sqlite");
    let mut child = match Command::new(env!("CARGO_BIN_EXE_capture-delegate-backend"))
        .args([
            "--socket",
            socket_path.to_str().expect("socket path is UTF-8"),
            "--store",
            store_path.to_str().expect("store path is UTF-8"),
        ])
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_file(&socket_path);
            let _ = fs::remove_dir_all(&directory);
            panic!("backend should start: {error}");
        }
    };

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(_)) => {
                let _ = child.wait();
                let _ = fs::remove_file(&socket_path);
                let _ = fs::remove_dir_all(&directory);
                panic!("backend exited before accepting socket connections");
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&socket_path);
                let _ = fs::remove_dir_all(&directory);
                panic!("backend status should be readable: {error}");
            }
        }

        match UnixStream::connect(&socket_path) {
            Ok(stream) => {
                drop(stream);
                return BackendProcess {
                    child,
                    directory,
                    socket_path,
                };
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&socket_path);
                let _ = fs::remove_dir_all(&directory);
                panic!("backend socket should accept connections: {error}");
            }
        }
        thread::yield_now();
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(&socket_path);
    let _ = fs::remove_dir_all(&directory);
    panic!("backend did not accept socket connections");
}

fn connect(socket_path: &Path) -> UnixStream {
    let stream = UnixStream::connect(socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(IO_DEADLINE))
        .expect("read deadline should be configured");
    stream
        .set_write_timeout(Some(IO_DEADLINE))
        .expect("write deadline should be configured");
    stream
}

fn assert_service_healthy(socket_path: &Path) {
    let mut stream = connect(socket_path);
    stream
        .write_all(b"{\"version\":1,\"type\":\"health\"}\n")
        .expect("request should write");

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("response should arrive before the read deadline");

    assert_eq!(
        response,
        "{\"version\":1,\"type\":\"health_response\",\"status\":\"ok\"}\n"
    );
}

fn assert_protocol_error(socket_path: &Path, request: &[u8], expected: &str) {
    let mut stream = connect(socket_path);
    stream.write_all(request).expect("request should write");

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("protocol error should arrive before the read deadline");

    assert_eq!(response, expected);
}

#[test]
fn health_request_receives_ok_response_before_deadline() {
    let backend = start_backend();
    assert_service_healthy(&backend.socket_path);
}

#[test]
fn incompatible_protocol_version_receives_actionable_error_frame() {
    let backend = start_backend();

    assert_protocol_error(
        &backend.socket_path,
        b"{\"version\":2,\"type\":\"health\"}\n",
        "{\"version\":1,\"type\":\"error\",\"code\":\"incompatible_version\"}\n",
    );
}

#[test]
fn unknown_request_type_receives_actionable_error_frame() {
    let backend = start_backend();

    assert_protocol_error(
        &backend.socket_path,
        b"{\"version\":1,\"type\":\"capture\"}\n",
        "{\"version\":1,\"type\":\"error\",\"code\":\"unknown_request_type\"}\n",
    );
}

#[test]
fn bound_socket_is_private_regardless_of_umask() {
    let backend = start_backend();
    let deadline = Instant::now() + IO_DEADLINE;
    loop {
        let mode = fs::symlink_metadata(&backend.socket_path)
            .expect("bound socket metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        if mode == 0o600 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "socket permissions should become private, got {mode:o}"
        );
        thread::yield_now();
    }
}

#[test]
fn malformed_client_does_not_terminate_service() {
    let backend = start_backend();
    let mut malformed = connect(&backend.socket_path);
    malformed
        .write_all(b"not-json\n")
        .expect("malformed request should write");
    drop(malformed);

    assert_service_healthy(&backend.socket_path);
}

#[test]
fn incomplete_client_does_not_terminate_service() {
    let backend = start_backend();
    let mut incomplete = connect(&backend.socket_path);
    incomplete
        .write_all(b"{\"version\":1")
        .expect("incomplete request should write");
    incomplete
        .shutdown(std::net::Shutdown::Write)
        .expect("write side should close");
    drop(incomplete);

    assert_service_healthy(&backend.socket_path);
}

#[test]
fn disconnected_client_does_not_terminate_service() {
    let backend = start_backend();
    drop(connect(&backend.socket_path));

    assert_service_healthy(&backend.socket_path);
}

#[test]
fn response_write_failure_does_not_terminate_service() {
    let backend = start_backend();
    let mut disconnected = connect(&backend.socket_path);
    disconnected
        .shutdown(std::net::Shutdown::Read)
        .expect("read side should close");
    disconnected
        .write_all(b"{\"version\":1,\"type\":\"health\"}\n")
        .expect("request should write before response is rejected");

    assert_service_healthy(&backend.socket_path);
}

#[test]
fn stalled_client_does_not_monopolize_accept_loop() {
    let backend = start_backend();
    let _stalled = connect(&backend.socket_path);

    assert_service_healthy(&backend.socket_path);
}

#[test]
fn stalled_clients_are_backpressured_by_bounded_worker_pool() {
    let backend = start_backend();
    let started = Instant::now();
    let _stalled_clients: Vec<_> = (0..8).map(|_| connect(&backend.socket_path)).collect();

    assert_service_healthy(&backend.socket_path);

    assert!(
        started.elapsed() >= Duration::from_millis(100),
        "health request should wait for a bounded worker slot"
    );
}

#[test]
fn byte_dribbling_client_is_closed_by_total_frame_deadline_and_service_survives() {
    let backend = start_backend();
    let mut dribbling = connect(&backend.socket_path);
    let mut writer = dribbling
        .try_clone()
        .expect("dribbling connection should be cloneable");
    let keep_dribbling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let writer_active = std::sync::Arc::clone(&keep_dribbling);
    let writer = thread::spawn(move || {
        while writer_active.load(Ordering::Relaxed) {
            if writer.write_all(b"x").is_err() {
                break;
            }
            thread::sleep(DRIBBLE_INTERVAL);
        }
    });

    let started = Instant::now();
    let mut byte = [0_u8; 1];
    let read_result = dribbling.read(&mut byte);
    let elapsed = started.elapsed();
    keep_dribbling.store(false, Ordering::Relaxed);
    writer.join().expect("dribbling writer should finish");

    assert_eq!(
        read_result.expect("dribbling connection should close"),
        0,
        "dribbling connection should close without a response"
    );
    assert!(
        elapsed <= DRIBBLE_CLOSE_UPPER_BOUND,
        "dribbling connection should close within the total frame deadline bound, took {elapsed:?}"
    );
    assert_service_healthy(&backend.socket_path);
}

#[test]
fn oversized_unterminated_client_is_bounded_and_service_survives() {
    let backend = start_backend();
    let mut oversized = connect(&backend.socket_path);
    oversized
        .write_all(&vec![b'x'; MAX_REQUEST_BYTES + 1])
        .expect("oversized request should write");

    let mut byte = [0_u8; 1];
    let result = oversized.read(&mut byte);
    assert_eq!(result.expect("oversized connection should close"), 0);
    assert_service_healthy(&backend.socket_path);
}

#[test]
fn valid_but_unterminated_request_is_rejected_and_service_survives() {
    let backend = start_backend();
    let mut unterminated = connect(&backend.socket_path);
    unterminated
        .write_all(b"{\"version\":1,\"type\":\"health\"}")
        .expect("unterminated request should write");
    unterminated
        .shutdown(std::net::Shutdown::Write)
        .expect("write side should close");

    let mut byte = [0_u8; 1];
    let result = unterminated.read(&mut byte);
    assert_eq!(result.expect("unterminated connection should close"), 0);
    assert_service_healthy(&backend.socket_path);
}
