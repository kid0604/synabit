#!/usr/bin/env bash

# Lifecycle-independent acceptance oracle for D2 server tombstone transport.
# Checkpoint state and Builder evidence are deliberately outside this oracle.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
server_root="$repo_root/sync-server"
d2_harness="$repo_root/.agents/oracles/d2_server_tombstone_transport.rs"
failure_count=0

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1"; failure_count=$((failure_count + 1)); }

sha256_file() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

check_digest() {
  local label="$1" file="$2" expected="$3" actual
  if [[ ! -f "$file" ]]; then
    fail "$label (missing $file)"
    return
  fi
  actual="$(sha256_file "$file")"
  if [[ "$actual" == "$expected" ]]; then
    pass "$label"
  else
    fail "$label (expected $expected, actual $actual)"
  fi
}

check_tree_digest() {
  local label="$1" root="$2" expected="$3"
  shift 3
  local actual
  if ! actual="$(python3 - "$repo_root" "$root" "$@" <<'PY'
import hashlib
import pathlib
import sys

repo = pathlib.Path(sys.argv[1]).resolve()
root = pathlib.Path(sys.argv[2]).resolve()
excluded = set(sys.argv[3:])
h = hashlib.sha256()
for path in sorted(item for item in root.rglob("*") if item.is_file()):
    relative = path.relative_to(repo).as_posix()
    if relative in excluded:
        continue
    h.update(relative.encode("utf-8"))
    h.update(b"\0")
    h.update(path.read_bytes())
    h.update(b"\0")
print(h.hexdigest())
PY
)"; then
    fail "$label (tree digest failed)"
    return
  fi
  if [[ "$actual" == "$expected" ]]; then
    pass "$label"
  else
    fail "$label (expected $expected, actual $actual)"
  fi
}

run_gate() {
  local label="$1"
  shift
  printf 'RUN   %s\n' "$label"
  if "$@"; then pass "$label"; else fail "$label"; fi
}

check_test_inventory() {
  local output
  printf 'LIST  immutable D2 compiled acceptance inventory\n'
  if ! output="$(cd "$server_root" && cargo test -p synabit-sync-server -- --list 2>&1)"; then
    printf '%s\n' "$output"
    fail 'immutable D2 compiled acceptance inventory (listing failed)'
    return
  fi
  if TEST_LISTING="$output" python3 - <<'PY'
import os

names = [
    line.split(": test", 1)[0]
    for line in os.environ["TEST_LISTING"].splitlines()
    if line.endswith(": test")
]
count = sum("d2_server_tombstone_transport::d2_tombstone_" in name for name in names)
if count != 3:
    print(f"expected 3 compiled D2 tests, found {count}")
    raise SystemExit(1)
print("compiled D2 inventory: 3 tests")
PY
  then
    pass 'immutable D2 compiled acceptance inventory'
  else
    printf '%s\n' "$output"
    fail 'immutable D2 compiled acceptance inventory'
  fi
}

printf 'Synabit D2 server tombstone transport — D2-SERVER-TOMBSTONE-TRANSPORT-V1 / D2-ORACLE-V1\n'

check_digest 'immutable D2 behavioral harness' "$d2_harness" '3c3c84ffc502485ca8532243f64dd7124809eb7a50d625f3488a0939e2996523'
check_tree_digest 'scope lock: complete client production tree' \
  "$app_root/src" '1acccc5145ad3db9bae7f7e37aeb37da9cfeac8319301c4b39d4248407f9c0cf'
check_tree_digest 'scope lock: shared protocol production tree' \
  "$server_root/synabit-protocol/src" '8cbb5b093e21f28aaf7c3da9b4b68921288523bb430027ac609f65e9fa5c3ba9'
check_tree_digest 'scope lock: server production outside mailbox and DB' \
  "$server_root/src" '56d253c5b2c821442acd7f6cc6bea0df7a2fd5b2eb92ba58aa650eda101d7c05' \
  'sync-server/src/mailbox.rs' 'sync-server/src/db.rs'

check_test_inventory
run_gate 'client Rust formatting' cargo fmt --manifest-path "$app_root/Cargo.toml" -- --check
run_gate 'protocol/server Rust formatting' cargo fmt --manifest-path "$server_root/Cargo.toml" --all -- --check
run_gate 'D2 server tombstone behavior' \
  cargo test --manifest-path "$server_root/Cargo.toml" -p synabit-sync-server \
  'd2_server_tombstone_transport::d2_tombstone_' -- --nocapture
run_gate 'shared protocol regression' \
  cargo test --manifest-path "$server_root/Cargo.toml" -p synabit-protocol
run_gate 'full sync-server workspace regression' \
  cargo test --manifest-path "$server_root/Cargo.toml" --workspace
run_gate 'full client Rust regression excluding known unrelated search gate' \
  cargo test --manifest-path "$app_root/Cargo.toml" --lib -- \
  --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'D2_ORACLE_V1_FAIL failures=%s\n' "$failure_count"
  exit 1
fi

printf 'D2_ORACLE_V1_PASS\n'
