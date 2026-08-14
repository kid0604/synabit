#!/usr/bin/env bash

# Immutable external repair oracle for Work package C slice C2A.
# Antigravity may read and execute this file but must never modify it.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
checkpoint_file="$repo_root/docs/sync_implementation_plan.md"
oracle_file="$repo_root/.agents/scripts/verify-work-package-c2a-repair1.sh"
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

printf 'Synabit C2A repair: C2A-REPAIR-1 / C2A-ORACLE-V2\n'

check_digest 'accepted A oracle immutable' "$repo_root/.agents/scripts/verify-work-package-a.sh" '5918897c035cc470b673cbd63c3655bb31bdcc65bf2fc0d4e22f51cfdeaa38f6'
check_digest 'accepted B1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b1.sh" '7dff56a65d6b047a717e95c9b2bc7cbfcf428494124bba768ce5f96296c8aa98'
check_digest 'accepted B2 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b2.sh" 'ca8731ac9c4b08d994ad570e241f21d01149f19ae7312c8a0d1ec057956b2a17'
check_digest 'accepted B3 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b3.sh" 'b0152d58b9591387cc02b188258d226827a22428e094d2afb7056b86efc0c7d7'
check_digest 'accepted B4 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b4.sh" '401b6f487a45ecceee4c31d9c87542e01a5b7ba20d88a737425343c3ba380e67'
check_digest 'accepted C1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-c1.sh" 'b41ed416450130c5f5da23e8a4b413937c6cd3470aaf4e02b890d3319c940e16'
check_digest 'rejected C2A V1 oracle remains immutable' "$repo_root/.agents/scripts/verify-work-package-c2a.sh" 'b9d82c6862c9df3cc9ad9a4755c2a6a46bd83b1234b7d46d9f9e460e054b6a3e'

check_digest 'repair scope lock: coordinator' "$app_root/src/sync/coordinator.rs" '613d0f988f545337f2278d31bb35f4b8d1b9e971414529342d085a32573c5b22'
check_digest 'repair scope lock: outbox' "$app_root/src/db/sync_outbox.rs" '9174a0eacf972732fecb52f256890d02f798bc211f289f906c22c83de4be0574'
check_digest 'repair scope lock: local preparation' "$app_root/src/sync/core/change.rs" 'e74b403f8a06dbc7d4a79b8f47931dc9288fd85b17aece25fe2e437a94df941e'
check_digest 'repair scope lock: adapter contract' "$app_root/src/sync/adapter/mod.rs" '01389e01536fc5d751f1ddc2c0311a46f704002ad4c85b343b5f613db7070e2c'
check_digest 'repair scope lock: GDrive adapter' "$app_root/src/sync/adapter/gdrive.rs" '64c5fc05010ed8023c5d644ad4f6e36bab6551bfeef32dabfb44a69d9dc50d91'
check_digest 'repair scope lock: server adapter' "$app_root/src/sync/adapter/server.rs" '404c26f559dd12e59ea801f053117a4a68df6d231081c29ef6e3664c243134ad'
check_digest 'repair scope lock: remote apply' "$app_root/src/sync/core/apply.rs" 'cf5e44f7771d53f22fe916da412365967ace8ae494b67f84be1932ae7d5d4a86'

if python3 - "$app_root/src/db/schema.rs" "$app_root/src/db/sync_inbox.rs" <<'PY'
import pathlib
import re
import sys

schema_path, inbox_path = map(pathlib.Path, sys.argv[1:])
schema = schema_path.read_text()
inbox = inbox_path.read_text()
all_sources = schema + "\n" + inbox
errors = []

def require(condition, message):
    if not condition:
        errors.append(message)

def fn_body(source, name):
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", source)
    if not match:
        return ""
    start = source.find("{", match.end())
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

def fn_decl(source, name):
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\((.*?)\)\s*(?:->\s*([^{{]+))?\{{", source, re.S)
    return match.group(0) if match else ""

def code_only(source):
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)

require(re.search(r"LATEST_SYNC_SCHEMA_VERSION\s*:\s*i64\s*=\s*6\s*;", schema) is not None,
        "R2A-01 schema version is not 6")
migration = code_only(fn_body(schema, "migrate_sync_schema_v6"))
for token in ("transaction", "CREATE TABLE sync_inbox_pages", "CREATE TABLE sync_inbox_page_entries", "sync_schema_meta", "tx.commit"):
    require(token in migration, f"R2A-01 v6 migration lacks {token!r}")
require("CREATE TABLE IF NOT EXISTS sync_inbox_pages" not in migration,
        "R2A-01 v6 silently accepts an incompatible pre-existing page table")
require("CREATE TABLE IF NOT EXISTS sync_inbox_page_entries" not in migration,
        "R2A-01 v6 silently accepts an incompatible pre-existing membership table")
require(re.search(r"CHECK\s*\(\s*entry_count\s*>=\s*0\s+AND\s+entry_count\s*<=\s*1000\s*\)", migration) is not None,
        "R2A-01 schema does not upper-bound page entry_count at 1000")

stage = code_only(fn_body(inbox, "stage_inbox_page"))
require(stage, "R2A-02 stage_inbox_page missing")
require("next_cursor.trim().is_empty()" in stage or "next_cursor.is_empty()" in stage,
        "R2A-02 stage does not reject empty next_cursor")
require("next_cursor == start_cursor" in stage,
        "R2A-02 stage does not reject non-advancing cursor")
for forbidden in ("entries.len() as u64", "idx as i64", "ex_has_more_raw == 1"):
    require(forbidden not in stage, f"R2A-02 stage retains unchecked/permissive conversion {forbidden!r}")
require(stage.count("remote_position") >= 3,
        "R2A-02 exact replay does not load and compare remote_position")
require("operation_id collision" in stage and "tx.commit" in stage,
        "R2A-02 stage replay/collision transaction boundary missing")

decode_bool = code_only(fn_body(inbox, "decode_inbox_page_bool"))
require(decode_bool, "R2A-02 missing shared checked SQLite boolean decoder")
for token in ("0 =>", "1 =>", "FromSqlConversionFailure"):
    require(token in decode_bool, f"R2A-02 checked boolean decoder lacks {token!r}")
require("decode_inbox_page_bool" in stage and "decode_inbox_page_bool" in code_only(fn_body(inbox, "get_inbox_page")),
        "R2A-02 stage replay and typed page read do not share checked boolean decoding")

entry_match = code_only(fn_body(inbox, "inbox_record_matches_staged_entry"))
require(entry_match, "R2A-02 missing shared exact inbox replay equality helper")
for token in (
    "remote_position", "remote_seq", "operation_id", "doc_hash", "entry_kind",
    "encrypted_payload", "payload_hash", "source_device",
):
    require(token in entry_match, f"R2A-02 replay equality helper omits {token!r}")
require("inbox_record_matches_staged_entry" in stage,
        "R2A-02 stage replay/collision path bypasses exact equality helper")

get_entries = code_only(fn_body(inbox, "get_inbox_page_entries"))
require(get_entries, "R2A-02 get_inbox_page_entries missing")
require(".min(MAX_INBOX_APPLY_BATCH)" not in get_entries,
        "R2A-02 page read still silently clamps invalid limits")
require("limit == 0" in get_entries and "limit > MAX_INBOX_APPLY_BATCH" in get_entries,
        "R2A-02 page read does not reject limits outside 1..=MAX")

mark_decl = fn_decl(inbox, "mark_inbox_page_applied_if_safe")
mark_safe = code_only(fn_body(inbox, "mark_inbox_page_applied_if_safe"))
require(mark_safe, "R2A-04 safe-page seam missing")
require("AppResult<bool>" not in mark_decl and "AppResult<()>" in mark_decl,
        "R2A-04 safe-page seam still models blockers as false instead of errors")
require("Ok(false)" not in mark_safe,
        "R2A-04 safe-page seam still silently returns Ok(false)")
require("InboxState::from_str" in mark_safe,
        "R2A-04 safe-page seam does not typed-decode every member state")
require("member_states.len() as u64" not in mark_safe,
        "R2A-04 safe-page seam retains unchecked member-count conversion")

commit = code_only(fn_body(inbox, "commit_applied_inbox_page_cursor"))
for token in ("transaction", "sync_provider_state", "sync_inbox_pages", "state = 'applied'", "cursor_committed", "tx.commit"):
    require(token in commit, f"R2A-05 atomic cursor seam lacks {token!r}")
require("ack_cursor" not in commit, "R2A-05 local cursor commit mutates ACK state")

snapshot = code_only(fn_body(all_sources, "snapshot_c2a_durable_scope_raw"))
require(snapshot, "R2A-03 missing shared complete raw C2A snapshot helper")
for token in (
    "rusqlite::types::Value", "sync_provider_state", "sync_inbox", "sync_inbox_pages",
    "sync_inbox_page_entries", "ORDER BY", "cursor", "ack_cursor", "sync_state",
    "incarnation_id", "remote_vault_id", "last_error", "created_at", "updated_at",
    "page_cursor", "remote_position", "remote_seq", "operation_id", "doc_hash",
    "entry_kind", "encrypted_payload", "payload_hash", "source_device", "state",
    "received_at", "applied_at", "start_cursor", "next_cursor", "has_more",
    "entry_count", "page_ordinal",
):
    require(token in snapshot, f"R2A-03 complete snapshot helper lacks {token!r}")

required_tests = {
    "legacy_v5_inbox_rows_survive_v6_page_ledger_migration_and_reopen": (
        "tempdir", "Connection::open", "migrate_sync_schema_v1", "migrate_sync_schema_v2",
        "migrate_sync_schema_v3", "migrate_sync_schema_v4", "migrate_sync_schema_v5",
        "migrate_sync_schema_v6", "drop(conn)", "after_reopen", "after_migration",
        "sync_schema_meta", "injected", "assert_eq!",
    ),
    "stage_inbox_page_persists_page_entries_and_membership_atomically": (
        "snapshot_c2a_durable_scope_raw", "CREATE TRIGGER", "after_failure", "before",
        "expected_success", "after_success", "assert_eq!",
    ),
    "stage_inbox_page_replay_is_exact_and_conflicts_roll_back": (
        "snapshot_c2a_durable_scope_raw", "next_cursor", "has_more", "remote_position",
        "operation_id", "entry_kind", "encrypted_payload", "before", "after", "assert_eq!",
    ),
    "empty_page_high_watermark_is_durable": (
        "tempdir", "Connection::open", "drop", "get_inbox_page", "start_cursor",
        "next_cursor", "has_more", "entry_count", "after_reopen", "assert_eq!",
    ),
    "inbox_page_numeric_sequence_1_2_10_is_ordered_and_bounded": (
        "get_inbox_page_entries", "Some(1)", "Some(2)", "Some(10)", "MAX_INBOX_APPLY_BATCH", "assert_eq!",
    ),
    "inbox_page_rejects_mixed_or_non_monotonic_remote_sequence_without_mutation": (
        "snapshot_c2a_durable_scope_raw", "mixed", "non_monotonic", "before", "after", "assert_eq!",
    ),
    "inbox_page_requires_all_members_safe_before_cursor_commit": (
        "snapshot_c2a_durable_scope_raw", "Pending", "Applying", "PendingAsset", "Failed",
        "Quarantined", "missing", "corrupt", "before", "after", "assert_eq!",
    ),
    "inbox_page_cursor_commit_is_atomic_scoped_and_does_not_ack": (
        "snapshot_c2a_durable_scope_raw", "CREATE TRIGGER", "before", "after_failure",
        "expected_success", "after_success", "ack_cursor", "assert_eq!",
    ),
    "inbox_page_ledger_isolated_by_vault_and_provider": (
        "snapshot_c2a_durable_scope_raw", "v1", "v2", "gdrive", "server",
        "before", "expected", "after", "assert_eq!",
    ),
    "stage_inbox_page_rejects_empty_next_cursor_and_corrupt_replay_metadata": (
        "stage_inbox_page", "ignore_check_constraints", "has_more", "next_cursor",
        "snapshot_c2a_durable_scope_raw", "before", "after", "assert_eq!",
    ),
    "get_inbox_page_entries_rejects_zero_and_oversized_limits": (
        "get_inbox_page_entries", "0", "MAX_INBOX_APPLY_BATCH + 1", "before", "after", "assert_eq!",
    ),
}

for name, tokens in required_tests.items():
    body = code_only(fn_body(all_sources, name))
    require(body, f"R2A evidence missing regression {name}")
    if body:
        for token in tokens:
            require(token in body, f"R2A {name} lacks executable evidence {token!r}")
        require("assert_eq!(1, 1)" not in body and "assert!(true)" not in body,
                f"R2A {name} contains tautological evidence")

atomic_stage = code_only(fn_body(all_sources, "stage_inbox_page_persists_page_entries_and_membership_atomically"))
atomic_cursor = code_only(fn_body(all_sources, "inbox_page_cursor_commit_is_atomic_scoped_and_does_not_ack"))
require(re.search(r"assert_eq!\s*\(\s*after_failure\s*,\s*before\s*\)", atomic_stage) is not None,
        "R2A atomic staging lacks exact complete rollback equality")
require(re.search(r"assert_eq!\s*\(\s*after_failure\s*,\s*before\s*\)", atomic_cursor) is not None,
        "R2A cursor commit lacks exact complete rollback equality")

raw_test_source = "\n".join(fn_body(all_sources, name) for name in required_tests)
require("References for oracle" not in raw_test_source and "oracle token" not in raw_test_source.lower(),
        "R2A tests retain explicit oracle-token comments")
require(re.search(r"let\s+_st_", raw_test_source) is None,
        "R2A tests retain unused state-token variables")

if errors:
    for error in errors:
        print(f"FAIL  {error}")
    print(f"STATIC MANIFEST FAILED: {len(errors)} condition(s) missing")
    raise SystemExit(1)

print("PASS  C2A repair production and evidence manifest")
PY
then
  pass 'C2A repair static manifest'
else
  fail 'C2A repair static manifest'
fi

if (( failure_count > 0 )); then
  printf 'C2A-REPAIR-1 RED/STATIC FAILURE: %d gate(s) failed; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'schema regressions' "$app_root" cargo test db::schema::tests
run_gate 'provider-state regressions' "$app_root" cargo test db::sync_provider_state::tests
run_gate 'accepted C1 outbox regressions' "$app_root" cargo test db::sync_outbox::tests
run_gate 'C2A inbox regressions' "$app_root" cargo test db::sync_inbox::tests
run_gate 'real v5-v6 migration/reopen/rollback' "$app_root" cargo test legacy_v5_inbox_rows_survive_v6_page_ledger_migration_and_reopen
run_gate 'complete atomic staging evidence' "$app_root" cargo test stage_inbox_page_persists_page_entries_and_membership_atomically
run_gate 'complete replay/collision evidence' "$app_root" cargo test stage_inbox_page_replay_is_exact_and_conflicts_roll_back
run_gate 'durable empty page reopen' "$app_root" cargo test empty_page_high_watermark_is_durable
run_gate 'numeric order and bounds' "$app_root" cargo test inbox_page_numeric_sequence_1_2_10_is_ordered_and_bounded
run_gate 'sequence rejection zero mutation' "$app_root" cargo test inbox_page_rejects_mixed_or_non_monotonic_remote_sequence_without_mutation
run_gate 'all unsafe member states block' "$app_root" cargo test inbox_page_requires_all_members_safe_before_cursor_commit
run_gate 'complete atomic cursor evidence' "$app_root" cargo test inbox_page_cursor_commit_is_atomic_scoped_and_does_not_ack
run_gate 'complete scope isolation evidence' "$app_root" cargo test inbox_page_ledger_isolated_by_vault_and_provider
run_gate 'empty cursor and corrupt replay rejection' "$app_root" cargo test stage_inbox_page_rejects_empty_next_cursor_and_corrupt_replay_metadata
run_gate 'invalid read limits rejected' "$app_root" cargo test get_inbox_page_entries_rejects_zero_and_oversized_limits
run_gate 'full app suite excluding documented unrelated search failure' "$app_root" cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'C2A-REPAIR-1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'C2A-REPAIR-1 / C2A-ORACLE-V2 PASSED. C2A is eligible for external re-audit.\n'
