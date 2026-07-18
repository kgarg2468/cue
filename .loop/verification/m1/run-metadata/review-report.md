## Blockers

- `backend/src/lib.rs`, ~1303 and ~1482: `run_metadata` is subject to the 8 KiB frame limit despite containing unbounded environment-name and argument data. A spawned `/bin/true` run with a roughly 7–8 KiB argument, or a backend environment with many long variable names, produces oversized metadata. `write_json_frame` returns `InvalidData`; execution unwinds before `run_exit`, so the client receives neither metadata nor an exit frame.

## Majors

- `backend/src/lib.rs`, ~1181–1258 and ~1278–1300: Several post-spawn errors return before metadata or exit is emitted. For example, output-drain thread creation can fail under thread/resource exhaustion after the child has spawned; the child is killed, but the socket closes with zero terminal frames. `fcntl`, `try_wait`, `wait`, and drain-thread panic paths have the same zero-metadata outcome.

- `backend/src/lib.rs`, ~1030–1034: The regexes do not redact all credentials in the specified classes. `password=abc` is skipped by the four-character minimum, `password='hunter2 secret'` and `password="correct horse"` are skipped because quoted values cannot contain whitespace and single quotes are unsupported, and `Bearer abc` is skipped by the eight-character minimum. These values are emitted unchanged and contribute zero redactions.

- `backend/src/lib.rs`, ~1150 and ~1294–1302: `finished_at` and `duration_ms` are captured after both output-drain threads join, rather than when process supervision reaches its terminal state. If a high-output process exits while the client applies socket backpressure, a drain thread can remain blocked until the client reads; metadata then reports that arbitrary delay as run duration. A process killed at a 200 ms timeout can therefore report a duration of several seconds or longer.

- `backend/src/lib.rs`, ~1152: Metadata collection introduces a start-process regression by propagating `current_dir()` failure before attempting the spawn or emitting a structured exit. If the backend was launched from a directory that is subsequently removed, every valid start request now closes with EOF and no protocol frame, whereas an absolute executable could previously spawn from that inherited directory.

## Minors

- `backend/src/lib.rs`, ~1030–1034: Ordinary prose is over-redacted. Output such as `Bearer authentication is required` becomes `Bearer [REDACTED] is required`, and `parser token: identifier` becomes `parser token: [REDACTED]`, because the Bearer and colon-assignment patterns cannot distinguish credentials from explanatory text.

## Notes

- `backend/src/lib.rs`, ~1294–1324: On successful paths, both drain threads are joined before metadata and all frame writes share one mutex, so no emitted `run_output` can follow metadata. The run ID remains registered through the metadata write and is released before `run_exit`; reuse attempted after metadata but before exit receives `duplicate_run_id`, while reuse after observing exit is safe.

- `backend/src/lib.rs`, ~1207–1318: The shared `AtomicUsize` does not lose concurrent stdout/stderr increments, and joining both drain threads establishes visibility before the final relaxed load. Rust regex matching avoids catastrophic backtracking, and `${1}[REDACTED]` correctly preserves the Bearer prefix.
