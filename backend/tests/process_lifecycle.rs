use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

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
        let _ = fs::remove_dir(&self.directory);
    }
}

fn start_backend() -> BackendProcess {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend should be inside repository");
    let directory = repository.join("target").join(format!(
        "ct-{}-{}",
        std::process::id(),
        TEMP_SOCKET_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).expect("test directory should be created");
    let socket_path = repository.join(format!(
        "p-{}-{}.sock",
        std::process::id(),
        TEMP_SOCKET_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_capture-delegate-backend"))
        .args([
            "--socket",
            socket_path.to_str().expect("socket path is UTF-8"),
        ])
        .spawn()
        .expect("backend should start");

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
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
                ) =>
            {
                thread::yield_now()
            }
            Err(error) => panic!("backend socket should accept connections: {error}"),
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    panic!("backend did not accept socket connections");
}

fn run_frames(backend: &BackendProcess, request: serde_json::Value) -> Vec<serde_json::Value> {
    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    BufReader::new(stream)
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect()
}

#[test]
fn run_metadata_frame_precedes_run_exit_with_process_details() {
    let backend = start_backend();
    let frames = run_frames(
        &backend,
        serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "metadata-run",
            "executable": "/bin/sh",
            "arguments": ["-c", "echo done"],
            "timeout_milliseconds": 2_000,
        }),
    );

    let output_index = frames
        .iter()
        .rposition(|frame| frame["type"] == "run_output")
        .expect("run should emit output");
    let metadata_indexes: Vec<_> = frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| (frame["type"] == "run_metadata").then_some(index))
        .collect();
    assert_eq!(metadata_indexes.len(), 1);
    let metadata_index = metadata_indexes[0];
    let exit_index = frames
        .iter()
        .position(|frame| frame["type"] == "run_exit")
        .expect("run should emit exit");
    assert!(output_index < metadata_index);
    assert_eq!(metadata_index + 1, exit_index);

    let metadata = &frames[metadata_index];
    assert!(metadata["pid"].as_u64().is_some_and(|pid| pid > 0));
    assert_eq!(metadata["pgid"], metadata["pid"]);
    assert_eq!(metadata["executable"], "/bin/sh");
    assert_eq!(
        metadata["arguments"],
        serde_json::json!(["-c", "echo done"])
    );
    assert!(
        metadata["working_directory"]
            .as_str()
            .is_some_and(|directory| !directory.is_empty())
    );
    assert!(metadata["started_at"].as_str() <= metadata["finished_at"].as_str());
    assert!(
        metadata["duration_ms"]
            .as_u64()
            .is_some_and(|duration| duration < 2_000)
    );
    assert!(
        metadata["environment_variable_names"]
            .as_array()
            .is_some_and(|names| names.iter().any(|name| name == "PATH"))
    );
    assert_eq!(metadata["redactions"], 0);
}

#[test]
fn oversized_run_metadata_is_truncated_before_run_exit() {
    let backend = start_backend();
    let frames = run_frames(
        &backend,
        serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "oversized-metadata-run",
            "executable": "/bin/echo",
            "arguments": ["x".repeat(7_904)],
            "timeout_milliseconds": 2_000,
        }),
    );

    let metadata_index = frames
        .iter()
        .position(|frame| frame["type"] == "run_metadata")
        .expect("run should emit metadata");
    let exit_index = frames
        .iter()
        .position(|frame| frame["type"] == "run_exit")
        .expect("run should emit exit");
    assert_eq!(metadata_index + 1, exit_index);

    let metadata = &frames[metadata_index];
    assert_eq!(metadata["run_id"], "oversized-metadata-run");
    assert!(metadata["pid"].as_u64().is_some_and(|pid| pid > 0));
    assert_eq!(metadata["executable"], "/bin/echo");
    assert!(metadata["working_directory"].as_str().is_some());
    assert!(metadata["started_at"].as_str().is_some());
    assert!(metadata["finished_at"].as_str().is_some());
    assert!(metadata["duration_ms"].as_u64().is_some());
    assert_eq!(
        metadata["environment_variable_names"],
        serde_json::json!([])
    );
    assert_eq!(metadata["arguments"], serde_json::json!([]));

    assert_eq!(frames[exit_index]["exit_code"], 0);
}

#[test]
fn secrets_are_redacted_from_streamed_output() {
    let backend = start_backend();
    let frames = run_frames(
        &backend,
        serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "redaction-run",
            "executable": "/bin/sh",
            "arguments": [
                "-c",
                "printf 'token=abcd1234efgh\\n'; printf 'AKIAABCDEFGHIJKLMNOP\\n' >&2",
            ],
            "timeout_milliseconds": 2_000,
        }),
    );
    let collect_output = |stream_name: &str| -> String {
        frames
            .iter()
            .filter(|frame| frame["type"] == "run_output" && frame["stream"] == stream_name)
            .filter_map(|frame| frame["output"].as_str())
            .collect()
    };
    let stdout = collect_output("stdout");
    let stderr = collect_output("stderr");

    assert!(stdout.contains("token=[REDACTED]"));
    assert!(!stdout.contains("abcd1234efgh"));
    assert!(stderr.contains("[REDACTED]"));
    assert!(!stderr.contains("AKIAABCDEFGHIJKLMNOP"));
    let metadata = frames
        .iter()
        .find(|frame| frame["type"] == "run_metadata")
        .expect("run should emit metadata");
    assert_eq!(metadata["redactions"], 2);
}

#[test]
fn pty_run_sees_a_controlling_tty() {
    let backend = start_backend();
    let frames = run_frames(
        &backend,
        serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "pty-controlling-tty-run",
            "executable": "/bin/sh",
            "arguments": [
                "-c",
                "test -t 0 && test -t 1 && test -t 2 && echo all-tty || echo no-tty",
            ],
            "timeout_milliseconds": 2_000,
            "pty": true,
        }),
    );
    let stdout: String = frames
        .iter()
        .filter(|frame| frame["type"] == "run_output" && frame["stream"] == "stdout")
        .filter_map(|frame| frame["output"].as_str())
        .collect();

    assert!(stdout.contains("all-tty"));
    assert!(!stdout.contains("no-tty"));
    assert_eq!(frames.last().expect("terminal frame")["exit_code"], 0);
}

#[test]
fn pty_merges_stderr_into_stdout() {
    let backend = start_backend();
    let frames = run_frames(
        &backend,
        serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "pty-merged-output-run",
            "executable": "/bin/sh",
            "arguments": ["-c", "test -t 1 || exit 7; echo out; echo err 1>&2"],
            "timeout_milliseconds": 2_000,
            "pty": true,
        }),
    );
    let stdout: String = frames
        .iter()
        .filter(|frame| frame["type"] == "run_output" && frame["stream"] == "stdout")
        .filter_map(|frame| frame["output"].as_str())
        .collect();

    assert!(stdout.contains("out\r\n"));
    assert!(stdout.contains("err\r\n"));
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["type"] == "run_output" && frame["stream"] == "stderr")
            .count(),
        0
    );
    assert_eq!(frames.last().expect("terminal frame")["exit_code"], 0);
}

#[test]
fn pty_output_is_redacted_and_counted() {
    let backend = start_backend();
    let frames = run_frames(
        &backend,
        serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "pty-redaction-run",
            "executable": "/bin/sh",
            "arguments": ["-c", "test -t 1 || exit 7; echo AKIAIOSFODNN7EXAMPLE"],
            "timeout_milliseconds": 2_000,
            "pty": true,
        }),
    );
    let stdout: String = frames
        .iter()
        .filter(|frame| frame["type"] == "run_output" && frame["stream"] == "stdout")
        .filter_map(|frame| frame["output"].as_str())
        .collect();
    let metadata_index = frames
        .iter()
        .position(|frame| frame["type"] == "run_metadata")
        .expect("run should emit metadata");
    let exit_index = frames
        .iter()
        .position(|frame| frame["type"] == "run_exit")
        .expect("run should emit exit");

    assert!(stdout.contains("[REDACTED]"));
    assert!(!stdout.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(
        frames[metadata_index]["redactions"]
            .as_u64()
            .is_some_and(|count| count >= 1)
    );
    assert!(metadata_index < exit_index);
    assert_eq!(frames[exit_index]["exit_code"], 0);
}

#[test]
fn non_pty_stderr_stays_separate() {
    let backend = start_backend();
    let frames = run_frames(
        &backend,
        serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "explicit-non-pty-run",
            "executable": "/bin/sh",
            "arguments": ["-c", "echo out; echo err 1>&2"],
            "timeout_milliseconds": 2_000,
            "pty": false,
        }),
    );
    let stdout: String = frames
        .iter()
        .filter(|frame| frame["type"] == "run_output" && frame["stream"] == "stdout")
        .filter_map(|frame| frame["output"].as_str())
        .collect();
    let stderr: String = frames
        .iter()
        .filter(|frame| frame["type"] == "run_output" && frame["stream"] == "stderr")
        .filter_map(|frame| frame["output"].as_str())
        .collect();

    assert!(stdout.contains("out\n"));
    assert!(!stdout.contains("err\n"));
    assert!(stderr.contains("err\n"));
    assert_eq!(frames.last().expect("terminal frame")["exit_code"], 0);
}

#[test]
fn spawn_failure_emits_no_metadata_frame() {
    let backend = start_backend();
    let frames = run_frames(
        &backend,
        serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "metadata-spawn-failure",
            "executable": "/definitely/does/not/exist/capture-delegate",
            "arguments": [],
            "timeout_milliseconds": 2_000,
        }),
    );

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "run_exit");
    assert_eq!(frames[0]["error_code"], "spawn_failed");
}

#[test]
fn cat_streams_output_before_one_terminal_exit_frame() {
    let backend = start_backend();
    let existing_file = backend.directory.join("existing.txt");
    fs::write(&existing_file, "present output\n").expect("existing fixture should be written");
    let missing_file = backend.directory.join("missing.txt");
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "rust-run",
        "executable": "/bin/cat",
        "arguments": [existing_file, missing_file],
        "timeout_milliseconds": 2_000,
    });

    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");

    let frame_lines: Vec<String> = BufReader::new(stream)
        .lines()
        .map(|line| line.expect("frame should be readable"))
        .collect();
    assert!(
        frame_lines.iter().all(|frame| frame.len() < 8 * 1024),
        "every response frame must remain bounded"
    );
    let frames: Vec<serde_json::Value> = frame_lines
        .iter()
        .map(|line| serde_json::from_str(line).expect("frame JSON"))
        .collect();
    let terminal_indexes: Vec<_> = frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| (frame["type"] == "run_exit").then_some(index))
        .collect();

    assert_eq!(
        terminal_indexes,
        vec![frames.len() - 1],
        "terminal must be unique and last"
    );
    assert!(frames.iter().any(|frame| {
        frame["type"] == "run_output"
            && frame["run_id"] == "rust-run"
            && frame["stream"] == "stdout"
            && frame["output"]
                .as_str()
                .is_some_and(|output| output.contains("present output"))
    }));
    assert!(frames.iter().any(|frame| {
        frame["type"] == "run_output"
            && frame["run_id"] == "rust-run"
            && frame["stream"] == "stderr"
            && frame["output"]
                .as_str()
                .is_some_and(|output| output.contains("missing.txt"))
    }));
    assert!(
        frames.last().expect("terminal frame")["exit_code"]
            .as_i64()
            .is_some_and(|exit_code| exit_code != 0),
        "cat should report a nonzero exit code for the missing file"
    );
}

#[test]
fn cat_preserves_a_utf8_scalar_split_at_the_output_read_boundary() {
    let backend = start_backend();
    let fixture = backend.directory.join("utf8-split.txt");
    let expected = format!("{}€", "a".repeat(1023));
    fs::write(&fixture, &expected).expect("UTF-8 fixture should be written");
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "utf8-run",
        "executable": "/bin/cat",
        "arguments": [fixture],
        "timeout_milliseconds": 2_000,
    });

    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");

    let frames: Vec<serde_json::Value> = BufReader::new(stream)
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("frame should be readable")).expect("frame JSON")
        })
        .collect();
    let output: String = frames
        .iter()
        .filter(|frame| frame["type"] == "run_output" && frame["stream"] == "stdout")
        .filter_map(|frame| frame["output"].as_str())
        .collect();

    assert_eq!(output, expected);
    assert!(!output.contains('\u{fffd}'));
}

#[test]
fn streaming_write_survives_backpressure_longer_than_the_request_timeout() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "backpressure-run",
        "executable": "/usr/bin/seq",
        "arguments": ["1", "500000"],
        "timeout_milliseconds": 10_000,
    });

    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");

    thread::sleep(Duration::from_millis(600));
    let frame_lines: Vec<String> = BufReader::new(stream)
        .lines()
        .map(|line| line.expect("complete frame should be readable after draining"))
        .collect();
    let frames: Vec<serde_json::Value> = frame_lines
        .iter()
        .map(|line| serde_json::from_str(line).expect("every frame should be complete JSON"))
        .collect();

    assert!(
        frames.iter().any(|frame| {
            frame["type"] == "run_output"
                && frame["run_id"] == "backpressure-run"
                && frame["output"]
                    .as_str()
                    .is_some_and(|output| !output.is_empty())
        }),
        "stream should retain output after backpressure"
    );
    assert!(
        frames
            .last()
            .is_some_and(|frame| frame["type"] == "run_exit"),
        "stream should finish with a complete terminal frame"
    );
}

#[test]
fn nonexistent_executable_returns_one_spawn_failure_terminal_frame() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "missing-run",
        "executable": "/definitely/does/not/exist/capture-delegate",
        "arguments": [],
        "timeout_milliseconds": 2_000,
    });

    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");

    let frames: Vec<serde_json::Value> = BufReader::new(stream)
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("frame should be readable")).expect("frame JSON")
        })
        .collect();

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "run_exit");
    assert_eq!(frames[0]["run_id"], "missing-run");
    assert!(frames[0]["exit_code"].is_null());
    assert_eq!(frames[0]["error_code"], "spawn_failed");
}

#[test]
fn cancel_on_a_separate_connection_preserves_prior_output_and_emits_cancelled_terminal_last() {
    let backend = start_backend();
    assert_eq!(
        cancel_process(&backend.socket_path, "unknown-run")["status"],
        "not_found"
    );
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "cancel-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "printf ready; sleep 2"],
        "timeout_milliseconds": 2_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");
    let mut reader = BufReader::new(stream);
    let mut first_frame = String::new();
    reader
        .read_line(&mut first_frame)
        .expect("prior output should be readable");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&first_frame).expect("frame JSON")["type"],
        "run_output"
    );

    assert_eq!(
        cancel_process(&backend.socket_path, "cancel-run")["status"],
        "accepted"
    );
    let frames: Vec<serde_json::Value> = reader
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["type"], "run_metadata");
    assert_eq!(frames[1]["type"], "run_exit");
    assert!(frames[1]["exit_code"].is_null());
    assert_eq!(frames[1]["error_code"], "cancelled");

    assert_eq!(
        cancel_process(&backend.socket_path, "cancel-run")["status"],
        "not_found"
    );
    let mut health =
        UnixStream::connect(&backend.socket_path).expect("health client should connect");
    health
        .write_all(b"{\"version\":1,\"type\":\"health\"}\n")
        .expect("health request should write");
    let mut health_frame = String::new();
    BufReader::new(health)
        .read_line(&mut health_frame)
        .expect("health response should read");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&health_frame).expect("health JSON")["status"],
        "ok"
    );
}

#[test]
fn cancel_kills_descendant_processes() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "cancel-descendant-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "sleep 30 & echo $!; wait"],
        "timeout_milliseconds": 30_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");
    let mut reader = BufReader::new(stream);
    let grandchild_pid = read_grandchild_pid(&mut reader);
    let _cleanup = DescendantGuard(grandchild_pid);

    assert_eq!(
        cancel_process(&backend.socket_path, "cancel-descendant-run")["status"],
        "accepted"
    );
    let frames: Vec<serde_json::Value> = reader
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    assert!(frames.last().is_some_and(|frame| {
        frame["type"] == "run_exit" && frame["error_code"] == "cancelled"
    }));
    assert!(
        wait_for_process_death(grandchild_pid),
        "grandchild sleep process {grandchild_pid} should be dead after cancellation"
    );
}

#[test]
fn pty_cancel_kills_the_process_group() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "pty-cancel-descendant-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "test -t 1 || exit 7; sleep 30 & echo $!; wait"],
        "timeout_milliseconds": 30_000,
        "pty": true,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");
    let mut reader = BufReader::new(stream);
    let grandchild_pid = read_grandchild_pid(&mut reader);
    let _cleanup = DescendantGuard(grandchild_pid);

    assert_eq!(
        cancel_process(&backend.socket_path, "pty-cancel-descendant-run")["status"],
        "accepted"
    );
    let frames: Vec<serde_json::Value> = reader
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    assert!(frames.last().is_some_and(|frame| {
        frame["type"] == "run_exit" && frame["error_code"] == "cancelled"
    }));
    assert!(
        wait_for_process_death(grandchild_pid),
        "grandchild sleep process {grandchild_pid} should be dead after PTY cancellation"
    );
}

#[test]
fn pause_and_resume_control_the_run_process_group() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "pause-resume-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "sleep 30 & echo $!; wait"],
        "timeout_milliseconds": 30_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");
    let mut reader = BufReader::new(stream);
    let grandchild_pid = read_grandchild_pid(&mut reader);
    let _cleanup = DescendantGuard(grandchild_pid);

    let pause_response = control_process(&backend.socket_path, "pause_process", "pause-resume-run");
    assert_eq!(pause_response["type"], "pause_response");
    assert_eq!(pause_response["status"], "accepted");
    assert!(
        wait_for_process_stopped_state(grandchild_pid, true),
        "grandchild sleep process {grandchild_pid} should stop after pausing the group"
    );

    let resume_response =
        control_process(&backend.socket_path, "resume_process", "pause-resume-run");
    assert_eq!(resume_response["type"], "resume_response");
    assert_eq!(resume_response["status"], "accepted");
    assert!(
        wait_for_process_stopped_state(grandchild_pid, false),
        "grandchild sleep process {grandchild_pid} should continue after resuming the group"
    );

    assert_eq!(
        cancel_process(&backend.socket_path, "pause-resume-run")["status"],
        "accepted"
    );
    let frames: Vec<serde_json::Value> = reader
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    assert!(frames.last().is_some_and(|frame| {
        frame["type"] == "run_exit" && frame["error_code"] == "cancelled"
    }));
}

#[test]
fn pause_unknown_run_and_malformed_pause_return_typed_responses() {
    let backend = start_backend();
    let unknown_response = control_process(&backend.socket_path, "pause_process", "unknown-run");
    assert_eq!(unknown_response["type"], "pause_response");
    assert_eq!(unknown_response["run_id"], "unknown-run");
    assert_eq!(unknown_response["status"], "not_found");

    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("pause client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("pause timeout should configure");
    stream
        .write_all(b"{\"version\":1,\"type\":\"pause_process\"}\n")
        .expect("malformed pause request should write");
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("protocol error should read");
    let response: serde_json::Value =
        serde_json::from_str(&response).expect("protocol error should be JSON");
    assert_eq!(response["type"], "error");
    assert_eq!(response["code"], "invalid_pause_process");
}

#[test]
fn send_input_and_close_stdin_drive_a_cat_run_to_natural_exit() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "stdin-cat-run",
        "executable": "/bin/cat",
        "arguments": [],
        "timeout_milliseconds": 10_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");

    let input_response = wait_for_registered_input_control(
        &backend.socket_path,
        serde_json::json!({
            "version": 1,
            "type": "send_input",
            "run_id": "stdin-cat-run",
            "data": "hello\n",
        }),
    );
    assert_eq!(input_response["type"], "input_response");
    assert_eq!(input_response["run_id"], "stdin-cat-run");
    assert_eq!(input_response["status"], "accepted");

    let mut reader = BufReader::new(stream);
    let mut output = String::new();
    while !output.contains("hello\n") {
        let frame = read_json_frame(&mut reader);
        assert_eq!(frame["type"], "run_output");
        if frame["stream"] == "stdout" {
            output.push_str(frame["output"].as_str().expect("output should be a string"));
        }
    }

    let close_response = control_process(&backend.socket_path, "close_stdin", "stdin-cat-run");
    assert_eq!(close_response["type"], "close_stdin_response");
    assert_eq!(close_response["run_id"], "stdin-cat-run");
    assert_eq!(close_response["status"], "accepted");

    let metadata = read_json_frame(&mut reader);
    assert_eq!(metadata["type"], "run_metadata");
    assert_eq!(metadata["run_id"], "stdin-cat-run");

    let terminal = read_json_frame(&mut reader);
    assert_eq!(terminal["type"], "run_exit");
    assert_eq!(terminal["run_id"], "stdin-cat-run");
    assert_eq!(terminal["exit_code"], 0);
    assert!(terminal.get("error_code").is_none());
}

#[test]
fn pty_input_round_trip_with_echo_disabled() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "pty-input-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "test -t 0 || exit 7; exec cat"],
        "timeout_milliseconds": 10_000,
        "pty": true,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");

    let input_response = wait_for_registered_input_control(
        &backend.socket_path,
        serde_json::json!({
            "version": 1,
            "type": "send_input",
            "run_id": "pty-input-run",
            "data": "hello\n",
        }),
    );
    assert_eq!(input_response["status"], "accepted");

    let mut reader = BufReader::new(stream);
    let mut output = String::new();
    while !output.contains("hello\r\n") {
        let frame = read_json_frame(&mut reader);
        assert_ne!(frame["type"], "run_exit");
        if frame["type"] == "run_output" && frame["stream"] == "stdout" {
            output.push_str(frame["output"].as_str().expect("output should be a string"));
        }
    }
    assert_eq!(output.matches("hello").count(), 1);
    assert_eq!(output, "hello\r\n");

    let close_response = control_process(&backend.socket_path, "close_stdin", "pty-input-run");
    assert_eq!(close_response["status"], "accepted");
    let after_close = input_control(
        &backend.socket_path,
        serde_json::json!({
            "version": 1,
            "type": "send_input",
            "run_id": "pty-input-run",
            "data": "after\n",
        }),
    );
    assert_eq!(after_close["status"], "closed");

    let metadata = read_json_frame(&mut reader);
    assert_eq!(metadata["type"], "run_metadata");
    let terminal = read_json_frame(&mut reader);
    assert_eq!(terminal["type"], "run_exit");
    assert_eq!(terminal["exit_code"], 0);
    assert!(terminal.get("error_code").is_none());
}

#[test]
fn close_stdin_establishes_a_stable_input_boundary() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "stable-stdin-boundary-run",
        "executable": "/bin/cat",
        "arguments": [],
        "timeout_milliseconds": 10_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");

    let before_response = wait_for_registered_input_control(
        &backend.socket_path,
        serde_json::json!({
            "version": 1,
            "type": "send_input",
            "run_id": "stable-stdin-boundary-run",
            "data": "before\n",
        }),
    );
    assert_eq!(before_response["status"], "accepted");
    let pause_response = control_process(
        &backend.socket_path,
        "pause_process",
        "stable-stdin-boundary-run",
    );
    assert_eq!(pause_response["status"], "accepted");
    assert_eq!(
        control_process(
            &backend.socket_path,
            "close_stdin",
            "stable-stdin-boundary-run"
        )["status"],
        "accepted"
    );
    let after_response = input_control(
        &backend.socket_path,
        serde_json::json!({
            "version": 1,
            "type": "send_input",
            "run_id": "stable-stdin-boundary-run",
            "data": "after\n",
        }),
    );
    assert_eq!(after_response["status"], "closed");
    assert_eq!(
        control_process(
            &backend.socket_path,
            "resume_process",
            "stable-stdin-boundary-run"
        )["status"],
        "accepted"
    );

    let frames: Vec<serde_json::Value> = BufReader::new(stream)
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    let stdout: String = frames
        .iter()
        .filter(|frame| frame["type"] == "run_output" && frame["stream"] == "stdout")
        .filter_map(|frame| frame["output"].as_str())
        .collect();
    assert!(stdout.contains("before\n"));
    assert!(!stdout.contains("after\n"));
    assert!(
        frames
            .last()
            .is_some_and(|frame| { frame["type"] == "run_exit" && frame["exit_code"] == 0 })
    );
}

#[test]
fn cancel_succeeds_while_a_paused_runs_stdin_pipe_is_full() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "paused-full-stdin-run",
        "executable": "/bin/cat",
        "arguments": [],
        "timeout_milliseconds": 30_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");

    let registration_response = wait_for_registered_input_control(
        &backend.socket_path,
        serde_json::json!({
            "version": 1,
            "type": "send_input",
            "run_id": "paused-full-stdin-run",
            "data": "",
        }),
    );
    assert_eq!(registration_response["status"], "accepted");
    assert_eq!(
        control_process(
            &backend.socket_path,
            "pause_process",
            "paused-full-stdin-run"
        )["status"],
        "accepted"
    );
    let chunk = "x".repeat(7 * 1024);
    let chunk_count = (512_usize * 1024).div_ceil(chunk.len());
    for _ in 0..chunk_count {
        let started = Instant::now();
        let response = wait_for_registered_input_control(
            &backend.socket_path,
            serde_json::json!({
                "version": 1,
                "type": "send_input",
                "run_id": "paused-full-stdin-run",
                "data": chunk,
            }),
        );
        assert_eq!(response["status"], "accepted");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "queued input response must arrive within the client read timeout"
        );
    }

    assert_eq!(
        cancel_process(&backend.socket_path, "paused-full-stdin-run")["status"],
        "accepted"
    );
    let frames: Vec<serde_json::Value> = BufReader::new(stream)
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    assert!(frames.last().is_some_and(|frame| {
        frame["type"] == "run_exit" && frame["error_code"] == "cancelled"
    }));
}

#[test]
fn input_controls_on_unknown_runs_return_not_found() {
    let backend = start_backend();
    let input_response = input_control(
        &backend.socket_path,
        serde_json::json!({
            "version": 1,
            "type": "send_input",
            "run_id": "never-started-run",
            "data": "hello",
        }),
    );
    assert_eq!(input_response["type"], "input_response");
    assert_eq!(input_response["run_id"], "never-started-run");
    assert_eq!(input_response["status"], "not_found");

    let close_response = control_process(&backend.socket_path, "close_stdin", "never-started-run");
    assert_eq!(close_response["type"], "close_stdin_response");
    assert_eq!(close_response["run_id"], "never-started-run");
    assert_eq!(close_response["status"], "not_found");
}

#[test]
fn malformed_input_controls_return_typed_errors() {
    let backend = start_backend();
    for (request, expected_code) in [
        (
            serde_json::json!({"version": 1, "type": "send_input", "data": "hello"}),
            "invalid_send_input",
        ),
        (
            serde_json::json!({
                "version": 1,
                "type": "send_input",
                "run_id": "missing-data-run",
            }),
            "invalid_send_input",
        ),
        (
            serde_json::json!({
                "version": 1,
                "type": "send_input",
                "run_id": "",
                "data": "hello",
            }),
            "invalid_send_input",
        ),
        (
            serde_json::json!({"version": 1, "type": "close_stdin"}),
            "invalid_close_stdin",
        ),
        (
            serde_json::json!({"version": 1, "type": "close_stdin", "run_id": ""}),
            "invalid_close_stdin",
        ),
    ] {
        let response = input_control(&backend.socket_path, request);
        assert_eq!(response["type"], "error");
        assert_eq!(response["code"], expected_code);
    }
}

#[test]
fn paused_run_still_times_out_and_kills_its_process_group() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "paused-timeout-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "sleep 30 & echo $!; wait"],
        "timeout_milliseconds": 5_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");
    let mut reader = BufReader::new(stream);
    let grandchild_pid = read_grandchild_pid(&mut reader);
    let _cleanup = DescendantGuard(grandchild_pid);

    assert_eq!(
        control_process(&backend.socket_path, "pause_process", "paused-timeout-run")["status"],
        "accepted"
    );
    let frames: Vec<serde_json::Value> = reader
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    assert!(frames.last().is_some_and(|frame| {
        frame["type"] == "run_exit" && frame["error_code"] == "timed_out"
    }));
    assert!(
        wait_for_process_death(grandchild_pid),
        "grandchild sleep process {grandchild_pid} should be dead after paused timeout"
    );
}

#[test]
fn sigterm_kills_active_run_groups_and_removes_socket() {
    assert_signal_shutdown(libc::SIGTERM, true);
}

#[test]
fn sigint_kills_active_run_groups() {
    assert_signal_shutdown(libc::SIGINT, false);
}

#[test]
fn timeout_kills_descendant_processes() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "timeout-descendant-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "sleep 30 & echo $!; wait"],
        "timeout_milliseconds": 2_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");
    let mut reader = BufReader::new(stream);
    let grandchild_pid = read_grandchild_pid(&mut reader);
    let _cleanup = DescendantGuard(grandchild_pid);
    let frames: Vec<serde_json::Value> = reader
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    assert!(frames.last().is_some_and(|frame| {
        frame["type"] == "run_exit" && frame["error_code"] == "timed_out"
    }));
    assert!(
        wait_for_process_death(grandchild_pid),
        "grandchild sleep process {grandchild_pid} should be dead after timeout"
    );
}

#[test]
fn natural_leader_exit_kills_descendant_processes() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "natural-exit-descendant-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "sleep 30 & echo $!; exit 0"],
        "timeout_milliseconds": 30_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");
    let mut reader = BufReader::new(stream);
    let grandchild_pid = read_grandchild_pid(&mut reader);
    let _cleanup = DescendantGuard(grandchild_pid);
    let frames: Vec<serde_json::Value> = reader
        .lines()
        .map(|line| serde_json::from_str(&line.expect("frame should read")).expect("frame JSON"))
        .collect();
    assert!(frames.last().is_some_and(|frame| {
        frame["type"] == "run_exit"
            && frame["exit_code"] == 0
            && frame["error_code"] == serde_json::Value::Null
    }));
    assert!(
        wait_for_process_death(grandchild_pid),
        "grandchild sleep process {grandchild_pid} should be dead after the leader exits naturally"
    );
}

#[test]
fn duplicate_active_run_id_is_rejected_and_reusable_after_full_teardown() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "reused-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "printf ready; sleep 2"],
        "timeout_milliseconds": 2_000,
    });
    let mut first = UnixStream::connect(&backend.socket_path).expect("first client should connect");
    first
        .write_all(format!("{request}\n").as_bytes())
        .expect("first request should write");
    let mut first_reader = BufReader::new(first);
    let mut output = String::new();
    first_reader
        .read_line(&mut output)
        .expect("first output should read");

    let mut duplicate =
        UnixStream::connect(&backend.socket_path).expect("duplicate client should connect");
    duplicate
        .write_all(format!("{request}\n").as_bytes())
        .expect("duplicate request should write");
    let mut duplicate_frame = String::new();
    BufReader::new(duplicate)
        .read_line(&mut duplicate_frame)
        .expect("duplicate error should read");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&duplicate_frame).expect("duplicate JSON")["code"],
        "duplicate_run_id"
    );

    assert_eq!(
        cancel_process(&backend.socket_path, "reused-run")["status"],
        "accepted"
    );
    loop {
        let mut frame = String::new();
        first_reader
            .read_line(&mut frame)
            .expect("first run frame should read");
        let frame: serde_json::Value = serde_json::from_str(&frame).expect("first run frame JSON");
        if frame["type"] == "run_exit" {
            assert_eq!(frame["error_code"], "cancelled");
            break;
        }
    }

    let mut reused =
        UnixStream::connect(&backend.socket_path).expect("reused client should connect");
    reused
        .write_all(format!("{request}\n").as_bytes())
        .expect("reused request should write");
    let mut reused_reader = BufReader::new(reused);
    let mut reused_output = String::new();
    reused_reader
        .read_line(&mut reused_output)
        .expect("reused output should read before cancelling");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reused_output).expect("reused output JSON")["type"],
        "run_output"
    );
    assert_eq!(
        cancel_process(&backend.socket_path, "reused-run")["status"],
        "accepted"
    );
    let terminal: Vec<serde_json::Value> = reused_reader
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("reused response should read")).expect("reused JSON")
        })
        .collect();
    assert_eq!(
        terminal.last().expect("terminal")["error_code"],
        "cancelled"
    );
}

#[test]
fn cancellation_and_timeout_races_emit_one_terminal_without_hanging() {
    let backend = start_backend();
    for index in 0..3 {
        let run_id = format!("cancel-timeout-race-{index}");
        let request = serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": run_id,
            "executable": "/bin/sh",
            "arguments": ["-c", "printf ready; sleep 2"],
            "timeout_milliseconds": 20,
        });
        let mut stream =
            UnixStream::connect(&backend.socket_path).expect("start client should connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout should configure");
        stream
            .write_all(format!("{request}\n").as_bytes())
            .expect("start request should write");
        let mut reader = BufReader::new(stream);
        let mut first = String::new();
        reader
            .read_line(&mut first)
            .expect("first frame should read");
        let first: serde_json::Value = serde_json::from_str(&first).expect("first frame JSON");
        let _ = cancel_process(&backend.socket_path, &run_id);
        let frames: Vec<serde_json::Value> = std::iter::once(first)
            .chain(reader.lines().map(|line| {
                serde_json::from_str(&line.expect("frame should read")).expect("frame JSON")
            }))
            .collect();
        let terminals: Vec<_> = frames
            .iter()
            .filter(|frame| frame["type"] == "run_exit")
            .collect();
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0], frames.last().expect("frames are nonempty"));
        assert!(matches!(
            terminals[0]["error_code"].as_str(),
            Some("cancelled" | "timed_out")
        ));
    }
}

#[test]
fn disconnecting_from_yes_stops_the_child_and_leaves_the_backend_responsive() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "disconnect-run",
        "executable": "/usr/bin/yes",
        "arguments": ["disconnect-regression"],
        "timeout_milliseconds": 2_000,
    });
    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    let mut first_frame = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut first_frame)
        .expect("first output frame should be readable");
    let first_frame: serde_json::Value =
        serde_json::from_str(&first_frame).expect("first output frame should be JSON");
    assert_eq!(first_frame["type"], "run_output");

    let child_deadline = Instant::now() + Duration::from_secs(2);
    let child_pid = loop {
        if let Some(pid) = child_pids(backend.child.id()).into_iter().next() {
            break pid;
        }
        assert!(
            Instant::now() < child_deadline,
            "yes child should become observable"
        );
        thread::sleep(Duration::from_millis(10));
    };
    drop(reader);

    let exit_deadline = Instant::now() + Duration::from_secs(2);
    while process_exists(child_pid) && Instant::now() < exit_deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let child_survived = process_exists(child_pid);
    if child_survived {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &child_pid.to_string()])
            .status();
    }

    let health_started = Instant::now();
    let mut health =
        UnixStream::connect(&backend.socket_path).expect("health client should connect");
    health
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("health read timeout should configure");
    health
        .write_all(b"{\"version\":1,\"type\":\"health\"}\n")
        .expect("health request should write");
    let mut health_frame = String::new();
    BufReader::new(health)
        .read_line(&mut health_frame)
        .expect("health response should be readable");
    let health_frame: serde_json::Value =
        serde_json::from_str(&health_frame).expect("health response should be JSON");

    assert_eq!(health_frame["status"], "ok");
    assert!(health_started.elapsed() < Duration::from_secs(1));
    assert!(!child_survived, "yes child must be reaped after disconnect");
}

#[test]
fn inherited_output_pipes_do_not_delay_the_terminal_frame() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "inherited-pipe-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "(sleep 5) & exit 0"],
        "timeout_milliseconds": 2_000,
    });
    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");

    let started = Instant::now();
    let frames: Vec<serde_json::Value> = BufReader::new(stream)
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("frame should arrive promptly")).expect("frame JSON")
        })
        .collect();

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        frames
            .last()
            .is_some_and(|frame| { frame["type"] == "run_exit" && frame["exit_code"] == 0 })
    );
}

#[test]
fn health_remains_prompt_while_eight_process_slots_are_occupied() {
    let backend = start_backend();
    let mut process_streams = Vec::new();
    for index in 0..8 {
        let request = serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": format!("blocked-run-{index}"),
            "executable": "/bin/sleep",
            "arguments": ["5"],
            "timeout_milliseconds": 10_000,
        });
        let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
        stream
            .write_all(format!("{request}\n").as_bytes())
            .expect("request should write");
        process_streams.push(stream);
    }

    let child_deadline = Instant::now() + Duration::from_secs(2);
    let child_processes = loop {
        let children = child_pids(backend.child.id());
        if children.len() == 8 || Instant::now() >= child_deadline {
            break children;
        }
        thread::sleep(Duration::from_millis(10));
    };
    if child_processes.len() != 8 {
        for pid in &child_processes {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &pid.to_string()])
                .status();
        }
        panic!("all eight process slots should become occupied");
    }

    let ninth_request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "capacity-run",
        "executable": "/bin/sleep",
        "arguments": ["5"],
        "timeout_milliseconds": 10_000,
    });
    let mut ninth_stream =
        UnixStream::connect(&backend.socket_path).expect("ninth client should connect");
    ninth_stream
        .set_read_timeout(Some(Duration::from_millis(750)))
        .expect("ninth read timeout should configure");
    let ninth_started = Instant::now();
    ninth_stream
        .write_all(format!("{ninth_request}\n").as_bytes())
        .expect("ninth request should write");
    let mut ninth_frame = String::new();
    BufReader::new(ninth_stream)
        .read_line(&mut ninth_frame)
        .expect("ninth request should receive a prompt terminal response");
    assert!(ninth_started.elapsed() < Duration::from_secs(1));
    let ninth_frame: serde_json::Value =
        serde_json::from_str(&ninth_frame).expect("ninth response should be JSON");
    assert_eq!(ninth_frame["type"], "run_exit");
    assert_eq!(ninth_frame["run_id"], "capacity-run");
    assert!(ninth_frame["exit_code"].is_null());
    assert_eq!(ninth_frame["error_code"], "capacity_exhausted");
    assert!(
        child_pids(backend.child.id()).len() <= 8,
        "a capacity rejection must not create another child process"
    );

    let socket_path = backend.socket_path.clone();
    let (health_sender, health_receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let mut stream = UnixStream::connect(socket_path).map_err(|error| error.to_string())?;
            stream
                .set_read_timeout(Some(Duration::from_millis(750)))
                .map_err(|error| error.to_string())?;
            stream
                .write_all(b"{\"version\":1,\"type\":\"health\"}\n")
                .map_err(|error| error.to_string())?;
            let mut frame = String::new();
            BufReader::new(stream)
                .read_line(&mut frame)
                .map_err(|error| error.to_string())?;
            Ok(frame)
        })();
        let _ = health_sender.send(result);
    });
    let health_result = health_receiver.recv_timeout(Duration::from_secs(1));

    for pid in &child_processes {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
    drop(process_streams);

    let health_frame = health_result
        .expect("health request should finish promptly")
        .expect("health request should succeed");
    let health_frame: serde_json::Value =
        serde_json::from_str(&health_frame).expect("health response should be JSON");
    assert_eq!(health_frame["status"], "ok");
    assert!(
        child_pids(backend.child.id()).len() <= 8,
        "child count must remain within the process-slot limit"
    );
}

#[test]
fn timeout_kills_the_direct_child_emits_one_terminal_last_and_releases_capacity() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "timeout-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "printf before-timeout; sleep 2"],
        "timeout_milliseconds": 200,
    });
    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    let started = Instant::now();
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    let frames: Vec<serde_json::Value> = BufReader::new(stream)
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("frame should be readable")).expect("frame JSON")
        })
        .collect();

    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(frames.iter().any(|frame| {
        frame["type"] == "run_output"
            && frame["output"]
                .as_str()
                .is_some_and(|output| output.contains("before-timeout"))
    }));
    let terminal_indexes: Vec<_> = frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| (frame["type"] == "run_exit").then_some(index))
        .collect();
    assert_eq!(terminal_indexes, vec![frames.len() - 1]);
    assert!(frames.last().is_some_and(|frame| {
        frame["exit_code"].is_null() && frame["error_code"] == "timed_out"
    }));

    let mut health = UnixStream::connect(&backend.socket_path).expect("health should connect");
    health
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("health timeout should configure");
    health
        .write_all(b"{\"version\":1,\"type\":\"health\"}\n")
        .expect("health should write");
    let mut health_frame = String::new();
    BufReader::new(health)
        .read_line(&mut health_frame)
        .expect("health should remain responsive");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&health_frame).expect("health JSON")["status"],
        "ok"
    );
}

#[test]
fn invalid_timeouts_are_rejected_before_process_admission() {
    let backend = start_backend();
    for timeout_milliseconds in [
        serde_json::Value::Null,
        serde_json::json!(0),
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!("100"),
    ] {
        let request = serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": "invalid-timeout-run",
            "executable": "/bin/sleep",
            "arguments": ["2"],
            "timeout_milliseconds": timeout_milliseconds,
        });
        let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("read timeout should configure");
        stream
            .write_all(format!("{request}\n").as_bytes())
            .expect("request should write");
        let mut frame = String::new();
        BufReader::new(stream)
            .read_line(&mut frame)
            .expect("protocol error should be readable");
        let frame: serde_json::Value = serde_json::from_str(&frame).expect("frame JSON");
        assert_eq!(frame["type"], "error");
        assert_eq!(frame["code"], "invalid_start_process");
        assert!(child_pids(backend.child.id()).is_empty());
    }

    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "missing-timeout-run",
        "executable": "/bin/sleep",
        "arguments": ["2"],
    });
    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    let mut frame = String::new();
    BufReader::new(stream)
        .read_line(&mut frame)
        .expect("protocol error should be readable");
    let frame: serde_json::Value = serde_json::from_str(&frame).expect("frame JSON");
    assert_eq!(frame["type"], "error");
    assert_eq!(frame["code"], "invalid_start_process");
    assert!(child_pids(backend.child.id()).is_empty());
}

#[test]
fn u64_max_timeout_allows_a_completed_process() {
    let backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "u64-max-timeout-run",
        "executable": "/usr/bin/true",
        "arguments": [],
        "timeout_milliseconds": u64::MAX,
    });
    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    let frames: Vec<serde_json::Value> = BufReader::new(stream)
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("frame should be readable")).expect("frame JSON")
        })
        .collect();
    let terminal = frames.last().expect("terminal frame should be present");
    assert_eq!(terminal["type"], "run_exit");
    assert_eq!(terminal["exit_code"], 0);
    assert!(terminal["error_code"].is_null());
}

#[test]
fn timed_out_runs_release_all_process_capacity() {
    let backend = start_backend();
    let mut streams = Vec::new();
    for index in 0..8 {
        let request = serde_json::json!({
            "version": 1,
            "type": "start_process",
            "run_id": format!("timeout-capacity-{index}"),
            "executable": "/bin/sleep",
            "arguments": ["2"],
            "timeout_milliseconds": 200,
        });
        let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout should configure");
        stream
            .write_all(format!("{request}\n").as_bytes())
            .expect("request should write");
        streams.push(stream);
    }
    for stream in streams {
        let frames: Vec<serde_json::Value> = BufReader::new(stream)
            .lines()
            .map(|line| {
                serde_json::from_str(&line.expect("frame should be readable")).expect("frame JSON")
            })
            .collect();
        assert!(frames.last().is_some_and(|frame| {
            frame["exit_code"].is_null() && frame["error_code"] == "timed_out"
        }));
    }

    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "capacity-released-run",
        "executable": "/usr/bin/true",
        "arguments": [],
        "timeout_milliseconds": 1_000,
    });
    let mut stream = UnixStream::connect(&backend.socket_path).expect("client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("request should write");
    let frames: Vec<serde_json::Value> = BufReader::new(stream)
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("frame should be readable")).expect("frame JSON")
        })
        .collect();
    assert!(
        frames
            .last()
            .is_some_and(|frame| { frame["exit_code"] == 0 && frame["error_code"].is_null() })
    );
}

fn child_pids(parent_pid: u32) -> Vec<u32> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-P", &parent_pid.to_string()])
        .output()
        .expect("child process query should run");
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8(output.stdout)
        .expect("child process query should be UTF-8")
        .lines()
        .map(|line| line.parse().expect("child PID should be numeric"))
        .collect()
}

fn process_exists(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .status()
        .expect("process observation should run")
        .success()
}

fn read_grandchild_pid(reader: &mut BufReader<UnixStream>) -> libc::pid_t {
    loop {
        let mut frame = String::new();
        reader
            .read_line(&mut frame)
            .expect("grandchild pid output should be readable");
        assert!(
            !frame.is_empty(),
            "connection closed before the grandchild pid was observed"
        );
        let frame: serde_json::Value = serde_json::from_str(&frame).expect("frame JSON");
        assert_ne!(
            frame["type"], "run_exit",
            "run exited before the grandchild pid was observed"
        );
        if frame["type"] == "run_output" && frame["stream"] == "stdout" {
            return frame["output"]
                .as_str()
                .expect("stdout output should be a string")
                .trim()
                .parse()
                .expect("stdout output should contain the grandchild pid");
        }
    }
}

/// Kills the tracked grandchild on drop — including panic unwinding — but only
/// after confirming the pid still belongs to the test's `sleep 30`, so a
/// recycled pid never receives a stray SIGKILL.
struct DescendantGuard(libc::pid_t);

impl Drop for DescendantGuard {
    fn drop(&mut self) {
        let observed = Command::new("/bin/ps")
            .args(["-o", "command=", "-p", &self.0.to_string()])
            .output();
        if let Ok(observed) = observed
            && String::from_utf8_lossy(&observed.stdout).contains("sleep 30")
        {
            // SAFETY: the observed pid still belongs to this test's descendant process.
            let _ = unsafe { libc::kill(self.0, libc::SIGKILL) };
        }
    }
}

fn wait_for_process_death(pid: libc::pid_t) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // SAFETY: signal zero only queries whether the test's observed pid still exists.
        if unsafe { libc::kill(pid, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_process_stopped_state(pid: libc::pid_t, stopped: bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = Command::new("/bin/ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .expect("process state should be observable");
        let state = String::from_utf8_lossy(&output.stdout);
        let state = state.trim_start();
        if !state.is_empty()
            && if stopped {
                state.starts_with('T')
            } else {
                !state.starts_with('T') && !state.starts_with('Z')
            }
        {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn assert_signal_shutdown(signal: libc::c_int, assert_socket_removed: bool) {
    let mut backend = start_backend();
    let request = serde_json::json!({
        "version": 1,
        "type": "start_process",
        "run_id": "signal-shutdown-run",
        "executable": "/bin/sh",
        "arguments": ["-c", "sleep 30 & echo $!; wait"],
        "timeout_milliseconds": 30_000,
    });
    let mut stream =
        UnixStream::connect(&backend.socket_path).expect("start client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("start request should write");
    let mut reader = BufReader::new(stream);
    let grandchild_pid = read_grandchild_pid(&mut reader);
    let _cleanup = DescendantGuard(grandchild_pid);

    let started = Instant::now();
    assert_eq!(
        // SAFETY: backend.child is live here and signal is SIGTERM or SIGINT.
        unsafe { libc::kill(backend.child.id() as libc::pid_t, signal) },
        0,
        "signal should be delivered to the backend"
    );
    let deadline = started + Duration::from_secs(6);
    let status = loop {
        if let Some(status) = backend
            .child
            .try_wait()
            .expect("backend exit should be observable")
        {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "backend should exit promptly after receiving signal {signal}"
        );
        thread::sleep(Duration::from_millis(10));
    };

    assert_eq!(status.code(), Some(0), "backend should exit successfully");
    assert!(
        wait_for_process_death(grandchild_pid),
        "grandchild sleep process {grandchild_pid} should be dead after backend shutdown"
    );
    if assert_socket_removed {
        assert!(
            !backend.socket_path.exists(),
            "backend socket should be removed during shutdown"
        );
    }
}

fn cancel_process(socket_path: &Path, run_id: &str) -> serde_json::Value {
    let request = serde_json::json!({"version": 1, "type": "cancel_process", "run_id": run_id});
    let mut stream = UnixStream::connect(socket_path).expect("cancel client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("cancel timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("cancel request should write");
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("cancel response should read");
    serde_json::from_str(&response).expect("cancel response JSON")
}

fn control_process(socket_path: &Path, request_type: &str, run_id: &str) -> serde_json::Value {
    let request = serde_json::json!({
        "version": 1,
        "type": request_type,
        "run_id": run_id,
    });
    let mut stream = UnixStream::connect(socket_path).expect("control client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("control timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("control request should write");
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("control response should read");
    serde_json::from_str(&response).expect("control response JSON")
}

fn input_control(socket_path: &Path, request: serde_json::Value) -> serde_json::Value {
    let mut stream = UnixStream::connect(socket_path).expect("input client should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("input timeout should configure");
    stream
        .write_all(format!("{request}\n").as_bytes())
        .expect("input request should write");
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("input response should read");
    serde_json::from_str(&response).expect("input response JSON")
}

fn wait_for_registered_input_control(
    socket_path: &Path,
    request: serde_json::Value,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let response = input_control(socket_path, request.clone());
        if response["status"] != "not_found" || Instant::now() >= deadline {
            return response;
        }
        thread::yield_now();
    }
}

fn read_json_frame(reader: &mut BufReader<UnixStream>) -> serde_json::Value {
    let mut frame = String::new();
    reader
        .read_line(&mut frame)
        .expect("response frame should read");
    serde_json::from_str(&frame).expect("response frame should be JSON")
}
