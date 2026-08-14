#!/usr/bin/env bash

# Lifecycle-independent acceptance oracle for D1-TOMBSTONE-IDENTITY-V2.
# Checkpoint state and Builder evidence are deliberately outside this oracle.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
server_root="$repo_root/sync-server"
d1_harness="$repo_root/.agents/oracles/d1_tombstone_identity.rs"
d1_structural="$repo_root/.agents/oracles/d1_tombstone_identity_structural_v2.py"
c2b_harness="$repo_root/.agents/oracles/c2b_oracle_v3.rs"
c2b_typed_compat="$repo_root/.agents/oracles/d1_c2b_typed_compat.rs"
c2b_structural="$repo_root/.agents/oracles/c2b_oracle_v3_structural.py"
protocol_file="$server_root/synabit-protocol/src/lib.rs"
change_file="$app_root/src/sync/core/change.rs"
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

run_client_gate() {
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
    "c2b_arch_closure_v2::c2b_arch_v5_": 4,
    "c2b_arch_closure_v3::c2b_arch_v6_": 3,
    "c2b_arch_closure_v4::c2b_arch_v7_": 2,
    "d1_tombstone_identity::d1_tombstone_": 2,
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

printf 'Synabit D1 typed tombstone identity — D1-TOMBSTONE-IDENTITY-V2 / D1-ORACLE-V2\n'

check_digest 'immutable D1 behavioral harness' "$d1_harness" '99148452aa93e0ef0e208468c9d6a0829d1b15f9a8df48f4250eb91998f8d0c6'
check_digest 'immutable D1 V2 structural analyzer' "$d1_structural" '6947b3d24519aaf65cbacf158c802a103f39c39d24e89f2fb0fd06e7f7b5383a'
check_digest 'accepted C2B behavioral harness' "$c2b_harness" 'f5950e49044929012a7bdcd1b4f40858092aba64701cd00adfc06a24471d6451'
check_digest 'immutable typed-delete C2B successor harness' "$c2b_typed_compat" 'd8e8e96ca3b3570d210f891ba0de806fd778f0057dc3063fe9966ddb77958c91'
check_digest 'accepted C2B structural analyzer' "$c2b_structural" '06e2da3a6aaf071d115153e76fb19c6049b8f6d4982a604f715eb15e8df0a17b'
check_digest 'scope lock: DB module boundary' "$app_root/src/db/mod.rs" '6c6ffed5584d2e2a7ccfaf08129488683bbae478e3a56d748721a2e4c1c84538'
check_digest 'scope lock: durable inbox DAO' "$app_root/src/db/sync_inbox.rs" '7847fe4dce809448e8383f748a991f0081bab5c012a38e5719e345adfe435276'
check_digest 'scope lock: durable outbox DAO' "$app_root/src/db/sync_outbox.rs" '9174a0eacf972732fecb52f256890d02f798bc211f289f906c22c83de4be0574'
check_digest 'scope lock: provider DAO' "$app_root/src/db/sync_provider_state.rs" 'c4dddc7d986c8acc0238eb41f07af8ea0efc48c98dd82ebf4a142138f60bbba9'
check_digest 'scope lock: schema' "$app_root/src/db/schema.rs" 'a3741d772faf9dfc4f12e5c210beb163d06dc50ff23b78581803254b1958b53b'
check_digest 'scope lock: legacy migration' "$app_root/src/db/legacy_sync_migration.rs" '6b75a595cae57f21a832390a11719dbcca2b8db6cb6a302efe9fb67cb4ebf835'
check_digest 'scope lock: remote document apply' "$app_root/src/sync/core/apply.rs" 'cf5e44f7771d53f22fe916da412365967ace8ae494b67f84be1932ae7d5d4a86'
check_digest 'scope lock: client protocol facade' "$app_root/src/sync/protocol.rs" '463935b9927fbc3984c5827521c60270d06bd5dc7dc663be0fd9b41d0e8da692'
check_digest 'scope lock: GDrive adapter' "$app_root/src/sync/adapter/gdrive.rs" '4d053d9df1c1784718de94c1813a7e28e8cfa4632abbe4a537d4421a8db46bc7'
check_digest 'scope lock: server adapter' "$app_root/src/sync/adapter/server.rs" 'a3af20be5dc9aff52348b1343383b9b518742d380884aabad42103e623016363'
check_digest 'scope lock: mailbox server' "$server_root/src/mailbox.rs" '58107930d22f4652206b73dffbf16544f813a683e604efdd791c8065992aaaee'

run_gate 'D1 typed tombstone structural bans' \
  python3 "$d1_structural" "$protocol_file" "$change_file" "$coordinator_file"
run_gate 'accepted C2B dispatcher structure' python3 "$c2b_structural" dispatcher "$coordinator_file"
run_gate 'accepted C2B typed validation structure' python3 "$c2b_structural" typed "$coordinator_file"
run_gate 'accepted C2B proxy bans' python3 "$c2b_structural" proxy "$coordinator_file"
run_gate 'accepted C2B raw snapshot structure' python3 "$c2b_structural" snapshot "$coordinator_file"
run_gate 'accepted C2B hygiene bans' python3 "$c2b_structural" hygiene "$coordinator_file"

check_test_inventory
run_gate 'client Rust formatting' cargo fmt --manifest-path "$app_root/Cargo.toml" -- --check
run_gate 'protocol/server Rust formatting' cargo fmt --manifest-path "$server_root/Cargo.toml" --all -- --check
run_gate 'shared protocol regression' cargo test --manifest-path "$server_root/Cargo.toml" -p synabit-protocol
run_client_gate 'D1 typed tombstone behavior' 'd1_tombstone_identity::d1_tombstone_'
run_client_gate 'accepted C2B dispatcher behavior' 'frc2b_r1_01'
run_client_gate 'accepted C2B typed payload behavior' 'frc2b_r1_02'
run_client_gate 'accepted C2B durable pull behavior' 'c2b_v3_pull'
run_client_gate 'accepted C2B provider mapping behavior' 'c2b_v3_provider'
run_client_gate 'accepted C2B malformed snapshot behavior' 'frc2b_r1_04'
run_client_gate 'accepted V5 migration/failure behavior' 'c2b_arch_closure_v2::c2b_arch_v5_'
run_client_gate 'accepted V6 exact-invariant behavior' 'c2b_arch_closure_v3::c2b_arch_v6_'
run_client_gate 'accepted V7 identity behavior' 'c2b_arch_closure_v4::c2b_arch_v7_'
run_gate 'full client Rust regression excluding known unrelated search gate' \
  cargo test --manifest-path "$app_root/Cargo.toml" --lib -- \
  --skip search::tests::test_unknown_type_filter_ignored
run_gate 'full sync-server workspace regression' \
  cargo test --manifest-path "$server_root/Cargo.toml" --workspace

if (( failure_count > 0 )); then
  printf 'D1_ORACLE_V2_FAIL failures=%s\n' "$failure_count"
  exit 1
fi

printf 'D1_ORACLE_V2_PASS\n'

