# Full-End-State Roadmap

The chain below is ordered. A milestone may begin only after all listed dependencies have passed their completion gates. This is an end-state roadmap, not an MVP reduction; all requirements remain governed by `traceability.md`.

| Milestone | Depends on | Specification coverage | Deliverables | Completion gate |
|---|---|---|---|---|
| M0 Foundation | — | 1–5, 17, 18.1–18.3; enabling controls for all sections | Swift shell, Rust service, versioned Unix-socket health contract, CI/test/lint scaffold, OSS baseline, and loop controls | Client and Rust service establish the bounded versioned local health handshake; CI parity, OSS baseline, loop controls, and M0 traceability evidence pass review |
| M1 IPC/process lifecycle | M0 | 9–10, 15; 18.13 run controls | Process/PTY supervisor, event stream, run lifecycle, cancellation/pause/resume, cleanup and secret-redacted logs | Controlled child process can be launched, streamed, paused/resumed/cancelled, time-limited, and cleaned up with durable state and tests |
| M2 Domain/persistence | M1 | 5, 8, 12, 14 audit; 13 indexing foundation | Durable Session/Source/Note/Artifact/Action/TaskPacket/Run/Project models, migrations, audit store, source links | Objects, task packets, source spans, audit events, and project links survive restart and have migration plus integrity tests |
| M3 Capture | M2 | 2, 4.2, 6.1, 6.3–6.4, 14 recording transparency; 18.4–18.8 capture | Mic/system/import capture, profiles, markers, recording UI/HUD, privacy controls | Every supported intentional start path displays active capture, persists sources/markers, finalizes safely, and honors privacy/exclusion behavior |
| M4 Transcription/live understanding | M3 | 6.2, 6.4, 15 confidence; 18.8–18.9 transcript | Local live/final transcription, reconciliation, optional diarization, live extraction and source-grounded notes | Incremental transcript and suggestions remain non-authoritative, confidence is visible, and finalization preserves exact source spans |
| M5 Notes/source grounding | M4 | 1, 4.1/4.4/4.8, 5, 6.7, 12, 18.6/18.9, 19.3–19.4 | Editable notes, sources, session detail, artifacts and human-only workflows | Generated note lines expose sources; no-agent capture remains polished and local; approved artifact updates are inspectable |
| M6 Actions/dispatch | M5 | 6.5–6.7, 7.1–7.7, 8, 15, 19.1–19.2 | Action cards, approval paths, task-packet export, authorization/reference/policy flows | All action modes and capability levels display specific permission manifests; lower-trust speech cannot authorize execution; ambiguity blocks auto-run |
| M7 Adapters/runtime | M6 | 9–10, 17 adapters; 19.5 | Codex/Claude/generic/copy adapters, isolated Build worktrees, capability connectors | Adapter contract works across modes; Read is externally enforced; Build uses isolated worktree; Act uses narrow capability grants |
| M8 Projects/artifacts/search/MCP/CLI | M7 | 12–13, 17 open components; 18.12/18.14–18.16 | Project mapping, artifact proposals, search/retrieval, local MCP server and CLI, command palette | Capture never requires a project; all search targets resolve with provenance; documented high-level MCP/CLI functions return scoped objects |
| M9 Authorization/reference resolution/policies/dependencies | M8 | 4.3/4.5, 7.4–7.7, 14, 15, 19.1–19.2 | Trust hierarchy, voice/reference resolution, policy engine, Touch ID sensitive gates, dependent actions | Authorization is auditable, reference ambiguity prompts selection, policy changes need UI confirmation, and dependent work waits correctly |
| M10 Reliability/notifications/onboarding/settings/security | M9 | 11, 14–15, 18.17–18.19 | Offline/cloud handoff, recovery states, notifications, onboarding, settings, privacy/security controls | Every specified failure path has a safe recovery state; onboarding can skip agents; security/privacy and notification preferences are enforced |
| M11 Complete UI/accessibility/states | M10 | 18.1–18.22 | All five surfaces, queue/run/project views, visual system, accessibility, empty/loading/error states | Screens meet dimensions/interaction behavior, keyboard and VoiceOver coverage, non-color status, reduced motion, contrast, and state QA |
| M12 Cloud | M11 | 4.6, 11, 14 cloud boundary, 17 hosted | Optional encrypted sync, teams, hosted workers, heavy processing, cloud disclosure | Local-first works without account; cloud uploads are explicitly itemized; hosted worker isolation, scoped credentials, budgets, teardown verified |
| M13 Evals/extensions | M12 | 16–17 | Evaluation harness, comparative metrics, team reporting, plugin SDK and scoped plugin grants | Opt-in comparisons measure required metrics, reports avoid unsupported claims, and plugins receive only scoped access |
| M14 Full audit | M13 | 1–21, 19–21 | Traceability closeout, full regression/accessibility/security audit, release evidence | Every ledger row is implemented and verified or explicitly rejected by an approved spec change; all journeys and success safeguards pass; no unauthorized destructive action |

## Global release controls

- M0 is verified: M0-FOUNDATION-001 passed fresh-login native-window re-verification, and PR `#1` remains merged as `3e6cf85`.
- M1 remains active on `loop/m1-run-timeout` after PR #5 merged as `9f40024`. The direct-process/streaming slice is verified, and the bounded per-run timeout slice is locally verified; PR/CI/merge and later M1 lifecycle slices remain pending.
- A single active milestone PR is permitted.
- Each completion gate requires linked implementation evidence, verification evidence, and PR status in the ledger.
- M14 may close only against the full specification, including positioning and qualitative success criteria.
