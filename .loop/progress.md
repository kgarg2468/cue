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

Status: delivered through PR #7; M1 remains in progress

- Extended protocol-v1 `start_process` with required positive `timeout_milliseconds` and Swift `timeoutMilliseconds`; typed timeout completion is `run_exit` with `exit_code: null` and `error_code: timed_out`.
- Started the timeout clock immediately before direct spawn, made observed child completion win over an elapsed deadline, and on timeout kill/reap the direct child, preserve preceding output, join drains, emit one terminal event last, and release process capacity.
- Rejected invalid timeout shapes before admission while accepting the full positive JSON-u64 range; focused real timeout and capacity-release tests passed 10/10.
- Full host verification passed 31 Rust tests, 15 Swift tests against the real backend, strict lints, release builds, and diff checks. Retained evidence: `.loop/verification/m1/run-timeout/verification.txt`.
- Independent correctness/security review is clear. M1 remains incomplete for PTYs, public pause/resume/cancel, process-group termination, waiting-input detection, redaction/metadata, durable state, and remaining cleanup.
- Both required CI runs passed on exact head `a6693cc`; PR #7 squash-merged as `3058370` and its remote branch was deleted.

Next cycle: select one next bounded M1 lifecycle slice from fresh `origin/main`; keep M1 and partial traceability rows in progress.

## Cycle 6 M1 run cancellation and loop recovery — 2026-07-18

Status: delivered through PR #9; M1 remains in progress

- Recovered the loop after the prior session died mid-cycle-5: broke the orphaned `.loop/LOCK` on direct evidence (no agent process, no commits or file changes since acquisition, never-refreshed heartbeat) with the user present, and recorded the takeover in the lock file. The loop now runs as durable job `a8a4fe0a` (created 2026-07-18, expires 2026-07-25) in session `1bbeeb14`; job `809366a7` died with the prior session.
- Adopted cycle 5's uncommitted 463-line cancel slice from its stale worktree after full review: versioned `cancel_process`/`cancel_response`, ActiveRuns registry with duplicate-run-id rejection and Drop-based release, typed `cancelled` terminals preserving prior output, and Swift `IPCClient.cancelProcess`. Fixed a registration race in its duplicate-run-id test during adoption.
- Independent Codex review (gpt-5.6-sol, high effort): no blockers, five major/three minor findings. Fixed the two confirmed in-slice issues pre-merge — run ids now release before every terminal frame so a client observing `run_exit` can immediately reuse the id (deterministically tested), and the cancel/timeout race test no longer assumes an output-first frame (5/5 stress reruns). Flagged inherent cancel/spawn/exit/timeout attribution races and the pre-existing non-reading-client drain stall as pending M1 work in S10-011.
- Full parity gate green locally (ledger, cargo fmt/clippy/test, swift format strict, release builds, 17 Swift tests against the real backend, health handshake); release-binary runtime demo shows cancel accepted on a separate connection with the `cancelled` terminal 0.01s later and post-exit cancel returning `not_found`. Evidence: `.loop/verification/m1/run-cancel/`.
- Both required CI runs passed on exact head `70854c6`; PR #9 squash-merged as `ea02d67` and its remote branch was deleted.
- Environmental note: `cleanup_does_not_unlink_replacement_socket` fails in any checkout whose path exceeds SUN_LEN for the test socket; unrelated to this slice.

Next cycle: select one next bounded M1 lifecycle slice (pause/resume or descendant process groups) from fresh `origin/main`; keep M1 and partial traceability rows in progress.

## Cycle 7 M1 descendant process-group termination — 2026-07-18

- Acquired the lock cleanly (token 42fe659c) with no contention; job a8a4fe0a expires 2026-07-25, so no renewal was due.
- Slice: every run now leads its own process group (`Command::process_group(0)`) and a `kill_process_group` helper (SIGKILL to the negative pgid plus direct-kill fallback) replaces all five direct `child.kill()` sites in `run_process`, so descendants die on cancel, timeout, natural leader exit, and the error paths. `libc = "0.2"` added.
- Implementation by a Codex gpt-5.6-terra worker (high effort) within an exact three-file allowlist; diff gate clean. The worker's sandbox blocked Unix-socket binding, so red→green TDD evidence, the full suite, CI parity, stress reruns, and release-binary runtime demos were produced in the workspace.
- Independent Codex sol review (high): no blockers, 3 major + 2 minor. Fixed pre-merge: natural leader exit skipped the group kill (`sh -c 'sleep 30 & exit 0'` leaked the descendant even after an accepted cancel — Exited branch now group-kills first, with a red-verified regression test and runtime demo showing the grandchild dead 0.00s after run_exit); pid-reuse-unsafe test cleanup (replaced with an identity-checked, panic-safe DescendantGuard); timeout-margin and pid-parse robustness (a first fix attempt set run timeout equal to the client read timeout and deterministically raced — caught by stress reruns). Flagged as future M1 scope: backend-shutdown group cleanup / dev Ctrl-C semantics, inherited run stdin, inherent post-reap pgid-reuse window.
- Evidence: `.loop/verification/m1/process-groups/` (cargo suite 9+13+17 green, three descendant tests 8/8 stress, verify-m0 parity green twice, cancel and natural-exit runtime demos).
- Delivered: PR #11 (`59f0f52`, `a07a6e2`), both verify-m0 CI runs green on both commits, squash-merged as `8877242`; closeout in this PR.
- Status: merged; M1 remains in progress. Next cycle: one bounded M1 slice — backend-shutdown active-run-group cleanup, or pause/resume.

## Cycle 8 M1 backend-shutdown active-run cleanup — 2026-07-18

- Acquired the lock cleanly (token 95ff3ccf); job a8a4fe0a expires 2026-07-25, so no renewal was due.
- Slice: SIGTERM/SIGINT now wake a dedicated "capture-delegate-shutdown" thread through an async-signal-safe self-pipe (`sigaction` handler writes one byte); the thread cancels every active run — whose process groups the existing supervision paths SIGKILL — waits up to 5s for drain, removes the socket only if it is still the bound dev/inode, and exits 0. Two integration tests send real SIGTERM/SIGINT to the spawned release-path binary and assert grandchild death, exit code 0, and socket removal.
- Implementation by a Codex gpt-5.6-sol worker (high effort) within a two-file allowlist; diff gate clean. Its sandbox blocked Unix-socket binding, so red→green TDD evidence (both signal tests fail "backend should exit successfully" pre-change), the full suite, 8/8 signal-test stress reruns, CI parity, and a release-binary runtime demo (grandchild dead 0.03s after SIGTERM, backend exit 0 at 0.05s, socket removed) were produced in the workspace.
- Independent Codex sol review (high): 1 blocker + 2 major + 1 minor, all verified against the source and fixed pre-merge by a second sol worker (diff-gated to `backend/src/lib.rs`, +137/−24): the drain-window admission race (a start_process registering after cancel_all got a fresh unflagged cancel bool that exit(0) could orphan — ActiveRuns now guards {runs, shutting_down} under one mutex, begin_shutdown closes admission atomically, and drain-window requests get a terminal `cancelled` run_exit, unit-tested); missing FD_CLOEXEC let every supervised child inherit both self-pipe ends; the blocking write end could wedge a handler under sustained signals (now O_NONBLOCK, EAGAIN = wake already queued); and a pthread_sigmask bracket closes the pre-install window where signals took the default disposition and skipped cleanup.
- Evidence: `.loop/verification/m1/shutdown-cleanup/` (verification.txt with dispositions, pre/post-fix cargo suites 10+13+19 green, verify-m0 parity green twice, runtime demo).
- Delivered: PR #13 (`6cecde4`, `05c4de4`), both verify-m0 CI runs green on both commits, squash-merged as `af7db55`; closeout in this PR.
- Status: merged; M1 remains in progress. Next cycle: one bounded M1 slice — pause/resume, or waiting-input detection with run stdin semantics.

## Cycle 9 M1 pause/resume process-group control — 2026-07-18

- Acquired the lock cleanly (token 2047c47e); job a8a4fe0a expires 2026-07-25, so no renewal was due.
- Slice: versioned `pause_process`/`resume_process` requests (spec adapter API `pause(runId)`/`resume(runId)`, one-click pause) SIGSTOP/SIGCONT the run's whole process group with existence-based accepted/not_found mirroring cancel; registry value became `RunControl {cancelled, paused, pgid}`; a pause landing in the register-to-spawn window is applied post-spawn; the timeout deadline deliberately keeps ticking while paused; Swift `IPCClient.pauseProcess`/`resumeProcess` round-trip against the real backend.
- Implementation by a Codex gpt-5.6-sol worker (high effort) within a four-file allowlist; diff gate clean. The prompt's Swift-test path was wrong — the worker flagged it and stayed inside its fence, and the orchestrator relocated the test into the existing `CaptureDelegateIntegrationTests` harness instead of shipping a duplicated harness.
- TDD red verified (all three new Rust integration tests fail against unmodified origin/main with unknown_request_type); green: 11 lib + 13 + 22 integration, pause tests 8/8 stress, verify-m0 parity green with 19 Swift tests, release-binary runtime demo (grandchild S→T 0.00s after pause, T→S 0.00s after resume, group dead 0.01s after cancel, SIGTERM shutdown still clean).
- Independent Codex sol review (high): 1 blocker + 1 major + 3 minor, all verified. Fixed pre-merge by a second sol worker (+42/−18): the SeqCst store-then-load handshake ordered memory but not signal delivery — a stalled post-spawn SIGSTOP could stop a group after its resume — so all {pgid, paused} transitions and their signals now serialize under the registry mutex; pause/resume racing terminal cleanup could signal a reaped, reusable pgid — terminal branches now retire the published pgid under the mutex before reaping (Exited's try_wait residual documented as the pre-existing inherent window); resume polling now rejects zombie 'Z'; paused-timeout budget raised for slow CI. One minor noted (register-to-spawn window not exercised end-to-end; covered by unit test + mutex serialization).
- Evidence: `.loop/verification/m1/pause-resume/` (verification.txt with dispositions, cargo-test.log, parity logs pre/post-fix, runtime-demo.log).
- Delivered: PR #15 (`8f3d2e6`, `9e081fc`), both verify-m0 CI runs green on both commits, squash-merged as `9291f52`; closeout in this PR.
- Status: merged; M1 remains in progress. Next cycle: one bounded M1 slice — waiting-input detection with run stdin semantics, or durable run state.

## Cycle 10 M1 run stdin semantics — 2026-07-18

- Acquired the lock cleanly (token 1db6b7a2); job a8a4fe0a expires 2026-07-25, no renewal due.
- Slice: `start_process` now pipes child stdin (the child previously inherited the backend's stdin — a hygiene bug); new v1 requests `send_input` → `input_response accepted|not_found|closed` and idempotent `close_stdin` → `close_stdin_response accepted|not_found`; malformed variants return `invalid_send_input`/`invalid_close_stdin`; Swift `IPCClient.sendInput`/`closeStdin` mirror the pause/resume client patterns. Spec: section 9 `answerClarification(runId, answer)` transport; S10-001/S10-009 groundwork (waiting-input detection stays pending on the PTY slice).
- Implementation: Codex gpt-5.6-sol (high), four-file allowlist, diff gate clean. The orchestrator caught a real defect the worker's sandbox could not observe (no socket tests there): input accepted in the register-to-publish window was silently dropped — the new cat test failed deterministically. A second sol worker added a pending-buffer state machine so accepted means delivered modulo racing exit.
- Independent Codex sol review (high): 2 blockers + 2 majors, all confirmed against the code. Blockers: send_input blocked in write_all holding the per-run stdin mutex (8 concurrent senders on a paused run's full pipe could consume every IPC slot and wedge the service including cancel), and publish_stdin's synchronous flush could block the supervision thread before publish_pgid so timeout/cancel never ran. Majors: accepted despite knowably-impossible delivery; close not a stable input boundary. Fixed by a third sol worker: senders only enqueue under the per-run mutex, the run's supervision thread drains to a non-blocking fd every tick (stopping on WouldBlock, closing the boundary on hard pipe errors), `input_response` gained status `closed`, and an acknowledged close rejects later sends while still delivering buffered bytes before EOF.
- Verification: TDD red on origin/main (unknown_request_type); green 13 lib + 13 + 27 integration; stdin tests 8/8 stress; verify-m0 parity green (19 Swift tests incl. real-backend stdin round-trip); release-binary runtime demo — echo 0.03s, post-close send → closed, 700 KiB enqueued on a paused run in 0.35s all accepted, cancel accepted 0.00s later with cancelled terminal, SIGTERM exit 0 and socket removed. Evidence: `.loop/verification/m1/run-stdin/`.
- Delivered: PR #17 (`bf89e72`, `2bdf1b9`), both verify-m0 CI runs green on both commits, squash-merged as `67d39f2`; closeout in this PR.
- Flagged future work: per-run stdin queue is unbounded; waiting-input detection needs PTY support.
- Status: merged; M1 remains in progress. Next cycle: one bounded M1 slice — PTY support (unlocks waiting-input detection), or run metadata/redaction.

## Cycle 11 — 2026-07-18
- Slice: M1 run metadata + secret redaction (S10-010), PR #19 (loop/m1-run-metadata).
- run_metadata frame (pid/pgid/executable/arguments/wd/timestamps/duration/env NAMES/redaction count) after final output, before run_exit, on all terminal paths; spawn failures emit none. Output redaction with fixed compiled pattern set, per-frame, both streams. Swift .metadata event.
- Provenance: sol impl worker (killed by command timeout post-diff; orchestrator finished one test-ordering edit + all verification). Independent sol review: 1 blocker + 2 majors confirmed → fixed by second sol worker (metadata truncation vs 8KiB cap + run_exit no longer suppressible; duration captured at terminal state; current_dir fallback). Accepted limitations documented: chunk-split secrets, fixed pattern set, prose over-redaction. Pre-existing zero-terminal-frame resource-error paths flagged as future durability work.
- Evidence: .loop/verification/m1/run-metadata/ (red evidence, cargo 15+13+31, parity, runtime demos incl. partial/full truncation, review report + dispositions).
- BLOCKED: GitHub Actions billing failure ("recent account payments have failed or your spending limit needs to be increased") — no CI job starts; both required verify-m0 checks red in ~2s with zero steps; rerun reproduced. Local verification fully green. Merge prohibited until user fixes billing and checks rerun green.
- Next cycle: if CI unblocked → rerun checks, merge PR #19, closeout (traceability S10-010/S10-001). Else busy-exit on the persisted blocker.

## Cycle 12 (firings 12–41) — 2026-07-18

- Firings 12–40 were bounded blocked-probes on the GitHub Actions billing failure: each reran a verify-m0 run on PR #19, confirmed the identical "recent account payments have failed or your spending limit needs to be increased" annotation on the fresh job, appended to `.loop/busy/blocked-retries.log`, and exited with the lock released — twenty-nine consecutive identical results.
- User directive: GitHub billing will not be enabled; retire CI and use gpt-5.6-sol as the CI substitute (ADR-007). Discovery while implementing it: the free-plan private repo has no branch protection at all — the merge block had been the loop's own CI gate, which the user redefined.
- Final pre-merge local parity run on PR #19 head `e8671d0` (only `.loop` state files beyond the verified code commit `7199894`): exit 0 (`verify-m0-premerge.log`). Gate change documented in a PR #19 comment; squash-merged as `9c36a5f`; branch deleted.
- Closeout: S10-010 → done with evidence; S10-001 implementation notes updated (redaction/metadata delivered; PTY, worktree, input-wait detection, durable cleanup remain); state.json → ready, billing blocker cleared (moot under ADR-007), active_pr null.
- Next cycle: one bounded M1 slice — PTY support (unlocks waiting-input detection S10-009), or durable run state.

## Cycle 13 — 2026-07-18

- Slice: M1 PTY runs (`pty: true` on start_process). Branch `loop/m1-pty`, PR #21, squash-merged as `8e58f7d`.
- Implementation (sol worker, high effort, diff-gated +661/−84 then fix round +136/−19): openpty before spawn; child pre_exec setsid + TIOCSCTTY + dup2 slave onto fds 0/1/2 (no process_group(0) on the pty branch — setsid preserves pgid==pid for cancel/pause/kill); slave ECHO cleared only; single nonblocking close-on-exec master shared via Arc<File> between a poll(50ms) drain thread and the existing stdin queue; output always stream "stdout" (stderr inherently merged); redaction/chunking/metadata unchanged. Swift IPCClient gained `pty: Bool = false`, serialized only when true.
- ADR-007 gate: red evidence (5 pty tests fail vs main; partial-line test fails vs pre-fix code), cargo 15 lib + 13 health + 39 lifecycle green, `verify-m0.sh` exit 0 pre- and post-fix (21 Swift tests), runtime demos (tty detection with CRLF merge, echo-off input round trip with natural VEOF exit, redaction+metadata order, SIGTERM cancel, partial-line close → exit 0 in 0.55s). Evidence: `.loop/verification/m1/pty/`.
- Independent sol review: 0 blockers, 2 majors, 2 minors. Fixed: partial-line VEOF (queue tracks written tail, appends double VEOF), pty timeout + cancel metadata-order tests, Swift wire-format pinning. Accepted/deferred: drain-thread-spawn failure yielding zero terminal frames — same pre-existing class as the pipe path; goes to the durability slice.
- Closeout: traceability S10-001 PTY portion recorded; state.json cycle 13.
- Next cycle: waiting-input detection (S10-009, now unlocked by PTY) or durable run state.

## Cycle 14 — 2026-07-18

- Slice: M1 waiting-input detection (S10-009). Branch `loop/m1-input-wait`, PR #23, squash-merged as `1ad2785`.
- Implementation (sol worker, high effort, diff-gated +622/−21; fix round +247/−23): opt-in `input_wait_detect_milliseconds` on start_process; the backend emits at most one `run_input_waiting` frame per quiet episode when output has been idle past the threshold, the stdin queue is empty/not mid-write, and the child's CPU time advanced <30ms over the window (proc_pidinfo PROC_PIDTASKINFO, mach-timebase converted); activity resets episodes; never after a terminal path; pipe + PTY. Swift: `inputWaitDetectMilliseconds` parameter and a `ProcessEvent.inputWaiting` decode case (orchestrator addition after the worker flagged the decode gap).
- ADR-007 gate: red evidence (3 positive/validation tests fail vs main; paused-suppression test fails vs pre-fix code), cargo 15+13+46 green, `verify-m0.sh` exit 0 pre- and post-fix (24 Swift tests; first run caught a swift-format layout error, fixed), runtime demo (waiting frame at 502ms of a 500ms threshold, two quiet episodes across delivered input, none after run_exit, clean shutdown). Evidence: `.loop/verification/m1/input-wait/`.
- Independent sol review: 1 blocker (supervision tick blocked on the writer mutex under client backpressure — fixed with try_lock + skip-tick; adjacent `?`-propagation defect fixed in the same pass), 1 major (paused runs emitted spurious waiting frames — suppressed while paused, rebaselined on resume), 1 minor (busy-silent test machine-dependent — wall-clock bound now). All confirmed and fixed pre-merge; regression tests added for the blocker and major.
- Closeout: S10-009 → implemented; S10-001 narrowed to worktree + durable cleanup; state.json cycle 14.
- Next cycle: durable run state or temp/worktree cleanup (S10-001 remainder).

## Cycle 15 — 2026-07-18
- Delivered M1 worktree isolation + cleanup (S10-001 remainder): opt-in `worktree_repository` on start_process; private 0700 worktree under a canonicalized managed temp root on a generated `capture-delegate/run-<sanitized-id>-<nonce>` branch; `worktree_path`/`worktree_branch` in run_metadata; terminal-first synchronous cleanup (remove --force → scoped remove_dir_all → prune → branch -D) with an idempotent drop guard; runtime failures emit `worktree_failed`; Swift `worktreeRepository` + `.worktreeFailed`.
- Routing: 1 sol implementation worker (+666/−10, 4-file allowlist, diff-gated), 1 sol read-only reviewer, 1 sol fix worker (+333/−48, 2-file allowlist). Orchestrator fixed a macOS `/var`→`/private/var` test comparison and one swift-format violation directly.
- Review outcomes: confirmed+fixed — shutdown-vs-cleanup exit race (cleanup-blocker guard on the run registry), setup-time path canonicalization + symlink-safe fallback deletion, bounded kill-on-deadline git runner (repository hooks can no longer wedge supervision), branch-ownership preflight before `branch -D`; plus nonce-collision test pinning, timeout/spawn-failure cleanup tests, 0700 assertion. Dispositioned: write-timeout wedge and zero-terminal-frame error paths (pre-existing, deferred durability slice), disconnect bounded by mandatory timeout, old-backend silent field-ignore (pty precedent).
- Evidence: `.loop/verification/m1/worktree/` (red, pre/post-fix suites, parity ×2 exit 0, runtime demos ×2, branch diff, verification.txt). PR #25 squash-merged `2a5406b`. Tests now 18 lib + 13 health_socket + 54 process_lifecycle Rust; 24 Swift.

## Cycle 16 — 2026-07-18
- Delivered M1 run-termination durability (S10-001 durability remainder), retiring two dispositioned review-debt classes: the non-reading-client write wedge (blocker class from PR #25 review) and zero-terminal-frame post-admission failures (major class from PRs #21/#25 reviews).
- Design: bounded 5s post-admission write timeout + torn-frame-safe `ClientWriter` (permanent dead-client flag checked before and after the mutex; no byte ever follows a torn frame; client death does not cancel the run); one-shot `RunTerminal` emitter routes every post-admission failure to a single best-effort `run_exit` with new additive `internal_error` code, preserving id-release → run_exit → worktree-cleanup → blocker-release; centralized post-spawn teardown joins both drains on every error path. Swift `.internalError`.
- Routing: 1 sol implementation worker (+494/−166, 4-file allowlist, diff-gated), 1 sol read-only reviewer, 1 sol fix worker (+183/−28, 2-file allowlist, mutation-checked test strengthenings).
- Review outcomes: 0 blockers; 1 major confirmed and fixed (detached drains could append run_output after run_exit on error paths); 3 test-soundness minors fixed (concurrent-writer torn-frame case, one-shot guard case, exact slow-reader output reconstruction).
- Evidence: `.loop/verification/m1/durability/` (red wedge failure vs main, pre/post-fix suites, parity ×2 exit 0, runtime demos ×2, verification.txt). PR #27 squash-merged `36d3705`. Tests now 22 lib + 13 health_socket + 56 process_lifecycle Rust; 25 Swift.

## Cycle 17 — 2026-07-18

- Recovered an orphaned cycle-17 lock only after its owner had exited and the heartbeat was stale; preserved the prior lock as `.loop/LOCK.orphaned.1784414509` and completed the already bounded four-file resource-safety slice.
- Delivered the final M1 S10-001/S10-011 remainder through PR #29, squash-merged as `6f7d647`: an atomic 1,048,576-byte pending-stdin limit with typed `capacity_exhausted`, no partial admission, closed-state precedence, and immediate same-control-connection retry/close; Swift `.capacityExhausted` decoding; private `.owners` PID sidecars; startup cleanup of only provably dead-owner direct `run-*` worktrees; bounded authenticated Git cleanup and guarded local fallback.
- TDD/runtime evidence: both new behaviors failed against archived `origin/main` `7a0869b`; the real paused-`cat` cap/retry/close flow, real startup orphan cleanup, and forged-repository-metadata regression are green. Fresh `scripts/verify-m0.sh` is green at 26 Rust lib + 13 health/socket + 58 lifecycle + 25 Swift tests, with release builds, lint, health handshake, and traceability validation passing.
- Review: Opus 4.8 cross-family PASS with no blocker/major. The first independent Sol final review found one confirmed MAJOR: a forged `.git` pointer could select a stale worktree admin entry in a foreign repository and delete its managed-prefix branch. Fixed with reciprocal canonical backlink authentication and a red-before/green-after cross-repository regression. Final Sol disposition review: PASS, prior MAJOR resolved, no blocker/major remains.
- Evidence: `.loop/verification/m1/resource-safety/` (red log, three focused green logs, full parity log, review chronology, verification summary). M1 is complete; M2 Domain/persistence is next and has not started.
- User checkpoint: autonomous cycles are paused after this closeout. Resume only on an explicit user request after hands-on testing.

## Cycle 18 — 2026-08-31 (crashed)

- The firing acquired the lock, added the rusqlite 0.40.2 (bundled) dependency for M2, and died (pid 58547) before any further work, leaving a dirty tree and a stale-hearted lock. No PR, no ledger entry of its own.

## Cycle 19 — 2026-08-31

- Context: the user explicitly resumed the loop after the M1 hands-on checkpoint and authorized taking over cycle 18's dead lock (ADR-006 evidence bar met: owner pid gone, heartbeat never refreshed, no activity). Cycle 18's dirty tree was discarded; its rusqlite decision was kept. Separately this day, the repo was made public: README rewritten (PR #32), a desktop screenshot redacted, and git-filter-repo purged the original blob — all pre-rewrite commit SHAs recorded in cycles 0–17 no longer resolve (PR numbers remain valid).
- Delivered M2 slice 1 (S05-001/S05-002 partial) on `loop/m2-persistence`, PR #33, squash-merged as `763d62d`: durable SQLite session store (WAL, foreign keys, busy timeout, forward-only `user_version` migrations inside BEGIN IMMEDIATE; 0600 store file, 0700 parent only when the store created it) with additive `create_session`/`list_sessions` protocol messages, byte- and count-bounded list responses (limit 50, limit+1 fetch, explicit `truncated` flag, pop-until-fits under the 8 KiB frame bound), `--store` CLI plumbing with a HOME default, Swift `Session`/`SessionListPage` client surface, restart-survival E2E, and `--store` isolation for every existing test suite and the health-handshake script.
- Routing: 1 opus-high implementation worker (diff-gated), Fable orchestrator fixes, 3 sol-xhigh read-only review rounds (`codex exec`; `codex review --base` cannot take a prompt).
- Review outcomes, all red-first: round 1 — BLOCKER newer-version stores were chmod/WAL-mutated before rejection (version check moved ahead of all mutations); MAJOR escape-heavy titles could persist yet exceed the response frame bound (draft → bound-check → insert ordering); MAJOR verify-m0's handshake script used the real default store and polluted `~/Library/Application Support/CaptureDelegate` (now passes `--store`; polluted files removed, pre-existing `sessions` dir untouched). Round 2 — BLOCKER version-check→WAL-conversion TOCTOU (advisory flock on a 0600 `<store>.lock` sidecar held across the whole open sequence; flocking the store file itself collides with SQLite's POSIX locks on macOS and failed 5 tests with DatabaseBusy — recorded honestly in red-evidence.log); MAJOR unconditional parent chmod 0700 (now only when created); MINOR zsh NOMATCH cleanup glob (`rm -rf` of the private runtime dir). Round 3 — PASS, all findings resolved.
- Gates at merge: 109 Rust tests (31 lib + 13 health_socket + 7 persistence + 58 process_lifecycle), 28 Swift tests, cargo fmt, clippy -D warnings, `verify-m0.sh` exit 0 (×3 across rounds), user store verified untouched before/after. Evidence: `.loop/verification/m2-persistence/`.
- Open items: GitHub support purge of the cached pre-rewrite screenshot blob is pending; PR #31 (older parallel lineage) still awaits the user's verdict; sibling worktrees must fetch/reset onto the rewritten history before pushing.
- Next cycle: continue M2 — typed session kinds (S05-001), source references (S05-003), or run persistence (S05-008), whichever forms the smallest coherent vertical slice.

## Cycle 20 — 2026-08-31

- Delivered M2 slice 2 (S05-001 complete) on `loop/m2-session-kinds`, PR #35, squash-merged as `3690564`: optional typed session kinds. Migration 2 (`ALTER TABLE sessions ADD COLUMN kind TEXT`, nullable) is the first real forward migration — a hand-built v1 store migrates to user_version 2 on open with legacy rows keeping NULL kind, proven by a unit test and a real-release-binary runtime demo. `create_session` accepts an optional case-sensitive kind from the spec §5 set (meeting, conversation, presentation, pair_work, personal_note, imported_audio); absence means uncategorized per §18.4 ("Auto" is UI-only and rejected as a wire value); responses carry kind only when set; Swift `Session.kind: String?` with the request key omitted when nil.
- Routing: 1 opus-high implementation worker (native Task subagent, diff-gated +239/−17 within a 4-file allowlist), Fable red contract tests + a worker-flagged test-soundness fix (the version-bump store builder now applies all MIGRATIONS), 2 sol-xhigh review rounds.
- Review: round 1 found one MAJOR — serde collapsed `"kind": null` to an absent field, bypassing the allowlist. Fixed red-first with a double-Option `deserialize_present` so null stays distinguishable from omission and is rejected. Round 2: PASS, resolved, no further findings.
- Gates at merge: 112 Rust tests (32 lib + 13 health_socket + 9 persistence + 58 process_lifecycle), 31 Swift tests, fmt, clippy -D warnings, verify-m0.sh exit 0 (×2), user store untouched. Evidence: `.loop/verification/m2-session-kinds/`.
- Traceability: S05-001 → verified (PRs #33 + #35). Deferred, flagged for later slices: Swift typed kind enum, session update/rename protocol, S18-SETUP-04 purpose picker UI (M11).
- Next cycle: continue M2 — source references (S05-003) or run persistence (S05-008), whichever yields the smaller coherent vertical slice.

## Cycle 21 — 2026-09-01

- Delivered M2 slice 3 (S05-003) on `loop/m2-source-refs`, PR #37, squash-merged as `d926c62`: persisted source references per spec §5 — migration 3 (sources table `REFERENCES sessions` + chronological index), `add_source`/`list_sources` protocol, Swift `Source`/`SourceListPage`. Validation order extends the slice-1 invariant: fields (`invalid_add_source`) → session existence (`unknown_session`) → draft → frame-bound check → insert. start/end are non-negative integer milliseconds via the serde_json::Value precedent (floats/negatives get error frames, not dropped connections); speaker mirrors the kind convention (optional, omitted when unattributed, explicit null rejected, ≤256B); text trimmed ≤4096B. Chronological pages pop from the END under the 8 KiB budget so the session's beginning survives.
- Routing: 1 opus-high implementation worker (native Task subagent, diff-gated +804/−4 within a 4-file allowlist), Fable red contract tests (3, red vs main 9a931c4), 1 sol-xhigh review round.
- Review: PASS with 3 MINORs — bounded-page byte-path coverage gap (fixed pre-merge: pop-from-end now pinned), shared-Request-envelope additivity and permissive Swift decode (both deferred and recorded in verification.txt for a protocol-hardening slice).
- Gates at merge: 116 Rust tests (33 lib + 13 health_socket + 12 persistence + 58 process_lifecycle), 35 Swift tests, fmt, clippy -D warnings, verify-m0.sh exit 0 ×2, user store untouched. Real-binary demo: v2→v3 migration on a legacy store, out-of-order insertion, speaker omission, rejections, restart survival. Evidence: `.loop/verification/m2-source-refs/`.
- Traceability: S05-003 → verified (PR #37). Deferred flags: no ON DELETE CASCADE yet; unknown_session conflates missing/nonexistent on list; ~50-byte escape-heavy margin sensitive to id-format changes.
- Next cycle: run persistence (S05-008) split into small slices, or session attachments/markers, whichever is the smaller coherent vertical.

## Cycle 22 — 2026-09-01

- Delivered M2 slice 4 (S05-008 partial, S01-005 groundwork) on `loop/m2-run-records`, PR #39, squash-merged as `35ab350`: durable run records. Migration 4 adds a `runs` table (surrogate PK because client-chosen run_ids are reusable; optional `session_id REFERENCES sessions`); `start_process` validates the double-Option session link (omitted = unlinked, null = invalid_start_process, unknown = unknown_session), inserts the record at admission after the duplicate_run_id gate so no failure path leaves an unclosable row, and closes it at-most-once through RunTerminal's one-shot guard before the run_exit frame; a startup sweep marks dangling `running` rows `interrupted` and is fatal on failure; `list_runs` pages newest-first under the 8 KiB budget; Swift `RunRecord`/`RunListPage`.
- The hard-won invariant (ADR-009): one backend per store, enforced by a lifetime flock on a `<store>.owner` sidecar keyed by the canonical store path, taken right after open_store; with cross-process contention gone, terminal closes are a single attempt (a retry loop was added in round 1 remediation, then deliberately removed — it could stall run supervision ~15s).
- Routing: 1 opus-high implementation worker (native Task subagent, diff-gated within allowlist), Fable red contract tests + review remediation, 5 sol-xhigh review rounds. A worker correction mattered: run_metadata is emitted post-reap, so the interrupted-run test's admission barrier polls list_runs for "running" instead.
- Review (all fixes red-first, reds in red-evidence.log): round 1 FAIL — sweep ordered before the duplicate-launch liveness gate and non-fatal, droppable terminal close, untested list_runs bounds. Round 2 FAIL — bind proves socket ownership, not store ownership (a second backend on another socket could sweep a live backend's runs); retry loop violates the supervision bound. Round 3 FAIL — lexical lock keying let a symlink alias mint its own owner lock (MAJOR, fixed via canonicalize) + stderr-taxonomy and orphaned-worker MINORs. Round 4 FAIL — a single MINOR claiming macOS `pkill -P` requires a pattern; refuted empirically (exit 0, child killed), reap hardened anyway with an explicit pattern + asserted success. Round 5 — PASS, zero findings, Sol retracted the pkill claim against Apple's pkill source.
- Gates at merge: 123 Rust tests (34 lib + 13 health_socket + 18 persistence + 58 process_lifecycle), 37 Swift tests, fmt, clippy -D warnings, verify-m0.sh exit 0 (r3/r4/r5 logs), user store untouched. Real-binary demo: v3→v4 migration, linked exit persisted, live "running" page, SIGKILL→interrupted with history intact. Evidence: `.loop/verification/m2-run-records/`.
- Traceability: S05-008 → in_progress (permissions/environment/events/cost/artifacts still pending); S05-002 evidence updated (sessions, sources, runs all durable). Recorded bounds: refusals before admission persist no record; lexically-keyed open-time `.lock`; hard-link aliases; Swift startProcess lacks the session-link parameter.
- Next cycle: continue M2 — run events/artifacts (S05-008 continuation) or session markers/attachments (S05-002), whichever forms the smaller coherent vertical slice.

## Cycle 23 — 2026-09-01

- Delivered M2 slice 5 (S05-002 partial: markers) on `loop/m2-markers`, PR #41, squash-merged as `4243901`: persisted user markers per spec §5/§6.3. Migration 5 adds a `markers` table (chronological index); `add_marker` requires a case-sensitive kind from the §6.3 set (important, decision, action, question, delegate, research — a marker IS its intent, unlike the optional session kind), validates at_ms as whole non-negative milliseconds and an optional speaker-convention note (omitted ≠ null, ≤4096B), in the fields → unknown_session → draft → frame-bound → insert order; `list_markers` pages chronologically under the 8 KiB budget popping from the end; Swift `Marker`/`MarkerListPage` with client tests.
- Routing: 1 opus-high implementation worker (native Task subagent, diff-gated +543/−1 within a 4-file allowlist, first-pass green on my 3 red contract tests), Fable red tests + review remediation, 2 sol-xhigh review rounds.
- Review round 1 FAIL, MAJOR fixed red-first and broadened: admission bound-checked the add/create response envelope, but a single-item list envelope is ~20 bytes larger — a boundary-sized escape-heavy record could persist yet be permanently unlistable (the byte budget pops even a lone record). The same latent gap existed in the MERGED sources and sessions paths; all three now probe the single-item list envelope at admission ("accepted implies listable alone"), pinned by three boundary-sweep tests kept under the 8 KiB request bound. Two MINORs were re-recorded cycle-21 deferrals (typed-envelope additivity, permissive Swift decode). Round 2 PASS; the reviewer's own MINOR — list_runs keeps the listability gap because run records grow terminal fields after admission — is deferred to the next run-records slice and recorded.
- Gates at merge: 130 Rust tests (35 unit + 13 health_socket + 24 persistence + 58 process_lifecycle), 41 Swift tests, fmt, clippy -D warnings, verify-m0.sh exit 0 ×2, user store untouched. Real-binary demo: hand-built v4 store migrates to v5 with the legacy session intact, markers attach to the pre-migration row, rejections, restart survival, 0600 hygiene incl. owner sidecar. Evidence: `.loop/verification/m2-markers/`.
- Traceability: S05-002 evidence updated (markers durable, PR #41). Carried bounds: runs listability probe (next runs slice), no ON DELETE CASCADE, client-hardening batch.
- Next cycle: run listability probe + run events/artifacts (S05-008 continuation), or session note/attachments (S05-002), whichever is the smaller coherent vertical slice.

## Cycle 24 — 2026-09-01

- Delivered M2 slice 6 (S05-008 hardening) on `loop/m2-run-listability`, PR #43, squash-merged as `6c18442`: closed the runs-listability gap deferred in PR #41 round 2, completing the "accepted implies listable alone" invariant across all four persisted record types. Run records grow at termination, so `start_process` now probes the single-item `list_runs` frame built from the real draft (identical generated id — no id-length race) with worst-case terminal fields substituted: status `interrupted`, exit_code `i32::MIN`, `capacity_exhausted` as the longest protocol error code for headroom, and `ended_at_ms` at `i64::MAX` (termination resamples the wall clock, which a forward clock step can widen past the start's width). Rejection answers `invalid_start_process` before insert, releasing the run-id registration and process slot.
- Routing: Fable-direct (~70 lines with the red test; review-remediation-class work below the delegation threshold), 3 sol-xhigh review rounds.
- Review: round 1 FAIL — probe timestamp not worst-case (fixed: i64::MAX; no black-box red test possible without clock control, correctness static) and clean-exit-only sweep (fixed: both terminal shapes swept per size, including persisted spawn_failed). Round 2 FAIL — 18-byte sweep steps could jump the ~10-byte error-code-budget window; fixed with plain single-byte run_id padding for exact one-byte granularity, mutant-proofed (removing the probe's error_code budget fails the sweep). Round 3 PASS, zero findings.
- Gates at merge: 131 Rust tests (35 unit + 13 health_socket + 25 persistence + 58 process_lifecycle), 41 Swift tests, fmt, clippy -D warnings, verify-m0.sh exit 0 ×3, user store untouched. Real-binary demo: normal run heads the page post-exit; 1329-NUL run_id rejected at admission with zero records left. Evidence: `.loop/verification/m2-run-listability/`.
- Next cycle: run events/artifacts (S05-008 continuation) or session note/attachments (S05-002), whichever forms the smaller coherent vertical slice.

## Cycle 25 — 2026-09-01

- Delivered M2 slice 7 (S05-002: per-session human note, spec §5) on `loop/m2-session-note`, PR #45, squash-merged as `23ec9f4`: the protocol's first update-class message. Migration 6 adds a nullable `note` column; `set_session_note` diverges deliberately from the add-path double-Option convention — omitted note is `invalid_set_session_note`, explicit null CLEARS (an update needs an erase), string is trimmed non-blank ≤4096B — ordering fields → `unknown_session` (get_session doubles as existence check) → single-item list-envelope probe of the would-be record → UPDATE with fresh `updated_at_ms` → reply from the store-returned row. Swift `Session.note` + `setSessionNote` where nil emits explicit JSON null.
- Routing: 1 opus-high implementation worker (native Task subagent, first-pass green within its 4-file allowlist; correctly flagged two defects in my red test — a 1400-NUL note exceeding the request frame cap and a fmt violation — instead of touching the do-not-modify file), Fable red contract tests + remediation, 2 sol-xhigh review rounds.
- Review: round 1 FAIL, one MAJOR — CONFIRMED: the admission probe reused the fetched row's `updated_at_ms` while the write stamps a fresh one; a narrow legacy stamp (the v5 fixture stores `7`) let an admitted update delist its session. My first repro passed misleadingly — sweeping 53 notes on one session lets the first accepted note restamp 13 digits and heal the narrowing — red only once the test re-narrowed via sqlite3 before every step (delisted at nulls=1330). Fixed in `ba4927e`: probe clone carries `updated_at_ms = i64::MAX` (serialized ceiling, runs-probe precedent; also covers a forward clock step between probe and write). Round 2 PASS, zero findings.
- Gates at merge: 136 Rust tests (36 unit + 13 health_socket + 29 persistence + 58 process_lifecycle), 44 Swift tests, fmt, clippy -D warnings, verify-m0.sh exit 0 (rounds logged), user store untouched. Real-binary demo: v5→v6 migration on a hand-built legacy store, note set on the pre-migration row, restart survival, explicit-null clear, rejections, 0600 hygiene. Evidence: `.loop/verification/m2-session-note/`.
- Traceability: S05-002 evidence += PR #45 (note durable; transcript/speakers/attachments/actions/projects pending). Recorded bounds: `session_exists`/`get_session` coexistence and the non-transactional two-lock update re-read — batched for a store-hardening slice.
- Next cycle: run events/artifacts (S05-008 continuation) or session attachments (S05-002), whichever forms the smaller coherent vertical slice.

## Cycle 26 — 2026-09-01

- Delivered M2 slice 8 (S05-002: transcript segments, spec §5) on `loop/m2-transcript`, PR #47, squash-merged as `b27c11a`: migration 7 adds `transcript_segments` (session-scoped spans with optional speaker, required bounded text); `add_transcript_segment`/`list_transcript` mirror the source-reference contract exactly — same validation order, `unknown_session` split, single-item list-envelope admission probe before insert, page limit 50 with limit+1 truncation probe and byte budget popping from the page end. Swift `TranscriptSegment`/`TranscriptPage` + client methods.
- Routing: 1 opus-high implementation worker (native Task subagent, first-pass green within its 4-file allowlist, diff-gated at 848 lines), Fable red contract tests + runtime demo, 1 sol-xhigh review round.
- Review: round 1 PASS, zero findings — first slice to clear review clean on the first round; the sources contract precedent plus three cycles of listability hardening left nothing to find.
- Gates at merge: 141 Rust tests (38 unit + 13 health_socket + 32 persistence + 58 process_lifecycle), 48 Swift tests, fmt, clippy -D warnings, verify-m0.sh exit 0, user store untouched. Real-binary demo: hand-built v6 store migrated 6→7, segments on the pre-migration session, inverted-span/unknown-session rejections, chronological listing after restart, 0600 hygiene. Evidence: `.loop/verification/m2-transcript/`.
- Traceability: S05-002 evidence += PR #47 (transcript + speakers durable; attachments/actions/projects pending). Recorded bound: index name `transcript_by_session_and_start` breaks the `<table>_by_*` prefix convention (rename needs a migration; cosmetic).
- Next cycle: session attachments (S05-002) or run events (S05-008), whichever forms the smaller coherent vertical slice.

## Cycle 27 — 2026-09-01

- Delivered M2 slice 9 (S05-008: run events, spec §5/§10/§14) on `loop/m2-run-events`, PR #49, squash-merged as `7b34e65`: migration 8 adds backend-authored `run_events` — seq-0 `launched` in the same transaction as run admission, seq-1 terminal event in the same transaction as the one-shot terminal close (kind = `error_code` else `exited`), startup sweep closes dangling trails with `interrupted`. `list_run_events` pages the trail with the double-Option `record_id`, `unknown_run` split, limit-51 truncation probe, and end-popping byte budget. Swift `RunEvent`/`RunEventPage`.
- Events carry no client-controlled strings, so the admission-probe invariant doesn't apply; Sol independently computed the worst-case 50-event frame (~11 KiB) and confirmed the byte-budget pop engages (down to 36 events) with correct truncation.
- Routing: 1 opus-high implementation worker (native Task subagent, first-pass green within its 4-file allowlist, diff-gated at 766 lines), Fable red contract tests + runtime demo, 1 sol-xhigh review round: PASS, zero findings (second consecutive clean round 1).
- Worker's key judgment call, kept and recorded: my dispatch said terminal-event kind is "the terminal status written to the row", but finish_run writes status `exited` for every self-closed run and distinguishes causes via error_code — a literal reading would have failed the red test, so kind maps `error_code.unwrap_or("exited")`.
- Gates at merge: 148 Rust tests (42 unit + 13 health_socket + 35 persistence + 58 process_lifecycle), 50 Swift tests, fmt, clippy -D warnings, verify-m0.sh exit 0, user store untouched. Real-binary demo: hand-built v7 store with a legacy run row migrated 7→8, [launched, exited] trail, unknown_run rejection, SIGKILL mid-run → restart sweep closes [launched, interrupted], 0600 hygiene. Evidence: `.loop/verification/m2-run-events/`.
- Traceability: S05-008 evidence += PR #49 (events durable; permissions/environment/cost/artifacts pending). Recorded bounds: pre-migration-8 run rows have no launch event; a second finish_run would double the terminal event (unreachable under RunTerminal + ADR-009).
- Two additional loop firings arrived mid-cycle and were folded into this single bounded cycle (lock heartbeat was fresh; no extra slices started).
- Next cycle: session attachments (S05-002, thin spec grounding — single mention) or run permissions/environment fields (S05-008), whichever forms the smaller coherent vertical.
