#!/usr/bin/env bash

# Immutable external oracle for C2B repair cycle 1.
# Antigravity may read and execute this file but must never modify it.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
checkpoint_file="$repo_root/docs/sync_implementation_plan.md"
oracle_file="$repo_root/.agents/scripts/verify-work-package-c2b-repair1.sh"
failure_count=0

expected_oracle_sha256="$(awk -F'`' '/^- External oracle SHA-256: `/{ print $2; exit }' "$checkpoint_file")"
if command -v shasum >/dev/null 2>&1; then
  actual_oracle_sha256="$(shasum -a 256 "$oracle_file" | awk '{ print $1 }')"
else
  actual_oracle_sha256="$(sha256sum "$oracle_file" | awk '{ print $1 }')"
fi

if [[ -z "$expected_oracle_sha256" || "$actual_oracle_sha256" != "$expected_oracle_sha256" ]]; then
  printf 'EXTERNAL_ORACLE_MUTATED expected=%s actual=%s\n' \
    "${expected_oracle_sha256:-missing}" "$actual_oracle_sha256"
  exit 2
fi

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1"; failure_count=$((failure_count + 1)); }

run_gate() {
  local label="$1"
  local working_dir="$2"
  shift 2
  printf 'RUN   %s\n' "$label"
  if (cd "$working_dir" && "$@"); then pass "$label"; else fail "$label"; fi
}

check_digest() {
  local label="$1" file="$2" expected="$3" actual
  if command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$file" | awk '{ print $1 }')"
  else
    actual="$(sha256sum "$file" | awk '{ print $1 }')"
  fi
  if [[ "$actual" == "$expected" ]]; then
    pass "$label"
  else
    fail "$label (expected $expected, actual $actual)"
  fi
}

check_exact_test_count() {
  local label="$1" filter="$2" expected="$3" output count
  printf 'LIST  %s\n' "$label"
  if ! output="$(cd "$app_root" && cargo test "$filter" -- --list 2>&1)"; then
    printf '%s\n' "$output"
    fail "$label (test listing failed)"
    return
  fi
  count="$(printf '%s\n' "$output" | awk '/: test$/ { n += 1 } END { print n + 0 }')"
  if [[ "$count" == "$expected" ]]; then
    pass "$label ($count tests)"
  else
    printf '%s\n' "$output"
    fail "$label (expected $expected tests, found $count)"
  fi
}

check_min_test_count() {
  local label="$1" filter="$2" minimum="$3" output count
  printf 'LIST  %s\n' "$label"
  if ! output="$(cd "$app_root" && cargo test "$filter" -- --list 2>&1)"; then
    printf '%s\n' "$output"
    fail "$label (test listing failed)"
    return
  fi
  count="$(printf '%s\n' "$output" | awk '/: test$/ { n += 1 } END { print n + 0 }')"
  if (( count >= minimum )); then
    pass "$label ($count tests)"
  else
    printf '%s\n' "$output"
    fail "$label (expected at least $minimum tests, found $count)"
  fi
}

printf 'Synabit C2B repair: C2B-REPAIR-1-HARDENED-V1 / C2B-ORACLE-V2\n'

check_digest 'accepted A oracle immutable' "$repo_root/.agents/scripts/verify-work-package-a.sh" '5918897c035cc470b673cbd63c3655bb31bdcc65bf2fc0d4e22f51cfdeaa38f6'
check_digest 'accepted B1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b1.sh" '7dff56a65d6b047a717e95c9b2bc7cbfcf428494124bba768ce5f96296c8aa98'
check_digest 'accepted B2 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b2.sh" 'ca8731ac9c4b08d994ad570e241f21d01149f19ae7312c8a0d1ec057956b2a17'
check_digest 'accepted B3 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b3.sh" 'b0152d58b9591387cc02b188258d226827a22428e094d2afb7056b86efc0c7d7'
check_digest 'accepted B4 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b4.sh" '401b6f487a45ecceee4c31d9c87542e01a5b7ba20d88a737425343c3ba380e67'
check_digest 'accepted C1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-c1.sh" 'b41ed416450130c5f5da23e8a4b413937c6cd3470aaf4e02b890d3319c940e16'
check_digest 'historical C2A V1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-c2a.sh" 'b9d82c6862c9df3cc9ad9a4755c2a6a46bd83b1234b7d46d9f9e460e054b6a3e'
check_digest 'accepted C2A V2 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-c2a-repair1.sh" '50d9dde4ce21c5d3c8f5abc23d0b6d7504da59e6dc457ba81a4fceac55549e7c'
check_digest 'rejected C2B V1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-c2b.sh" 'bc43d4dfa8672b74f2759958e390a08f81f5bb99ff71fb24a320afc65b618592'

check_digest 'scope lock: accepted schema v6' "$app_root/src/db/schema.rs" '542c97ba149ac5809444f0b3572e995e8f7ead6cdd338ed461d85437c0d946de'
check_digest 'scope lock: accepted outbox DAO' "$app_root/src/db/sync_outbox.rs" '9174a0eacf972732fecb52f256890d02f798bc211f289f906c22c83de4be0574'
check_digest 'scope lock: accepted provider DAO' "$app_root/src/db/sync_provider_state.rs" '202035fb064777915f23319726eca774974bd2b66686b285a36d18b559785325'
check_digest 'scope lock: accepted local preparation' "$app_root/src/sync/core/change.rs" 'e74b403f8a06dbc7d4a79b8f47931dc9288fd85b17aece25fe2e437a94df941e'
check_digest 'scope lock: accepted remote apply' "$app_root/src/sync/core/apply.rs" 'cf5e44f7771d53f22fe916da412365967ace8ae494b67f84be1932ae7d5d4a86'
check_digest 'scope lock: accepted protocol facade' "$app_root/src/sync/protocol.rs" '463935b9927fbc3984c5827521c60270d06bd5dc7dc663be0fd9b41d0e8da692'
check_digest 'scope lock: DB module boundary' "$app_root/src/db/mod.rs" '549dc383862fb6f4e30ee7664f9a5b9bf3d2b14f11a6046266b5f36ea81a1ffd'

if python3 - \
  "$app_root/src/sync/adapter/mod.rs" \
  "$app_root/src/sync/adapter/gdrive.rs" \
  "$app_root/src/sync/adapter/server.rs" \
  "$app_root/src/sync/coordinator.rs" \
  "$app_root/src/db/sync_inbox.rs" <<'PY'
import pathlib
import re
import sys

adapter_path, gdrive_path, server_path, coordinator_path, inbox_path = map(pathlib.Path, sys.argv[1:])
adapter = adapter_path.read_text()
gdrive = gdrive_path.read_text()
server = server_path.read_text()
coordinator = coordinator_path.read_text()
inbox = inbox_path.read_text()
all_sources = "\n".join((adapter, gdrive, server, coordinator, inbox))
errors = []

def require(condition, message):
    if not condition:
        errors.append(message)

def braced_body(source, start_pattern):
    match = re.search(start_pattern, source, re.S)
    if not match:
        return ""
    start = source.find("{", match.end() - 1)
    if start < 0:
        return ""
    depth = 0
    for index in range(start, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start:index + 1]
    return ""

def fn_body(source, name):
    return braced_body(source, rf"\bfn\s+{re.escape(name)}\b[^{{]*?\(")

def fn_decl(source, name):
    match = re.search(rf"\bfn\s+{re.escape(name)}\b([^{{]*?)\{{", source, re.S)
    return match.group(0) if match else ""

def code_only(source):
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)

coordinator_code = code_only(coordinator)
sync_body = code_only(fn_body(coordinator, "sync"))

# RC2B-00: accepted B4/C1 runtime must be restored, not approximated.
preflight = code_only(fn_body(coordinator, "preflight_provider_state"))
require(preflight, "RC2B-00 missing accepted production preflight_provider_state")
for token in ("ensure_sync_provider_state", "get_sync_provider_state", "get_sync_plan", "incarnation_id", "reconcile_sync_provider_plan"):
    require(token in preflight, f"RC2B-00 provider preflight lacks {token!r}")

dispatch = code_only(fn_body(coordinator, "dispatch_durable_outbox_at"))
dispatch_wrapper = code_only(fn_body(coordinator, "dispatch_durable_outbox"))
require(dispatch, "RC2B-00 missing accepted injected-time dispatch_durable_outbox_at")
require("dispatch_durable_outbox_at" in dispatch_wrapper, "RC2B-00 dispatcher wrapper does not call injected-time seam")
for token in (
    "quarantine_incomplete_dispatchable_outbox", "get_dispatchable_outbox",
    "mark_outbox_batch_sent", "outbox_record_to_sync_operation", "adapter.push",
    "validate_push_ack_batch", "commit_accepted_outbox_operation",
    "schedule_outbox_retry", "persist_batch_retry_with_context",
):
    require(token in dispatch, f"RC2B-00 durable dispatcher lacks {token!r}")
require(fn_body(coordinator, "validate_push_ack_batch"), "RC2B-00 missing accepted complete ACK validator")
require(fn_body(coordinator, "persist_batch_retry_with_context"), "RC2B-00 missing accepted retry context seam")

for token in ("preflight_provider_state", "prepare_durable_outbox_operations", "pull_pages_durable"):
    require(token in sync_body, f"RC2B-00 active sync lacks {token!r}")
require(sync_body.count("dispatch_durable_outbox") >= 2,
        "RC2B-00 active sync does not drain pre-existing and newly prepared outbox")
require(".push(" not in sync_body, "RC2B-00 active sync calls adapter push directly")
preflight_pos = sync_body.find("preflight_provider_state")
first_dispatch = sync_body.find("dispatch_durable_outbox")
detect_pos = sync_body.find("detect_local_changes")
prepare_pos = sync_body.find("prepare_durable_outbox_operations")
second_dispatch = sync_body.find("dispatch_durable_outbox", first_dispatch + 1)
pull_pos = sync_body.find("pull_pages_durable")
require(-1 not in (preflight_pos, first_dispatch, detect_pos, prepare_pos, second_dispatch, pull_pos)
        and preflight_pos < first_dispatch < detect_pos < prepare_pos < second_dispatch < pull_pos,
        "RC2B-00 active coordinator phase ordering is not preflight->drain->detect/prepare->drain->pull")
require("get_sync_plan" not in sync_body,
        "RC2B-00 active sync obtains a second plan outside production preflight")

# RC2B-01/02: cursor ownership, recovery ordering and typed durable failures.
pull = code_only(fn_body(coordinator, "pull_pages_durable"))
pull_decl = fn_decl(coordinator, "pull_pages_durable")
process = code_only(fn_body(coordinator, "process_staged_inbox_page"))
resume = code_only(fn_body(coordinator, "resume_durable_inbox_before_pull"))
resume_decl = fn_decl(coordinator, "resume_durable_inbox_before_pull")
require(pull and process and resume, "RC2B-01 durable pull/process/resume production seams must all exist")
require("advance_sync_provider_cursor_cas" not in pull,
        "RC2B-01 durable pull directly advances cursor outside page-ledger commit")
require("start_cursor" not in pull_decl,
        "RC2B-01 durable pull accepts a stale start_cursor snapshot")
for token in ("mark_inbox_page_applied_if_safe", "commit_applied_inbox_page_cursor"):
    require(token in process, f"RC2B-01 page processor lacks sole cursor-owner seam {token!r}")
require("safe_to_commit" not in process,
        "RC2B-02 page processor retains silent safe_to_commit success path")
require("return Err" in process or "Err(" in process,
        "RC2B-02 page processor cannot return actionable blocker/failure")
require("result.pulled +=" not in process,
        "RC2B-03 page processor double-counts apply_doc_payload pulled result")
failed_branch = re.search(r"InboxState::Failed\s*=>\s*\{(.*?)\n\s*\}", process, re.S)
require(failed_branch is not None, "RC2B-02 page processor lacks explicit Failed branch")
if failed_branch is not None:
    branch = failed_branch.group(1)
    require("transition_inbox_state" in branch and "InboxState::Applying" in branch,
            "RC2B-02 Failed row is not CAS-transitioned to Applying before retry")
for state in ("InboxState::PendingAsset", "InboxState::Quarantined"):
    match = re.search(rf"{re.escape(state)}.*?=>\s*\{{(.*?)\n\s*\}}", process, re.S)
    require(match is not None and ("Err(" in match.group(1) or "return Err" in match.group(1)),
            f"RC2B-02 existing blocker {state} does not return actionable error")

validate_decl = fn_decl(coordinator, "validate_and_parse_remote_entry")
validate_body = code_only(fn_body(coordinator, "validate_and_parse_remote_entry"))
require("InboxApplyFailureKind" in validate_decl,
        "RC2B-02 remote validation does not return a typed failure kind")
require("contains(" not in process and ".to_string()" not in process,
        "RC2B-02 processor classifies failure through error-string matching")
for token in ("Corrupt", "PendingAsset", "UnsupportedDelete"):
    require(token in validate_body, f"RC2B-02 typed validation lacks {token!r}")

ack_pos = pull.find("retry_remote_ack_gap")
resume_pos = pull.find("resume_durable_inbox_before_pull")
first_pull_pos = pull.find("adapter.pull_page")
require(-1 not in (ack_pos, resume_pos, first_pull_pos) and ack_pos < resume_pos < first_pull_pos,
        "RC2B-01 recovery order is not ACK-gap -> resume -> new pull")
require("get_sync_provider_state" in pull and pull.find("get_sync_provider_state") > resume_pos,
        "RC2B-01 durable pull does not reread cursor after recovery")
require("adapter" in resume_decl and "retry_remote_ack_gap" in resume,
        "RC2B-01 resume cannot ACK every recovered local commit")

# RC2B-03: strict conversion, initial/terminal pages, overflow and asset staging.
convert_decl = fn_decl(coordinator, "remote_entry_to_inbox_entry")
convert = code_only(fn_body(coordinator, "remote_entry_to_inbox_entry"))
require("AppResult" in convert_decl, "RC2B-03 remote converter is not fail-closed AppResult")
require("remote_position" in convert and ".trim().is_empty()" in convert,
        "RC2B-03 remote converter does not reject empty provider position before DB")
require("checked_add" in pull and ".ok_or_else" in pull and "unwrap_or" not in pull,
        "RC2B-03 rx_bytes accounting is not checked/fail-closed")
terminal_helper = code_only(fn_body(coordinator, "is_terminal_noop_page"))
require(terminal_helper, "RC2B-03 missing explicit terminal no-op classifier")
for token in ("entries.is_empty", "has_more", "next_cursor", "current_cursor"):
    require(token in terminal_helper, f"RC2B-03 terminal no-op classifier lacks {token!r}")
require("is_terminal_noop_page" in pull and pull.find("is_terminal_noop_page") < pull.find("stage_inbox_page"),
        "RC2B-03 terminal no-op is not handled before durable staging")
require("start_cursor cannot be empty for non-empty page" not in inbox,
        "RC2B-03 inbox staging still rejects a valid initial empty provider cursor")
server_map = code_only(fn_body(server, "map_pull_page_response"))
require(not ("AssetReference" in server_map and "UnsupportedCapability" in server_map),
        "RC2B-03 server mapper rejects AssetReference before durable staging")

# RC2B-04/05: real test injection and exact snapshot shape.
applier_trait = braced_body(coordinator, r"\btrait\s+InboxEntryApplier\s*\{")
production_applier = braced_body(coordinator, r"\bstruct\s+ProductionInboxEntryApplier\b")
require(applier_trait and "apply" in applier_trait, "RC2B-04 missing production InboxEntryApplier interface")
require(production_applier and "ProductionInboxEntryApplier" in sync_body,
        "RC2B-04 active sync does not use live production inbox applier")
require("InboxEntryApplier" in process and "InboxEntryApplier" in pull,
        "RC2B-04 durable runtime bypasses injected production apply seam")

snapshot_decl = fn_decl(coordinator, "snapshot_c2b_runtime_raw")
snapshot = code_only(fn_body(coordinator, "snapshot_c2b_runtime_raw"))
require(re.search(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\][\s\S]*?fn\s+snapshot_c2b_runtime_raw", coordinator) is not None,
        "RC2B-05 runtime snapshot is not test-only")
require("C2bRuntimeSnapshot" in snapshot_decl and "struct C2bRuntimeSnapshot" in coordinator,
        "RC2B-05 runtime snapshot has no typed five-scope result")
for token in ("sync_provider_state", "sync_inbox", "sync_inbox_pages", "sync_inbox_page_entries", "sync_outbox", "ORDER BY"):
    require(token in snapshot, f"RC2B-05 complete snapshot lacks {token!r}")
require("SELECT *" not in snapshot and "HashMap" not in snapshot and "_dummy" not in snapshot,
        "RC2B-05 snapshot uses SELECT */HashMap/oracle-token filler")
for token in ("ack_cursor", "remote_position", "remote_seq", "operation_id", "entry_kind", "encrypted_payload", "payload_hash", "source_device", "state", "last_error"):
    require(token in snapshot, f"RC2B-05 explicit snapshot omits {token!r}")

# RC2B-06: every named regression must execute real seams; tautologies are fatal.
required_c2b_tests = {
    "c2b_server_and_gdrive_positions_are_provider_native": ("RemoteEntry", "remote_position", "remote_seq", "pull_page", "assert_eq!"),
    "c2b_page_is_staged_before_apply_and_local_commit_before_ack": ("pull_pages_durable", "snapshot_c2b_runtime_raw", "ack_calls", "apply_calls", "assert_eq!"),
    "c2b_restart_resumes_staged_page_before_new_pull": ("pull_pages_durable", "stage_inbox_page", "snapshot_c2b_runtime_raw", "pull_calls", "assert_eq!"),
    "c2b_applying_crash_state_reapplies_without_duplicate_terminal_transition": ("process_staged_inbox_page", "InboxState::Applying", "apply_calls", "snapshot_c2b_runtime_raw", "assert_eq!"),
    "c2b_corrupt_middle_entry_blocks_cursor_ack_and_later_page": ("pull_pages_durable", "Quarantined", "ack_calls", "pull_calls", "snapshot_c2b_runtime_raw", "assert_eq!"),
    "c2b_verified_own_operation_requires_device_or_scoped_outbox_evidence": ("is_verified_own_operation", "insert_outbox_record", "vault", "provider", "assert_eq!"),
    "c2b_unverified_source_is_validated_and_applied": ("process_staged_inbox_page", "apply_calls", "InboxState::Applied", "assert_eq!"),
    "c2b_ack_failure_preserves_local_commit_and_restart_retries_gap_before_pull": ("pull_pages_durable", "ack_calls", "pull_calls", "cursor_committed", "snapshot_c2b_runtime_raw", "assert_eq!"),
    "c2b_two_updates_same_document_apply_in_page_order": ("process_staged_inbox_page", "apply_calls", "page_ordinal", "operation_id", "assert_eq!"),
    "c2b_asset_and_delete_block_page_in_durable_typed_states": ("process_staged_inbox_page", "PendingAsset", "UnsupportedDelete", "snapshot_c2b_runtime_raw", "assert_eq!"),
    "c2b_empty_advancing_page_commits_and_acks": ("pull_pages_durable", "entry_count", "ack_calls", "snapshot_c2b_runtime_raw", "assert_eq!"),
}

test_bodies = []
for name, tokens in required_c2b_tests.items():
    body = code_only(fn_body(all_sources, name))
    require(body, f"RC2B-06 missing dynamic regression {name}")
    if body:
        test_bodies.append(body)
        for token in tokens:
            require(token in body, f"RC2B-06 {name} lacks production evidence {token!r}")

accepted_coordinator_tests = {
    "bootstrap_required_provider_state_stops_before_local_push_or_pull": ("preflight_provider_state", "push_calls", "pull_calls", "before", "after"),
    "cursor_cas_failure_prevents_ack_and_next_page_with_real_provider_state": ("pull_pages_durable", "ack_calls", "pull_calls", "get_sync_provider_state"),
    "restart_redelivers_preexisting_sent_outbox_without_redetection": ("dispatch_durable_outbox", "OutboxState::Sent", "OutboxState::Acknowledged", "operation_id"),
    "adapter_failure_preserves_outbox_and_schedules_bounded_retry": ("dispatch_durable_outbox", "retry_count", "next_retry_at", "last_error"),
    "partial_ack_commits_only_accepted_operation": ("dispatch_durable_outbox", "accepted", "rejected", "operation_id"),
    "missing_or_unknown_ack_never_acknowledges_outbox": ("dispatch_durable_outbox", "missing", "unknown", "operation_id"),
    "incomplete_outbox_record_fails_closed_before_network": ("dispatch_durable_outbox", "push_calls", "before", "after"),
    "incomplete_due_row_is_quarantined_once_without_starving_valid_rows": ("dispatch_durable_outbox", "push_calls", "first", "second"),
    "retry_persistence_failure_keeps_adapter_and_database_context": ("dispatch_durable_outbox", "CREATE TRIGGER", "network error", "injected"),
}

for name, tokens in accepted_coordinator_tests.items():
    body = code_only(fn_body(coordinator, name))
    require(body, f"RC2B-00 missing restored coordinator regression {name}")
    if body:
        test_bodies.append(body)
        for token in tokens:
            require(token in body, f"RC2B-00 restored regression {name} lacks production evidence {token!r}")
        require("assert" in body, f"RC2B-00 restored regression {name} has no executable assertion")

raw_tests = "\n".join(test_bodies)
require(re.search(r'assert_eq!\s*\(\s*"([^"]*)"\s*,\s*"\1"\s*\)', raw_tests, re.S) is None,
        "RC2B-06 tests contain identical string-literal self assertions")
require(re.search(r'assert_eq!\s*\(\s*([A-Za-z0-9_:]+)\s*,\s*\1\s*\)', raw_tests) is None,
        "RC2B-06 tests contain identical constant/path self assertions")
for banned in ("assert!(true)", "assert_eq!(1, 1)", "_dummy", "oracle token", "references for oracle"):
    require(banned.lower() not in raw_tests.lower(), f"RC2B-06 tests contain banned proxy evidence {banned!r}")
require(re.search(r"let\s+_[A-Za-z0-9_]*\s*=", raw_tests) is None,
        "RC2B-06 tests contain unused token variables")

if errors:
    for error in errors:
        print(f"FAIL  {error}")
    print(f"HARDENED STATIC MANIFEST FAILED: {len(errors)} condition(s) missing")
    raise SystemExit(1)

print("PASS  C2B repair production and dynamic-evidence manifest")
PY
then
  pass 'C2B repair hardened static manifest'
else
  fail 'C2B repair hardened static manifest'
fi

if (( failure_count > 0 )); then
  printf 'C2B REPAIR RED/STATIC FAILURE: %d gate(s) failed; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'accepted provider-state regressions' "$app_root" cargo test db::sync_provider_state::tests
run_gate 'accepted durable-outbox regressions' "$app_root" cargo test db::sync_outbox::tests
run_gate 'accepted durable-inbox regressions' "$app_root" cargo test db::sync_inbox::tests
run_gate 'adapter regressions' "$app_root" cargo test sync::adapter::
check_exact_test_count 'exact C2B focused regression inventory' 'c2b_' 11
check_min_test_count 'restored coordinator regression inventory' 'sync::coordinator::' 19

if (( failure_count == 0 )); then
  run_gate 'C2B focused regressions' "$app_root" cargo test c2b_
  run_gate 'restored coordinator regressions' "$app_root" cargo test sync::coordinator::
  run_gate 'full app suite excluding documented unrelated search failure' "$app_root" cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored
fi

if (( failure_count > 0 )); then
  printf 'C2B-REPAIR-1-HARDENED-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'C2B-REPAIR-1-HARDENED-V1 / C2B-ORACLE-V2 PASSED. Await external audit; do not self-accept C2B.\n'
