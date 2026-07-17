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
