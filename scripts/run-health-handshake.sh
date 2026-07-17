#!/bin/zsh
set -euo pipefail

repository_root="${0:A:h:h}"
module_cache_path="${TMPDIR:-/private/tmp}/capture-delegate-module-cache"
backend_binary="$repository_root/target/release/capture-delegate-backend"
health_binary="$repository_root/.build/release/capture-delegate-health"
runtime_directory="$(mktemp -d /private/tmp/cd-health.XXXXXX)"
chmod 700 "$runtime_directory"
socket_path="$runtime_directory/health.sock"

cleanup() {
  local exit_code=$?
  if [[ -n "${backend_pid:-}" ]]; then
    kill "$backend_pid" 2>/dev/null || true
    wait "$backend_pid" 2>/dev/null || true
  fi
  rm -f "$socket_path"
  rmdir "$runtime_directory" 2>/dev/null || true
  return "$exit_code"
}
trap cleanup EXIT

cd "$repository_root"
mkdir -p "$module_cache_path"
export CLANG_MODULE_CACHE_PATH="$module_cache_path"
cargo build --release -p capture_delegate_backend
swift build -c release --disable-sandbox
"$backend_binary" --socket "$socket_path" &
backend_pid=$!

for _ in {1..100}; do
  [[ -S "$socket_path" ]] && break
  kill -0 "$backend_pid" 2>/dev/null || {
    print -u2 "backend exited before binding $socket_path"
    exit 1
  }
  sleep 0.02
done

[[ -S "$socket_path" ]] || { print -u2 "backend did not bind $socket_path"; exit 1; }
kill -0 "$backend_pid" 2>/dev/null || { print -u2 "backend is not alive before client"; exit 1; }
health_status=0
"$health_binary" --socket "$socket_path" || health_status=$?
kill -0 "$backend_pid" 2>/dev/null || { print -u2 "backend exited during handshake"; exit 1; }
(( health_status == 0 )) || exit "$health_status"
