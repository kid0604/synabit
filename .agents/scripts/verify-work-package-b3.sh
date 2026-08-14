#!/usr/bin/env bash

# Immutable external acceptance oracle for Work package B, slice B3.
# Antigravity may read and execute this file but must never modify it.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
checkpoint_file="$repo_root/docs/sync_implementation_plan.md"
oracle_file="$repo_root/.agents/scripts/verify-work-package-b3.sh"
crdt_file="$app_root/src/db/crdt.rs"
kv_file="$app_root/src/db/kv.rs"
change_file="$app_root/src/sync/core/change.rs"
apply_file="$app_root/src/sync/core/apply.rs"
coordinator_file="$app_root/src/sync/coordinator.rs"
nodes_file="$app_root/src/commands/nodes.rs"
identity_file="$app_root/src/sync/core/identity.rs"
sync_mod_file="$app_root/src/sync/mod.rs"
old_sync_migration_file="$app_root/src/sync/migration.rs"
b2_oracle="$repo_root/.agents/scripts/verify-work-package-b2.sh"
b2_oracle_expected_sha256="ca8731ac9c4b08d994ad570e241f21d01149f19ae7312c8a0d1ec057956b2a17"

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

pass() {
  printf 'PASS  %s\n' "$1"
}

fail() {
  printf 'FAIL  %s\n' "$1"
  failure_count=$((failure_count + 1))
}

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

printf 'Synabit Work package B slice B3 frozen gate: B3-CLOSURE-V1 / B3-ORACLE-V3\n'

if python3 - \
  "$crdt_file" \
  "$kv_file" \
  "$change_file" \
  "$apply_file" \
  "$coordinator_file" \
  "$nodes_file" \
  "$identity_file" \
  "$sync_mod_file" \
  "$old_sync_migration_file" <<'PY'
import pathlib
import re
import sys

(
    crdt_path,
    kv_path,
    change_path,
    apply_path,
    coordinator_path,
    nodes_path,
    identity_path,
    sync_mod_path,
    old_migration_path,
) = map(pathlib.Path, sys.argv[1:])

errors = []

def require(condition, message):
    if not condition:
        errors.append(message)

def read_required(path):
    require(path.is_file(), f"required source file missing: {path}")
    return path.read_text() if path.is_file() else ""

def function_args(source, name):
    match = re.search(rf"pub\s+fn\s+{re.escape(name)}\s*\((.*?)\)\s*(?:->|where|\{{)", source, re.S)
    return match.group(1) if match else None

def test_body(source, name):
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", source)
    if not match:
        return None
    start = source.find("{", match.end())
    if start < 0:
        return None
    depth = 0
    for index in range(start, len(source)):
        char = source[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[start:index + 1]
    return None

crdt = read_required(crdt_path)
kv = read_required(kv_path)
change = read_required(change_path)
apply = read_required(apply_path)
coordinator = read_required(coordinator_path)
nodes = read_required(nodes_path)
identity = read_required(identity_path)
sync_mod = read_required(sync_mod_path)

scoped_methods = (
    "get_crdt_doc",
    "save_crdt_delta",
    "save_crdt_snapshot",
    "replace_crdt_snapshot",
    "compact_crdt_history",
    "compact_all_crdt",
    "export_snapshots",
    "delete_crdt_doc",
    "get_node_id_by_path",
    "get_path_by_node_id",
    "get_document_paths",
    "upsert_document_path",
    "delete_document_path",
    "get_document_baseline",
    "upsert_document_baseline",
    "delete_document_baseline",
)

for method in scoped_methods:
    args = function_args(crdt, method)
    require(args is not None, f"B3-01/B3-03 missing DAO method {method}")
    if args is not None:
        require(
            re.search(r"\bvault_id\s*:\s*&str\b", args) is not None,
            f"B3-01 {method} does not require explicit vault_id: &str",
        )

for method in ("get_document_baseline", "upsert_document_baseline", "delete_document_baseline"):
    args = function_args(crdt, method)
    if args is not None:
        require(
            re.search(r"\bprovider_id\s*:\s*&str\b", args) is not None,
            f"B3-03 {method} does not require explicit provider_id: &str",
        )

for table in (
    "sync_crdt_documents",
    "sync_crdt_updates",
    "sync_document_paths",
    "sync_document_baselines",
):
    require(table in crdt, f"B3-01/B3-03 active DAO does not use {table}")

legacy_sql = re.compile(
    r"(?:FROM|INTO|UPDATE|DELETE\s+FROM)\s+(?:crdt_documents|crdt_updates|document_paths)\b",
    re.I,
)
require(
    legacy_sql.search(crdt) is None,
    "B3-01 active DAO still reads or writes an unscoped legacy CRDT/path table",
)
require(
    ".flatten()" not in crdt and "unwrap_or(None)" not in crdt,
    "B3-05 active DAO still swallows SQLite row/decode errors",
)

get_crdt_body = test_body(crdt, "get_crdt_doc") or ""
require(
    "if let Ok(peer_id)" not in get_crdt_body and ".ok()" not in get_crdt_body,
    "B3-04 get_crdt_doc still swallows durable peer-id/set-peer errors",
)

get_peer_body = test_body(crdt, "get_or_create_peer_id") or ""
require(
    re.search(r'self\.get_kv\s*\(\s*"device_peer_id"\s*\)\s*\?', get_peer_body)
    is not None
    and "if let Ok" not in get_peer_body
    and "let _ =" not in get_peer_body
    and ".unwrap_or" not in get_peer_body
    and ".ok()" not in get_peer_body,
    "B3-04 peer-id lookup/create still swallows DB or decode failures",
)
get_kv_body = test_body(kv, "get_kv") or ""
require(
    "unwrap_or(None)" not in get_kv_body
    and "unwrap_or_default()" not in get_kv_body
    and ".flatten()" not in get_kv_body,
    "B3-04 get_kv still converts SQLite iteration/type errors into missing/default values",
)

delete_crdt_body = test_body(crdt, "delete_crdt_doc") or ""
require(
    (".transaction(" in delete_crdt_body or ".unchecked_transaction(" in delete_crdt_body)
    and ".commit()" in delete_crdt_body,
    "B3-01 delete_crdt_doc does not atomically delete snapshot and deltas",
)

replace_crdt_body = test_body(crdt, "replace_crdt_snapshot") or ""
require(
    (".transaction(" in replace_crdt_body or ".unchecked_transaction(" in replace_crdt_body)
    and ".commit()" in replace_crdt_body,
    "B3-01 replace_crdt_snapshot is not one atomic snapshot-upsert/delta-delete transaction",
)
replace_import_at = replace_crdt_body.find(".import(")
replace_tx_at = max(
    replace_crdt_body.find(".transaction("),
    replace_crdt_body.find(".unchecked_transaction("),
)
require(
    replace_import_at >= 0 and replace_tx_at >= 0 and replace_import_at < replace_tx_at,
    "B3-01 replace_crdt_snapshot does not validate the Loro snapshot before opening/mutating its transaction",
)

active_sources = "\n".join((change, apply, coordinator, nodes))
for key_prefix in ("sync_hash_", "p2p_sync:sha256:"):
    require(
        key_prefix not in active_sources,
        f"B3-03 active path still uses global KV baseline prefix {key_prefix}",
    )

require(
    "adapter.adapter_id()" in coordinator,
    "B3-04 coordinator does not derive the stable provider scope from adapter_id",
)
require(
    "vault_identity.vault_id" in coordinator,
    "B3-04 coordinator does not derive the vault scope from VaultIdentity",
)
require(
    re.search(r"compact_all_crdt\s*\(\s*&?vault_id", coordinator) is not None,
    "B3-04 pre-flight compaction is not explicitly vault-scoped",
)

ignored_durable_calls = re.compile(
    r"let\s+_\s*=\s*db\.(?:save_crdt_|delete_crdt_|upsert_document_path|"
    r"delete_document_path|upsert_document_baseline|delete_document_baseline)"
)
require(
    ignored_durable_calls.search(active_sources) is None,
    "B3-04 active callers still discard CRDT/path/baseline write failures",
)

require(
    re.search(r"fn\s+sync_crdt_snapshot_replace\s*\(.*?\)\s*->\s*AppResult\s*<\s*\(\s*\)\s*>", nodes, re.S)
    is not None,
    "B3-04 sync_crdt_snapshot_replace does not return AppResult<()> to its callers",
)
require(
    re.search(r"fn\s+crdt_apply_safe\s*\(.*?\)\s*->\s*AppResult\s*<\s*\(\s*\)\s*>", nodes, re.S)
    is not None,
    "B3-04 crdt_apply_safe does not return AppResult<()> to its callers",
)
require(
    re.search(r"if\s+let\s+Err\s*\([^)]*\)\s*=\s*db\.(?:save_crdt_|delete_crdt_)", nodes)
    is None,
    "B3-04 node helpers still log-and-continue after durable CRDT write failures",
)
require(
    re.search(r"if\s+let\s+Ok\s*\([^)]*\)\s*=\s*db\.get_crdt_doc", nodes) is None,
    "B3-04 node scan still converts CRDT read/corruption errors into a silent skip",
)
require(
    "get_document_baseline(&vault_id, \"gdrive\"" not in nodes
    and "get_document_baseline" not in nodes,
    "B3-03 provider-agnostic node scan still guesses or consumes a provider baseline",
)
require(
    "delete_crdt_doc" not in nodes,
    "B3-05 derived node scan/delete still clears durable scoped CRDT state",
)
require(
    "Err(_) => rel_path.clone()" not in nodes,
    "B3-05 node creation still falls back from durable UUID identity to rel_path",
)

scan_all_body = test_body(nodes, "scan_all_nodes") or ""
require(
    "get_or_assign_node_id" in scan_all_body and "upsert_document_path" in scan_all_body,
    "B3-05 scan_all_nodes still writes CRDT under derived relative-path node IDs",
)

pull_markdown_body = test_body(apply, "pull_markdown") or ""
require(
    "match db.get_crdt_doc" not in pull_markdown_body
    and pull_markdown_body.count("get_crdt_doc(vault_id, node_id)") == 1
    and "db.get_crdt_doc(vault_id, node_id)?" in pull_markdown_body,
    "B3-05 pull_markdown still treats corrupt scoped CRDT as a missing document and overwrites it",
)
require(
    "delete_crdt_doc" not in apply,
    "B3-01 active remote apply still performs non-atomic delete-then-save replacement",
)

pull_json_body = test_body(apply, "pull_json") or ""
winner_if_at = pull_json_body.find("if final_text != local_text")
winner_replace_at = pull_json_body.find("replace_crdt_snapshot")
winner_if_end = -1
if winner_if_at >= 0:
    winner_if_open = pull_json_body.find("{", winner_if_at)
    if winner_if_open >= 0:
        depth = 0
        for index in range(winner_if_open, len(pull_json_body)):
            if pull_json_body[index] == "{":
                depth += 1
            elif pull_json_body[index] == "}":
                depth -= 1
                if depth == 0:
                    winner_if_end = index
                    break
require(
    winner_if_at >= 0
    and winner_if_end >= 0
    and winner_replace_at > winner_if_end,
    "B3-01 pull_json still persists the final CRDT winner only when file bytes change",
)

prepare_push_body = test_body(change, "prepare_push_operations") or ""
require(
    "_ =>" not in prepare_push_body,
    "B3-04 deletion preparation still swallows scoped path lookup/decode errors",
)

crdt_apply_safe_body = test_body(nodes, "crdt_apply_safe") or ""
require(
    "match db.get_crdt_doc" not in crdt_apply_safe_body
    and "db.get_crdt_doc(vault_id, node_id)?" in crdt_apply_safe_body,
    "B3-05 crdt_apply_safe still overwrites corrupt scoped CRDT instead of propagating the read error",
)
require(
    "sync_crdt_snapshot_replace" not in crdt_apply_safe_body,
    "B3-05 crdt_apply_safe still overwrites durable state after a CRDT apply error",
)

node_identity_body = test_body(identity, "get_or_assign_node_id") or ""
require(
    "Fallback if not a json object" not in node_identity_body,
    "B3-05 JSON/canvas identity still falls back to a relative path instead of a durable node UUID or actionable error",
)

if old_migration_path.is_file():
    old_migration = old_migration_path.read_text()
    require(
        legacy_sql.search(old_migration) is None,
        "B3-04 obsolete compiled sync/migration.rs still mutates unscoped legacy tables",
    )
require(
    re.search(r"pub\s+mod\s+migration\s*;", sync_mod) is None or not old_migration_path.is_file(),
    "B3-04 obsolete sync migration module remains on the compiled path",
)

required_tests = {
    "vault_scoped_crdt_path_and_baseline_crud_isolated": (
        "vault_a",
        "vault_b",
        "same_doc",
        "same_path",
        "baseline",
        "apply_text_update",
        "delta_a",
        "delta_b",
        "assert_eq!",
    ),
    "cross_vault_crdt_path_and_baseline_mutations_preserve_other_vault": (
        "before",
        "after",
        "vault_a",
        "vault_b",
        "delete_crdt_doc",
        "delete_document_path",
        "delete_document_baseline",
        "after_doc_a",
        "after_updates_a",
        "after_paths_a",
        "after_baselines_a",
        "before_doc_a",
        "before_updates_a",
        "before_paths_a",
        "before_baselines_a",
        "delta_a",
        "delta_b",
        "apply_text_update",
        "assert_eq!",
    ),
    "vault_scoped_export_contains_only_requested_documents": (
        "export_snapshots",
        "vault_a",
        "vault_b",
        "assert_eq!",
    ),
    "scoped_crdt_compaction_rolls_back_on_failure": (
        "CREATE TRIGGER",
        "compact_crdt_history",
        "apply_text_update",
        "compaction_failed",
        "before",
        "after",
        "before_doc_b",
        "after_doc_b",
        "assert_eq!",
    ),
    "corrupt_scoped_crdt_and_path_rows_fail_closed": (
        "BLOB",
        "is_err",
        "sync_crdt_updates",
        "sync_document_paths",
        "get_document_paths(vault_a)",
        "before_paths",
        "after_paths",
        "assert_eq!",
    ),
    "delete_crdt_doc_rolls_back_on_second_statement_failure": (
        "CREATE TRIGGER",
        "delete_crdt_doc",
        "apply_text_update",
        "delete_failed",
        "before",
        "after",
        "assert_eq!",
    ),
    "peer_id_read_and_parse_errors_fail_closed_without_replacement": (
        "device_peer_id",
        "get_or_create_peer_id",
        "BLOB",
        "is_err",
        "before",
        "after",
        "assert_eq!",
    ),
    "replace_crdt_snapshot_rejects_invalid_and_rolls_back_on_delete_failure": (
        "replace_crdt_snapshot",
        "CREATE TRIGGER",
        "replace_failed",
        "invalid",
        "before",
        "after",
        "assert_eq!",
    ),
    "json_pull_persists_winner_when_file_bytes_already_match": (
        "pull_json",
        "replace_crdt_snapshot",
        "get_crdt_doc",
        "assert_eq!",
    ),
    "crdt_apply_safe_preserves_corrupt_durable_state": (
        "crdt_apply_safe",
        "sync_crdt_updates",
        "is_err",
        "before",
        "after",
        "assert_eq!",
    ),
    "json_node_identity_never_falls_back_to_relative_path": (
        "get_or_assign_node_id",
        "is_err",
        "before",
        "after",
        "assert_eq!",
    ),
}

test_sources = "\n".join((crdt, kv, change, apply, coordinator, nodes, identity))
for test_name, evidence_tokens in required_tests.items():
    body = test_body(test_sources, test_name)
    require(body is not None, f"B3-06 missing production-dependent regression {test_name}")
    if body is not None:
        for token in evidence_tokens:
            require(
                token in body,
                f"B3-06 {test_name} lacks required evidence token {token!r}",
            )
        if test_name == "cross_vault_crdt_path_and_baseline_mutations_preserve_other_vault":
            require(
                body.count("assert_eq!") >= 12 and ".len()" not in body,
                "B3-06 cross-vault mutation test does not assert both A mutation and B preservation across all four tables",
            )
        if test_name == "delete_crdt_doc_rolls_back_on_second_statement_failure":
            require(
                "unwrap_err" in body and '.contains("delete_failed")' in body,
                "B3-06 delete rollback test does not prove execution reached the injected second-statement failure",
            )
        if test_name == "scoped_crdt_compaction_rolls_back_on_failure":
            require(
                "delta_b" in body and "vec![7, 8]" not in body,
                "B3-06 compaction preservation fixture still stores an invalid raw-byte Loro delta",
            )

if errors:
    for error in errors:
        print(f"FAIL  {error}")
    print(f"STATIC MANIFEST FAILED: {len(errors)} required condition(s) missing")
    raise SystemExit(1)

print("PASS  B3 static API, active-path, fail-closed and evidence-shape manifest")
PY
then
  pass 'B3 static manifest'
else
  fail 'B3 static manifest'
fi

if (( failure_count > 0 )); then
  printf 'B3-CLOSURE-V1 BASELINE/STATIC FAILURE: %d gate(s) failed; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

if command -v shasum >/dev/null 2>&1; then
  b2_oracle_actual_sha256="$(shasum -a 256 "$b2_oracle" | awk '{ print $1 }')"
else
  b2_oracle_actual_sha256="$(sha256sum "$b2_oracle" | awk '{ print $1 }')"
fi
if [[ "$b2_oracle_actual_sha256" == "$b2_oracle_expected_sha256" ]]; then
  pass 'accepted B2 oracle digest remains immutable'
else
  fail "accepted B2 oracle digest remains immutable (expected $b2_oracle_expected_sha256, actual $b2_oracle_actual_sha256)"
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'accepted B2 schema regressions' "$app_root" cargo test db::schema::tests
run_gate 'accepted B2 legacy migration regressions' "$app_root" cargo test db::legacy_sync_migration::tests
run_gate 'accepted B1 identity regressions' "$app_root" cargo test sync::core::identity::tests
run_gate \
  'B3 two-vault CRDT/path/baseline CRUD isolation' \
  "$app_root" \
  cargo test vault_scoped_crdt_path_and_baseline_crud_isolated
run_gate \
  'B3 cross-vault mutation full-state isolation' \
  "$app_root" \
  cargo test cross_vault_crdt_path_and_baseline_mutations_preserve_other_vault
run_gate \
  'B3 vault-scoped export isolation' \
  "$app_root" \
  cargo test vault_scoped_export_contains_only_requested_documents
run_gate \
  'B3 compaction rollback atomicity' \
  "$app_root" \
  cargo test scoped_crdt_compaction_rolls_back_on_failure
run_gate \
  'B3 corrupt scoped rows fail closed' \
  "$app_root" \
  cargo test corrupt_scoped_crdt_and_path_rows_fail_closed
run_gate \
  'B3 atomic scoped delete rollback' \
  "$app_root" \
  cargo test delete_crdt_doc_rolls_back_on_second_statement_failure
run_gate \
  'B3 peer-id read and parse failures fail closed' \
  "$app_root" \
  cargo test peer_id_read_and_parse_errors_fail_closed_without_replacement
run_gate \
  'B3 snapshot replacement validation and rollback' \
  "$app_root" \
  cargo test replace_crdt_snapshot_rejects_invalid_and_rolls_back_on_delete_failure
run_gate \
  'B3 JSON equal-bytes winner is persisted' \
  "$app_root" \
  cargo test json_pull_persists_winner_when_file_bytes_already_match
run_gate \
  'B3 active CRDT apply preserves corrupt durable state' \
  "$app_root" \
  cargo test crdt_apply_safe_preserves_corrupt_durable_state
run_gate \
  'B3 JSON identity never falls back to relative path' \
  "$app_root" \
  cargo test json_node_identity_never_falls_back_to_relative_path
run_gate \
  'app all targets excluding documented unrelated search failure' \
  "$app_root" \
  cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'B3-CLOSURE-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'B3-CLOSURE-V1 / B3-ORACLE-V3 PASSED. This accepts only Package B slice B3; Work package B remains open until B4.\n'
