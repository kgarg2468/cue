# M1.5 "First Capture Journey" — Computer Use scenario log

- Slice: `m1.5/first-capture`
- Branch: `loop/m15-first-capture`
- Verified: 2026-07-19 (cycle 18)
- Method: real Computer Use against the packaged `.context/Capture Delegate.app`
  (ad-hoc signed, identifier `com.capturedelegate.app`), driven via macOS
  Accessibility (System Events) with atomic in-script `screencapture` evidence.
  No simulator, no mocked UI. Backend runtime: `capture-delegate-backend` on
  `/tmp/capture-delegate-runtime/health.sock`.
- Contract: `ux-contract.md` (binding). Evidence: `screens/` (38 images).

## Verdicts

| # | Scenario | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | Cold launch → Today, empty states | PASS | s1-01, s1-02-* |
| 2 | First capture end-to-end (record, pause, title/note, stop) | PASS | s2-01…s2-07 |
| 3 | Playback + Today list + search | PASS | s3-01…s3-03 |
| 4 | Relaunch persistence (encrypted store) | PASS | s4-01 |
| 5 | Menu bar popover (idle / start / stop / recents) | PASS | s5-01…s5-03 |
| 6 | ⌥Space toggle + capture HUD (record/pause/resume/stop) | PASS* | s6-01…s6-03 |
| 7 | ⌘K command palette (actions, search fall-through, open capture) | PASS* | s7-01…s7-03 |
| 8 | Runtime offline honesty (chip, explainer, capture works offline) | PASS | s8-01…s8-03 |
| 9 | Secure-save failure + recovery (Try Again preserves audio) | PASS | s9-01, s9-02 |
| 10 | Microphone permission journey (explainer / Not now / deny / re-allow) | PASS | s10-01…s10-05 |

\* = passes contract with findings noted below (F6/F7).

## Scenario 10 detail (run this session)

1. `tccutil reset Microphone com.capturedelegate.app`, fresh packaged app.
2. Start capture → in-app pre-permission explainer (s10-01): mic glyph,
   "Capture needs your microphone", truthful bullets ("Recording uses the
   microphone on this Mac", "Your audio stays here, encrypted — it is never
   uploaded"), Not now / Continue.
3. "Not now" → calm return to Today (s10-02); status item stays "idle"; no
   phantom recording state anywhere.
4. Start again → Continue → real macOS TCC prompt (s10-03) shows the corrected
   usage string: "Capture Delegate records audio from this Mac's microphone
   only when you start a capture. Recordings stay on this Mac, encrypted."
5. "Don't Allow" → denied sheet (s10-04): mic.slash, "Microphone access is
   off", truthful copy, exact path "System Settings → Privacy & Security →
   Microphone", Cancel + Open System Settings. Clicking Open System Settings
   deep-links straight to the Microphone pane (verified: System Settings
   window title "Microphone").
6. Reset TCC, relaunch, Continue → Allow → recording starts immediately
   (s10-05: Live capture, red timer, honest "Silent" level readout since no
   one was speaking). ⌘. stopped and saved (session #5 on disk). App left in
   the ALLOWED state for the user's hands-on test.

## Keyboard shortcut verification

All contract shortcuts exist and were exercised: ⌘N (File > New Capture —
starts capture, verified live), ⌘⇧P pause, ⌘. stop & save, ⌘L jump to live,
⌘K palette, ⌘F search, ⌘1 Today, ⌘2 Moments, ⌥Space toggle (see F6).

## Findings ledger

- **F1** (deferred, decisions.md): runtime socket path convention
  (`/tmp/capture-delegate-runtime/`) should move to per-user dir before multi-
  user support. Not user-visible in M1.5.
- **F2** (deferred, decisions.md): no XCUITest target; Computer Use via
  Accessibility is the only automated UI check. Revisit at M2.
- **F3** (routed to Opus visual review): judgment call on destructive-red
  usage (Discard button in failure sheet, trash in detail view).
- **F4**: app activation needed explicit `frontmost` set after `open` —
  workaround in test harness only, no product change.
- **F5** (accessibility gap): popover and HUD buttons expose no AX
  names/descriptions (`description` returns "button"). VoiceOver users cannot
  identify Start/Pause/Stop in the popover or HUD. Recommend `.accessibilityLabel`
  pass in M1.5 follow-up.
- **F6** (real-world collision): Raycast owns ⌥Space globally and swallows it
  before the app's local monitor sees it. With Raycast running, the shortcut
  is dead. Tested with Raycast quit; relaunched after. Needs either a
  rebindable shortcut or first-run detection later.
- **F7** (gap vs placeholder copy): ⌘K palette placeholder says "…or open a
  capture" and Enter does open the top search hit, but capture rows never
  render inline in the palette list. Either render matches inline or soften
  the placeholder.
- **F8** (nav nit): after a successful Try Again retry from the failure sheet,
  the app lands on the idle Live capture view instead of the saved capture's
  detail. Saved data is correct (session 294E0BCD, listed in Today).
- **F9** (fixed this session): packaging script silently produced an .app with
  NO `NSMicrophoneUsageDescription` — PlistBuddy's `-c` parser cannot carry an
  apostrophe and its parse error did not trip `set -euo pipefail`. Fixed by
  switching that key to `plutil -replace`; verified in the shipped bundle.
- Environment notes (not our bugs): Grammarly injects a "G" overlay bubble in
  the note text field; HUD drag was verified via AX `set position` (window is
  movable) but a real mouse-drag gesture could not be automated on this host —
  user should confirm by hand.

## Test data left on disk

`~/Library/Application Support/CaptureDelegate/sessions/` holds 5 encrypted
test sessions (Standup notes with Sol 1:29; Untitled 2:58, 1:36, 0:06, 0:0x).
All `.enc`-only (no plaintext `ftyp` markers); temp dir cleaned after saves.

## Fix pass (cycle 18, post-review)

Both independent reviews came back before merge: Opus visual review
(approve-with-nits; F7 major, F8 ship-blocker, F3 resolved as neutral-is-
correct + add delete confirmation, plus AX/taste nits) and Sol technical
review (REQUEST-CHANGES; 1 blocker + 6 majors + 7 minors, see PR body).
Per cycle-17 precedent, blocker/majors were fixed and re-reviewed before the
pause:

- Commit 4f47b78 (Sol worker): start/save single-flight guards; plaintext
  temp lifecycle with PID + process-start-identity ownership, launch
  reconciliation and termination cleanup; ordered non-optimistic mutation
  pipeline; staged non-destructive export; corrupt-session-tolerant
  `listWithProblems()`; playback timer deinit; pause/resume error surfacing;
  stale staging-dir cleanup. Six new red/green core tests (18 total, green).
- Commit 1fea360 (Opus worker + orchestrator): **F7 fixed** — ⌘K palette now
  renders matching captures inline (cap 5 + overflow row, row 0 is the Enter
  target, arrow-wrap, Escape closes); **F8 fixed** — root cause was a
  NavigationStack path replacement issued during the failure sheet's dismissal
  transition being dropped; retry now sequences routing after dismissal
  (`recoveringFromFailure`), landing on the saved capture's detail; **F5
  fixed** — AX labels on popover controls and idle toolbar button; honest
  save-failure copy disclosing the private plaintext temp file; capture-named
  delete confirmations in detail + Moments views; quieter toolbar New Capture;
  tabular digits in moment rows.
- Deferred findings recorded as ADR-009 (decisions.md): AAD/format
  versioning, per-root store locking, Developer ID signing (blocked on user
  identity), universal binary, testable AppModel target, F1 socket path,
  F6 rebindable shortcut.
- Commit 2016873 (orchestrator, after Sol delta re-review REQUEST-CHANGES):
  minor #15 — `isSaving` now holds through retry routing so a capture started
  in the 400 ms window can't be navigated away; honest copy in the permission
  explainer, Today card, and mic usage description ("encrypted when saved",
  not "encrypted").
- Commit c82ed0d (orchestrator, after Sol's confirmation pass OBJECTed):
  **data-loss blocker fixed** — quitting with an unresolved save failure used
  to lose the only audio copy, because next-launch reconciliation deleted the
  dead-owner temp file. Save failure now renames the temp with a `held-`
  prefix; reconciliation always preserves `held-` files; retry/export/discard
  follow the renamed URL. Two new red/green tests (19 core tests, green).
  ADR-009 addendum rewritten to match implemented behavior (crash-window
  residual deferred to M2's pending-capture manifest).

**Delta retest status:** build/lint green and all 19 core tests pass on the
fixed tree; the app was repackaged from it. The targeted Computer Use retest
of the changed flows (palette inline rows, retry→saved-detail routing, delete
dialogs, toolbar/AX spot-checks) could NOT run at closeout — the host Mac was
on the lock screen (2:17 AM). The s7/s9 evidence sets therefore show the
pre-fix behavior for F7/F8. A retest is scheduled for the morning; failing
that, the user's hands-on pass covers exactly these flows. Note: repackaging
changed the ad-hoc CDHash, so macOS will re-prompt for microphone on the
first capture of the new bundle — expected, and itself a re-run of scenario 10.

## Delta retest (2026-07-19, ~02:45–03:15 PDT, machine unlocked by user)

Tested bundle: packaged from the fixed tree at each step (worktree
`.context/Capture Delegate.app`, verified by process start time + `pgrep`
path). Driven by real Computer Use (System Events + screencapture) with the
user present.

- **Relaunch/Today (pass):** fresh launch shows the honest Today copy and the
  user's own five captures from their 12:32–12:53 AM hands-on session
  (`delta-01-relaunch-today.png`).
- **NEW BUG (fixed): display-title search mismatch.** Typing "untitled" in the
  ⌘K palette and ⌘F search matched the raw stored title — empty for untitled
  captures — so five visible "Untitled capture" rows were unfindable. Fixed in
  commit 2daee91: both predicates now match
  `SessionDisplay.title(...)` + note (`delta-02-palette-inline-results.png`
  shows the pre-fix miss rendering).
- **NEW BLOCKER (fixed): palette ghost rows / Enter-target divergence.** With
  query "untitled", the palette deterministically rendered four stale command
  rows ("Start capture", "Go to Today", "Go to Moments", "Search captures")
  plus one session row, while the underlying item array was entirely sessions
  (`delta-03-palette-ghost-rows.png`; control query "xyz" rendered the empty
  state correctly, `delta-05-palette-empty-state-control.png`). Empirical
  proof of the mismatch: pressing Enter with "Start capture" highlighted
  opened a session detail instead (`delta-04-ghost-row-enter-mismatch.png`).
  Root cause: `.id(index)` on each row overrode the
  `ForEach(id: \.element.id)` identity, so the LazyVStack never re-diffed
  existing rows when typing reshaped the list. Fixed in commit dac0fe0
  (identity removed; selection scroll now targets the element id). Build,
  lint, and all 19 core tests green; both bundles repackaged.
- **Interrupted:** the post-fix visual re-probe of the palette, the
  retry→saved-detail routing re-run, and the delete-dialog/AX spot checks were
  halted mid-flight: the user began actively using the machine (the app
  window was closed while the probe ran, and continuing to inject keystrokes
  risked typing into the user's foreground apps). These remain open for
  either an idle-machine re-run or the user's hands-on pass; the fixes
  themselves are committed and covered by the Sol re-review below.
- **Gates on the fixed tree (dac0fe0):** full `scripts/verify-m0.sh` parity
  from a fresh `/tmp` worktree exits 0; independent Sol (gpt-5.6-sol, xhigh)
  review of commits 2daee91 + dac0fe0 returned **CONFIRM** with all five
  checks PASS (identity restoration, scrollTo snapshot consistency, Enter
  alignment, predicate consistency, caption placement); one non-blocking
  adjacent note about the conditional caption being a lazy-container slow
  path.
