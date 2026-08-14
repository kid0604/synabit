#!/usr/bin/env bash

# Immutable external acceptance oracle for C2B-REPAIR-2-FINAL-V1.
# Builder may execute this file but may not modify it or its oracle resources.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
plan_file="$repo_root/docs/sync_implementation_plan.md"
evidence_file="$repo_root/.agents/runtime/sync-next-evidence.tsv"
harness_file="$repo_root/.agents/oracles/c2b_oracle_v3.rs"
structural_file="$repo_root/.agents/oracles/c2b_oracle_v3_structural.py"
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
import sys

listing = os.environ["TEST_LISTING"].splitlines()
names = [line.split(": test", 1)[0] for line in listing if line.endswith(": test")]
expected = {
    "frc2b_r1_01": 5,
    "frc2b_r1_02": 3,
    "c2b_v3_pull": 10,
    "c2b_v3_provider": 1,
    "frc2b_r1_04": 2,
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

check_checkpoint_and_evidence() {
  python3 - "$plan_file" "$evidence_file" <<'PY'
import pathlib
import re
import sys

plan_path = pathlib.Path(sys.argv[1])
evidence_path = pathlib.Path(sys.argv[2])
plan = plan_path.read_text(encoding="utf-8")

def field(name):
    matches = re.findall(rf"^- {re.escape(name)}: `([^`]*)`$", plan, re.M)
    if len(matches) != 1:
        raise SystemExit(f"checkpoint field {name!r} must occur once")
    return matches[0]

expected = {
    "Closure contract ID": "C2B-REPAIR-2-FINAL-V1",
    "Closure contract status": "frozen",
    "External oracle version": "C2B-ORACLE-V3",
    "External audit status": "repair_required",
    "Next batch ID": "C2B-REPAIR-2-FINAL",
}
for name, value in expected.items():
    actual = field(name)
    if actual != value:
        raise SystemExit(f"{name}: expected {value!r}, found {actual!r}")
if field("Execution status") not in {"building", "qa_failed"}:
    raise SystemExit("oracle verification requires building or qa_failed checkpoint state")

if not evidence_path.is_file():
    raise SystemExit("evidence TSV is missing")
lines = evidence_path.read_text(encoding="utf-8").splitlines()
header = "criterion_id\tstatus\tproduction\tobservation\tevidence\tcounterfactual"
if not lines or lines[0] != header:
    raise SystemExit("evidence TSV header is invalid")
expected_ids = [
    "C2BF-DISPATCH",
    "C2BF-TYPED",
    "C2BF-PULL",
    "C2BF-PROVIDERS",
    "C2BF-SNAPSHOT",
    "C2BF-HYGIENE",
]
actual_ids = []
for index, line in enumerate(lines[1:], start=2):
    cells = line.split("\t")
    if len(cells) != 6:
        raise SystemExit(f"evidence row {index} must contain six TSV cells")
    criterion, status, production, observation, evidence, counterfactual = cells
    if status != "PASS":
        raise SystemExit(f"evidence row {index} status is not PASS")
    if re.search(r"[^\s:]+:\d+", production) is None:
        raise SystemExit(f"evidence row {index} lacks source path:line")
    if any(len(value.strip()) < 16 for value in (observation, evidence, counterfactual)):
        raise SystemExit(f"evidence row {index} contains a placeholder cell")
    actual_ids.append(criterion)
if actual_ids != expected_ids:
    raise SystemExit(f"evidence IDs/order differ: expected={expected_ids} actual={actual_ids}")
print("checkpoint and six evidence rows are coherent")
PY
}

printf 'Synabit C2B final repair — C2B-REPAIR-2-FINAL-V1 / C2B-ORACLE-V3\n'

check_digest 'immutable behavioral harness' "$harness_file" 'f5950e49044929012a7bdcd1b4f40858092aba64701cd00adfc06a24471d6451'
check_digest 'immutable structural analyzer' "$structural_file" '06e2da3a6aaf071d115153e76fb19c6049b8f6d4982a604f715eb15e8df0a17b'

check_digest 'scope lock: accepted schema v6' "$app_root/src/db/schema.rs" '542c97ba149ac5809444f0b3572e995e8f7ead6cdd338ed461d85437c0d946de'
check_digest 'scope lock: accepted outbox DAO' "$app_root/src/db/sync_outbox.rs" '9174a0eacf972732fecb52f256890d02f798bc211f289f906c22c83de4be0574'
check_digest 'scope lock: accepted provider DAO' "$app_root/src/db/sync_provider_state.rs" '202035fb064777915f23319726eca774974bd2b66686b285a36d18b559785325'
check_digest 'scope lock: accepted local preparation' "$app_root/src/sync/core/change.rs" 'e74b403f8a06dbc7d4a79b8f47931dc9288fd85b17aece25fe2e437a94df941e'
check_digest 'scope lock: accepted remote apply' "$app_root/src/sync/core/apply.rs" 'cf5e44f7771d53f22fe916da412365967ace8ae494b67f84be1932ae7d5d4a86'
check_digest 'scope lock: accepted protocol facade' "$app_root/src/sync/protocol.rs" '463935b9927fbc3984c5827521c60270d06bd5dc7dc663be0fd9b41d0e8da692'
check_digest 'scope lock: DB module boundary' "$app_root/src/db/mod.rs" '549dc383862fb6f4e30ee7664f9a5b9bf3d2b14f11a6046266b5f36ea81a1ffd'

run_gate 'FRC2B-R1-01 dispatcher structural bans' python3 "$structural_file" dispatcher "$coordinator_file"
run_gate 'FRC2B-R1-02 typed validation structural bans' python3 "$structural_file" typed "$coordinator_file"
run_gate 'FRC2B-R1-03 no Builder proxy padding' python3 "$structural_file" proxy "$coordinator_file"
run_gate 'FRC2B-R1-04 exact raw snapshot structure' python3 "$structural_file" snapshot "$coordinator_file"
run_gate 'FRC2B-R1-05 no token or handoff padding' python3 "$structural_file" hygiene "$coordinator_file"

check_test_inventory
run_gate 'Rust formatting' cargo fmt --manifest-path "$app_root/Cargo.toml" -- --check
run_cargo_gate 'C2BF-DISPATCH behavioral outcomes' 'frc2b_r1_01'
run_cargo_gate 'C2BF-TYPED exact typed payloads' 'frc2b_r1_02'
run_cargo_gate 'C2BF-PULL durable ordering and recovery' 'c2b_v3_pull'
run_cargo_gate 'C2BF-PROVIDERS native position mapping' 'c2b_v3_provider'
run_cargo_gate 'C2BF-SNAPSHOT malformed raw data' 'frc2b_r1_04'
run_gate 'C2BF-HYGIENE checkpoint and evidence' check_checkpoint_and_evidence

run_gate 'full Rust library regression excluding known unrelated search gate' \
  cargo test --manifest-path "$app_root/Cargo.toml" --lib -- \
  --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'C2B_ORACLE_V3_FAIL failures=%s\n' "$failure_count"
  exit 1
fi

printf 'C2B_ORACLE_V3_PASS\n'
