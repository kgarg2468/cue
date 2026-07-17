#!/bin/zsh
set -euo pipefail

repository_root="${0:A:h:h}"
executable_path="${1:-$repository_root/.build/release/CaptureDelegateApp}"
timeout_seconds="${CAPTURE_DELEGATE_LAUNCH_TIMEOUT_SECONDS:-10}"
diagnostics_path="${TMPDIR:-/private/tmp}/capture-delegate-app-launch-${$}"
process_id=""
succeeded=false

mkdir -p "$diagnostics_path"

cleanup() {
  if [[ -n "$process_id" ]] && kill -0 "$process_id" 2>/dev/null; then
    kill "$process_id" 2>/dev/null || true
    wait "$process_id" 2>/dev/null || true
  fi
  if [[ "$succeeded" == "true" ]]; then
    rm -rf -- "$diagnostics_path"
  fi
}
trap cleanup EXIT

record_diagnostics() {
  {
    print -- "executable: $executable_path"
    print -- "pid: ${process_id:-not started}"
    print -- "last System Events state: ${application_state:-unavailable}"
    if [[ -n "$process_id" ]]; then
      ps -p "$process_id" -o pid=,stat=,command= 2>&1 || true
      lsappinfo info -only ApplicationType "$process_id" 2>&1 || true
      lsappinfo info "$process_id" 2>&1 || true
    fi
  } > "$diagnostics_path/runtime.txt"
  print -u2 -- "App launch smoke failed; diagnostics: $diagnostics_path"
  print -u2 -- "--- stdout ---"
  cat "$diagnostics_path/stdout.txt" >&2 || true
  print -u2 -- "--- stderr ---"
  cat "$diagnostics_path/stderr.txt" >&2 || true
  print -u2 -- "--- runtime ---"
  cat "$diagnostics_path/runtime.txt" >&2 || true
  if [[ -s "$diagnostics_path/system-events-error.txt" ]]; then
    print -u2 -- "--- System Events error ---"
    cat "$diagnostics_path/system-events-error.txt" >&2 || true
  fi
}

system_events_state() {
  osascript - "$process_id" <<'APPLESCRIPT'
on run argv
    set targetPID to item 1 of argv as integer
    tell application "System Events"
        set matchingProcesses to every process whose unix id is targetPID
        if (count of matchingProcesses) is 0 then return "missing|false|0"
        set targetProcess to item 1 of matchingProcesses
        set isBackgroundOnly to background only of targetProcess
        set isVisible to visible of targetProcess
        set windowCount to count of windows of targetProcess
        return (isBackgroundOnly as text) & "|" & (isVisible as text) & "|" & (windowCount as text)
    end tell
end run
APPLESCRIPT
}

state_is_regular_and_windowed() {
  local state="$1"
  local background_only="${state%%|*}"
  local remainder="${state#*|}"
  local visible="${remainder%%|*}"
  local window_count="${remainder##*|}"

  [[ "$background_only" == "false" && "$visible" == "true" && "$window_count" == <-> ]] \
    && ((window_count >= 1))
}

if [[ ! -x "$executable_path" ]]; then
  print -u2 -- "Release executable is missing or not executable: $executable_path"
  exit 1
fi

"$executable_path" >"$diagnostics_path/stdout.txt" 2>"$diagnostics_path/stderr.txt" &
process_id=$!
deadline=$((SECONDS + timeout_seconds))
application_state="unavailable"

while (( SECONDS < deadline )); do
  if ! kill -0 "$process_id" 2>/dev/null; then
    record_diagnostics
    exit 1
  fi

  if application_state=$(system_events_state 2>"$diagnostics_path/system-events-error.txt"); then
    if state_is_regular_and_windowed "$application_state"; then
      window_count="${application_state##*|}"
      sleep 1
      if kill -0 "$process_id" 2>/dev/null \
        && application_state=$(system_events_state 2>>"$diagnostics_path/system-events-error.txt") \
        && state_is_regular_and_windowed "$application_state"; then
        print -- "App launch smoke passed: regular foreground process with $window_count native window(s)."
        succeeded=true
        exit 0
      fi
    fi
  fi
  sleep 0.2
done

record_diagnostics
exit 1
