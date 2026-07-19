# M1.5 UX contract — First Capture Journey

Authored before implementation by a fresh Claude Opus 4.8 context on 2026-07-18. The author was read-only and did not implement or review the slice.

## Product thesis

Capture Delegate is a calm, editorial macOS notes app that happens to record: fast to start, honest about what it does, and quiet everywhere except the single unmistakable recording signal.

- Granola supplies the note-first hierarchy and restrained tone.
- Raycast supplies keyboard speed and a command palette containing only real actions.
- Apple HIG supplies native split navigation, toolbar, menu bar, focus, type, materials, accessibility, light/dark behavior, and reduced motion.
- System red is reserved for active recording. All other states use mostly neutral system colors; every state also has text and a distinct SF Symbol.
- No gradients, fake waveforms, fixture content, fabricated AI results, shimmer, or celebratory save animation.

M1.5 is microphone-only. It must not imply that transcription, AI summaries, system/mixed audio, projects, actions, agents, runs, or cloud sync work. Projects, Actions, and Agent Runs may exist only as honest explanatory destinations without dead controls.

## Complete journey

Launch to a real empty Today view → Start Capture from toolbar, menu bar, or Option-Space → explain/request microphone permission → record with timer and level truth → pause/resume → stop → save with Keychain-backed encryption → appear in Today and Moments → open Session Detail → play the real audio and edit a human note → quit and relaunch → reopen and play the same session.

At this checkpoint the user can:

- record microphone audio, pause, resume, and stop;
- type and persist a human-authored title and note;
- browse captures in Today and Moments;
- search by title or note;
- open, play, scrub, rename, edit, and delete a session;
- see the local runtime's real health without capture depending on it.

## Main window

- One regular macOS main window, default 1180×760 and minimum 920×620.
- Native `NavigationSplitView`; default destination Today.
- Sidebar order: Today, Moments, Projects, Actions, Agent Runs, Search.
- Toolbar order: native sidebar toggle, current title, search entry, prominent “New Capture”, Command-K palette, real local-runtime status.
- While recording, New Capture becomes a live chip with distinct recording/paused text, timer, and jump-to-live behavior.
- Today is a centered 640–720 point editorial column with time-of-day greeting, one strong capture card, an active-capture row when relevant, and at most five real recent moments. Empty sections are omitted.
- Moments is a newest-first native list with title, duration, date, microphone source, open-on-Return, and working rename/delete actions.
- Search matches real titles and notes locally and opens real Session Detail views.
- Session Detail is the reader: editable title, date/duration/source, accessible playback controls and scrubber, and an editable human note.

Final empty/future copy:

- Today: “Capture something worth keeping.” / “Record a meeting, a conversation, a presentation, or a passing idea.” / “It stays on this Mac, encrypted.”
- Moments: “No moments yet.” / “Every capture you make lands here, newest first.”
- Projects: “Projects will group related captures, folders, and repositories.” / “They arrive with agent delegation. For now, everything lives in Moments.”
- Actions: “Actions turn part of a capture into work you can hand to an agent.” / “This becomes available once agents are connected. Your captures and notes work fully today.”
- Agent Runs: “Agent runs will show what an agent is doing and what it returned.” / “Nothing runs yet — agent delegation comes in a later milestone.”
- Search prompt: “Search your captures by title or note.”
- Empty note placeholder: “Add a note… what mattered, decisions, follow-ups.”

## Menu bar and live capture

- An always-present native `MenuBarExtra` uses a neutral idle shape and a distinct filled recording shape, not tint alone.
- Idle popover: product name, working Start Capture, “Records from the microphone”, up to three real recents or “No captures yet”, Open Main Window, and Settings only if Settings actually exists.
- Recording/paused popover: textual state, timer, title/source, working Pause/Resume and Stop, and Open Main Window.
- A compact floating HUD is preferred: about 360×56, draggable, above normal windows, and containing textual recording/paused state, monospaced timer, real input-level truth, Pause/Resume, and Stop. It must never imply a transcript exists.

## Required state contract

All capture entry points use the same state machine and permission gate.

1. **Clean empty:** no samples, fixtures, fake summaries, or hidden placeholders.
2. **Permission unknown:** before the system prompt, explain “Capture needs your microphone”, “Recording uses the microphone on this Mac”, and “Your audio stays here, encrypted — it is never uploaded”; offer Not now and Continue.
3. **Permission denied/restricted:** show `mic.slash`, “Microphone access is off”, a truthful System Settings path, Cancel, and a working Open System Settings action.
4. **Recording:** editable title and note, `record.circle.fill` plus “Recording”, monospaced timer, real meter, textual “receiving audio”/“silent”, Pause, and Stop.
5. **Paused:** `pause.circle.fill` plus “Paused”, frozen timer/meter, Resume, and Stop.
6. **Saving:** a nonblocking spinner plus the word “Saving”; never claim success early.
7. **Saved:** route to the new real Session Detail; a quiet checkmark confirmation is allowed.
8. **Playback:** decrypt to memory, use an accessible native slider, show current/total time, and expose play/pause. Missing/corrupt/decrypt failure says “This recording couldn't be opened.”
9. **Runtime disconnected:** neutral `Runtime · Offline`, with copy explaining that recording, notes, and playback do not need the future agent runtime.
10. **Encryption/persistence failure:** “This capture couldn't be saved securely”; name the real key/disk cause; tell the truth about where the recording currently lives — held in a private temporary file on this Mac, readable only by the user's own macOS account, which saving encrypts — rather than claiming it never touched disk unencrypted; provide working Try Again, explicit Export audio, and confirmed Discard. Never silently drop the finalized recording.

## Keyboard and accessibility

- Option-Space starts capture while idle and stops/saves while active when the app is active; do not silently require global Accessibility/Input Monitoring permission.
- Command-N starts capture; Command-Shift-P pauses/resumes; Command-period stops; Command-L jumps to live capture; Command-K opens the palette; Command-F searches; Command-1/2 navigate Today/Moments.
- Space toggles playback only when Session Detail/playback is focused; Left/Right scrub five seconds.
- Command-K lists only implemented, context-valid capture/navigation/playback/session commands and real session matches.
- Every visible control has a precise label and hint. All controls are reachable by keyboard with native focus indication.
- Recording, paused, saving, saved, offline, and failure are always text plus different shapes; grayscale remains unambiguous.
- Timer is accessible but does not announce continuously. The meter has a textual alternative. Playback uses a labeled native slider.
- Reduce Motion eliminates pulsing and sliding; high contrast, light/dark appearance, resizing, and VoiceOver must retain clarity.

## Persistence and implementation boundary

- Use `AVAudioRecorder` with AAC `.m4a`, metering, `pause()`, and continued `record()`.
- Store one encrypted audio blob and encrypted metadata per session under Application Support.
- Use CryptoKit AES-GCM with a generated 256-bit symmetric key stored in Keychain; plaintext audio may exist only in the private temporary recording location while capture/save recovery is active.
- Playback decrypts directly to `Data` and initializes `AVAudioPlayer(data:)`; do not write a plaintext playback copy.
- Runtime health consumes the existing IPC health path/convention and remains informational.

## Named Computer Use scenarios

1. **Cold empty launch:** packaged app, default/minimum resize, all destinations, honest states, keyboard traversal, runtime state.
2. **First permission and capture:** toolbar start, real system permission, real audio level, edit title/note, pause, resume, stop, saving, saved.
3. **Menu and shortcut capture:** menu bar and Option-Space entry, live menu controls, stop/save.
4. **Browse and replay:** Today/Moments/search, open, play/pause/scrub, edit title/note, delete confirmation.
5. **Relaunch durability:** quit, cold relaunch, reopen both real sessions, play real audio.
6. **Permission denial/recovery:** real denied state and working System Settings path; no false recording.
7. **Runtime offline:** capture and playback continue with calm, truthful offline explanation.
8. **Secure-save failure:** injected key/store failure preserves recovery audio and exposes working retry/export/discard without plaintext persistence.
9. **Accessibility/taste:** accessibility tree contains every expected labeled control; keyboard-only flow, grayscale semantics, Reduce Motion, light/dark, and resize remain usable.

## Acceptance gate

The slice fails if any visible affordance is dead, any unimplemented capability appears working, any accepted screen contains mock content, the packaged app is not used, the real data path is bypassed, playback writes plaintext to disk, a failure loses audio silently, or the experience reads as a backend dashboard instead of a native notes app.

The independent Opus reviewer may block for weak hierarchy, excessive chrome, unclear recording truth, non-native interaction, misleading copy, or failure to make the saved moment—not the engine—the center of the product.
