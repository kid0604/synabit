#!/usr/bin/env bash

# Lifecycle-independent acceptance oracle for C2B-ARCH-CLOSURE-4-V2.
# Checkpoint state and Builder evidence are deliberately outside this oracle.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
harness_v3="$repo_root/.agents/oracles/c2b_oracle_v3.rs"
harness_v5="$repo_root/.agents/oracles/c2b_arch_closure_v2.rs"
harness_v6="$repo_root/.agents/oracles/c2b_arch_closure_v3.rs"
harness_v7="$repo_root/.agents/oracles/c2b_arch_closure_v4.rs"
structural_v3="$repo_root/.agents/oracles/c2b_oracle_v3_structural.py"
coordinator_file="$app_root/src/sync/coordinator.rs"
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

run_gate() {
  local label="$1"
  shift
  printf 'RUN   %s\n' "$label"
  if "$@"; then pass "$label"; else fail "$label"; fi
}

run_cargo_gate() {
  local label="$1" filter="$2"
  printf 'RUN   %s\n' "$label"
  if (cd "$app_root" && cargo test --lib "$filter" -- --nocapture); then
    pass "$label"
  else
    fail "$label"
  fi
}

check_test_inventory() {
  local output
  printf 'LIST  immutable compiled acceptance inventory\n'
  if ! output="$(cd "$app_root" && cargo test --lib -- --list 2>&1)"; then
    printf '%s\n' "$output"
    fail 'immutable compiled acceptance inventory (listing failed)'
    return
  fi
  if TEST_LISTING="$output" python3 - <<'PY'
import os

listing = os.environ["TEST_LISTING"].splitlines()
names = [line.split(": test", 1)[0] for line in listing if line.endswith(": test")]
expected = {
    "frc2b_r1_01": 5,
    "frc2b_r1_02": 3,
    "c2b_v3_pull": 10,
    "c2b_v3_provider": 1,
    "frc2b_r1_04": 2,
    "c2b_arch_closure_v1::c2b_arch_": 0,
    "c2b_arch_closure_v2::c2b_arch_v5_": 4,
    "c2b_arch_closure_v3::c2b_arch_v6_": 3,
    "c2b_arch_closure_v4::c2b_arch_v7_": 2,
}
errors = []
for marker, count in expected.items():
    actual = sum(marker in name for name in names)
    if actual != count:
        errors.append(f"{marker}: expected {count}, found {actual}")
if errors:
    print("; ".join(errors))
    raise SystemExit(1)
print("compiled inventory: " + ", ".join(f"{key}={value}" for key, value in expected.items()))
PY
  then
    pass 'immutable compiled acceptance inventory'
  else
    printf '%s\n' "$output"
    fail 'immutable compiled acceptance inventory'
  fi
}

printf 'Synabit C2B architecture closure 4 — C2B-ARCH-CLOSURE-4-V2 / C2B-ORACLE-V7R1\n'

check_digest 'immutable V3 behavioral harness' "$harness_v3" 'f5950e49044929012a7bdcd1b4f40858092aba64701cd00adfc06a24471d6451'
check_digest 'immutable V3 structural analyzer' "$structural_v3" '06e2da3a6aaf071d115153e76fb19c6049b8f6d4982a604f715eb15e8df0a17b'
check_digest 'immutable V5 closure harness' "$harness_v5" 'cdebeba62a73baf2286340ee06578d61df663558d2f895d896a75e0bbdc51d18'
check_digest 'immutable V6 closure harness' "$harness_v6" 'bf92b5c35d1e80bf68b231c28f54462765a5b94d367850d93c28fe5d4eef4095'
check_digest 'immutable V7 closure harness' "$harness_v7" 'd69af783db53822ed9460a65cec01de5fc2c4146b725587cebf4eb236a40c18e'

check_digest 'scope lock: accepted coordinator and snapshots' "$coordinator_file" '4037274937c43bfe4618ae5df6d417785e6b9305f70d993cb9ff48d39c91cb29'
check_digest 'scope lock: DB module boundary' "$app_root/src/db/mod.rs" '6c6ffed5584d2e2a7ccfaf08129488683bbae478e3a56d748721a2e4c1c84538'
check_digest 'scope lock: accepted provider DAO and corrupt fixture' "$app_root/src/db/sync_provider_state.rs" 'c4dddc7d986c8acc0238eb41f07af8ea0efc48c98dd82ebf4a142138f60bbba9'
check_digest 'scope lock: accepted legacy migration and corrupt fixture' "$app_root/src/db/legacy_sync_migration.rs" '6b75a595cae57f21a832390a11719dbcca2b8db6cb6a302efe9fb67cb4ebf835'
check_digest 'scope lock: accepted outbox DAO' "$app_root/src/db/sync_outbox.rs" '9174a0eacf972732fecb52f256890d02f798bc211f289f906c22c83de4be0574'
check_digest 'scope lock: accepted local preparation' "$app_root/src/sync/core/change.rs" 'e74b403f8a06dbc7d4a79b8f47931dc9288fd85b17aece25fe2e437a94df941e'
check_digest 'scope lock: accepted remote apply' "$app_root/src/sync/core/apply.rs" 'cf5e44f7771d53f22fe916da412365967ace8ae494b67f84be1932ae7d5d4a86'
check_digest 'scope lock: accepted protocol facade' "$app_root/src/sync/protocol.rs" '463935b9927fbc3984c5827521c60270d06bd5dc7dc663be0fd9b41d0e8da692'

run_gate 'FRC2B-R1-01 dispatcher structural bans' python3 "$structural_v3" dispatcher "$coordinator_file"
run_gate 'FRC2B-R1-02 typed validation structural bans' python3 "$structural_v3" typed "$coordinator_file"
run_gate 'FRC2B-R1-03 no Builder proxy padding' python3 "$structural_v3" proxy "$coordinator_file"
run_gate 'FRC2B-R1-04 exact raw snapshot structure' python3 "$structural_v3" snapshot "$coordinator_file"
run_gate 'FRC2B-R1-05 no token or handoff padding' python3 "$structural_v3" hygiene "$coordinator_file"

check_test_inventory
run_gate 'Rust formatting' cargo fmt --manifest-path "$app_root/Cargo.toml" -- --check
run_cargo_gate 'accepted C2B dispatcher behavior' 'frc2b_r1_01'
run_cargo_gate 'accepted C2B typed payload behavior' 'frc2b_r1_02'
run_cargo_gate 'accepted C2B durable pull behavior' 'c2b_v3_pull'
run_cargo_gate 'accepted C2B provider mapping behavior' 'c2b_v3_provider'
run_cargo_gate 'accepted C2B malformed snapshot behavior' 'frc2b_r1_04'
run_cargo_gate 'accepted V5 migration/failure behavior' 'c2b_arch_closure_v2::c2b_arch_v5_'
run_cargo_gate 'accepted V6 exact-invariant behavior' 'c2b_arch_closure_v3::c2b_arch_v6_'
run_cargo_gate 'C2B V7 all-state identity behavior' 'c2b_arch_closure_v4::c2b_arch_v7_'

run_gate 'full Rust library regression excluding known unrelated search gate' \
  cargo test --manifest-path "$app_root/Cargo.toml" --lib -- \
  --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'C2B_ORACLE_V7R1_FAIL failures=%s\n' "$failure_count"
  exit 1
fi

printf 'C2B_ORACLE_V7R1_PASS\n'
