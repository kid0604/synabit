#!/usr/bin/env bash

set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
plan_file="$repo_root/docs/sync_implementation_plan.md"
mode="${1:-}"
reason="${2:-unspecified}"

case "$mode" in
  begin|qa-fail|block|complete) ;;
  *)
    printf 'usage: %s begin|qa-fail|block|complete [reason]\n' "$0"
    exit 12
    ;;
esac

if [[ "$mode" == "begin" || "$mode" == "qa-fail" ]]; then
  bash "$script_dir/sync-next-preflight.sh"
fi

if [[ "$mode" == "complete" ]]; then
  evidence_file="${2:-}"
  if [[ -n "$evidence_file" && "$evidence_file" != /* ]]; then
    evidence_file="$repo_root/$evidence_file"
  fi
  if [[ -z "$evidence_file" || ! -f "$evidence_file" ]]; then
    printf 'SYNC_NEXT_INVALID_EVIDENCE: complete requires an existing TSV artifact\n'
    exit 15
  fi
  reason="$evidence_file"
  bash "$script_dir/sync-next-verify.sh"
fi

python3 - "$plan_file" "$mode" "$reason" <<'PY'
import datetime
import hashlib
import os
import pathlib
import re
import sys

plan_path = pathlib.Path(sys.argv[1])
mode = sys.argv[2]
reason = re.sub(r"[^A-Za-z0-9_.:-]+", "_", sys.argv[3]).strip("_")[:120] or "unspecified"
text = plan_path.read_text(encoding="utf-8")
tick = chr(96)


def get(name):
    pattern = re.compile(rf"^- {re.escape(name)}: {re.escape(tick)}([^\n{re.escape(tick)}]*){re.escape(tick)}$", re.M)
    matches = pattern.findall(text)
    if len(matches) != 1:
        raise SystemExit(f"checkpoint field {name!r} must occur exactly once")
    return matches[0]


def put(source, name, value):
    pattern = re.compile(rf"^- {re.escape(name)}: {re.escape(tick)}[^\n{re.escape(tick)}]*{re.escape(tick)}$", re.M)
    replacement = f"- {name}: {tick}{value}{tick}"
    updated, count = pattern.subn(replacement, source)
    if count != 1:
        raise SystemExit(f"checkpoint field {name!r} update count was {count}")
    return updated


execution = get("Execution status")
loops = int(get("Internal repair loops"))
batch = get("Next batch ID")
contract = get("Closure contract ID")
oracle = get("External oracle version")
evidence_digest = None
evidence_ids = []

if mode == "complete":
    evidence_path = pathlib.Path(sys.argv[3])
    raw_evidence = evidence_path.read_bytes()
    evidence_digest = hashlib.sha256(raw_evidence).hexdigest()
    evidence_text = raw_evidence.decode("utf-8")
    lines = evidence_text.splitlines()
    expected_header = "criterion_id\tstatus\tproduction\tobservation\tevidence\tcounterfactual"
    if not lines or lines[0] != expected_header:
        raise SystemExit("SYNC_NEXT_INVALID_EVIDENCE: invalid TSV header")

    begin_marker = "<!-- ACTIVE_CONTRACT_BEGIN -->"
    end_marker = "<!-- ACTIVE_CONTRACT_END -->"
    if text.count(begin_marker) != 1 or text.count(end_marker) != 1:
        raise SystemExit("SYNC_NEXT_INVALID_EVIDENCE: active contract markers missing")
    contract_text = text.split(begin_marker, 1)[1].split(end_marker, 1)[0]
    expected_ids = []
    for line in contract_text.splitlines():
        if not line.startswith("#### "):
            continue
        match = re.match(r"#### `([A-Z][A-Z0-9-]{2,})", line)
        if match:
            expected_ids.append(match.group(1))

    banned = re.compile(r"\b(?:none|n/a|token|test name|count only)\b", re.I)
    for index, line in enumerate(lines[1:], start=2):
        cells = line.split("\t")
        if len(cells) != 6:
            raise SystemExit(f"SYNC_NEXT_INVALID_EVIDENCE: row {index} must have 6 cells")
        criterion_id, status, production, observation, evidence, counterfactual = cells
        if status != "PASS":
            raise SystemExit(f"SYNC_NEXT_INVALID_EVIDENCE: {criterion_id} status must be PASS")
        if re.search(r"[^\s:]+:\d+", production) is None:
            raise SystemExit(f"SYNC_NEXT_INVALID_EVIDENCE: {criterion_id} lacks source path:line")
        for label, value in (
            ("observation", observation),
            ("evidence", evidence),
            ("counterfactual", counterfactual),
        ):
            if len(value.strip()) < 16 or banned.search(value):
                raise SystemExit(
                    f"SYNC_NEXT_INVALID_EVIDENCE: {criterion_id} has placeholder {label}"
                )
        evidence_ids.append(criterion_id)

    if evidence_ids != expected_ids:
        raise SystemExit(
            "SYNC_NEXT_INVALID_EVIDENCE: criterion order/set mismatch "
            f"expected={expected_ids} actual={evidence_ids}"
        )

updated = text

if mode == "begin":
    if execution != "ready_for_builder":
        raise SystemExit(f"begin requires ready_for_builder, found {execution}")
    updated = put(updated, "Execution status", "building")
    updated = put(updated, "Internal QA result", "NOT_RUN")
    updated = put(updated, "Internal verification result", "NOT_RUN")

elif mode == "qa-fail":
    if execution not in {"building", "qa_failed"}:
        raise SystemExit(f"qa-fail requires building/qa_failed, found {execution}")
    if loops >= 2:
        updated = put(updated, "Execution status", "blocked")
        updated = put(updated, "Internal QA result", f"BLOCKED_AFTER_2_REPAIRS:{reason}")
    else:
        loops += 1
        updated = put(updated, "Execution status", "qa_failed")
        updated = put(updated, "Internal QA result", f"FAIL_SELF_REVIEW:{reason}")
        updated = put(updated, "Internal repair loops", str(loops))
    updated = put(updated, "Internal verification result", "NOT_RUN")

elif mode == "block":
    if execution == "awaiting_external_audit":
        raise SystemExit("cannot block a batch already awaiting external audit")
    updated = put(updated, "Execution status", "blocked")
    updated = put(updated, "Internal QA result", f"BLOCKED:{reason}")
    updated = put(updated, "Internal verification result", "BLOCKED")

elif mode == "complete":
    if execution not in {"building", "qa_failed"}:
        raise SystemExit(f"complete requires building/qa_failed, found {execution}")
    updated = put(updated, "Execution status", "awaiting_external_audit")
    updated = put(updated, "Last internally completed batch", batch)
    updated = put(updated, "Internal QA result", "PASS_SELF_REVIEW_NOT_EXTERNAL")
    updated = put(updated, "Internal verification result", f"PASS:{oracle}")

    log_heading = "## 8. Bounded implementation log"
    log_start = updated.find(log_heading)
    if log_start < 0:
        raise SystemExit("bounded implementation log section is missing")
    log_text = updated[log_start:]
    entry_count = len(re.findall(r"^### ", log_text, re.M))
    if entry_count >= 12:
        raise SystemExit("bounded implementation log has 12 entries; external compaction required")
    today = datetime.date.today().isoformat()
    entry = (
        f"\n\n### {today} — Internal handoff for {batch}\n\n"
        f"- Contract: {contract}.\n"
        f"- Self-review: PASS_SELF_REVIEW_NOT_EXTERNAL.\n"
        f"- Criteria: {','.join(evidence_ids)}.\n"
        f"- Evidence TSV SHA-256: {evidence_digest}.\n"
        f"- Verification: {oracle} exited 0 with unchanged digest.\n"
        f"- Repair loops: {loops}.\n"
        "- State: awaiting_external_audit; no package acceptance claimed.\n"
    )
    updated = updated.rstrip() + entry

if updated == text:
    raise SystemExit("checkpoint update produced no change")

temp_path = plan_path.with_name(plan_path.name + ".sync-next.tmp")
with temp_path.open("x", encoding="utf-8") as handle:
    handle.write(updated)
    handle.flush()
    os.fsync(handle.fileno())
os.replace(temp_path, plan_path)
directory_fd = os.open(plan_path.parent, os.O_RDONLY)
try:
    os.fsync(directory_fd)
finally:
    os.close(directory_fd)

new_execution = {
    "begin": "building",
    "block": "blocked",
    "complete": "awaiting_external_audit",
}.get(mode, "blocked" if loops >= 2 else "qa_failed")
print(f"SYNC_NEXT_CHECKPOINT_UPDATED mode={mode} execution={new_execution} batch={batch}")
PY
