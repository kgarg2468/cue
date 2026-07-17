use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const CLIENT_IO_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_CONCURRENT_CLIENTS: usize = 8;

#[derive(Deserialize)]
struct Request {
    version: u32,
    #[serde(rename = "type")]
    request_type: String,
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
    remove_stale_socket(socket_path)?;
    let listener = UnixListener::bind(socket_path)?;
    let bound_metadata = fs::symlink_metadata(socket_path)?;
    let _cleanup = SocketCleanup {
        path: socket_path.to_path_buf(),
        identity: SocketIdentity::from_metadata(&bound_metadata),
    };
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;
    let worker_slots = WorkerSlots::new(MAX_CONCURRENT_CLIENTS);

    loop {
        let worker_slot = worker_slots.acquire();
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) => {
                eprintln!("IPC accept error: {error}");
                continue;
            }
        };
        if let Err(error) = thread::Builder::new()
            .name("capture-delegate-ipc".to_owned())
            .spawn(move || {
                let _worker_slot = worker_slot;
                let _ = handle_connection(stream);
            })
        {
            eprintln!("IPC worker spawn error: {error}");
        }
    }
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

fn handle_connection(mut stream: UnixStream) -> io::Result<()> {
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
    } else if request.request_type != "health" {
        write_protocol_error(&mut stream, "unknown_request_type")?;
    } else {
        let response = HealthResponse {
            version: PROTOCOL_VERSION,
            response_type: "health_response",
            status: "ok",
        };
        serde_json::to_writer(&mut stream, &response)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
    }

    Ok(())
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
    use super::{SocketCleanup, SocketIdentity};
    use std::fs;
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
