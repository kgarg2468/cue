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
