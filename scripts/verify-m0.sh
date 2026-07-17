#!/bin/zsh
set -euo pipefail

repository_root="${0:A:h:h}"
module_cache_path="${TMPDIR:-/private/tmp}/capture-delegate-module-cache"
cd "$repository_root"

mkdir -p "$module_cache_path"
export CLANG_MODULE_CACHE_PATH="$module_cache_path"

python3 - .loop/traceability.md <<'PY'
import pathlib
import sys

ledger_path = pathlib.Path(sys.argv[1])
rows = {}
for line in ledger_path.read_text().splitlines():
    if not line.startswith("|"):
        continue
    cells = [cell.strip() for cell in line.strip("|").split("|")]
    if len(cells) != 8 or cells[0] in {"Requirement ID", "---"}:
        continue
    rows[cells[0]] = cells

original_ids = [identifier for identifier in rows if identifier.startswith(("CTL-", "S"))]
if len(original_ids) != 287:
    raise SystemExit(f"traceability must retain 287 original rows; found {len(original_ids)}")

expected_m0_ids = [f"M0-FOUNDATION-{index:03d}" for index in range(1, 7)]
actual_m0_ids = sorted(identifier for identifier in rows if identifier.startswith("M0-FOUNDATION-"))
if actual_m0_ids != expected_m0_ids:
    raise SystemExit(
        f"traceability must contain {expected_m0_ids}; found {actual_m0_ids}"
    )

valid_statuses = {"todo", "in_progress", "implemented", "verified", "blocked", "rejected"}
for identifier, cells in rows.items():
    implementation, verification, pr, status = cells[4], cells[5], cells[6], cells[7]
    if status not in valid_statuses:
        raise SystemExit(f"{identifier} has invalid status {status!r}")
    if status in {"implemented", "verified", "blocked"} and implementation == "—":
        raise SystemExit(f"{identifier} requires implementation evidence for status {status}")
    if status in {"implemented", "verified", "blocked"} and verification == "—":
        raise SystemExit(f"{identifier} requires verification or blocker evidence for status {status}")
    if status == "verified" and pr == "—":
        raise SystemExit(f"{identifier} requires PR evidence for verified status")

for identifier in expected_m0_ids:
    cells = rows[identifier]
    if cells[7] not in {"in_progress", "implemented", "verified", "blocked"}:
        raise SystemExit(
            f"{identifier} must be in_progress, implemented, verified, or blocked; found {cells[7]!r}"
        )
PY

cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
swift format lint --recursive --strict Sources Tests Package.swift
cargo build --release -p capture_delegate_backend
swift build -c release --disable-sandbox
CAPTURE_DELEGATE_BACKEND_BINARY="$repository_root/target/release/capture-delegate-backend" \
  CAPTURE_DELEGATE_HEALTH_BINARY="$repository_root/.build/release/capture-delegate-health" \
  swift test --disable-sandbox
scripts/run-health-handshake.sh
