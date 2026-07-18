# Blockers

1. `backend/src/lib.rs:2041` — The supervision thread takes the socket-writer mutex with a blocking `lock()`, despite the socket having no write timeout (`:938`). If a client stops reading while an output drain is blocked holding that mutex, activity is not recorded because recording occurs only after the write completes (`:2026-2028`). Once the child becomes CPU-quiet from backpressure, input-wait detection blocks behind the drain forever. A separate cancel request returns `accepted`, but the supervision loop cannot observe it, kill the child, or release the process slot. Use a nonblocking writer acquisition and retry on a later tick.

# Majors

1. `backend/src/lib.rs:1661` — Detection ignores `control.paused`. Pause immediately after output but before the threshold; SIGSTOP makes output and CPU quiescent, so the backend emits `run_input_waiting` for an intentionally paused process. This produces a false “Needs input” notification. Suppress detection while paused and reset/rebaseline appropriately on resume.

# Minors

1. `backend/tests/process_lifecycle.rs:1055` — The busy-silent negative test is scheduler-dependent and can also pass vacuously. A fast machine may finish the fixed iteration count before 300 ms, exercising no CPU gate; a heavily loaded machine may schedule the process for under 30 ms during the window, legitimately causing the test to fail. Test the CPU predicate with an injectable/synthetic sampler rather than a wall-clock shell loop.

# Notes

- Frame integrity and terminal ordering are otherwise sound: output and waiting frames share the socket mutex, and waiting is evaluated only in `ProcessSupervision::Running`, before metadata/exit handling.
- The stdin double-check prevents emission with queued input or during a child write. No opposing stdin→writer nested lock path was found.
- Episode state emits once and resets only after recorded output or delivered stdin activity; the activity rechecks close the observed races.
- PID reuse is avoided: CPU sampling occurs only after `try_wait()` reports the child running; an exited-but-unreaped PID cannot be reused, and terminal branches do not sample.
- The Mach-time conversion is correct. Apple’s XNU tests likewise convert `PROC_PIDTASKINFO` deltas from Mach units before treating them as nanoseconds. [Apple XNU recount test](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/tests/recount/recount_perf_tests.c)
- Validation, nil-field wire compatibility, PTY/pipe decoding, and Swift event decoding appear sound.
