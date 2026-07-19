# Architecture Decision Records

## ADR-001 — Native macOS presentation layer

**Status:** accepted, 2026-07-17
**Decision:** Build the macOS client with SwiftUI, using AppKit where native lifecycle, menu bar, floating HUD, accessibility, windowing, or system integration requires it.
**Rationale:** The product is macOS-only and requires calm, keyboard-fluent, accessible native surfaces.
**Consequences:** UI work must preserve native macOS behavior; AppKit bridges are allowed where SwiftUI does not provide the required control.

## ADR-002 — Separate local runtime service

**Status:** accepted, 2026-07-17
**Decision:** Run capture, process execution, isolation, and durable local-runtime responsibilities in a separate Rust service.
**Rationale:** Agent child processes, PTYs, worktrees, cleanup, and lifecycle enforcement must survive or be managed independently from UI presentation.
**Consequences:** The frontend remains a client of the service; runtime capability enforcement is not delegated to prompts.

## ADR-003 — Versioned Unix-socket IPC

**Status:** accepted, 2026-07-17
**Decision:** The SwiftUI/AppKit client and Rust service communicate through a versioned Unix-domain socket protocol.
**Rationale:** Local, explicit, evolvable IPC supports process separation and compatibility checks without network exposure.
**Consequences:** Every message and handshake must carry or negotiate a protocol version; incompatible versions fail safely with actionable status.

## ADR-004 — One active milestone PR

**Status:** accepted, 2026-07-17
**Decision:** Only one milestone implementation PR may be active at any time.
**Rationale:** Ordered dependencies and evidence remain reviewable when the loop focuses on a single milestone.
**Consequences:** Later milestone work stays blocked until the active milestone completion gate and PR lifecycle are resolved.

## ADR-005 — Full-spec traceability is the completion authority

**Status:** accepted, 2026-07-17
**Decision:** The end-state specification, represented by stable rows in `traceability.md`, is authoritative for completion; milestone labels do not reduce scope.
**Rationale:** The supplied specification explicitly describes the full product, not an MVP.
**Consequences:** A requirement is complete only when its ledger row has implementation and verification evidence and an accepted PR reference where applicable.

## ADR-006 — Orphaned loop locks may be broken before the 90-minute staleness window

**Status:** accepted, 2026-07-18
**Decision:** A `.loop/LOCK` may be taken over before its heartbeat reaches the 90-minute staleness threshold only when there is affirmative evidence the owning session is dead: the recorded owner process no longer exists, no agent session is active for it, the heartbeat has never refreshed since acquisition, and no file or commit activity has occurred since. The takeover must preserve the old lock file (renamed with an `.orphaned.<epoch>` suffix) and be recorded in `progress.md`.
**Rationale:** Cycle 6 found a lock left by a session that died mid-cycle-5; waiting for nominal staleness would have idled four consecutive firings with zero safety benefit, since the evidence bar rules out a live competing writer.
**Consequences:** Future firings apply the evidence checklist instead of blind timeout-waiting; absent that evidence, the 90-minute rule remains binding.

## ADR-007 — CI retired; merge gate is local verification plus independent Codex review

**Status:** accepted, 2026-07-18 (explicit user directive)
**Decision:** GitHub Actions is permanently unavailable on this account (the user will not enable billing). The "CI green" merge gate is replaced by: (1) a full execution of `scripts/verify-m0.sh` directly on the dev machine, with the log saved into the slice's `.loop/verification/` directory, and (2) an independent Codex gpt-5.6-sol review with every confirmed blocker resolved before merge. The verify-m0 workflow file remains in the repo for possible future re-enable; its unstartable check runs are not a merge signal.
**Rationale:** On a free-plan private repo, Actions jobs cannot start (billing) and branch protection is unavailable, so CI can neither run nor gate; twenty-nine consecutive rerun probes returned the identical billing annotation. The parity script executes the same steps the CI job ran, on the same machine that hosts all runtime verification. The user directed this substitution on 2026-07-18.
**Consequences:** Every future merge must attach a fresh local parity log as evidence. The Codex sandbox cannot bind Unix sockets, so test execution stays on the dev machine while sol provides the independent review. "Never merge red CI" is henceforth interpreted as "never merge without a green local parity run and a clear independent review."

## ADR-008 — UX-first vertical slices and mandatory user checkpoints

**Status:** accepted, 2026-07-18 (explicit user directive)
**Decision:** Insert M1.5 First Capture Journey after the completed M1 and make visible vertical slices the delivery unit. M1.5 must ship the native app shell, real microphone capture with pause/resume/stop, minimum Keychain-backed encrypted local persistence, Today/Moments/session detail/playback, and relaunch durability. Later domain/backend work must ship with its usable app surface; M11 is reserved for cohesion, accessibility, and polish. Every UI-bearing slice requires an Opus-authored UX contract, Opus implementation of user-facing SwiftUI, real Computer Use on the packaged `.app`, a fresh independent Opus visual review, independent gpt-5.6-sol technical review, retained UX evidence, and explicit pre-merge user approval.
**Rationale:** Backend-first sequencing deferred the experience users must evaluate and allowed internal completion signals to stand in for product quality. The user directed the loop to prove the capture journey early and to make every later capability visible, real-data-backed, and reviewable before merge.
**Consequences:** UX evidence is retained at `.loop/verification/<milestone>/<slice>/ux/` with screenshots, AX trees, scenario log, real-data/backend provenance, and Opus review. Process/window-count smoke, unit tests, previews, and mocks cannot satisfy product verification. UI PRs pause with `user_verdict: pending`; rejection reopens the same milestone, while Opus or Computer Use unavailability blocks rather than downgrades. Internal-only fixes do not count as product progress. ADR-007 local parity and Sol review remain mandatory; GitHub Actions remains retired.

## ADR-009 — M1.5 review-finding deferrals

**Status:** accepted, 2026-07-19 (cycle 18)
**Decision:** From the M1.5 first-capture reviews, the following confirmed findings are deferred rather than fixed in this slice, each with a designated landing spot: (1) AES-GCM associated-data binding of ciphertext to session UUID/file role, plus a versioned on-disk format and migration — M2 Domain/persistence (changing the format now would invalidate existing sessions without a migration story); (2) per-root store locking across multiple store instances — M2 (the app runs one store per process today); (3) Developer ID signing, hardened runtime, and notarization — blocked on the user providing a signing identity; ad-hoc signing remains for local testing with the known consequence that each rebuild re-prompts TCC; (4) universal (Intel) binary packaging — deferred until distribution matters; the dev/test machine is arm64; (5) moving AppModel coordination behind injectable protocols into a testable target, and an XCUITest target (F2) — M2; Computer Use remains the UI verification mechanism; (6) runtime socket path convention under `/tmp` (F1) — revisit before multi-user support; (7) a rebindable capture shortcut resolving the ⌥Space/Raycast collision (F6) — M3 capture-surface work.
**Rationale:** Each is real but either requires user-held resources (signing identity), a format-versioning story that belongs to the persistence milestone, or scope that would swamp the slice while the user is waiting to test the visible journey.
**Consequences:** These items are tracked here rather than silently dropped; M2's plan must pick up (1), (2), and (5) explicitly, and the M1.5 PR description must list them as known limitations.

**Addendum (cycle 18, post delta re-review):** Sol's delta re-review confirmed the blocker and majors #1/#5/#7 fixed and accepted the deferrals above, leaving two residuals now also recorded as deliberate checkpoint tradeoffs: (8) on termination with an unresolved save failure, the private plaintext temp recording is intentionally preserved (0700 directory, owner-only) rather than deleted — it is the only copy of the user's audio; next-launch reconciliation must never treat such a pending-owner file as orphaned, and the M2 persistence work should add a durable pending-capture manifest so quit-with-failure round-trips losslessly; (9) persistence-mutation failures surface via reload plus `loadErrorMessage`, currently rendered only on the Today view — M2 UI work should surface store errors on Moments and detail surfaces too. The remaining copy-honesty instances Sol flagged (explainer, Today card, usage description) were fixed in this slice, as was the new minor #15 (capture started during the 400 ms retry-routing window could be navigated away; `isSaving` now holds until routing completes).
