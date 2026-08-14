#!/usr/bin/env bash

# Immutable external acceptance oracle for Work package C, slice C1.
# Antigravity may read and execute this file but must never modify it.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
checkpoint_file="$repo_root/docs/sync_implementation_plan.md"
oracle_file="$repo_root/.agents/scripts/verify-work-package-c1.sh"

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
  printf 'Only the external auditor may update this oracle and its checkpoint hash.\n'
  exit 2
fi

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1"; failure_count=$((failure_count + 1)); }

run_gate() {
  local label="$1"
  local working_dir="$2"
  shift 2
  printf 'RUN   %s\n' "$label"
  if (cd "$working_dir" && "$@"); then
    pass "$label"
  else
    fail "$label"
  fi
}

check_digest() {
  local label="$1"
  local file="$2"
  local expected="$3"
  local actual
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

printf 'Synabit Work package C slice C1: C1-EVIDENCE-CLOSURE-V1 / C1-ORACLE-V7\n'

check_digest 'accepted A oracle remains immutable' \
  "$repo_root/.agents/scripts/verify-work-package-a.sh" \
  '5918897c035cc470b673cbd63c3655bb31bdcc65bf2fc0d4e22f51cfdeaa38f6'
check_digest 'accepted B1 oracle remains immutable' \
  "$repo_root/.agents/scripts/verify-work-package-b1.sh" \
  '7dff56a65d6b047a717e95c9b2bc7cbfcf428494124bba768ce5f96296c8aa98'
check_digest 'accepted B2 oracle remains immutable' \
  "$repo_root/.agents/scripts/verify-work-package-b2.sh" \
  'ca8731ac9c4b08d994ad570e241f21d01149f19ae7312c8a0d1ec057956b2a17'
check_digest 'accepted B3 oracle remains immutable' \
  "$repo_root/.agents/scripts/verify-work-package-b3.sh" \
  'b0152d58b9591387cc02b188258d226827a22428e094d2afb7056b86efc0c7d7'
check_digest 'accepted B4 oracle remains immutable' \
  "$repo_root/.agents/scripts/verify-work-package-b4.sh" \
  '401b6f487a45ecceee4c31d9c87542e01a5b7ba20d88a737425343c3ba380e67'

if python3 - \
  "$app_root/src/db/schema.rs" \
  "$app_root/src/db/sync_outbox.rs" \
  "$app_root/src/sync/core/change.rs" \
  "$app_root/src/sync/coordinator.rs" \
  "$app_root/src/sync/adapter/mod.rs" <<'PY'
import pathlib
import re
import sys

schema_path, outbox_path, change_path, coordinator_path, adapter_path = map(pathlib.Path, sys.argv[1:])
errors = []

def require(condition, message):
    if not condition:
        errors.append(message)

def read(path):
    require(path.is_file(), f"required source missing: {path}")
    return path.read_text() if path.is_file() else ""

def fn_body(source, name):
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", source)
    if not match:
        return None
    start = source.find("{", match.end())
    if start < 0:
        return None
    depth = 0
    for index in range(start, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start:index + 1]
    return None

def fn_decl(source, name):
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\((.*?)\)\s*(?:->\s*([^{{]+))?\{{", source, re.S)
    return match.group(0) if match else ""

def code_only(source):
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)

schema = read(schema_path)
outbox = read(outbox_path)
change = read(change_path)
coordinator = read(coordinator_path)
adapter = read(adapter_path)
all_sources = "\n".join((schema, outbox, change, coordinator, adapter))

require(
    re.search(r"LATEST_SYNC_SCHEMA_VERSION\s*:\s*i64\s*=\s*5\s*;", schema) is not None,
    "C1-01 sync schema version is not exactly 5",
)
migration = fn_body(schema, "migrate_sync_schema_v5") or ""
require(migration, "C1-01 missing explicit transactional migrate_sync_schema_v5")
for token in ("transaction", "doc_hash", "sync_outbox", "sync_schema_meta", "tx.commit"):
    require(token in migration, f"C1-01 v5 migration lacks {token!r}")
require(
    "legacy_outbox_v5_migration_preserves_rows_and_quarantines_unreconstructable_records" in schema,
    "C1-01 missing legacy outbox migration preservation/quarantine regression",
)

record_match = re.search(r"pub\s+struct\s+OutboxRecord\s*\{(.*?)\n\}", outbox, re.S)
record_body = record_match.group(1) if record_match else ""
require(record_match is not None, "C1-02 OutboxRecord missing")
for field in (
    "operation_id", "doc_hash", "entry_kind", "node_id", "rel_path", "source_hash",
    "original_timestamp", "encrypted_payload", "payload_hash", "asset_ref_blob", "state",
    "retry_count", "next_retry_at", "last_error",
):
    require(field in record_body, f"C1-02 OutboxRecord cannot durably reconstruct field {field!r}")

ack_match = re.search(r"pub\s+struct\s+OperationAck\s*\{(.*?)\n\}", adapter, re.S)
ack_body = ack_match.group(1) if ack_match else ""
require(ack_match is not None, "C1-03 OperationAck missing")
for field in ("operation_id", "accepted", "remote_position", "error"):
    require(field in ack_body, f"C1-03 OperationAck lacks {field!r}")
require(
    re.search(r"remote_position\s*:\s*Option\s*<\s*u64\s*>", ack_body) is not None,
    "C1-FR7 OperationAck.remote_position is not the frozen Option<u64> contract",
)

for function_name in (
    "enqueue_or_reuse_outbox_operation",
    "outbox_record_to_sync_operation",
    "mark_outbox_batch_sent",
    "commit_accepted_outbox_operation",
    "schedule_outbox_retry",
):
    require(fn_body(outbox, function_name), f"C1-04 missing production outbox seam {function_name}")

enqueue_body = fn_body(outbox, "enqueue_or_reuse_outbox_operation") or ""
require("transaction" in enqueue_body, "C1-04 source-hash reuse decision and insert are not one transaction")
for token in ("source_hash", "operation_id", "acknowledged", "query_row"):
    require(token in enqueue_body, f"C1-04 source-hash reuse seam lacks {token!r}")
for token in ("record.validate", "record.rel_path", "record.source_hash", "sent", "failed"):
    require(token in enqueue_body, f"C1-FR6 exact pending-source reuse lacks {token!r}")
require("candidate_a" not in enqueue_body and "candidate_b" not in enqueue_body,
        "C1-FR8 production enqueue contains evidence-token variables instead of a typed existing-row decision")
require("tx.commit().ok()" not in code_only(enqueue_body),
        "C1-FINAL-02 pending-source reuse suppresses transaction commit failure")
require("ORDER BY" in enqueue_body and "LIMIT 1" in enqueue_body,
        "C1-FINAL-02 legacy duplicate pending-source selection is not deterministic")

convert_body = fn_body(outbox, "outbox_record_to_sync_operation") or ""
convert_decl = fn_decl(outbox, "outbox_record_to_sync_operation")
for token in ("operation_id", "doc_hash", "entry_kind", "node_id", "rel_path", "encrypted_payload", "payload_hash", "original_timestamp"):
    require(token in convert_body, f"C1-04 durable row conversion drops {token!r}")
require("AppResult" in convert_decl, "C1-FR5 outbox conversion is not fail-closed AppResult<SyncOperation>")
require("unwrap_or" not in convert_body and "unwrap_or_default" not in convert_body,
        "C1-FR5 outbox conversion fabricates zero/empty operation fields")
require("record.validate" in convert_body,
        "C1-FINAL-01 durable conversion bypasses complete OutboxRecord validation")
for fallback in ("None => [0; 32]", "None => String::new()", "None => Vec::new()"):
    require(fallback not in code_only(convert_body),
            f"C1-FINAL-01 durable conversion still fabricates missing data via {fallback!r}")

complete_validate = fn_body(outbox, "validate_complete") or ""
require(complete_validate, "ARC1-01 missing state-independent complete-row validator")
if complete_validate:
    for token in (
        "doc_hash", "rel_path", "source_hash", "encrypted_payload", "payload_hash",
        "SyncEntryKind::Upsert", "SyncEntryKind::Delete", "SyncEntryKind::AssetReference",
        "asset_ref_blob",
    ):
        require(token in complete_validate, f"ARC1-01 complete-row/entry-kind validation omits {token!r}")
    require("self.state != OutboxState::Failed" not in code_only(complete_validate),
            "ARC1-01 complete validation still exempts every Failed row")

insert_body = fn_body(outbox, "insert_outbox_record") or ""
for seam_name, seam_body in (
    ("insert_outbox_record", insert_body),
    ("enqueue_or_reuse_outbox_operation", enqueue_body),
    ("outbox_record_to_sync_operation", convert_body),
    ("commit_accepted_outbox_operation", fn_body(outbox, "commit_accepted_outbox_operation") or ""),
):
    require("validate_complete" in seam_body,
            f"ARC1-01 {seam_name} bypasses the state-independent complete-row validator")

quarantine_body = fn_body(outbox, "quarantine_incomplete_dispatchable_outbox") or ""
require(quarantine_body, "ARC1-01 missing durable quarantine seam for malformed dispatchable rows")
if quarantine_body:
    for token in (
        "transaction", "ready", "sent", "failed", "next_retry_at", "last_error",
        "doc_hash", "rel_path", "source_hash", "encrypted_payload", "payload_hash",
        "tx.commit",
    ):
        require(token in quarantine_body, f"ARC1-01 malformed-row quarantine lacks {token!r}")
    require("next_retry_at = NULL" in quarantine_body,
            "ARC1-01 quarantined malformed rows remain retryable and can starve future batches")
    for token in ("rows_affected", "rec.state.as_str", "rec.next_retry_at", "state = ?6", "next_retry_at IS ?7"):
        require(token in quarantine_body,
                f"ARCR1-01 quarantine update is not an exact snapshot CAS; lacks {token!r}")

dispatch_query = fn_body(outbox, "get_dispatchable_outbox") or ""
require("'ready'" in dispatch_query, "C1-05 ready rows are not dispatchable")
require("'sent'" in dispatch_query, "C1-05 crash-left sent rows are not redelivered")
require("'failed'" in dispatch_query and "next_retry_at" in dispatch_query, "C1-05 due failed rows do not obey retry scheduling")
require("'prepared'" not in dispatch_query, "C1-05 incomplete prepared rows can reach adapter.push")
require("LIMIT" in dispatch_query and "ORDER BY" in dispatch_query, "C1-05 dispatch is not bounded and deterministic")

commit_body = fn_body(outbox, "commit_accepted_outbox_operation") or ""
commit_decl = fn_decl(outbox, "commit_accepted_outbox_operation")
for token in ("transaction", "sync_outbox", "sync_document_baselines", "acknowledged", "tx.commit"):
    require(token in commit_body, f"C1-06 accepted commit lacks atomic boundary token {token!r}")
for token in ("source_hash", "state = 'sent'", "rows_affected", "sync_document_paths", "DELETE FROM"):
    require(token in commit_body, f"C1-FR3 accepted commit lacks {token!r}")
require("hex::encode(doc_hash)" not in commit_body,
        "C1-FR3 accepted commit writes path/doc hash as the content baseline")
require("now" in commit_decl,
        "ARCR1-03 accepted commit has no injected timestamp for independently exact snapshots")
require("strftime" not in code_only(commit_body),
        "ARCR1-03 accepted commit still derives a hidden wall-clock timestamp inside SQLite")

retry_body = fn_body(outbox, "schedule_outbox_retry") or ""
retry_decl = fn_decl(outbox, "schedule_outbox_retry")
for token in ("retry_count", "next_retry_at", "last_error", "MAX_OUTBOX_RETRY_DELAY"):
    require(token in retry_body, f"C1-07 retry seam lacks {token!r}")
for token in ("vault_id", "provider_id", "now"):
    require(token in retry_decl, f"C1-FR2 retry seam is not scoped/injectable; signature lacks {token!r}")
for token in ("transaction", "state = 'sent'", "rows_affected"):
    require(token in retry_body, f"C1-FR2 retry mutation lacks transactional CAS token {token!r}")
require("unwrap_or" not in retry_body, "C1-FR2 retry seam suppresses missing-row/query errors")
require("chrono::Utc::now" not in retry_body, "C1-FR2 retry seam ignores injected deterministic now")
require("Err(_)" not in retry_body,
        "C1-FINAL-03 retry seam collapses database read failures into a false CAS miss")

batch_retry_body = fn_body(outbox, "schedule_outbox_batch_retry") or ""
require(batch_retry_body, "C1-FINAL-03 missing all-or-nothing retry transition for adapter/protocol batch failure")
for token in ("transaction", "vault_id", "provider_id", "operation_ids", "state = 'sent'", "rows_affected", "tx.commit"):
    require(token in batch_retry_body, f"C1-FINAL-03 batch retry seam lacks {token!r}")

mark_body = fn_body(outbox, "mark_outbox_batch_sent") or ""
mark_decl = fn_decl(outbox, "mark_outbox_batch_sent")
for token in ("vault_id", "provider_id", "now"):
    require(token in mark_decl, f"C1-FR2 sent-batch seam is not scoped/injectable; signature lacks {token!r}")
for token in ("transaction", "rows_affected", "ready", "failed", "sent", "tx.commit"):
    require(token in mark_body, f"C1-FR2 sent-batch mutation lacks {token!r}")
for token in ("HashSet", "duplicate", "next_retry_at", "SELECT state"):
    require(token in mark_body, f"ARC1-03 strict sent-batch prevalidation lacks {token!r}")
require(mark_body.find("SELECT state") >= 0 and mark_body.find("UPDATE sync_outbox") >= 0
        and mark_body.find("SELECT state") < mark_body.find("UPDATE sync_outbox"),
        "ARC1-03 sent-batch mutates before validating the complete input batch")
mark_code = code_only(mark_body)
require("OutboxState::Prepared" not in mark_code and "OutboxState::UploadingAssets" not in mark_code,
        "ARCR1-02 sent-batch prevalidation still admits Prepared/UploadingAssets")

prepare_body = fn_body(change, "prepare_durable_outbox_operations") or ""
require(prepare_body, "C1-08 missing durable local preparation seam")
for token in ("enqueue_or_reuse_outbox_operation", "source_hash", "OutboxState::Ready"):
    require(token in prepare_body, f"C1-08 durable preparation lacks {token!r}")
require("upsert_document_baseline" not in prepare_body, "C1-08 preparation advances baseline before remote acceptance")
require("unwrap_or_default" not in prepare_body, "C1-FR4 preparation silently accepts invalid/empty source hashes")
require("if let Ok(content)" not in prepare_body and "if let Ok(text)" not in prepare_body,
        "C1-FR4 preparation silently skips unreadable or unsupported files")
require("delete_source_hash" in prepare_body,
        "C1-FR4 delete preparation does not use a deterministic domain-separated source identity")
delete_hash_body = fn_body(change, "delete_source_hash") or ""
require(delete_hash_body, "C1-FINAL-04 delete source identity is not a testable production helper")
for token in ("vault_id", "provider_id", "node_id", "rel_path", "blake3"):
    require(token in delete_hash_body, f"C1-FINAL-04 delete source helper lacks {token!r}")
require("DELETE_SOURCE_HASH_DOMAIN_V1" in delete_hash_body,
        "ARC1-02 delete source identity lacks a versioned domain separator")
require("to_le_bytes" in delete_hash_body and "Hasher" in delete_hash_body,
        "ARC1-02 delete source identity does not length-prefix each input field")
require('"delete_source_hash:{}:{}:{}:{}"' not in code_only(delete_hash_body),
        "ARC1-02 delete source identity still uses ambiguous delimiter concatenation")
hash_validation_pos = prepare_body.find("hex::decode")
file_read_pos = prepare_body.find("fs::read")
path_write_pos = prepare_body.find("upsert_document_path")
require(hash_validation_pos >= 0 and path_write_pos >= 0 and hash_validation_pos < path_write_pos,
        "C1-FINAL-04 upsert mutates durable path before source-hash validation")
require(file_read_pos >= 0 and path_write_pos >= 0 and file_read_pos < path_write_pos,
        "C1-FINAL-04 upsert mutates durable path before file read/type validation")

sync_body = fn_body(coordinator, "sync") or ""
require("prepare_durable_outbox_operations" in sync_body, "C1-08 active coordinator bypasses durable preparation")
require("dispatch_durable_outbox" in sync_body, "C1-08 active coordinator does not drain durable outbox")
require("prepare_push_operations" not in sync_body, "C1-08 active coordinator still builds ephemeral push operations")
require("adapter.push(push_ops)" not in sync_body, "C1-08 active coordinator still pushes an ephemeral vector")
require(sync_body.count("dispatch_durable_outbox") >= 2,
        "C1-FR4 active coordinator does not drain pre-existing outbox before detection and new rows after preparation")
first_dispatch = sync_body.find("dispatch_durable_outbox")
detect_local = sync_body.find("detect_local_changes")
prepare_durable = sync_body.find("prepare_durable_outbox_operations")
second_dispatch = sync_body.find("dispatch_durable_outbox", first_dispatch + 1)
require(first_dispatch >= 0 and detect_local >= 0 and first_dispatch < detect_local,
        "C1-FR4 pre-existing outbox drain is not before local detection")
require(prepare_durable >= 0 and second_dispatch > prepare_durable,
        "C1-FR4 newly prepared outbox drain is not after durable preparation")

dispatch_wrapper_body = fn_body(coordinator, "dispatch_durable_outbox") or ""
dispatch_at_body = fn_body(coordinator, "dispatch_durable_outbox_at") or ""
require(dispatch_at_body, "ARCR1-04 missing injected-now production dispatcher seam")
require("dispatch_durable_outbox_at" in dispatch_wrapper_body,
        "ARCR1-04 wall-clock dispatcher wrapper does not delegate to the injected-now seam")
dispatch_body = dispatch_at_body or dispatch_wrapper_body
require(dispatch_body, "C1-09 missing production durable dispatcher")
for token in (
    "get_dispatchable_outbox", "mark_outbox_batch_sent", "outbox_record_to_sync_operation",
    "adapter.push", "commit_accepted_outbox_operation", "schedule_outbox_retry",
):
    require(token in dispatch_body, f"C1-09 durable dispatcher lacks {token!r}")
require("operation_id" in dispatch_body, "C1-09 dispatcher does not correlate per-operation ACK by identity")
require("adapter.push(push_ops).await?" not in dispatch_body,
        "C1-FR1 adapter error returns before durable retry scheduling")
require("validate_push_ack_batch" in dispatch_body,
        "C1-FR1 dispatcher does not two-phase validate the complete ACK set before mutations")
require("outbox_record_to_sync_operation(rec)?" in dispatch_body,
        "C1-FR5 dispatcher does not propagate fail-closed durable conversion")
require("quarantine_incomplete_dispatchable_outbox" in dispatch_body,
        "ARC1-01 dispatcher does not durably quarantine malformed rows before selecting a batch")

retry_context_body = fn_body(coordinator, "persist_batch_retry_with_context") or ""
retry_context_decl = fn_decl(coordinator, "persist_batch_retry_with_context")
require(retry_context_body, "ARC1-03 missing shared adapter/protocol retry-persistence context seam")
if retry_context_body:
    for token in ("schedule_outbox_batch_retry", "cause", "persistence", "format!"):
        require(token in retry_context_body, f"ARC1-03 retry-persistence context seam lacks {token!r}")
require(dispatch_body.count("persist_batch_retry_with_context") >= 2,
        "ARC1-03 adapter and protocol failures do not share context-preserving retry persistence")
require("AppResult" not in retry_context_decl and "AppError" in retry_context_decl,
        "ARCR1-05 retry context helper models an always-error path as AppResult")
require("unwrap_err" not in code_only(dispatch_body),
        "ARCR1-05 dispatcher can panic by unwrap_err on the retry context helper")
require("&AppError" not in retry_context_decl,
        "ARCR2-04 retry context helper borrows and string-rewraps the original AppError instead of returning it")
require("AppError::General(cause.to_string())" not in code_only(retry_context_body),
        "ARCR2-04 successful retry persistence changes the original error variant/message")
require(re.search(r"(?:return\s+cause\s*;|\bcause\s*\n?\s*\})", code_only(retry_context_body)) is not None,
        "ARCR2-04 retry context helper does not return the owned original cause on persistence success")
require("&cause" not in code_only(dispatch_body),
        "ARCR2-04 dispatcher still passes the original cause by reference to a lossy helper")

ack_validation = fn_body(coordinator, "validate_push_ack_batch") or ""
require(ack_validation, "C1-FR1 missing production complete ACK-set validator")
for token in ("HashSet", "duplicate", "unknown", "missing", "operation_id"):
    require(token in ack_validation, f"C1-FR1 ACK validator lacks {token!r}")
require("must provide an error" not in code_only(ack_validation),
        "C1-FINAL-05 rejected ACK without error becomes whole-batch protocol failure instead of a per-row fallback rejection")
require(dispatch_body.count("schedule_outbox_batch_retry") >= 2,
        "C1-FINAL-03 adapter and protocol failures do not use atomic whole-batch retry scheduling")

require('let _ = "dispatch_durable_outbox"' not in all_sources,
        "C1-FR8 tests contain dispatch string tokens instead of production calls")
require('let _ = "prepare_durable_outbox_operations"' not in all_sources,
        "C1-FR8 tests contain preparation string tokens instead of production calls")

required_tests = {
    "same_source_hash_reuses_exact_durable_outbox_operation": (
        "enqueue_or_reuse_outbox_operation", "candidate_a", "candidate_b", "operation_id", "assert_eq!"
    ),
    "outbox_roundtrip_reconstructs_exact_sync_operation": (
        "outbox_record_to_sync_operation", "expected", "actual", "assert_eq!"
    ),
    "restart_redelivers_preexisting_sent_outbox_without_redetection": (
        "dispatch_durable_outbox", "OutboxState::Sent", "push", "operation_id", "OutboxState::Acknowledged", "assert_eq!"
    ),
    "accepted_outbox_commit_atomically_updates_baseline_and_ack_state": (
        "commit_accepted_outbox_operation", "CREATE TRIGGER", "before", "after_failure", "after_success", "assert_eq!"
    ),
    "adapter_failure_preserves_outbox_and_schedules_bounded_retry": (
        "dispatch_durable_outbox", "retry_count", "next_retry_at", "last_error", "operation_id", "assert"
    ),
    "partial_ack_commits_only_accepted_operation": (
        "dispatch_durable_outbox", "accepted", "rejected", "operation_id", "assert_eq!"
    ),
    "missing_or_unknown_ack_never_acknowledges_outbox": (
        "dispatch_durable_outbox", "missing", "unknown", "operation_id", "assert"
    ),
    "durable_preparation_does_not_advance_baseline_before_acceptance": (
        "prepare_durable_outbox_operations", "get_document_baseline", "commit_accepted_outbox_operation", "assert"
    ),
}

for name, tokens in required_tests.items():
    body = fn_body(all_sources, name)
    require(body is not None, f"C1-10 missing production-dependent regression {name}")
    if body is not None:
        for token in tokens:
            require(token in body, f"C1-10 {name} lacks evidence token {token!r}")

reuse_test = fn_body(all_sources, "same_source_hash_reuses_exact_durable_outbox_operation") or ""
require("[1; 16]" in reuse_test and "[2; 16]" in reuse_test, "C1-10 reuse test does not start from competing operation IDs")
require(reuse_test.count("enqueue_or_reuse_outbox_operation") >= 2, "C1-10 reuse test does not execute the production seam twice")

atomic_test = fn_body(all_sources, "accepted_outbox_commit_atomically_updates_baseline_and_ack_state") or ""
require("DROP TRIGGER" in atomic_test, "C1-10 atomic commit test never removes its injected DB failure before the success case")
require(re.search(r"assert_eq!\s*\(\s*after_failure\s*,\s*before\s*\)", atomic_test) is not None,
        "C1-10 atomic commit failure is not a complete durable-state zero-mutation comparison")

restart_test = fn_body(all_sources, "restart_redelivers_preexisting_sent_outbox_without_redetection") or ""
require("prepare_durable_outbox_operations" not in restart_test, "C1-10 restart test redetects/reprepares instead of draining a pre-existing row")

partial_test = fn_body(all_sources, "partial_ack_commits_only_accepted_operation") or ""
require(partial_test.count("operation_id") >= 4, "C1-10 partial ACK fixture does not distinguish request and response identities")

for name in (
    "restart_redelivers_preexisting_sent_outbox_without_redetection",
    "adapter_failure_preserves_outbox_and_schedules_bounded_retry",
    "partial_ack_commits_only_accepted_operation",
    "missing_or_unknown_ack_never_acknowledges_outbox",
):
    body = fn_body(coordinator, name) or ""
    require(body, f"C1-FR8 {name} is not colocated with the production dispatcher")
    require("dispatch_durable_outbox" in body and ".await" in body,
            f"C1-FR8 {name} does not await the production dispatcher")

prepare_test = fn_body(change, "durable_preparation_does_not_advance_baseline_before_acceptance") or ""
require(prepare_test, "C1-FR8 preparation/baseline boundary test is not colocated with production preparation")
require("prepare_durable_outbox_operations" in prepare_test,
        "C1-FR8 preparation/baseline test does not call the production preparer")

roundtrip_test = fn_body(outbox, "outbox_roundtrip_reconstructs_exact_sync_operation") or ""
require("insert_outbox_record" in roundtrip_test and "get_outbox_by_id" in roundtrip_test,
        "C1-FR8 roundtrip test bypasses durable DAO insert/read")
require(re.search(r"assert_eq!\s*\(\s*actual\s*,\s*expected\s*\)", roundtrip_test) is not None,
        "C1-FR8 roundtrip test compares only operation_id instead of the complete operation")

for name in (
    "incomplete_outbox_record_fails_closed_before_network",
    "pending_source_reuse_covers_sent_failed_and_exact_scope",
    "outbox_state_mutations_are_scoped_cas_and_batch_atomic",
    "delete_source_identity_is_deterministic_and_invalid_inputs_fail",
):
    require(fn_body(all_sources, name), f"C1-FR8 missing hardened regression {name}")

atomic_test = fn_body(outbox, "accepted_outbox_commit_atomically_updates_baseline_and_ack_state") or ""
require("OutboxState::Sent" in atomic_test,
        "C1-FR8 accepted-commit test starts from the wrong non-sent state")
require("snapshot_outbox_baseline_and_path" in atomic_test,
        "C1-FR8 accepted-commit test lacks complete outbox+baseline+path snapshots")

roundtrip_code = code_only(roundtrip_test)
require(re.search(r"assert_eq!\s*\(\s*actual\s*,\s*expected\s*\)", roundtrip_code) is not None,
        "C1-FINAL-06 exact roundtrip assertion exists only in a comment")

atomic_code = code_only(atomic_test)
require("assert_ne!" not in atomic_code,
        "C1-FINAL-06 accepted-commit success uses difference-only instead of exact expected state")
for token in ("source_hash", "hex::encode", "expected_success"):
    require(token in atomic_code, f"C1-FINAL-06 accepted-commit test lacks exact success evidence {token!r}")

accepted_snapshot_body = code_only(fn_body(outbox, "snapshot_outbox_baseline_and_path") or "")
accepted_snapshot_decl = fn_decl(outbox, "snapshot_outbox_baseline_and_path")
require("bool" not in accepted_snapshot_decl,
        "ARCR1-04 accepted snapshot reduces exact path mapping to a boolean")
for token in ("content_hash", "updated_at", "doc_id", "rel_path"):
    require(token in accepted_snapshot_body,
            f"ARCR1-04 accepted snapshot omits exact baseline/path field {token!r}")
require("after_success.0.as_ref().unwrap().updated_at" not in atomic_code,
        "ARCR1-04 accepted Upsert expected state copies timestamp from actual output")

incomplete_test = fn_body(coordinator, "incomplete_outbox_record_fails_closed_before_network") or ""
incomplete_code = code_only(incomplete_test)
require(incomplete_test, "C1-FINAL-06 incomplete-row test is not on the production dispatcher")
for token in ("dispatch_durable_outbox", ".await", "push_calls", "doc_hash", "rel_path", "encrypted_payload", "payload_hash", "source_hash", "before", "after"):
    require(token in incomplete_code, f"C1-FINAL-06 incomplete-row dispatcher test lacks {token!r}")

reuse_scope_code = code_only(fn_body(outbox, "pending_source_reuse_covers_sent_failed_and_exact_scope") or "")
for token in ("OutboxState::Sent", "OutboxState::Failed", "OutboxState::Acknowledged", '"v2"', '"server"', "different"):
    require(token in reuse_scope_code, f"C1-FINAL-06 pending-source scope/state test lacks {token!r}")

state_scope_code = code_only(fn_body(outbox, "outbox_state_mutations_are_scoped_cas_and_batch_atomic") or "")
for token in ('"v2"', '"server"', "same_operation_id", "before", "after"):
    require(token in state_scope_code, f"C1-FINAL-06 scoped CAS test lacks cross-scope evidence {token!r}")

delete_test_code = code_only(fn_body(change, "delete_source_identity_is_deterministic_and_invalid_inputs_fail") or "")
require("assert_eq!(1, 1)" not in delete_test_code,
        "C1-FINAL-06 delete-source regression is a tautology")
require(delete_test_code.count("delete_source_hash") >= 3,
        "C1-FINAL-06 delete-source regression does not prove deterministic and distinct counterfactuals")
for token in ("LocalChange", "prepare_durable_outbox_operations", "invalid", "before", "after"):
    require(token in delete_test_code, f"C1-FINAL-06 delete/invalid preparation regression lacks {token!r}")

prepare_test_code = code_only(prepare_test)
require("vec![]" not in prepare_test_code,
        "C1-FINAL-06 preparation baseline test calls production with an empty change set")
for token in ("LocalChange", "tempdir", "prepare_durable_outbox_operations", "source_hash", "baseline_before", "baseline_after"):
    require(token in prepare_test_code, f"C1-FINAL-06 preparation baseline test lacks real fixture evidence {token!r}")

for name in (
    "restart_redelivers_preexisting_sent_outbox_without_redetection",
    "adapter_failure_preserves_outbox_and_schedules_bounded_retry",
    "partial_ack_commits_only_accepted_operation",
    "missing_or_unknown_ack_never_acknowledges_outbox",
):
    test_code = code_only(fn_body(coordinator, name) or "")
    require("before" in test_code and "after" in test_code,
            f"C1-FINAL-06 {name} lacks explicit before/after durable snapshots")

partial_test_code = code_only(fn_body(coordinator, "partial_ack_commits_only_accepted_operation") or "")
require("reverse" in partial_test_code,
        "C1-FINAL-06 partial ACK regression does not prove ID correlation with reversed response order")

missing_test_code = code_only(fn_body(coordinator, "missing_or_unknown_ack_never_acknowledges_outbox") or "")
require("UPDATE sync_outbox SET state = 'ready'" not in missing_test_code,
        "C1-FINAL-06 ACK protocol fixtures reuse/mutate one DB instead of independent durable states")
for token in ("missing_before", "missing_after", "unknown_before", "unknown_after", "duplicate_before", "duplicate_after"):
    require(token in missing_test_code, f"C1-FINAL-06 ACK protocol test lacks independent snapshot {token!r}")

migration_test = code_only(fn_body(schema, "legacy_outbox_v5_migration_preserves_rows_and_quarantines_unreconstructable_records") or "")
for token in ("before_rows", "after_rows", "after_reopen", "assert_eq!"):
    require(token in migration_test, f"C1-FINAL-06 v5 migration test lacks complete snapshot evidence {token!r}")

for name in (
    "accepted_delete_removes_baseline_and_path_atomically",
    "rejected_ack_without_error_schedules_only_that_row",
    "retry_batch_failure_is_atomic_and_backoff_is_capped",
):
    require(fn_body(all_sources, name), f"C1-FINAL-06 missing final counterfactual regression {name}")

architecture_tests = (
    "outbox_validation_rejects_incomplete_new_rows_and_entry_kind_mismatch",
    "incomplete_due_row_is_quarantined_once_without_starving_valid_rows",
    "delete_source_hash_is_unambiguous_for_delimiter_counterexamples",
    "sent_batch_rejects_duplicate_not_due_and_rolls_back_second_item",
    "retry_persistence_failure_keeps_adapter_and_database_context",
    "legacy_outbox_v5_migration_complete_snapshot_survives_real_reopen",
)
for name in architecture_tests:
    require(fn_body(all_sources, name), f"C1-ARCH missing architecture-rework regression {name}")

validation_test = code_only(fn_body(outbox, "outbox_validation_rejects_incomplete_new_rows_and_entry_kind_mismatch") or "")
if validation_test:
    for token in (
        "missing_doc_hash", "missing_rel_path", "missing_source_hash",
        "missing_encrypted_payload", "missing_payload_hash", "upsert_with_asset_ref",
        "asset_without_asset_ref", "OutboxState::Failed", "next_retry_at",
        "insert_outbox_record", "enqueue_or_reuse_outbox_operation",
    ):
        require(token in validation_test, f"ARC1-01 complete validation regression lacks {token!r}")
    for token in ("before_validation", "after_validation"):
        require(token in validation_test,
                f"ARCR1-01 validation rejection lacks exact durable zero-mutation evidence {token!r}")
    require(re.search(r"assert_eq!\s*\(\s*after_validation\s*,\s*before_validation\s*\)", validation_test) is not None,
            "ARCR1-01 validation rejection never exact-compares durable state")
    require("get_dispatchable_outbox" not in validation_test,
            "ARCR2-01 validation zero-mutation snapshot still hides Prepared/Acknowledged/inert rows")
    require("snapshot_all_scoped_outbox" in validation_test,
            "ARCR2-01 validation test does not use the complete all-state scoped outbox snapshot")
    require(validation_test.count("insert_outbox_record") >= 8,
            "ARCR2-01 validation test does not exercise insert for every malformed fixture")
    require(validation_test.count("enqueue_or_reuse_outbox_operation") >= 8,
            "ARCR2-01 validation test does not exercise enqueue for every malformed fixture")

validation_snapshot_body = code_only(fn_body(outbox, "snapshot_all_scoped_outbox") or "")
validation_snapshot_decl = fn_decl(outbox, "snapshot_all_scoped_outbox")
require("AppResult" in validation_snapshot_decl,
        "ARCR2-01 complete validation snapshot cannot propagate durable read/decode errors")
for token in ("SELECT operation_id", "sync_outbox", "get_outbox_by_id", "ORDER BY"):
    require(token in validation_snapshot_body,
            f"ARCR2-01 complete validation snapshot lacks all-row evidence {token!r}")
require("get_dispatchable_outbox" not in validation_snapshot_body and "unwrap_or_default" not in validation_snapshot_body,
        "ARCR2-01 complete validation snapshot still filters states or swallows errors")
require(re.search(
    r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*(?:pub(?:\s*\([^)]*\))?\s+)?fn\s+snapshot_all_scoped_outbox\s*\(",
    outbox,
    re.S,
) is not None, "EVC1-01 snapshot_all_scoped_outbox ships as a production API instead of a test-only seam")
require("try_into" in validation_snapshot_body,
        "EVC1-01 snapshot helper does not checked-convert raw operation IDs")
require("if blob.len() == 16" not in validation_snapshot_body and "Ok(arr)" not in validation_snapshot_body,
        "EVC1-01 snapshot helper still fabricates [0;16] for malformed operation IDs")
require("operation_id length must be 16 bytes" in validation_snapshot_body,
        "EVC1-01 malformed operation-ID error is not actionable")

malformed_id_test = code_only(fn_body(
    outbox,
    "snapshot_all_scoped_outbox_rejects_malformed_operation_id_without_aliasing_zero_id",
) or "")
require(malformed_id_test,
        "EVC1-01 missing dynamic malformed-ID/all-zero-ID alias regression")
if malformed_id_test:
    for token in (
        "zero_id", "malformed_id", "vec![0u8; 16]", "INSERT INTO sync_outbox",
        "snapshot_all_scoped_outbox", "is_err", "operation_id",
    ):
        require(token in malformed_id_test,
                f"EVC1-01 malformed-ID alias regression lacks {token!r}")

quarantine_test = code_only(fn_body(coordinator, "incomplete_due_row_is_quarantined_once_without_starving_valid_rows") or "")
if quarantine_test:
    for token in (
        "dispatch_durable_outbox", "push_calls", "next_retry_at", "last_error",
        "OutboxState::Failed", "valid", "first", "second", "assert_eq!",
    ):
        require(token in quarantine_test, f"ARC1-01 quarantine/liveness regression lacks {token!r}")
    require(quarantine_test.count("dispatch_durable_outbox") >= 2,
            "ARC1-01 quarantine/liveness regression does not prove progress on the next dispatch")
    require("assert_ne!" not in quarantine_test,
            "ARCR1-01 quarantine/liveness test still proves only that something changed")
    for token in ("expected_first", "expected_second"):
        require(token in quarantine_test,
                f"ARCR1-01 quarantine/liveness test lacks exact expected snapshot {token!r}")
    expected_first_match = re.search(
        r"let\s+expected_first\s*=\s*(.*?);\s*assert_eq!\s*\(\s*after_first",
        quarantine_test,
        re.S,
    )
    require(expected_first_match is not None,
            "ARCR2-02 quarantine test does not executable-compare expected_first to after_first")
    if expected_first_match is not None:
        require("after_first" not in expected_first_match.group(1),
                "ARCR2-02 expected_first is circular because it copies the actual after_first snapshot")
    require("before_quarantine" in quarantine_test or "expected_corrupt" in quarantine_test,
            "ARCR2-02 quarantine expected row is not independently derived from seed/before state")

delete_collision_test = code_only(fn_body(change, "delete_source_hash_is_unambiguous_for_delimiter_counterexamples") or "")
if delete_collision_test:
    for token in ('"a:b"', '"b:c"', "assert_ne!", "delete_source_hash"):
        require(token in delete_collision_test, f"ARC1-02 delimiter-collision regression lacks {token!r}")
    require(delete_collision_test.count("delete_source_hash") >= 4,
            "ARC1-02 delimiter-collision regression lacks both colliding legacy tuples and scope controls")

sent_batch_test = code_only(fn_body(outbox, "sent_batch_rejects_duplicate_not_due_and_rolls_back_second_item") or "")
if sent_batch_test:
    for token in (
        "duplicate", "not_due", "second_item", "before", "after", "assert_eq!",
        "mark_outbox_batch_sent", "next_retry_at",
    ):
        require(token in sent_batch_test, f"ARC1-03 strict sent-batch regression lacks {token!r}")
    require(sent_batch_test.count("mark_outbox_batch_sent") >= 3,
            "ARC1-03 strict sent-batch regression does not execute duplicate, not-due and rollback cases")
    for token in ("wrong_state", "OutboxState::Prepared"):
        require(token in sent_batch_test,
                f"ARCR1-02 sent-batch regression omits an independent wrong-state item; lacks {token!r}")
    require(sent_batch_test.count("mark_outbox_batch_sent") >= 4,
            "ARCR1-02 sent-batch regression does not execute a separate wrong-state rollback fixture")

retry_context_test = code_only(fn_body(coordinator, "retry_persistence_failure_keeps_adapter_and_database_context") or "")
if retry_context_test:
    for token in (
        "dispatch_durable_outbox", "CREATE TRIGGER", "network error", "injected",
        "before", "after", "push_calls", "assert",
    ):
        require(token in retry_context_test, f"ARC1-03 retry context regression lacks {token!r}")
    require("||" not in retry_context_test,
            "ARCR2-04 retry-context assertion permits dropping the underlying network cause")
    require(re.search(r"assert!\s*\(\s*err_str\.contains\s*\(\s*network_error\s*\)\s*\)", retry_context_test) is not None,
            "ARCR2-04 retry-context test does not independently require the underlying network error")

require(re.search(r"assert_eq!\s*\(\s*after_success\s*,\s*expected_success\s*\)", atomic_code) is not None,
        "ARC1-04 accepted Upsert test still does not exact-compare its complete expected tuple")
delete_atomic_code = code_only(fn_body(outbox, "accepted_delete_removes_baseline_and_path_atomically") or "")
for token in ("CREATE TRIGGER", "DROP TRIGGER", "before", "after_failure", "after_success", "expected_success"):
    require(token in delete_atomic_code, f"ARC1-04 accepted Delete atomic regression lacks {token!r}")
require(re.search(r"assert_eq!\s*\(\s*after_failure\s*,\s*before\s*\)", delete_atomic_code) is not None,
        "ARC1-04 accepted Delete failure does not exact-compare complete zero-mutation state")
require(re.search(r"assert_eq!\s*\(\s*after_success\s*,\s*expected_success\s*\)", delete_atomic_code) is not None,
        "ARC1-04 accepted Delete success does not exact-compare complete expected state")

for name in (
    "restart_redelivers_preexisting_sent_outbox_without_redetection",
    "adapter_failure_preserves_outbox_and_schedules_bounded_retry",
    "partial_ack_commits_only_accepted_operation",
    "missing_or_unknown_ack_never_acknowledges_outbox",
    "rejected_ack_without_error_schedules_only_that_row",
):
    test_code = code_only(fn_body(coordinator, name) or "")
    require("snapshot_dispatch_scope" in test_code,
            f"ARC1-05 {name} bypasses the complete outbox/baseline/path snapshot helper")

dispatch_snapshot_body = code_only(fn_body(coordinator, "snapshot_dispatch_scope") or "")
dispatch_snapshot_decl = fn_decl(coordinator, "snapshot_dispatch_scope")
require("AppResult" in dispatch_snapshot_decl,
        "ARCR1-05 dispatch snapshot cannot propagate DAO/raw query failures")
require("get_dispatchable_outbox" not in dispatch_snapshot_body,
        "ARCR1-05 dispatch snapshot omits acknowledged/inert rows by reading only dispatchable records")
require("unwrap_or_default" not in dispatch_snapshot_body,
        "ARCR1-05 dispatch snapshot converts durable read errors into an empty snapshot")
for token in ("SELECT operation_id", "sync_outbox", "get_outbox_by_id", "ORDER BY"):
    require(token in dispatch_snapshot_body,
            f"ARCR1-05 dispatch snapshot does not enumerate every scoped outbox row; lacks {token!r}")

for name in (
    "restart_redelivers_preexisting_sent_outbox_without_redetection",
    "adapter_failure_preserves_outbox_and_schedules_bounded_retry",
    "partial_ack_commits_only_accepted_operation",
    "missing_or_unknown_ack_never_acknowledges_outbox",
    "incomplete_outbox_record_fails_closed_before_network",
    "rejected_ack_without_error_schedules_only_that_row",
    "retry_persistence_failure_keeps_adapter_and_database_context",
):
    exact_code = code_only(fn_body(coordinator, name) or "")
    require("dispatch_durable_outbox_at" in exact_code,
            f"ARCR1-04 {name} uses a hidden wall clock instead of injected deterministic now")
    require("assert_ne!" not in exact_code,
            f"ARCR1-05 {name} still proves only that something changed")
    require("expected_after" in exact_code,
            f"ARCR1-05 {name} lacks an independently constructed complete expected snapshot")

for name in (
    "restart_redelivers_preexisting_sent_outbox_without_redetection",
    "adapter_failure_preserves_outbox_and_schedules_bounded_retry",
    "partial_ack_commits_only_accepted_operation",
    "rejected_ack_without_error_schedules_only_that_row",
    "retry_persistence_failure_keeps_adapter_and_database_context",
):
    before_code = code_only(fn_body(coordinator, name) or "")
    require("expected_before" in before_code,
            f"ARCR2-03 {name} declares a before snapshot but never constructs its complete expectation")
    require(re.search(r"assert_eq!\s*\(\s*before\s*,\s*expected_before\s*\)", before_code) is not None,
            f"ARCR2-03 {name} does not executable-compare the complete before snapshot")

ack_before_code = code_only(fn_body(coordinator, "missing_or_unknown_ack_never_acknowledges_outbox") or "")
for prefix in ("missing", "unknown", "duplicate"):
    require(f"{prefix}_expected_before" in ack_before_code,
            f"ARCR2-03 ACK fixture {prefix} lacks a complete expected-before snapshot")
    require(re.search(
        rf"assert_eq!\s*\(\s*{prefix}_before\s*,\s*{prefix}_expected_before\s*\)",
        ack_before_code,
    ) is not None, f"ARCR2-03 ACK fixture {prefix} leaves its before snapshot non-executable")

for name in (
    "partial_ack_commits_only_accepted_operation",
    "rejected_ack_without_error_schedules_only_that_row",
):
    capture_code = code_only(fn_body(coordinator, name) or "")
    for token in ("captured_ops.lock", "expected_ops", "assert_eq!"):
        require(token in capture_code,
                f"ARCR2-03 {name} does not exact-compare adapter input; lacks {token!r}")
    require("let _ =" not in capture_code,
            f"ARCR2-03 {name} retains evidence-token filler instead of executable assertions")
    require("outbox_record_to_sync_operation" not in capture_code,
            f"EVC1-02 {name} constructs expected adapter input with the production converter under test")
    require(capture_code.count("SyncOperation {") >= 2,
            f"EVC1-02 {name} does not independently construct both expected wire operations")
    for token in (
        "operation_id", "doc_hash", "entry_kind", "node_id", "rel_path",
        "encrypted_payload", "payload_hash", "timestamp",
    ):
        require(token in capture_code,
                f"EVC1-02 {name} independent wire expectation omits {token!r}")

partial_capture_code = code_only(fn_body(coordinator, "partial_ack_commits_only_accepted_operation") or "")
for forbidden in (
    "accepted_op_id", "rejected_op_id", "let accepted =", "let rejected =",
    "assert!(accepted", "assert_eq!(rec1.operation_id,", "assert_eq!(rec2.operation_id,",
):
    require(forbidden not in partial_capture_code,
            f"EVC1-03 partial ACK retains proxy/self evidence {forbidden!r}")

adapter_failure_code = code_only(fn_body(coordinator, "adapter_failure_preserves_outbox_and_schedules_bounded_retry") or "")
require("Adapter push failed: Sync error: network error" in adapter_failure_code,
        "ARCR2-04 adapter-failure test does not lock the exact original dispatcher cause")
require(re.search(r"assert_eq!\s*\([^;]*Adapter push failed: Sync error: network error", adapter_failure_code, re.S) is not None,
        "ARCR2-04 adapter-failure result can still pass after lossy AppError string rewrapping")

complete_incomplete_test = code_only(fn_body(coordinator, "incomplete_outbox_record_fails_closed_before_network") or "")
for token in (
    "missing_doc_hash", "missing_rel_path", "missing_source_hash",
    "missing_encrypted_payload", "missing_payload_hash", "snapshot_dispatch_scope",
):
    require(token in complete_incomplete_test,
            f"ARC1-05 independent incomplete-field dispatcher evidence lacks {token!r}")
require(complete_incomplete_test.count("dispatch_durable_outbox") >= 5,
        "ARC1-05 incomplete-field dispatcher evidence does not execute five independent fixtures")

migration_reopen_test = code_only(fn_body(schema, "legacy_outbox_v5_migration_complete_snapshot_survives_real_reopen") or "")
if migration_reopen_test:
    for token in (
        "tempdir", "db_path", "drop(conn)", "Connection::open", "before_rows",
        "after_rows", "after_reopen", "encrypted_payload", "payload_hash",
        "asset_ref_blob", "source_hash", "original_timestamp", "retry_count",
        "next_retry_at", "last_error", "created_at", "updated_at", "assert_eq!",
    ):
        require(token in migration_reopen_test, f"ARC1-06 real-reopen migration evidence lacks {token!r}")
    require(migration_reopen_test.count("Connection::open") >= 2,
            "ARC1-06 migration evidence never closes and reopens the file-backed database")
    for token in ("expected_after", "after_second_reopen"):
        require(token in migration_reopen_test,
                f"ARCR1-06 migration evidence lacks independent transform/idempotence snapshot {token!r}")
    require(migration_reopen_test.count("Connection::open") >= 3,
            "ARCR1-06 migration evidence does not reopen again after the second migration")
    require(re.search(r"assert_eq!\s*\(\s*after_rows\s*,\s*expected_after\s*\)", migration_reopen_test) is not None,
            "ARCR1-06 first migration result is not exact-compared to an independent expected transform")
    require(re.search(r"assert_eq!\s*\(\s*after_second_reopen\s*,\s*after_rows\s*\)", migration_reopen_test) is not None,
            "ARCR1-06 second migration/reopen idempotence is not exact-compared")

if errors:
    for error in errors:
        print(f"FAIL  {error}")
    print(f"STATIC MANIFEST FAILED: {len(errors)} required condition(s) missing")
    raise SystemExit(1)

print("PASS  C1 durable outbox runtime and evidence-shape manifest")
PY
then
  pass 'C1 static manifest'
else
  fail 'C1 static manifest'
fi

if (( failure_count > 0 )); then
  printf 'C1-EVIDENCE-CLOSURE-V1 BASELINE/STATIC FAILURE: %d gate(s) failed; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'accepted B1 identity regressions' "$app_root" cargo test sync::core::identity::tests
run_gate 'accepted B2 schema regressions' "$app_root" cargo test db::schema::tests
run_gate 'accepted B2 migration regressions' "$app_root" cargo test db::legacy_sync_migration::tests
run_gate 'accepted B3 CRDT/path regressions' "$app_root" cargo test db::crdt::tests
run_gate 'accepted B4 provider-state regressions' "$app_root" cargo test db::sync_provider_state::tests
run_gate 'C1 outbox DAO regressions' "$app_root" cargo test db::sync_outbox::tests
run_gate 'C1 same-source durable reuse' "$app_root" cargo test same_source_hash_reuses_exact_durable_outbox_operation
run_gate 'C1 exact outbox-operation roundtrip' "$app_root" cargo test outbox_roundtrip_reconstructs_exact_sync_operation
run_gate 'C1 restart sent-row redelivery' "$app_root" cargo test restart_redelivers_preexisting_sent_outbox_without_redetection
run_gate 'C1 atomic accepted commit' "$app_root" cargo test accepted_outbox_commit_atomically_updates_baseline_and_ack_state
run_gate 'C1 adapter failure retry retention' "$app_root" cargo test adapter_failure_preserves_outbox_and_schedules_bounded_retry
run_gate 'C1 partial ACK isolation' "$app_root" cargo test partial_ack_commits_only_accepted_operation
run_gate 'C1 missing/unknown ACK rejection' "$app_root" cargo test missing_or_unknown_ack_never_acknowledges_outbox
run_gate 'C1 baseline acceptance boundary' "$app_root" cargo test durable_preparation_does_not_advance_baseline_before_acceptance
run_gate 'C1 incomplete durable row fails closed' "$app_root" cargo test incomplete_outbox_record_fails_closed_before_network
run_gate 'C1 reuse covers sent/failed/exact scope' "$app_root" cargo test pending_source_reuse_covers_sent_failed_and_exact_scope
run_gate 'C1 scoped CAS and atomic batch state' "$app_root" cargo test outbox_state_mutations_are_scoped_cas_and_batch_atomic
run_gate 'C1 deterministic delete source and invalid input' "$app_root" cargo test delete_source_identity_is_deterministic_and_invalid_inputs_fail
run_gate 'C1 accepted delete atomic cleanup' "$app_root" cargo test accepted_delete_removes_baseline_and_path_atomically
run_gate 'C1 rejected ACK fallback retry' "$app_root" cargo test rejected_ack_without_error_schedules_only_that_row
run_gate 'C1 atomic batch retry and capped backoff' "$app_root" cargo test retry_batch_failure_is_atomic_and_backoff_is_capped
run_gate 'C1 complete new-row and entry-kind validation' "$app_root" cargo test outbox_validation_rejects_incomplete_new_rows_and_entry_kind_mismatch
run_gate 'C1 malformed due-row quarantine and liveness' "$app_root" cargo test incomplete_due_row_is_quarantined_once_without_starving_valid_rows
run_gate 'C1 unambiguous delete-source encoding' "$app_root" cargo test delete_source_hash_is_unambiguous_for_delimiter_counterexamples
run_gate 'C1 strict atomic sent-batch prevalidation' "$app_root" cargo test sent_batch_rejects_duplicate_not_due_and_rolls_back_second_item
run_gate 'C1 retry persistence preserves causal context' "$app_root" cargo test retry_persistence_failure_keeps_adapter_and_database_context
run_gate 'C1 v5 complete migration snapshot real reopen' "$app_root" cargo test legacy_outbox_v5_migration_complete_snapshot_survives_real_reopen
run_gate 'C1 malformed operation ID snapshot fails without zero-ID alias' "$app_root" cargo test snapshot_all_scoped_outbox_rejects_malformed_operation_id_without_aliasing_zero_id
run_gate \
  'app all targets excluding documented unrelated search failure' \
  "$app_root" \
  cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'C1-EVIDENCE-CLOSURE-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'C1-EVIDENCE-CLOSURE-V1 / C1-ORACLE-V7 PASSED. Slice C1 is eligible for external acceptance.\n'
