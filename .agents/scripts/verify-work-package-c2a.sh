#!/usr/bin/env bash

# Immutable external acceptance oracle for Work package C, slice C2A.
# Antigravity may read and execute this file but must never modify it.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
checkpoint_file="$repo_root/docs/sync_implementation_plan.md"
oracle_file="$repo_root/.agents/scripts/verify-work-package-c2a.sh"
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

printf 'Synabit Work package C slice C2A: C2A-DURABLE-INBOX-PAGE-LEDGER-V1 / C2A-ORACLE-V1\n'

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
check_digest 'accepted C1 oracle remains immutable' \
  "$repo_root/.agents/scripts/verify-work-package-c1.sh" \
  'b41ed416450130c5f5da23e8a4b413937c6cd3470aaf4e02b890d3319c940e16'

# C2A is a persistence-foundation slice. Lock the accepted C1/runtime surface so
# Builder cannot hide coordinator, adapter, apply, outbox, asset, or protocol
# changes inside this batch.
check_digest 'C2A scope lock: coordinator unchanged' \
  "$app_root/src/sync/coordinator.rs" \
  '613d0f988f545337f2278d31bb35f4b8d1b9e971414529342d085a32573c5b22'
check_digest 'C2A scope lock: outbox unchanged' \
  "$app_root/src/db/sync_outbox.rs" \
  '9174a0eacf972732fecb52f256890d02f798bc211f289f906c22c83de4be0574'
check_digest 'C2A scope lock: local preparation unchanged' \
  "$app_root/src/sync/core/change.rs" \
  'e74b403f8a06dbc7d4a79b8f47931dc9288fd85b17aece25fe2e437a94df941e'
check_digest 'C2A scope lock: adapter contract unchanged' \
  "$app_root/src/sync/adapter/mod.rs" \
  '01389e01536fc5d751f1ddc2c0311a46f704002ad4c85b343b5f613db7070e2c'
check_digest 'C2A scope lock: GDrive adapter unchanged' \
  "$app_root/src/sync/adapter/gdrive.rs" \
  '64c5fc05010ed8023c5d644ad4f6e36bab6551bfeef32dabfb44a69d9dc50d91'
check_digest 'C2A scope lock: server adapter unchanged' \
  "$app_root/src/sync/adapter/server.rs" \
  '404c26f559dd12e59ea801f053117a4a68df6d231081c29ef6e3664c243134ad'
check_digest 'C2A scope lock: remote apply unchanged' \
  "$app_root/src/sync/core/apply.rs" \
  'cf5e44f7771d53f22fe916da412365967ace8ae494b67f84be1932ae7d5d4a86'

if python3 - \
  "$app_root/src/db/schema.rs" \
  "$app_root/src/db/sync_inbox.rs" <<'PY'
import pathlib
import re
import sys

schema_path, inbox_path = map(pathlib.Path, sys.argv[1:])
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

def struct_body(source, name):
    match = re.search(rf"(?:pub\s+)?struct\s+{re.escape(name)}\s*\{{(.*?)\n\}}", source, re.S)
    return match.group(1) if match else ""

def code_only(source):
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)

schema = read(schema_path)
inbox = read(inbox_path)
all_sources = schema + "\n" + inbox

require(
    re.search(r"LATEST_SYNC_SCHEMA_VERSION\s*:\s*i64\s*=\s*6\s*;", schema) is not None,
    "C2A-01 sync schema version is not exactly 6",
)
require(
    re.search(r"6\s*=>\s*migrate_sync_schema_v6\s*\(\s*conn\s*\)\s*\?", schema) is not None,
    "C2A-01 migration runner does not execute migrate_sync_schema_v6",
)
migration = code_only(fn_body(schema, "migrate_sync_schema_v6") or "")
require(migration, "C2A-01 missing explicit migrate_sync_schema_v6")
for token in (
    "transaction", "CREATE TABLE sync_inbox_pages", "CREATE TABLE sync_inbox_page_entries",
    "start_cursor", "next_cursor", "has_more", "entry_count", "page_ordinal",
    "FOREIGN KEY", "sync_provider_state", "sync_inbox", "sync_schema_meta", "tx.commit",
):
    require(token in migration, f"C2A-01 v6 migration lacks {token!r}")
for token in ("CHECK (has_more IN (0, 1))", "CHECK (entry_count >= 0)", "CHECK (page_ordinal >= 0)"):
    require(token in migration, f"C2A-01 page ledger schema lacks {token!r}")

page_state = re.search(r"pub\s+enum\s+InboxPageState\s*\{(.*?)\n\}", inbox, re.S)
require(page_state is not None, "C2A-02 missing typed InboxPageState")
if page_state:
    for variant in ("Staged", "Applied", "CursorCommitted"):
        require(variant in page_state.group(1), f"C2A-02 InboxPageState lacks {variant}")

page_record = struct_body(inbox, "InboxPageRecord")
require(page_record, "C2A-02 missing InboxPageRecord")
for field in (
    "vault_id", "provider_id", "start_cursor", "next_cursor", "has_more",
    "entry_count", "state", "received_at", "updated_at",
):
    require(field in page_record, f"C2A-02 InboxPageRecord lacks {field!r}")

membership_record = struct_body(inbox, "InboxPageEntryRecord")
require(membership_record, "C2A-02 missing InboxPageEntryRecord")
for field in ("start_cursor", "page_ordinal", "operation_id"):
    require(field in membership_record, f"C2A-02 InboxPageEntryRecord lacks {field!r}")

stage_decl = fn_decl(inbox, "stage_inbox_page")
stage_body = code_only(fn_body(inbox, "stage_inbox_page") or "")
require(stage_body, "C2A-03 missing transactional stage_inbox_page")
for token in ("start_cursor", "next_cursor", "has_more", "entries", "received_at"):
    require(token in stage_decl, f"C2A-03 stage_inbox_page signature lacks {token!r}")
for token in (
    "transaction", "sync_inbox_pages", "sync_inbox", "sync_inbox_page_entries",
    "page_ordinal", "ON CONFLICT", "operation_id collision", "tx.commit",
):
    require(token in stage_body, f"C2A-03 stage transaction lacks {token!r}")
require(stage_body.find("sync_inbox_pages") < stage_body.find("tx.commit"),
        "C2A-03 page high-watermark is not persisted before transaction commit")
require("entries.is_empty()" not in stage_body or "return Ok(InboxStageResult" not in stage_body,
        "C2A-03 empty advancing page still returns before durable page-ledger insert")

get_page = code_only(fn_body(inbox, "get_inbox_page") or "")
get_entries = code_only(fn_body(inbox, "get_inbox_page_entries") or "")
require(get_page, "C2A-04 missing get_inbox_page")
require(get_entries, "C2A-04 missing bounded get_inbox_page_entries")
for token in ("sync_inbox_page_entries", "JOIN sync_inbox", "page_ordinal", "ORDER BY", "LIMIT"):
    require(token in get_entries, f"C2A-04 ordered page read lacks {token!r}")
require("MAX_INBOX_APPLY_BATCH" in get_entries,
        "C2A-04 page read does not enforce the frozen bounded apply limit")

mark_safe = code_only(fn_body(inbox, "mark_inbox_page_applied_if_safe") or "")
require(mark_safe, "C2A-05 missing mark_inbox_page_applied_if_safe")
for token in (
    "transaction", "sync_inbox_page_entries", "applied", "ignored_own_operation",
    "entry_count", "state = 'staged'", "rows_affected", "tx.commit",
):
    require(token in mark_safe, f"C2A-05 safe-page CAS lacks {token!r}")

commit_cursor = code_only(fn_body(inbox, "commit_applied_inbox_page_cursor") or "")
require(commit_cursor, "C2A-06 missing atomic page/cursor commit seam")
for token in (
    "transaction", "sync_inbox_pages", "sync_provider_state", "start_cursor",
    "next_cursor", "state = 'applied'", "cursor_committed", "rows_affected", "tx.commit",
):
    require(token in commit_cursor, f"C2A-06 page/cursor commit lacks {token!r}")
require("ack_cursor" not in commit_cursor,
        "C2A-06 local page/cursor transaction mutates remote ACK state")

required_tests = {
    "legacy_v5_inbox_rows_survive_v6_page_ledger_migration_and_reopen": (
        "migrate_sync_schema_v6", "before", "after", "reopen", "assert_eq!",
    ),
    "stage_inbox_page_persists_page_entries_and_membership_atomically": (
        "stage_inbox_page", "CREATE TRIGGER", "before", "after_failure", "after_success", "assert_eq!",
    ),
    "stage_inbox_page_replay_is_exact_and_conflicts_roll_back": (
        "stage_inbox_page", "conflict", "before", "after", "assert_eq!",
    ),
    "empty_page_high_watermark_is_durable": (
        "stage_inbox_page", "entry_count", "get_inbox_page", "assert_eq!",
    ),
    "inbox_page_numeric_sequence_1_2_10_is_ordered_and_bounded": (
        "stage_inbox_page", "get_inbox_page_entries", "1", "2", "10", "assert_eq!",
    ),
    "inbox_page_rejects_mixed_or_non_monotonic_remote_sequence_without_mutation": (
        "stage_inbox_page", "remote_seq", "mixed", "non_monotonic", "before", "after", "assert_eq!",
    ),
    "inbox_page_requires_all_members_safe_before_cursor_commit": (
        "mark_inbox_page_applied_if_safe", "transition_inbox_state", "Pending", "PendingAsset",
        "Failed", "Quarantined", "cursor", "before", "after", "assert_eq!",
    ),
    "inbox_page_cursor_commit_is_atomic_scoped_and_does_not_ack": (
        "commit_applied_inbox_page_cursor", "CREATE TRIGGER", "ack_cursor", "before", "after_failure", "after_success", "assert_eq!",
    ),
    "inbox_page_ledger_isolated_by_vault_and_provider": (
        "stage_inbox_page", "commit_applied_inbox_page_cursor", "v1", "v2", "gdrive", "server", "assert_eq!",
    ),
}

for name, tokens in required_tests.items():
    body = code_only(fn_body(all_sources, name) or "")
    require(body, f"C2A-07 missing regression {name}")
    if body:
        for token in tokens:
            require(token in body, f"C2A-07 {name} lacks executable evidence {token!r}")
        require("assert_eq!(1, 1)" not in body and "assert!(true)" not in body,
                f"C2A-07 {name} contains tautological evidence")

atomic_stage = code_only(fn_body(all_sources, "stage_inbox_page_persists_page_entries_and_membership_atomically") or "")
require(re.search(r"assert_eq!\s*\(\s*after_failure\s*,\s*before\s*\)", atomic_stage) is not None,
        "C2A-07 atomic staging failure does not exact-compare complete durable state")
atomic_cursor = code_only(fn_body(all_sources, "inbox_page_cursor_commit_is_atomic_scoped_and_does_not_ack") or "")
require(re.search(r"assert_eq!\s*\(\s*after_failure\s*,\s*before\s*\)", atomic_cursor) is not None,
        "C2A-07 atomic cursor failure does not exact-compare page and provider state")

if errors:
    for error in errors:
        print(f"FAIL  {error}")
    print(f"STATIC MANIFEST FAILED: {len(errors)} required condition(s) missing")
    raise SystemExit(1)

print("PASS  C2A durable inbox page-ledger and cursor-boundary manifest")
PY
then
  pass 'C2A static manifest'
else
  fail 'C2A static manifest'
fi

if (( failure_count > 0 )); then
  printf 'C2A-DURABLE-INBOX-PAGE-LEDGER-V1 RED/STATIC FAILURE: %d gate(s) failed; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'accepted B2 schema regressions' "$app_root" cargo test db::schema::tests
run_gate 'accepted B4 provider-state regressions' "$app_root" cargo test db::sync_provider_state::tests
run_gate 'accepted C1 outbox regressions' "$app_root" cargo test db::sync_outbox::tests
run_gate 'C2A inbox DAO regressions' "$app_root" cargo test db::sync_inbox::tests
run_gate 'C2A v5-to-v6 durable page-ledger migration' "$app_root" cargo test legacy_v5_inbox_rows_survive_v6_page_ledger_migration_and_reopen
run_gate 'C2A atomic page staging' "$app_root" cargo test stage_inbox_page_persists_page_entries_and_membership_atomically
run_gate 'C2A exact replay and collision rollback' "$app_root" cargo test stage_inbox_page_replay_is_exact_and_conflicts_roll_back
run_gate 'C2A durable empty-page watermark' "$app_root" cargo test empty_page_high_watermark_is_durable
run_gate 'C2A numeric sequence and bounded page read' "$app_root" cargo test inbox_page_numeric_sequence_1_2_10_is_ordered_and_bounded
run_gate 'C2A mixed/non-monotonic sequence rejection' "$app_root" cargo test inbox_page_rejects_mixed_or_non_monotonic_remote_sequence_without_mutation
run_gate 'C2A all-members-safe page boundary' "$app_root" cargo test inbox_page_requires_all_members_safe_before_cursor_commit
run_gate 'C2A atomic local page/cursor commit' "$app_root" cargo test inbox_page_cursor_commit_is_atomic_scoped_and_does_not_ack
run_gate 'C2A vault/provider page-ledger isolation' "$app_root" cargo test inbox_page_ledger_isolated_by_vault_and_provider
run_gate \
  'app all targets excluding documented unrelated search failure' \
  "$app_root" \
  cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'C2A-DURABLE-INBOX-PAGE-LEDGER-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'C2A-DURABLE-INBOX-PAGE-LEDGER-V1 / C2A-ORACLE-V1 PASSED. Slice C2A is eligible for external audit.\n'
