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
