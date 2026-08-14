#!/usr/bin/env bash

set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
plan_file="$repo_root/docs/sync_implementation_plan.md"

python3 - "$plan_file" "$repo_root" <<'PY'
import hashlib
import os
import pathlib
import re
import sys

plan_path = pathlib.Path(sys.argv[1])
repo_root = pathlib.Path(sys.argv[2]).resolve()

if not plan_path.is_file():
    print("SYNC_NEXT_INVALID_CHECKPOINT: plan file is missing")
    raise SystemExit(12)

text = plan_path.read_text(encoding="utf-8")
tick = chr(96)

required_fields = (
    "Current milestone",
    "Execution status",
    "Internal repair loops",
    "External audit status",
    "Closure contract ID",
    "Closure contract status",
    "External oracle version",
    "External oracle path",
    "External oracle SHA-256",
    "External oracle result",
    "Next batch ID",
    "Stop boundary",
    "Workflow control SHA-256",
)


def fail(message, code=12):
    print(f"SYNC_NEXT_INVALID_CHECKPOINT: {message}")
    raise SystemExit(code)


def read_field(name):
    prefix = f"- {name}: "
    matches = [line for line in text.splitlines() if line.startswith(prefix)]
    if len(matches) != 1:
        fail(f"field {name!r} must occur exactly once, found {len(matches)}")
    raw = matches[0][len(prefix):].strip()
    if len(raw) < 2 or raw[0] != tick or raw[-1] != tick:
        fail(f"field {name!r} must be enclosed by inline-code delimiters")
    return raw[1:-1]


fields = {name: read_field(name) for name in required_fields}

control_digest = fields["Workflow control SHA-256"]
if re.fullmatch(r"[0-9a-f]{64}", control_digest) is None:
    fail("Workflow control SHA-256 must be 64 lowercase hex characters")

control_files = (
    ".agents/agents.md",
    ".agents/rules/synabit-sync.md",
    ".agents/workflows/sync-next.md",
    ".agents/skills/implement-sync-batch.md",
    ".agents/skills/audit-sync-batch.md",
    ".agents/scripts/sync-next-preflight.sh",
    ".agents/scripts/sync-next-verify.sh",
    ".agents/scripts/sync-next-checkpoint.sh",
)
control_hasher = hashlib.sha256()
for relative in control_files:
    control_path = repo_root / relative
    if not control_path.is_file():
        fail(f"workflow control is missing: {relative}")
    control_hasher.update(relative.encode("utf-8"))
    control_hasher.update(b"\0")
    control_hasher.update(control_path.read_bytes())
    control_hasher.update(b"\0")
actual_control_digest = control_hasher.hexdigest()
if actual_control_digest != control_digest:
    print(
        "BLOCKED_WORKFLOW_CONTROL_MUTATED "
        f"expected={control_digest} actual={actual_control_digest}"
    )
    raise SystemExit(14)

execution_states = {
    "blocked",
    "ready_for_builder",
    "building",
    "qa_failed",
    "awaiting_external_audit",
}
external_states = {
    "not_started",
    "repair_required",
    "awaiting_external_audit",
    "accepted",
    "testability_or_architecture_blocker",
}
contract_states = {"not_published", "frozen", "accepted", "retired"}

if fields["Execution status"] not in execution_states:
    fail(f"unknown Execution status {fields['Execution status']!r}")
if fields["External audit status"] not in external_states:
    fail(f"unknown External audit status {fields['External audit status']!r}")
if fields["Closure contract status"] not in contract_states:
    fail(f"unknown Closure contract status {fields['Closure contract status']!r}")

try:
    repair_loops = int(fields["Internal repair loops"])
except ValueError:
    fail("Internal repair loops must be an integer")
if repair_loops < 0 or repair_loops > 2:
    fail("Internal repair loops must be between 0 and 2")

execution = fields["Execution status"]
external = fields["External audit status"]
next_batch = fields["Next batch ID"]

if execution == "awaiting_external_audit" or external == "awaiting_external_audit":
    print("SYNC_NEXT_AWAITING_EXTERNAL_AUDIT")
    print(f"milestone={fields['Current milestone']}")
    print(f"batch={next_batch}")
    raise SystemExit(11)

if (
    execution == "blocked"
    or external == "testability_or_architecture_blocker"
    or next_batch.startswith("BLOCKED")
):
    print("SYNC_NEXT_BLOCKED")
    print(f"milestone={fields['Current milestone']}")
    print(f"reason={fields['Stop boundary']}")
    raise SystemExit(10)

if execution not in {"ready_for_builder", "building", "qa_failed"}:
    fail(f"Execution status {execution!r} is not executable")
if external not in {"not_started", "repair_required"}:
    fail(f"External audit status {external!r} is not executable")
if fields["Closure contract status"] != "frozen":
    fail("an executable batch requires Closure contract status frozen")
if fields["Closure contract ID"].startswith(("NONE", "BLOCKED")):
    fail("an executable batch requires a published contract ID")
if "RETIRED" in fields["External oracle version"].upper():
    fail("retired oracle cannot execute")
if fields["External oracle result"] != "RED_BASELINE":
    fail("External oracle result must be exactly RED_BASELINE before execution")

digest = fields["External oracle SHA-256"]
if re.fullmatch(r"[0-9a-f]{64}", digest) is None:
    fail("External oracle SHA-256 must be 64 lowercase hex characters")

oracle_rel = pathlib.PurePosixPath(fields["External oracle path"])
if oracle_rel.is_absolute() or ".." in oracle_rel.parts:
    fail("External oracle path must be a safe repository-relative path")
oracle_path = (repo_root / pathlib.Path(*oracle_rel.parts)).resolve()
scripts_root = (repo_root / ".agents" / "scripts").resolve()
if os.path.commonpath((str(oracle_path), str(scripts_root))) != str(scripts_root):
    fail("External oracle must be under .agents/scripts")
if not oracle_path.is_file():
    fail(f"External oracle does not exist: {oracle_rel}")

actual_digest = hashlib.sha256(oracle_path.read_bytes()).hexdigest()
if actual_digest != digest:
    print(f"BLOCKED_EXTERNAL_ORACLE_MUTATED expected={digest} actual={actual_digest}")
    raise SystemExit(13)

begin_marker = "<!-- ACTIVE_CONTRACT_BEGIN -->"
end_marker = "<!-- ACTIVE_CONTRACT_END -->"
if text.count(begin_marker) != 1 or text.count(end_marker) != 1:
    fail("active contract markers must each occur exactly once")
start = text.index(begin_marker) + len(begin_marker)
end = text.index(end_marker)
if start >= end:
    fail("active contract markers are out of order")
contract = text[start:end]
contract_id = fields["Closure contract ID"]
if contract_id not in contract:
    fail("marked active contract does not contain Closure contract ID")

criterion_ids = []
for line in contract.splitlines():
    if not line.startswith("#### "):
        continue
    label = line[5:].strip()
    if label.startswith(tick) and tick in label[1:]:
        candidate = label[1:].split(tick, 1)[0]
    else:
        candidate = label.split(None, 1)[0]
    if re.fullmatch(r"[A-Z][A-Z0-9-]{2,}", candidate):
        criterion_ids.append(candidate)

if not criterion_ids:
    fail("marked active contract contains no criterion IDs")
if len(criterion_ids) > 8:
    fail(f"active contract has {len(criterion_ids)} criteria; maximum is 8")
if len(set(criterion_ids)) != len(criterion_ids):
    fail("active contract contains duplicate criterion IDs")

print("SYNC_NEXT_READY")
print(f"milestone={fields['Current milestone']}")
print(f"execution_status={execution}")
print(f"batch={next_batch}")
print(f"contract={contract_id}")
print(f"criteria={','.join(criterion_ids)}")
print(f"repair_loops={repair_loops}")
print(f"oracle={fields['External oracle version']}")
print(f"oracle_path={fields['External oracle path']}")
print(f"oracle_sha256={digest}")
PY
