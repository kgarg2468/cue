# Blockers (must fix before merge)

none

# Majors

- [confidence 10/10] `backend/src/lib.rs:246-260` — `close_stdin` does not reliably produce EOF on a PTY. Send data without a newline to canonical `cat`, or disable `ICANON`, then close stdin: the single `0x04` does not terminate the next read. Later sends return `"closed"`, but the child remains blocked until cancellation or timeout. The test at `backend/tests/process_lifecycle.rs:864` misses both cases because it sends `"hello\n"` in canonical mode.

- [confidence 9/10] `backend/src/lib.rs:1389-1413` — a successfully spawned PTY run can terminate without `run_metadata` or `run_exit`. If PTY allocation and child spawn succeed but output-drain thread creation fails, the child is killed and reaped before the function returns `Err`; the client observes socket EOF only. This violates the all-spawned-terminal-path contract.

# Minors

- [confidence 9/10] `backend/tests/process_lifecycle.rs:234-335,685-723` — tests do not cover PTY timeout or pause/resume, and cancellation does not assert preceding metadata. A PTY-specific PGID or terminal-order regression could pass the suite.

- [confidence 10/10] `Tests/CaptureDelegateIPCTests/IPCClientTests.swift:12-30` — no assertion pins omission of default `pty: false`, nor focused serialization of `pty: true`. Emitting `"pty":false` by default would violate the wire requirement while current tests remain green.

# Notes

- Normal FD lifecycle is sound: failure paths drop both descriptors, the parent closes the slave after spawn, the master is close-on-exec and nonblocking, and the child closes the original slave after `dup2`.

- The PGID invariant holds. `setsid()` completes before `spawn()` returns, so `child.id()` is the session and process-group ID. Publication remains serialized with pause/resume.

- Cancellation or timeout during the 50 ms PTY poll is handled correctly: group termination closes the slave, buffered output drains, and EOF/EIO ends output before metadata and exit.

- No steady-state PTY poll busy-loop was found. Existing unbounded socket-write backpressure can still delay joining and terminal frames, but that limitation predates this PR.

- The non-PTY branch preserves pipe setup, supervision, redaction accounting, 1 KiB chunks, and progressive metadata truncation.

- `.loop/traceability.md:227` changing S10-010 from `done` to `verified` is sane. The red-on-main claim is supported by the recorded failure log.
