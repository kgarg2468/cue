# Development Loop Progress

## Cycle 0 — 2026-07-17

Status: initialized

- Established durable loop state for recurring job `809366a7` (every 10 minutes; expires 2026-07-24).
- Set `M0 Foundation` as the active milestone on `loop/m0-foundation`.
- Recorded the approved M0–M14 delivery chain and its dependency gates.
- Created full end-state requirements traceability for specification sections 1–21.
- No product implementation, PR, runtime lock, or source change was made.

Next cycle: implement only M0 requirements, attach implementation and verification evidence to the ledger, and keep one active milestone PR at a time.

## Cycle 0 review remediation — 2026-07-17

Status: in progress

- Added regression coverage for bounded/concurrent Rust client handling, malformed and disconnected clients, newline framing, socket identity cleanup, Swift SIGPIPE suppression, and bounded response reads.
- Aligned CI with the single `scripts/verify-m0.sh` parity gate and added ledger evidence-state validation.
- Corrected M0 scope so persistence and migrations remain in M2; added explicit M0 foundation rows without changing the original requirement rows.
- Scheduled recurring-job renewal for 2026-07-23 with an explicit `blocked_expired` failure transition.
- Hardened the standalone handshake plan around a private runtime directory and backend liveness checks.
- Remediation is not marked verified or merged; final parity verification and PR evidence remain outstanding.

## Cycle 0 local implementation verification — 2026-07-17

Status: implemented locally; independent final review, PR, CI, and merge pending

- Completed the M0 Foundation implementation and recorded concrete source and local verification evidence for CTL-001, CTL-002, and M0-FOUNDATION-001 through M0-FOUNDATION-006.
- Passed the ledger gate, full `scripts/verify-m0.sh` local parity suite, and the recorded native-app/backend runtime checks.
- No implementation PR has been opened; no CI run, independent final review, merge, or verified/merged lifecycle status is claimed.

Next cycle: obtain independent final review, open the single M0 PR, and attach CI/PR evidence before any verification or merge lifecycle advancement.

## Cycle 0 PR delivery — 2026-07-17

Status: in review

- Committed the bounded M0 Foundation slice as `a89e3e1`.
- Pushed `loop/m0-foundation` and opened PR `#1` against `main`.
- Independent spec-compliance and code-quality reviews reported no blocking findings.
- GitHub CI and merge remain pending; M0 traceability rows stay `implemented` until those gates pass.

## Cycle 0 post-merge runtime audit — 2026-07-17

Status: blocked

- PR #1 merged as `3e6cf85`; both CI runs passed, and local `scripts/verify-m0.sh` still passes.
- The post-merge completion gate is red: `scripts/verify-app-launch.sh` failed deterministically on 10/10 consecutive launches. LaunchServices/System Events reported CaptureDelegateApp foreground and visible with zero windows; CoreGraphics reported no window.
- Tested and reverted three hypotheses without resolution: AppDelegate activation, a single SwiftUI Window scene, and an explicit AppKit-owned NSWindow. Current source remains unchanged.
- Marked M0-FOUNDATION-001 blocked while retaining its implementation evidence and recording the failed runtime verification. CTL-001, CTL-002, and M0-FOUNDATION-002 through M0-FOUNDATION-006 are verified with PR/merge/CI evidence.
- M1 remains dependency-blocked; no milestone advancement was made.

## Cycle 1 native-window environment investigation — 2026-07-17

Status: blocked pending fresh GUI login

- Confirmed the durable loop remains active as job `809366a7` and full non-UI parity passes from a clean exported checkout.
- The unchanged app and a standalone AppKit control both alternated between observable windows and foreground zero-window processes during this long-lived GUI session. Binary hashes remained unchanged across Swift build/test commands.
- Reverted all temporary lifecycle instrumentation and source hypotheses; product source is identical to `origin/main`.
- Per systematic-debugging stop criteria, no fourth speculative source fix was attempted. Trustworthy re-verification requires a fresh interactive macOS login, which was not performed because it would disrupt the user.
- M0 remains blocked and M1 remains dependency-blocked.

## Cycle 2 M0 closeout — 2026-07-17

Status: ready for M1

- After fresh loginwindow at 18:57:46 local, `scripts/verify-app-launch.sh` passed 10/10 consecutive runs; retained CoreGraphics screenshot `.loop/verification/m0/fresh-login/app.png` shows a nonblank native window with centered “Capture → Delegate”.
- Verified M0-FOUNDATION-001 against unchanged PR #1 implementation commit `3e6cf85`; the prior native-window blocker was session-specific and no product source changed.
- Cleared M0 blockers and made M1 IPC/process lifecycle dependency-ready; no M1 implementation, branch, or PR was started.

## Cycle 3 M1 process streaming — 2026-07-18

Status: delivered through PR #5; M1 remains in progress

- Implemented a bounded first M1 slice on `loop/m1-process-streaming`: direct executable/argv launch, callback stdout/stderr events, exactly one terminal exit, typed `spawn_failed` and `capacity_exhausted`, UTF-8 preservation, and per-frame byte-dribble deadlines.
- Bounded execution to eight children with synchronous ninth-run rejection while retaining eight short IPC workers so health remains serviceable. Added disconnect cancellation/reaping for active producers and direct-child completion independent of descendant-held output pipes.
- Passed the retained authoritative gate in `.loop/verification/m1/process-streaming/full-verification.txt`: Rust format/clippy/release, 26 Rust tests, Swift strict format/release, 13 Swift tests against the real Rust backend, private `0700` runtime plus `0600` socket handshake, and diff check.
- Independent correctness and execution-boundary security reviews are clear in `.loop/verification/m1/process-streaming/reviews.txt`. A future production launcher must create/verify the private runtime directory; no production launcher exists in this slice.
- Initial CI exposed a timing race in the inherited M0 worker-saturation test; the test-only clock correction passed 20/20 locally, the full gate passed, and independent remediation review was clear. Both required CI runs then passed on `9bfd3f2`.
- PR #5 squash-merged as `9f40024` and its remote branch was deleted.
- M1 remains incomplete. PTYs, explicit timeout, pause/resume/cancel, waiting-input detection, secret redaction/process metadata, durable run state, and remaining cleanup behavior are not claimed.

Next cycle: select and implement the next bounded M1 slice from fresh `origin/main`; keep the milestone and partial traceability rows in progress.

## Cycle 4 M1 per-run timeout — 2026-07-18

Status: PR #7 open; CI and merge pending

- Extended protocol-v1 `start_process` with required positive `timeout_milliseconds` and Swift `timeoutMilliseconds`; typed timeout completion is `run_exit` with `exit_code: null` and `error_code: timed_out`.
- Started the timeout clock immediately before direct spawn, made observed child completion win over an elapsed deadline, and on timeout kill/reap the direct child, preserve preceding output, join drains, emit one terminal event last, and release process capacity.
- Rejected invalid timeout shapes before admission while accepting the full positive JSON-u64 range; focused real timeout and capacity-release tests passed 10/10.
- Full host verification passed 31 Rust tests, 15 Swift tests against the real backend, strict lints, release builds, and diff checks. Retained evidence: `.loop/verification/m1/run-timeout/verification.txt`.
- Independent correctness/security review is clear. M1 remains incomplete for PTYs, public pause/resume/cancel, process-group termination, waiting-input detection, redaction/metadata, durable state, and remaining cleanup.

Next cycle: wait for PR #7 green CI and final review, then merge and close out this bounded timeout slice before beginning another M1 feature.
