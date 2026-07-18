# Full-Spec Requirements Traceability Ledger

Status values: `todo`, `in_progress`, `implemented`, `verified`, `blocked`, `rejected`. Evidence must be concrete links, test identifiers, screenshots, logs, or accepted review references; an em dash means none yet. `implemented` and `verified` require implementation evidence; `verified` also requires verification and PR evidence. The parity gate validates these evidence-state rules, the 287 original rows, and the explicit M0 rows. Requirement IDs are stable and never reused.

| Requirement ID | Spec reference | Requirement | Milestone | Implementation evidence | Verification evidence | PR | Status |
|---|---|---|---|---|---|---|---|
| CTL-001 | Loop control | Durable control state records cycle, active milestone, branch, job cadence, expiry, blockers, and summary | M0 | `.loop/state.json` | `python3 -m json.tool .loop/state.json`; PR #1 merged as `3e6cf85`; both CI runs passed | PR #1 (`3e6cf85`) | verified |
| CTL-002 | Loop control | Full end-state roadmap and completion authority are durable | M0 | `.loop/roadmap.md`; `.loop/decisions.md` (ADR-005) | `rg -q 'M14 Full audit' .loop/roadmap.md && rg -q 'ADR-005' .loop/decisions.md`; PR #1 merged as `3e6cf85`; both CI runs passed | PR #1 (`3e6cf85`) | verified |
| M0-FOUNDATION-001 | Approved M0 plan | Swift macOS application and IPC client shell | M0 | `Package.swift`; `Sources/CaptureDelegateApp/`; `Sources/CaptureDelegateIPC/` (PR #1, `3e6cf85`) | After fresh loginwindow at 2026-07-17 18:57:46 local, `scripts/verify-app-launch.sh` passed 10/10 consecutive runs; retained CoreGraphics screenshot `.loop/verification/m0/fresh-login/app.png` shows a nonblank native window with centered “Capture → Delegate”. The prior blocker was session-specific; no product source changed. | PR #1 (`3e6cf85`) | verified |
| M0-FOUNDATION-002 | Approved M0 plan | Rust local backend service shell | M0 | `backend/src/main.rs`; `backend/src/lib.rs` | `scripts/verify-m0.sh` passed locally; PR #1 merged as `3e6cf85`; both CI runs passed | PR #1 (`3e6cf85`) | verified |
| M0-FOUNDATION-003 | Approved M0 plan | Versioned bounded Unix-socket health contract | M0 | `backend/src/lib.rs`; `Sources/CaptureDelegateIPC/IPCClient.swift` | `scripts/verify-m0.sh` passed locally; PR #1 merged as `3e6cf85`; both CI runs passed | PR #1 (`3e6cf85`) | verified |
| M0-FOUNDATION-004 | Approved M0 plan | CI, test, lint, and parity verification scaffold | M0 | `.github/workflows/ci.yml`; `scripts/verify-m0.sh` | `scripts/verify-m0.sh` passed locally; both CI runs passed; PR #1 merged as `3e6cf85` | PR #1 (`3e6cf85`) | verified |
| M0-FOUNDATION-005 | Approved M0 plan | Open-source repository baseline | M0 | `README.md`; `LICENSE`; `.gitignore` | `git check-ignore target .build .loop/LOCK` passed locally; PR #1 merged as `3e6cf85`; both CI runs passed | PR #1 (`3e6cf85`) | verified |
| M0-FOUNDATION-006 | Approved M0 plan | Durable loop controls and traceability authority | M0 | `.loop/state.json`; `.loop/traceability.md`; `.loop/progress.md`; `.loop/roadmap.md`; `.loop/decisions.md` | `python3 -m json.tool .loop/state.json`; `scripts/verify-m0.sh` passed locally; PR #1 merged as `3e6cf85`; both CI runs passed | PR #1 (`3e6cf85`) | verified |
| S01-001 | 1 | Deliver an open-source, local-first macOS app for intentional spoken sessions and actionable traceable handoffs | M0 | — | — | — | todo |
| S01-002 | 1 | Convert capture into readable human notes | M5 | — | — | — | todo |
| S01-003 | 1 | Provide source-grounded project context where useful | M5 | — | — | — | todo |
| S01-004 | 1 | Provide optional action cards for Codex, Claude Code, and other agents | M6 | — | — | — | todo |
| S01-005 | 1 | Preserve link from speech through agent work to user acceptance | M2 | — | — | — | todo |
| S01-006 | 1 | Remain useful as a private polished recording and notes app without agents | M5 | — | — | — | todo |
| S01-007 | 1 | Do not assume sessions are meetings, statements are authoritative, or actions should auto-run | M6 | — | — | — | todo |
| S01-008 | 1 | Support source-grounded note to approved action to task packet to run to result to human decision to project-context flow | M14 | — | — | — | todo |
| S02-INC-01 | 2 Included | Capture intentional macOS audio from microphone, system audio, or both | M3 | — | — | — | todo |
| S02-INC-02 | 2 Included | Provide live and finalized transcription | M4 | — | — | — | todo |
| S02-INC-03 | 2 Included | Capture online meetings without a meeting bot | M3 | — | — | — | todo |
| S02-INC-04 | 2 Included | Capture in-person conversations and presentations through Mac microphone | M3 | — | — | — | todo |
| S02-INC-05 | 2 Included | Support personal voice notes and thinking aloud | M3 | — | — | — | todo |
| S02-INC-06 | 2 Included | Provide human-readable notes | M5 | — | — | — | todo |
| S02-INC-07 | 2 Included | Source-link decisions, ideas, questions, and actions | M5 | — | — | — | todo |
| S02-INC-08 | 2 Included | Support optional project and repository association | M8 | — | — | — | todo |
| S02-INC-09 | 2 Included | Extract actions during and after capture | M4 | — | — | — | todo |
| S02-INC-10 | 2 Included | Provide copyable prompts, interactive Codex/Claude sessions, and autonomous local/hosted execution | M7 | — | — | — | todo |
| S02-INC-11 | 2 Included | Provide Read, Plan, Build, and Act permission levels and user-defined policies | M9 | — | — | — | todo |
| S02-INC-12 | 2 Included | Store local-first with optional cloud sync and hosted execution | M12 | — | — | — | todo |
| S02-INC-13 | 2 Included | Expose agent-neutral task schema and adapter API; open-source client/runtime/schemas/SDK | M13 | — | — | — | todo |
| S02-EXC-01 | 2 Excluded | Do not ship iPhone, iPad, Watch, wearable, or non-macOS clients | M14 | — | — | — | todo |
| S02-EXC-02 | 2 Excluded | Do not permit secret, invisible, or always-on recording | M3 | — | — | — | todo |
| S02-EXC-03 | 2 Excluded | Do not execute automatically solely from another speaker’s words | M9 | — | — | — | todo |
| S02-EXC-04 | 2 Excluded | Do not become a full IDE, general workflow builder, generic company-memory search product, or proprietary coding agent | M14 | — | — | — | todo |
| S02-EXC-05 | 2 Excluded | Do not auto-merge, deploy, destructively operate, or communicate externally without explicit authorization | M9 | — | — | — | todo |
| S03-001 | 3 Personal user | Preserve private speech and produce clean notes, key moments, questions, and reminders without forced project workflow | M5 | — | — | — | todo |
| S03-002 | 3 Hackathon participant | Produce team notes and explicit-command read-only parallel research during in-person discussion | M6 | — | — | — | todo |
| S03-003 | 3 Developer | Produce grounded review/investigation action with PR, objective, source, permission, deliverable and one-click read-only launch | M7 | — | — | — | todo |
| S03-004 | 3 Founder/product lead | Support notes, living briefs, and selected research/implementation tasks | M8 | — | — | — | todo |
| S03-005 | 3 Agent-heavy team | Provide common task format, policies, local/cloud execution, run history, and source-to-output traceability | M12 | — | — | — | todo |
| S04-001 | 4.1 | Notes remain first-class and agent controls are an extension of notes | M5 | — | — | — | todo |
| S04-002 | 4.2 | Start recording only by explicit button, menu action, shortcut, or supported calendar trigger and visibly indicate active state | M3 | — | — | — | todo |
| S04-003 | 4.3 | Treat conversation as context; only authenticated direct command, confirmation, or preconfigured policy authorizes execution | M9 | — | — | — | todo |
| S04-004 | 4.4 | Link important claims, decisions, requirements, constraints, and objectives to inspectable transcript spans/timestamps | M5 | — | — | — | todo |
| S04-005 | 4.5 | Show specific read/change/publish/execute permission for every run; no single autonomous toggle | M9 | — | — | — | todo |
| S04-006 | 4.6 | Perform recording, transcription, storage, and basic extraction locally by default; cloud is optional | M12 | — | — | — | todo |
| S04-007 | 4.7 | Use one provider-neutral internal task format; providers are adapters | M6 | — | — | — | todo |
| S04-008 | 4.8 | Return agent results to originating action, session, and project | M5 | — | — | — | todo |
| S05-001 | 5 Session | Model intentional session types: meeting, conversation, presentation, pair work, personal note, imported audio | M2 | — | — | — | todo |
| S05-002 | 5 Session | Persist session audio sources, transcript, speakers, markers, attachments, note, actions, sources, projects, and runs | M2 | — | — | — | todo |
| S05-003 | 5 Source reference | Persist stable session ID, start/end ms, speaker, and exact source text | M2 | — | — | — | todo |
| S05-004 | 5 Note | Provide editable summaries, decisions, ideas, questions, tasks/custom sections; generated lines expose sources | M5 | — | — | — | todo |
| S05-005 | 5 Artifact | Support optional living artifacts across sessions, edits, and results; do not require artifact updates | M8 | — | — | — | todo |
| S05-006 | 5 Action | Model extracted/manual actions as drafts requiring approval or policy before execution | M6 | — | — | — | todo |
| S05-007 | 5 Task packet | Model structured provider-neutral approved-action packet | M6 | — | — | — | todo |
| S05-008 | 5 Run | Persist agent, permissions, environment, events, cost, status, result, and artifacts for each run | M2 | — | — | — | todo |
| S05-009 | 5 Project | Map optional project names/aliases, folders, repos, agents, documents, artifacts, and policies | M8 | — | — | — | todo |
| S06-001 | 6.1 | Start session from menu bar, main app, shortcut, or calendar prompt | M3 | — | — | — | todo |
| S06-002 | 6.1 | Remember/select mic-only, system-only, both, selected-app when supported, and imported-file audio profiles | M3 | — | — | — | todo |
| S06-003 | 6.1 | Display recording state, elapsed time, active sources, transcript status, privacy mode, action count, and running agents | M3 | — | — | — | todo |
| S06-004 | 6.2 | Incrementally identify important statements, decisions, ideas, questions, commitments, assignments, references, and dispatch commands | M4 | — | — | — | todo |
| S06-005 | 6.2 | Make live extraction suggestions only; never silent authority | M4 | — | — | — | todo |
| S06-006 | 6.3 | Support button, shortcut, and voice markers for important, decision, action, question, Codex send, and Claude read-only research | M3 | — | — | — | todo |
| S06-007 | 6.3 | Treat markers as high-confidence post-processing boundaries | M4 | — | — | — | todo |
| S06-008 | 6.4 | On stop: finalize/reconcile transcript, optionally diarize, extract note/drafts, resolve references, flag ambiguity/contradiction, preserve sources, suggest artifact update | M4 | — | — | — | todo |
| S06-009 | 6.4 | Navigate to Session Detail rather than agent dashboard after capture | M11 | — | — | — | todo |
| S06-010 | 6.5 | Support Save, Copy prompt, Open interactive, Run now, Dismiss, and Edit action approval paths | M6 | — | — | — | todo |
| S06-011 | 6.6 | Package approved actions, start/open agents, stream status, handle clarification/permission, and write back results | M7 | — | — | — | todo |
| S06-012 | 6.7 | Result cards include outcome, evidence/files, artifacts, branch/PR, questions, follow-up, time/cost, requested/denied escalations | M7 | — | — | — | todo |
| S06-013 | 6.7 | Permit accept, follow-up, interactive continuation, rerun, person handoff, new action, artifact attachment, and explained rejection | M7 | — | — | — | todo |
| S07-001 | 7.1 | Support action types research, PR review, repo inspect, plan, document draft, build, tests, bug investigation, issue, review posting, external update, custom | M6 | — | — | — | todo |
| S07-002 | 7.1 | Use taxonomy for defaults without limiting instructions | M6 | — | — | — | todo |
| S07-003 | 7.2 Capture | Capture mode contacts no agent and retains action in notes/list | M6 | — | — | — | todo |
| S07-004 | 7.2 Prepare | Prepare mode creates editable/copyable packet and prompt | M6 | — | — | — | todo |
| S07-005 | 7.2 Open | Open mode launches/resumes interactive packet-preloaded agent that waits before acting | M7 | — | — | — | todo |
| S07-006 | 7.2 Run | Run mode autonomously executes under permission/resource policy | M7 | — | — | — | todo |
| S07-007 | 7.3 Read | Read permits approved inspection, PR/diff/docs analysis and linked context; forbids modifications, comments, branches, external change | M7 | — | — | — | todo |
| S07-008 | 7.3 Plan | Plan adds proposed plans/architecture/work breakdown only as artifacts, not repo changes | M7 | — | — | — | todo |
| S07-009 | 7.3 Build | Build edits isolated worktree/sandbox and produces diff; forbids merge/deploy/publish/primary worktree write absent explicit grant | M7 | — | — | — | todo |
| S07-010 | 7.3 Act | Act permits only separately granted external side effects | M7 | — | — | — | todo |
| S07-011 | 7.4 | Use per-run filesystem/git/GitHub/network/secrets permission manifest and render it in plain language pre-run | M9 | — | — | — | todo |
| S07-012 | 7.5 | Enforce ascending authorization: other speaker, detected action, marker, voice command, confirmation, policy, Touch ID | M9 | — | — | — | todo |
| S07-013 | 7.5 | Prevent lower-trust source from authorizing higher-risk capability | M9 | — | — | — | todo |
| S07-014 | 7.6 | Resolve “that” from selected action, marked action, high-confidence candidate, then ask user if ambiguous | M9 | — | — | — | todo |
| S07-015 | 7.7 | Allow policies to select task type, explicit-command condition, agent, mode, capability, concurrency/time/budget limits | M9 | — | — | — | todo |
| S07-016 | 7.7 | Require explicit UI confirmation to create/change policies; safe policies may auto-run | M9 | — | — | — | todo |
| S08-001 | 8 | Version provider-neutral packet with ID/timestamps, origin/source refs, project/revision, action, execution, included/excluded context | M6 | — | — | — | todo |
| S08-002 | 8 | Packet action defines type/title/objective/scope/constraints/deliverable and execution agent/mode/capability/time/cost | M6 | — | — | — | todo |
| S08-003 | 8 | Export packet as Markdown, JSON, YAML via clipboard, local CLI, local MCP, adapter, and hosted API | M8 | — | — | — | todo |
| S09-001 | 9 | Every integration implements validate, prepare, open, run, stream, clarify, permission, pause/resume/cancel, collect result interface | M7 | — | — | — | todo |
| S09-002 | 9 Codex | Codex opens local interactive, accepts structured context, runs noninteractive where supported, preserves run ID, streams, externally enforces read/worktree | M7 | — | — | — | todo |
| S09-003 | 9 Claude | Claude Code implements equivalent documented CLI/SDK behavior with explicit tool/filesystem permissions | M7 | — | — | — | todo |
| S09-004 | 9 Generic CLI | Permit configured interactive/autonomous command templates and working directory | M7 | — | — | — | todo |
| S09-005 | 9 Copy-only | Render/copy model-specific prompt without local install | M6 | — | — | — | todo |
| S09-006 | 9 Adapter rule | Never scrape/reuse unsupported desktop subscription credentials; use documented session, user API, or organization credentials | M9 | — | — | — | todo |
| S10-001 | 10 | Local helper launches processes/PTYS, worktrees, enforces limits, streams stdout/stderr/events, detects waiting input, redacts logs, records metadata, terminates/culls resources | M1 | `backend/src/lib.rs`; `Sources/CaptureDelegateIPC/IPCClient.swift`; direct-process and event-stream portions implemented on `loop/m1-process-streaming`; PTY, worktree, input-wait, redaction/metadata, explicit lifecycle controls, and durable cleanup remain pending | `.loop/verification/m1/process-streaming/full-verification.txt`; `.loop/verification/m1/process-streaming/reviews.txt` | PR #5 (`9f40024`; both CI runs passed) | in_progress |
| S10-002 | 10 Read-only | Enforce read-only outside prompt using permissions/copy/sandbox/allowlist/no GitHub writes/no primary worktree write where practical | M7 | — | — | — | todo |
| S10-003 | 10 Build | Build uses dedicated worktree/generated branch/explicit writable paths/test/lint commands/no auto-push/final diff+summary | M7 | — | — | — | todo |
| S10-004 | 10 Act | Act uses capability-specific connector and narrow authorization token; review posting never implies merging | M7 | — | — | — | todo |
| S11-001 | 11 | Core recorder/local-agent flow works without cloud; cloud is optional | M12 | — | — | — | todo |
| S11-002 | 11 Sync | Sync notes/artifacts/packets/policies/run summaries across Macs; raw audio local unless per-session/workspace encrypted opt-in | M12 | — | — | — | todo |
| S11-003 | 11 Teams | Share selected objects without automatic private raw-recording access | M12 | — | — | — | todo |
| S11-004 | 11 Hosted | Hosted worker checks approved revision, scopes credentials, executes supported agent, enforces budget/time, returns events/artifacts, destroys worker | M12 | — | — | — | todo |
| S11-005 | 11 Heavy processing | Offer optional higher-accuracy transcription, diarization, synthesis, contradiction, multimodal, and batch reprocessing | M12 | — | — | — | todo |
| S11-006 | 11 Notifications | Notify completion, escalation, clarification, unavailable Mac, and team approval need | M10 | — | — | — | todo |
| S11-007 | 11 Boundary | Before cloud, disclose exactly uploaded packet/excerpts/repo snapshot and excluded audio/unrelated/private notes | M12 | — | — | — | todo |
| S12-001 | 12 | Project association suggestions use speech, repo/PR, calendar, active app/folder, participants, aliases and remain editable | M8 | — | — | — | todo |
| S12-002 | 12 | Project page contains objective, decisions, questions, actions, resources, sessions, artifacts, results, policies | M8 | — | — | — | todo |
| S12-003 | 12 | Artifact updates are proposed; user accepts all, individual changes, or keeps independent | M8 | — | — | — | todo |
| S13-001 | 13 | Search sessions, transcripts, speakers, decisions, actions, results, projects, artifacts, repositories, and PRs | M8 | — | — | — | todo |
| S13-002 | 13 | Search results show type, title, match excerpt, timestamp, project, action/run status and can seed action | M8 | — | — | — | todo |
| S13-003 | 13 | Local MCP/CLI exposes specified sessions, sources, projects, actions, runs, artifacts high-level functions | M8 | — | — | — | todo |
| S14-001 | 14 Privacy | Default to local raw audio/transcription/encryption, Keychain keys, no account/model training, per-session deletion/retention | M10 | — | — | — | todo |
| S14-002 | 14 Transparency | Provide persistent/menu indicators, optional sound, source labels, pause, exclusions, private mode | M3 | — | — | — | todo |
| S14-003 | 14 Injection | Treat speech/imports as untrusted quoted context; separate source, user objective, and manifest authority | M9 | — | — | — | todo |
| S14-004 | 14 Touch ID | When enabled require Touch ID for connector grants, public posting, push, PR, secrets, deploy, deletion/mutation | M9 | — | — | — | todo |
| S14-005 | 14 Audit | Audit authorizer, source, packet version, permissions, accessed files/services, requests/responses, artifacts, final status | M2 | — | — | — | todo |
| S15-001 | 15 | Ambiguous actions are Needs review and cannot auto-run | M9 | — | — | — | todo |
| S15-002 | 15 | Missing repository offers choose folder, GitHub connect, or copy-only recovery | M10 | — | — | — | todo |
| S15-003 | 15 | Missing agent offers installation guidance, alternate agent, or copy prompt | M10 | — | — | — | todo |
| S15-004 | 15 | Offline/asleep local runs queue and may move to cloud only if enabled | M10 | — | — | — | todo |
| S15-005 | 15 | Permission request pauses run and offers allow once, project policy, deny with exact impact | M9 | — | — | — | todo |
| S15-006 | 15 | Clarification appears in Needs you with exact source and reason | M10 | — | — | — | todo |
| S15-007 | 15 | Low-confidence text is marked and never sole source for autonomous task | M4 | — | — | — | todo |
| S15-008 | 15 | Detect duplicate project actions and suggest merge/link/keep both | M8 | — | — | — | todo |
| S15-009 | 15 | Failed result includes category, last success, logs, changed-files state, safe retry, recovery prompt | M10 | — | — | — | todo |
| S16-001 | 16 Extraction | Measure action detection, correct speaker/span, objective/constraint accuracy, and card edit/rejection | M13 | — | — | — | todo |
| S16-002 | 16 Dispatch | Measure project/repo resolution, capability recommendation, and policy behavior | M13 | — | — | — | todo |
| S16-003 | 16 Outcome | Measure deliverable completion, clarifications, violations, acceptance, and context reconstruction | M13 | — | — | — | todo |
| S16-004 | 16 Comparative | For opted-in benchmarks compare raw transcript, summary, and structured packet on completion, constraints, clarifications, tokens, accepted time, edits | M13 | — | — | — | todo |
| S16-005 | 16 Reporting | Team metrics may report handoffs, editing, autonomous read completion, clarifications, acceptance; avoid unsupported vanity claims | M13 | — | — | — | todo |
| S17-001 | 17 OSS | Open-source macOS client, capture/transcription, DB schema, packet/manifest, MCP/CLI, adapters/SDK, mapping, helper, import/export | M13 | — | — | — | todo |
| S17-002 | 17 Hosted | Optional hosted service supplies sync, teams, hosted execution, cloud processing, managed connectors, org policy/audit | M12 | — | — | — | todo |
| S17-003 | 17 Plugins | Support capture, transcription, note, detector, adapter, connector, artifact, renderer plugins with scoped DB access | M13 | — | — | — | todo |
| S18-001 | 18.1 | UI is calm/native/keyboard fluent, note-first and not enterprise orchestration; every screen answers captured, became, happening | M11 | — | — | — | todo |
| S18-002 | 18.2 | Provide five surfaces: menu popover, floating HUD, main window, dispatch sheet, notifications | M11 | — | — | — | todo |
| S18-003 | 18.3 | Main window default/minimum sizes are 1180×760/920×620; collapsible sidebar has Today/Moments/Projects/Actions/Runs/Search/pins | M11 | — | — | — | todo |
| S18-004 | 18.3 | Toolbar has sidebar, title, global search, New Capture, palette, account/sync, compact run pill | M11 | — | — | — | todo |
| S18-005 | 18.4 | Menu idle shows start, quick profiles, recents, agents, main/settings; recording shows status, markers, pause/stop; request state handles permission | M11 | — | — | — | todo |
| S18-006 | 18.4 | Menu popover supports full keyboard navigation | M11 | — | — | — | todo |
| S18-007 | 18.5 | Movable 360×56 HUD shows recording/source/controls and expanded transcript/action view | M11 | — | — | — | todo |
| S18-008 | 18.5 | HUD stays above normal windows unless disabled, avoids controls, collapses, always signals recording, hides text on shared screens when enabled | M11 | — | — | — | todo |
| S18-009 | 18.6 | Today is calm overview with Start capture, Needs you, Ready, Recent, Running; empty sections collapse | M11 | — | — | — | todo |
| S18-010 | 18.7 | Capture setup offers title, audio choices, active apps/screenshots/repo context, local/retention privacy, optional purpose without required categorization | M11 | — | — | — | todo |
| S18-011 | 18.8 | Optional live window has transcript, detected rail, controls/direct field; explicit dispatch visibly starts, suggestions do not focus-steal, edits do not stop capture | M11 | — | — | — | todo |
| S18-012 | 18.9 | Session Detail is primary reader: header, Note/Transcript/Sources default Note, Actions/Context/Info inspector | M11 | — | — | — | todo |
| S18-013 | 18.9 | Notes offer source count/jump/edit/incorrect/action/artifact; transcript has speaker gutter/timestamps/confidence/playback/selection/context menu; actions show resolution state | M11 | — | — | — | todo |
| S18-014 | 18.10 | Compact/expanded action cards show objective, source/playback, project, deliverable, access and Copy/Open/Run/Edit controls | M11 | — | — | — | todo |
| S18-015 | 18.10 | Statuses Draft, Needs review, Ready, Queued, Running, Needs you, Complete, Failed, Dismissed use text and icon not color alone | M11 | — | — | — | todo |
| S18-016 | 18.11 | Dispatch sheet selects agent/mode/capability, plain-language allow/deny, local/hosted, time/cost; Build shows isolated worktree/diff/push/PR explicit grants | M11 | — | — | — | todo |
| S18-017 | 18.12 | Actions queue groups Needs you/Ready/Running/Completed; filters status/project/agent/capability/session/speaker; bulk action limits and confirmation | M11 | — | — | — | todo |
| S18-018 | 18.13 | Runs screen provides list/control center, progress/live/context/permissions/artifacts, pause/stop/terminal, permission/clarification/complete result; post review creates new Act action | M11 | — | — | — | todo |
| S18-019 | 18.14 | Project screen is lightweight, shows objective/questions/actions/artifacts/moments/resources, and proposes updates rather than manual pages | M11 | — | — | — | todo |
| S18-020 | 18.15 | ⌘K palette supports listed capture/mark/send/open/show/create/search/switch commands and natural-language resolution | M11 | — | — | — | todo |
| S18-021 | 18.16 | Provide customizable defaults for Alt+Space, Alt+Shift+Space, Alt+A/D/I/Q, ⌘K, ⌘Return, ⌘Shift+Return, Space, ⌘L | M11 | — | — | — | todo |
| S18-022 | 18.17 | Completion/input/permission/failure notifications are concise/actionable and suppress private transcript unless previews enabled | M10 | — | — | — | todo |
| S18-023 | 18.18 | Onboard under three minutes: privacy, explained audio permission/test meter, optional validated agent setup/command, autonomy preference, sample capture demo | M10 | — | — | — | todo |
| S18-024 | 18.19 | Settings cover General, Audio/transcription, Agents, Automation/YAML, Projects/repos, Privacy, Notifications specified controls | M10 | — | — | — | todo |
| S18-025 | 18.20 | Design is calm/editorial/native/trustworthy/minimal; system type/monospace technical values; text/icon color semantics; subtle/reduced motion/no layout shift; SF Symbols and explicit permission icons | M11 | — | — | — | todo |
| S18-026 | 18.21 | Full VoiceOver, keyboard capture/mark/dispatch/run, playback captions/transcript, non-color status, hit targets, reduced motion/contrast, resizable text/inspectors, non-color speakers | M11 | — | — | — | todo |
| S18-027 | 18.22 | Provide specified No sessions, No agents, No actions, nonblocking Processing, and Low-confidence states | M11 | — | — | — | todo |
| S19-001 | 19.1 | Work-meeting journey: capture→draft sourced PR action→voice read-only dispatch/confirmation→local run→permission→risk review→session/queue→separate Act posting | M14 | — | — | — | todo |
| S19-002 | 19.2 | Hackathon journey: suggested research→Codex/Claude parallel read research→dependent Claude Plan/no code→approved artifact update | M14 | — | — | — | todo |
| S19-003 | 19.3 | Personal-note journey creates clean local note/reminder without execution intent or agent action | M14 | — | — | — | todo |
| S19-004 | 19.4 | Presentation journey captures system audio, summarizes claims/questions, copies sourced Claude prompt without integration/account | M14 | — | — | — | todo |
| S19-005 | 19.5 | Pair-work journey marks bug/repo, prepares investigation, Build Claude worktree, tests/diff/editor review, separate Act PR confirmation | M14 | — | — | — | todo |
| S20-001 | 20 | Measure time spoken assignment→prepared action and explicit dispatch→agent start | M13 | — | — | — | todo |
| S20-002 | 20 | Measure suggested acceptance, no-edit start, clarifications, constraint violations, accepted-without-rework, trusted read-only auto-run | M13 | — | — | — | todo |
| S20-003 | 20 | Measure notes-only and delegation retention and maintain zero unauthorized destructive actions | M14 | — | — | — | todo |
| S20-004 | 20 | Satisfy qualitative outcome: user keeps listening/talking while safe existing-agent work starts from right conversation context | M14 | — | — | — | todo |
| S21-001 | 21 | Position primarily as “Capture anything. Delegate anything.” | M14 | — | — | — | todo |
| S21-002 | 21 | Position explanatorily as open-source macOS recorder that turns spoken moments into notes and AI-agent work | M14 | — | — | — | todo |
| S21-003 | 21 | Position for developers as source capture, task approval, and Codex/Claude start without re-explanation | M14 | — | — | — | todo |
| S21-004 | 21 | Differentiate from Granola by permissioned work and returned agent result tied to original source | M14 | — | — | — | todo |

## Enumerated behavior expansion

| Requirement ID | Spec reference | Requirement | Milestone | Implementation evidence | Verification evidence | PR | Status |
|---|---|---|---|---|---|---|---|
| S02-INC-14 | 2 Included | Provide copyable prompts | M6 | — | — | — | todo |
| S02-INC-15 | 2 Included | Launch interactive Codex sessions | M7 | — | — | — | todo |
| S02-INC-16 | 2 Included | Launch interactive Claude Code sessions | M7 | — | — | — | todo |
| S02-INC-17 | 2 Included | Support autonomous local execution | M7 | — | — | — | todo |
| S02-INC-18 | 2 Included | Support autonomous hosted execution | M12 | — | — | — | todo |
| S05-010 | 5 Artifact | Support hackathon brief artifacts | M8 | — | — | — | todo |
| S05-011 | 5 Artifact | Support project plan and research memo artifacts | M8 | — | — | — | todo |
| S05-012 | 5 Artifact | Support customer-problem brief, weekly status, and architecture-decision-record artifacts | M8 | — | — | — | todo |
| S06-014 | 6.2 | Identify commitments during live understanding | M4 | — | — | — | todo |
| S06-015 | 6.2 | Identify assignments during live understanding | M4 | — | — | — | todo |
| S06-016 | 6.2 | Resolve PR, repository, ticket, file, URL, product, and person references during live understanding | M4 | — | — | — | todo |
| S07-TYPE-01 | 7.1 | Support `research` actions | M6 | — | — | — | todo |
| S07-TYPE-02 | 7.1 | Support `review_pull_request` actions | M6 | — | — | — | todo |
| S07-TYPE-03 | 7.1 | Support `inspect_repository` actions | M6 | — | — | — | todo |
| S07-TYPE-04 | 7.1 | Support `plan_change` actions | M6 | — | — | — | todo |
| S07-TYPE-05 | 7.1 | Support `draft_document` actions | M6 | — | — | — | todo |
| S07-TYPE-06 | 7.1 | Support `build_change` actions | M6 | — | — | — | todo |
| S07-TYPE-07 | 7.1 | Support `run_tests` actions | M6 | — | — | — | todo |
| S07-TYPE-08 | 7.1 | Support `investigate_bug` actions | M6 | — | — | — | todo |
| S07-TYPE-09 | 7.1 | Support `create_issue` actions | M6 | — | — | — | todo |
| S07-TYPE-10 | 7.1 | Support `post_review` actions | M6 | — | — | — | todo |
| S07-TYPE-11 | 7.1 | Support `update_external_tool` actions | M6 | — | — | — | todo |
| S07-TYPE-12 | 7.1 | Support `custom` actions | M6 | — | — | — | todo |
| S07-READ-01 | 7.3 Read | Permit reading repository files | M7 | — | — | — | todo |
| S07-READ-02 | 7.3 Read | Permit PR or diff inspection | M7 | — | — | — | todo |
| S07-READ-03 | 7.3 Read | Permit documentation search and read-only analysis commands | M7 | — | — | — | todo |
| S07-READ-04 | 7.3 Read | Permit linked session-context reading | M7 | — | — | — | todo |
| S07-READ-05 | 7.3 Read | Forbid repository modification, comments, branches, and external-system changes | M7 | — | — | — | todo |
| S07-MAN-01 | 7.4 | Manifest separately specifies filesystem read/write paths | M9 | — | — | — | todo |
| S07-MAN-02 | 7.4 | Manifest separately specifies worktree/branch/commit permissions | M9 | — | — | — | todo |
| S07-MAN-03 | 7.4 | Manifest separately specifies PR read/comment/open/merge permissions | M9 | — | — | — | todo |
| S07-MAN-04 | 7.4 | Manifest separately specifies documentation/arbitrary-web network permissions and secrets access | M9 | — | — | — | todo |
| S08-004 | 8 | Packet source references preserve timing, speaker, and source text | M6 | — | — | — | todo |
| S08-005 | 8 | Packet project data supports provider/slug/local path and revision type/value | M8 | — | — | — | todo |
| S08-006 | 8 | Packet context separately includes and excludes scoped data | M6 | — | — | — | todo |
| S09-007 | 9 Interface | Adapter validates installation | M7 | — | — | — | todo |
| S09-008 | 9 Interface | Adapter prepares packet with permission manifest | M7 | — | — | — | todo |
| S09-009 | 9 Interface | Adapter opens interactive and runs autonomously | M7 | — | — | — | todo |
| S09-010 | 9 Interface | Adapter streams events and handles clarification/permission answers | M7 | — | — | — | todo |
| S09-011 | 9 Interface | Adapter pauses, resumes, cancels, and collects result | M7 | — | — | — | todo |
| S10-005 | 10 Responsibilities | Launch child processes and PTYs | M1 | `backend/src/lib.rs` direct `Command::new(executable).args(arguments)` launch; `Sources/CaptureDelegateIPC/IPCClient.swift` typed `start_process`; PTY launch remains pending | Rust `cat_process_outputs_before_one_exit_frame`, `nonexistent_executable_emits_a_structured_terminal_frame`; Swift `Swift process API receives cat output before one terminal event`; `.loop/verification/m1/process-streaming/full-verification.txt`; independent reviews clear | PR #5 (`9f40024`; both CI runs passed) | in_progress |
| S10-006 | 10 Responsibilities | Create isolated Build worktrees | M7 | — | — | — | todo |
| S10-007 | 10 Responsibilities | Enforce local timeout and concurrency limits | M1 | `backend/src/lib.rs` bounded eight-client/eight-process pools, synchronous `capacity_exhausted`, required per-run timeout with direct-child kill/reap and typed `timed_out`; broader lifecycle controls remain pending | Process admission/health tests from PR #5; Rust timeout, invalid-admission, u64-max, completion-precedence, and capacity-release tests; Swift request/decode and real-backend timeout tests; `.loop/verification/m1/run-timeout/verification.txt`; independent review clear | PR #5 (`9f40024`; both CI runs passed); PR #7 (`3058370`; both CI runs passed) | in_progress |
| S10-008 | 10 Responsibilities | Stream stdout, stderr, and structured events | M1 | `backend/src/lib.rs` bounded concurrent pipe drains and serialized `run_output`/`run_exit`; `Sources/CaptureDelegateIPC/IPCClient.swift` callback event stream with typed output/exit | Rust lifecycle, UTF-8, backpressure, inherited-pipe tests; Swift callback and real-backend integration tests; `.loop/verification/m1/process-streaming/full-verification.txt`; independent reviews clear | PR #5 (`9f40024`; both CI runs passed) | in_progress |
| S10-009 | 10 Responsibilities | Detect agent waits for input | M1 | — | — | — | todo |
| S10-010 | 10 Responsibilities | Redact secrets from logs and record process/environment metadata | M1 | — | — | — | todo |
| S10-011 | 10 Responsibilities | Terminate cancellation/budget-expired runs and clean temporary files/worktrees | M1 | `backend/src/lib.rs` cancels, kills, and reaps an active producer after output disconnect, kills/reaps direct children on per-run timeout while releasing capacity, and now exposes a versioned `cancel_process` request with an ActiveRuns registry (duplicate run-id rejection, Drop-based release) and typed `cancelled` terminals; `Sources/CaptureDelegateIPC/IPCClient.swift` adds `cancelProcess`; budget expiry, pause/resume, descendant process groups, and temporary/worktree cleanup remain pending | PR #5 disconnect/inherited-pipe tests; Rust timeout output/order/capacity tests; Rust cancel/duplicate/race integration tests; Swift real-backend timeout and cancel tests; `.loop/verification/m1/run-timeout/verification.txt`; `.loop/verification/m1/run-cancel/verification.txt` | PR #5 (`9f40024`; both CI runs passed); PR #7 (`3058370`; both CI runs passed); PR #9 (`ea02d67`; both CI runs passed; independent Codex review clear, two confirmed findings fixed pre-merge) | in_progress |
| S11-008 | 11 Heavy processing | Support optional high-accuracy transcription and speaker separation | M12 | — | — | — | todo |
| S11-009 | 11 Heavy processing | Support long-session synthesis and cross-session contradiction detection | M12 | — | — | — | todo |
| S11-010 | 11 Heavy processing | Support multimodal slide/screenshot analysis and improved-extraction batch reprocessing | M12 | — | — | — | todo |
| S12-004 | 12 Association | Suggest projects from spoken names and repository/PR references | M8 | — | — | — | todo |
| S12-005 | 12 Association | Suggest projects from calendar title and active app/folder | M8 | — | — | — | todo |
| S12-006 | 12 Association | Suggest projects from linked participants and aliases | M8 | — | — | — | todo |
| S13-TARGET-01 | 13 | Search session titles and transcript text | M8 | — | — | — | todo |
| S13-TARGET-02 | 13 | Search speakers, decisions, and actions | M8 | — | — | — | todo |
| S13-TARGET-03 | 13 | Search agent results, projects, and artifacts | M8 | — | — | — | todo |
| S13-TARGET-04 | 13 | Search repositories and pull requests | M8 | — | — | — | todo |
| S13-MCP-01 | 13 | MCP/CLI provides sessions.search and sessions.get_note | M8 | — | — | — | todo |
| S13-MCP-02 | 13 | MCP/CLI provides sources.get_excerpt and projects.get_current_state | M8 | — | — | — | todo |
| S13-MCP-03 | 13 | MCP/CLI provides actions.list/get_task_packet/create and runs.get_result | M8 | — | — | — | todo |
| S13-MCP-04 | 13 | MCP/CLI provides artifacts.get and artifacts.propose_update | M8 | — | — | — | todo |
| S14-006 | 14 Privacy | Store encryption keys in macOS Keychain | M10 | — | — | — | todo |
| S14-007 | 14 Privacy | Do not train hosted models on user data | M12 | — | — | — | todo |
| S14-008 | 14 Transparency | Support optional recording start/stop sound and visible active source labels | M3 | — | — | — | todo |
| S14-009 | 14 Transparency | Support one-click pause, app exclusion list, and private-session mode | M3 | — | — | — | todo |
| S15-010 | 15 Failure | Permission UI identifies exact command and whether files change | M9 | — | — | — | todo |
| S15-011 | 15 Failure | Safe retry options and copyable recovery prompt are available after failure | M10 | — | — | — | todo |
| S16-006 | 16 | Team operational reporting identifies metrics as comparisons only where backed by real outcomes | M13 | — | — | — | todo |
| S17-004 | 17 Components | Open-source local MCP server and CLI | M8 | — | — | — | todo |
| S17-005 | 17 Components | Open-source Codex/Claude adapters and generic adapter SDK | M13 | — | — | — | todo |
| S17-006 | 17 Components | Open-source project/repository mapping, run helper, and import/export format | M13 | — | — | — | todo |
| S17-PLUG-01 | 17 Plugins | Plugin category: capture source | M13 | — | — | — | todo |
| S17-PLUG-02 | 17 Plugins | Plugin category: transcription engine | M13 | — | — | — | todo |
| S17-PLUG-03 | 17 Plugins | Plugin category: note template and action detector | M13 | — | — | — | todo |
| S17-PLUG-04 | 17 Plugins | Plugin category: agent adapter and external connector | M13 | — | — | — | todo |
| S17-PLUG-05 | 17 Plugins | Plugin category: artifact type and result renderer | M13 | — | — | — | todo |
| S18-SIDEBAR-01 | 18.3 | Sidebar offers Today, Moments, Projects, Actions, Agent Runs, and Search | M11 | — | — | — | todo |
| S18-SIDEBAR-02 | 18.3 | Sidebar supports pinned projects | M11 | — | — | — | todo |
| S18-HUD-01 | 18.5 | HUD supports Important, Action, Pause, Stop controls | M11 | — | — | — | todo |
| S18-HUD-02 | 18.5 | Expanded HUD supports Decision and Question markers plus detected action Save/Send | M11 | — | — | — | todo |
| S18-SETUP-01 | 18.7 | Capture setup supports microphone+system, microphone-only, and system-only choices | M11 | — | — | — | todo |
| S18-SETUP-02 | 18.7 | Capture setup supports active-app names, marked screenshots, and current-repository context toggles | M11 | — | — | — | todo |
| S18-SETUP-03 | 18.7 | Capture setup supports local raw-audio retention and post-30-day deletion choices | M11 | — | — | — | todo |
| S18-SETUP-04 | 18.7 | Capture purpose options are Auto, Meeting, Conversation, Presentation, Personal note, Pair work | M11 | — | — | — | todo |
| S18-NOTE-01 | 18.9 | Note default sections include Summary, Decisions, Ideas, and Open questions | M11 | — | — | — | todo |
| S18-TRANS-01 | 18.9 | Transcript playback highlights active text and supports selection-to-action | M11 | — | — | — | todo |
| S18-TRANS-02 | 18.9 | Transcript context menu marks decision, idea, question, or source | M11 | — | — | — | todo |
| S18-ACTION-01 | 18.10 | Action component exposes Draft, Needs review, Ready, Queued, Running, Needs you, Complete, Failed, Dismissed individually | M11 | — | — | — | todo |
| S18-DISPATCH-01 | 18.11 | Dispatch supports Copy prompt, Open interactive session, and background Run choices | M11 | — | — | — | todo |
| S18-DISPATCH-02 | 18.11 | Dispatch supports Read, Plan, Build, Act capabilities | M11 | — | — | — | todo |
| S18-DISPATCH-03 | 18.11 | Dispatch shows capability plain-language positive and negative permissions | M11 | — | — | — | todo |
| S18-DISPATCH-04 | 18.11 | Dispatch supports local Mac or hosted worker and maximum time/cost limits | M11 | — | — | — | todo |
| S18-DISPATCH-05 | 18.11 | Build dispatch supports isolated worktree, show diff, explicit push, explicit open PR | M11 | — | — | — | todo |
| S18-QUEUE-01 | 18.12 | Queue filter supports status, project, agent, capability, session, source speaker | M11 | — | — | — | todo |
| S18-QUEUE-02 | 18.12 | Safe bulk actions are dismiss, project assign, agent change; autonomous bulk requires summary confirmation | M11 | — | — | — | todo |
| S18-RUN-01 | 18.13 | Run detail provides Progress, Live output, Context, Permissions, Artifacts tabs | M11 | — | — | — | todo |
| S18-RUN-02 | 18.13 | Run progress shows packet load, PR open, implementation read, test compare, review preparation | M11 | — | — | — | todo |
| S18-RUN-03 | 18.13 | Clarification displays source context and reply field | M11 | — | — | — | todo |
| S18-CMD-01 | 18.15 | Palette starts microphone and microphone+system capture and stops capture | M11 | — | — | — | todo |
| S18-CMD-02 | 18.15 | Palette marks action, sends selected action, opens latest session, shows running agents | M11 | — | — | — | todo |
| S18-CMD-03 | 18.15 | Palette creates clipboard action, searches sessions, and switches project | M11 | — | — | — | todo |
| S18-SET-01 | 18.19 General | Settings support launch-at-login, menu behavior, default profile, retention, auto-title, shortcuts | M10 | — | — | — | todo |
| S18-SET-02 | 18.19 Audio | Settings support device, system capture, transcription engine/language, diarization, model downloads, post-finalization deletion | M10 | — | — | — | todo |
| S18-SET-03 | 18.19 Agents | Per-adapter settings support install, command, auth, model, capability, mappings, local/hosted preference | M10 | — | — | — | todo |
| S18-SET-04 | 18.19 Automation | Visual policy editor supports explicit requested task, agent/read access, auto run, time/cost/concurrency limits; advanced YAML edit | M10 | — | — | — | todo |
| S18-SET-05 | 18.19 Projects | Project settings support aliases, folders, repos, agent, worktree, commands, context files | M10 | — | — | — | todo |
| S18-SET-06 | 18.19 Privacy | Privacy settings support cloud processing, audio sync, retention, exclusions, redaction, export/delete, diagnostics | M10 | — | — | — | todo |
| S18-SET-07 | 18.19 Notifications | Notification settings support completion, clarification, permission, failure, preview visibility | M10 | — | — | — | todo |
| S18-DESIGN-01 | 18.20 | Use macOS system typeface and readable human-note line height/measure | M11 | — | — | — | todo |
| S18-DESIGN-02 | 18.20 | Render commands, paths, technical values, packets in monospaced type | M11 | — | — | — | todo |
| S18-DESIGN-03 | 18.20 | Color semantics: neutral captured/saved, blue ready, purple running, orange input, green complete, red failure/destructive | M11 | — | — | — | todo |
| S18-DESIGN-04 | 18.20 | Use subtle insert motion, only recording pulses, respect Reduce Motion, avoid stream layout shifts | M11 | — | — | — | todo |
| S18-DESIGN-05 | 18.20 | Prefer SF Symbols and explicit eye/list/hammer/act/lock/waveform/link permission/provenance icons | M11 | — | — | — | todo |
| S18-EMPTY-01 | 18.22 | No-sessions state invites meeting/conversation/presentation/idea capture and Start Capture | M11 | — | — | — | todo |
| S18-EMPTY-02 | 18.22 | No-agents state preserves notes and offers connect/not-now | M11 | — | — | — | todo |
| S18-EMPTY-03 | 18.22 | No-actions state explains manual action creation from transcript selection | M11 | — | — | — | todo |
| S18-EMPTY-04 | 18.22 | Processing permits transcript reading while note refinement continues without blocking spinner | M11 | — | — | — | todo |
| S19-006 | 19.1 | Work-meeting voice dispatch uses two-second confirmation toast | M11 | — | — | — | todo |
| S19-007 | 19.2 | Dependent Plan action waits for both parallel research runs and is non-coding | M9 | — | — | — | todo |
| S19-008 | 19.5 | User can review Build diff in app or preferred editor before separate PR Act confirmation | M11 | — | — | — | todo |
