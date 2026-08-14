#!/usr/bin/env bash

set -u
set -o pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
app_root="${repo_root}/src-tauri"
checkpoint_file="${repo_root}/docs/sync_implementation_plan.md"
oracle_file="${repo_root}/.agents/scripts/verify-work-package-b2.sh"
schema_file="${app_root}/src/db/schema.rs"
db_mod_file="${app_root}/src/db/mod.rs"
migration_file="${app_root}/src/db/legacy_sync_migration.rs"
identity_file="${app_root}/src/sync/core/identity.rs"
failure_count=0

expected_oracle_hash="$(awk -F'`' '/^- External oracle SHA-256: `/{ print $2; exit }' "$checkpoint_file")"
if command -v shasum >/dev/null 2>&1; then
  actual_oracle_hash="$(shasum -a 256 "$oracle_file" | awk '{ print $1}')"
else
  actual_oracle_hash="$(sha256sum "$oracle_file" | awk '{ print $1}')"
fi

if [[ -z "$expected_oracle_hash" || "$actual_oracle_hash" != "$expected_oracle_hash" ]]; then
  printf 'EXTERNAL_ORACLE_MUTATED expected=%s actual=%s\n' \
    "${expected_oracle_hash:-missing}" "$actual_oracle_hash"
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

require_file() {
  local label="$1"
  local file="$2"
  if [[ -f "$file" ]]; then
    pass "$label"
  else
    fail "$label"
  fi
}

require_pattern() {
  local label="$1"
  local pattern="$2"
  shift 2
  if command -v rg >/dev/null 2>&1; then
    if rg -q "$pattern" "$@" 2>/dev/null; then
      pass "$label"
    else
      fail "$label"
    fi
  else
    if grep -q -E "$pattern" "$@" 2>/dev/null; then
      pass "$label"
    else
      fail "$label"
    fi
  fi
}

require_absent_pattern() {
  local label="$1"
  local pattern="$2"
  shift 2
  if command -v rg >/dev/null 2>&1; then
    if rg -q "$pattern" "$@" 2>/dev/null; then
      fail "$label"
    else
      pass "$label"
    fi
  else
    if grep -q -E "$pattern" "$@" 2>/dev/null; then
      fail "$label"
    else
      pass "$label"
    fi
  fi
}

extract_range() {
  local file="$1"
  local start_pattern="$2"
  local end_pattern="$3"
  [[ -f "$file" ]] || return 0
  awk -v start_pattern="$start_pattern" -v end_pattern="$end_pattern" '
    $0 ~ start_pattern { in_range = 1 }
    in_range { print }
    in_range && $0 ~ end_pattern && $0 !~ start_pattern { exit }
  ' "$file"
}

require_pattern_in_range() {
  local label="$1"
  local pattern="$2"
  local file="$3"
  local start_pattern="$4"
  local end_pattern="$5"
  local source_range
  source_range="$(extract_range "$file" "$start_pattern" "$end_pattern")"
  if [[ "$source_range" =~ $pattern ]]; then
    pass "$label"
  else
    fail "$label"
  fi
}

require_absent_pattern_in_range() {
  local label="$1"
  local pattern="$2"
  local file="$3"
  local start_pattern="$4"
  local end_pattern="$5"
  local source_range
  source_range="$(extract_range "$file" "$start_pattern" "$end_pattern")"
  if [[ "$source_range" =~ $pattern ]]; then
    fail "$label"
  else
    pass "$label"
  fi
}

require_min_pattern_count_in_range() {
  local label="$1"
  local pattern="$2"
  local minimum="$3"
  local file="$4"
  local start_pattern="$5"
  local end_pattern="$6"
  local count
  count="$({ extract_range "$file" "$start_pattern" "$end_pattern" || true; } | awk -v pattern="$pattern" '$0 ~ pattern { count++ } END { print count + 0 }')"
  if (( count >= minimum )); then
    pass "$label"
  else
    fail "$label (found ${count}, need at least ${minimum})"
  fi
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

printf 'Synabit Work package B slice B2 frozen gate: B2-CLOSURE-V1 / B2-ORACLE-V3\n'

require_pattern \
  'B2-01 advances the versioned sync schema to v4' \
  'LATEST_SYNC_SCHEMA_VERSION:[[:space:]]*i64[[:space:]]*=[[:space:]]*4' \
  "$schema_file"
require_pattern_in_range \
  'B2-01 migration runner dispatches the explicit v4 step' \
  '4[[:space:]]*=>[[:space:]]*migrate_sync_schema_v4' \
  "$schema_file" \
  'fn run_sync_schema_migrations' \
  '^}'
require_pattern \
  'B2-01 defines the explicit v4 migration' \
  'fn migrate_sync_schema_v4' \
  "$schema_file"
require_pattern_in_range \
  'B2-01 v4 schema work is transactional' \
  '\.transaction\(' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-01 v4 advances schema metadata inside the migration' \
  'sync_schema_meta' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_absent_pattern_in_range \
  'B2-01 v4 does not ignore transaction, version, or commit failures' \
  'let _ =|\.ok\(\)|unwrap\(|unwrap_or_default' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'

require_pattern_in_range \
  'B2-02 creates vault-scoped CRDT document storage' \
  'CREATE TABLE sync_crdt_documents' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 creates vault-scoped CRDT update storage' \
  'CREATE TABLE sync_crdt_updates' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 creates vault-scoped durable path storage' \
  'CREATE TABLE sync_document_paths' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 creates vault/provider-scoped baseline storage' \
  'CREATE TABLE sync_document_baselines' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 creates durable legacy backup rows' \
  'CREATE TABLE sync_legacy_backup_rows' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 creates durable migration decision state' \
  'CREATE TABLE sync_legacy_migration_state' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_min_pattern_count_in_range \
  'B2-02 scoped target schema keys CRDT and paths by vault plus document' \
  'PRIMARY KEY[[:space:]]*\(vault_id,[[:space:]]*doc_id\)' \
  2 \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 path uniqueness is inside one vault' \
  'UNIQUE[[:space:]]*\(vault_id,[[:space:]]*rel_path\)' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 baseline identity includes vault, provider, and relative path' \
  'PRIMARY KEY[[:space:]]*\(vault_id,[[:space:]]*provider_id,[[:space:]]*rel_path\)' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 scoped CRDT updates preserve the production legacy update id' \
  'update_id[[:space:]]+INTEGER[[:space:]]+NOT NULL' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 scoped CRDT updates preserve production delta bytes' \
  'delta[[:space:]]+BLOB[[:space:]]+NOT NULL' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 scoped CRDT updates preserve production timestamps' \
  'timestamp[[:space:]]+INTEGER[[:space:]]+NOT NULL' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 scoped CRDT update ordering is unique inside a vault' \
  'PRIMARY KEY[[:space:]]*\(vault_id,[[:space:]]*update_id\)' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_min_pattern_count_in_range \
  'B2-02 CRDT and path targets reference durable vault identity' \
  'FOREIGN KEY[[:space:]]*\(vault_id\)[[:space:]]*REFERENCES[[:space:]]+sync_vaults' \
  3 \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 baseline target references durable provider identity' \
  'FOREIGN KEY[[:space:]]*\(vault_id,[[:space:]]*provider_id\)[[:space:]]*REFERENCES[[:space:]]+sync_provider_state' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 backup rows identify their migration version' \
  'migration_version[[:space:]]+INTEGER[[:space:]]+NOT NULL' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 backup rows preserve deterministic source ordering' \
  'source_order[[:space:]]+INTEGER[[:space:]]+NOT NULL' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 migration state records a typed decision' \
  'decision[[:space:]]+TEXT[[:space:]]+NOT NULL' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 migration state records completion metadata' \
  'completed_at[[:space:]]+INTEGER' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 migration state records actionable error metadata' \
  'last_error[[:space:]]+TEXT' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_min_pattern_count_in_range \
  'B2-02 backup and migration state both identify migration version' \
  'migration_version[[:space:]]+INTEGER[[:space:]]+NOT NULL' \
  2 \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 backup rows cannot duplicate one logical legacy source row' \
  'UNIQUE[[:space:]]*\(migration_version,[[:space:]]*source_table,[[:space:]]*source_key\)' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'
require_pattern_in_range \
  'B2-02 migration decision values are schema constrained' \
  'CHECK[[:space:]]*\(decision[[:space:]]+IN' \
  "$schema_file" \
  'fn migrate_sync_schema_v4' \
  '^}'

require_file \
  'B2-03 has one production legacy sync migration module' \
  "$migration_file"
require_pattern \
  'B2-03 production module is compiled by db mod' \
  'mod legacy_sync_migration' \
  "$db_mod_file"
require_pattern \
  'B2-03 defines one identity-bound migration seam' \
  'fn migrate_legacy_sync_state_for_vault' \
  "$migration_file"
require_pattern_in_range \
  'B2-03 migration seam requires the registered VaultIdentity' \
  'identity[[:space:]]*:[[:space:]]*&VaultIdentity' \
  "$migration_file" \
  'fn migrate_legacy_sync_state_for_vault' \
  '^}'
require_absent_pattern \
  'B2-03 legacy migration never invents a vault UUID' \
  'Uuid::new_v4|uuid::Uuid::new_v4' \
  "$migration_file"
require_pattern_in_range \
  'B2-03 active identity path invokes legacy migration after registration' \
  'migrate_legacy_sync_state_for_vault' \
  "$identity_file" \
  'fn load_or_register_vault_identity' \
  '^}'

require_pattern \
  'B2-04 defines a durable legacy-state backup operation' \
  'fn backup_legacy_sync_state' \
  "$migration_file"
require_pattern \
  'B2-04 inventories legacy CRDT documents' \
  'crdt_documents' \
  "$migration_file"
require_pattern \
  'B2-04 inventories legacy CRDT updates' \
  'crdt_updates' \
  "$migration_file"
require_pattern \
  'B2-04 inventories legacy document paths' \
  'document_paths' \
  "$migration_file"
require_pattern \
  'B2-04 inventories legacy cursor keys' \
  'sync_cursor_' \
  "$migration_file"
require_pattern \
  'B2-04 inventories legacy baseline keys' \
  'sync_hash_' \
  "$migration_file"
require_pattern \
  'B2-04 reads the exact production CRDT update columns' \
  'SELECT id, doc_id, delta, timestamp FROM crdt_updates' \
  "$migration_file"
require_pattern \
  'B2-04 reads the exact production document path timestamp' \
  'path_updated_at' \
  "$migration_file"
require_absent_pattern \
  'B2-04 never queries proxy-only legacy update columns' \
  'update_data|created_at FROM crdt_updates' \
  "$migration_file"
require_pattern \
  'B2-04 can reconstruct the complete legacy snapshot from durable backup' \
  'fn reconstruct_legacy_state_from_backup' \
  "$migration_file"
require_absent_pattern_in_range \
  'B2-04 production migration does not swallow backup or apply failures' \
  'let _ =|\.ok\(\)|unwrap\(|unwrap_or' \
  "$migration_file" \
  'fn migrate_legacy_sync_state_for_vault' \
  '^}'

require_pattern \
  'B2-05 exposes typed deterministic migration decisions' \
  'enum LegacyMigrationDecision' \
  "$migration_file"
require_pattern_in_range \
  'B2-05 distinguishes a completed deterministic migration' \
  'Migrated' \
  "$migration_file" \
  'enum LegacyMigrationDecision' \
  '^}'
require_pattern_in_range \
  'B2-05 distinguishes ambiguity requiring bootstrap' \
  'BootstrapRequired' \
  "$migration_file" \
  'enum LegacyMigrationDecision' \
  '^}'
require_pattern_in_range \
  'B2-05 distinguishes an already completed migration' \
  'AlreadyComplete' \
  "$migration_file" \
  'enum LegacyMigrationDecision' \
  '^}'
require_pattern \
  'B2-05 ambiguous legacy provider state is marked bootstrap_required' \
  "bootstrap_required" \
  "$migration_file"
require_pattern \
  'B2-05 migration mutates the real durable provider-state table' \
  'sync_provider_state' \
  "$migration_file"
require_pattern_in_range \
  'B2-05 bootstrap outcome writes the sync_state column' \
  "sync_state[[:space:]]*=[[:space:]]*'bootstrap_required'|sync_state[^\n]*bootstrap_required" \
  "$migration_file" \
  'fn migrate_legacy_sync_state_for_vault' \
  '^}'
require_absent_pattern_in_range \
  'B2-05 bootstrap outcome never stores a state sentinel in cursor' \
  "cursor[[:space:]]*=[[:space:]]*'bootstrap_required'" \
  "$migration_file" \
  'fn migrate_legacy_sync_state_for_vault' \
  '^}'
require_pattern_in_range \
  'B2-05 deterministic mapping validates the registered canonical identity' \
  'canonical_root' \
  "$migration_file" \
  'fn migrate_legacy_sync_state_for_vault' \
  '^}'
require_absent_pattern_in_range \
  'B2-05 baseline migration never guesses a provider from an underscore' \
  'rest\.find\(|\("default",[[:space:]]*rest\)' \
  "$migration_file" \
  'fn migrate_legacy_sync_state_for_vault' \
  '^}'
require_absent_pattern_in_range \
  'B2-05 scoped apply never silently ignores a conflicting durable row' \
  'INSERT OR IGNORE INTO sync_' \
  "$migration_file" \
  'fn migrate_legacy_sync_state_for_vault' \
  '^}'
require_pattern_in_range \
  'B2-05 legacy target mutation is transactional' \
  '\.transaction\(' \
  "$migration_file" \
  'fn migrate_legacy_sync_state_for_vault' \
  '^}'

require_pattern \
  'B2-06 defines a complete raw legacy-state snapshot helper' \
  'fn snapshot_legacy_state' \
  "$migration_file"
require_pattern \
  'B2-06 defines a vault-scoped target snapshot helper' \
  'fn snapshot_vault_scoped_sync_state' \
  "$migration_file"
require_pattern \
  'B2-06 defines a durable backup snapshot helper' \
  'fn snapshot_legacy_backup' \
  "$migration_file"
require_pattern \
  'B2-06 defines a complete migration-state snapshot helper' \
  'fn snapshot_legacy_migration_state' \
  "$migration_file"
require_pattern \
  'B2-06 defines a complete provider-state snapshot helper' \
  'fn snapshot_provider_states' \
  "$migration_file"
require_pattern_in_range \
  'B2-06 provider snapshot reads all ten durable provider-state columns' \
  'SELECT vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id, remote_vault_id, last_error, created_at, updated_at' \
  "$migration_file" \
  'fn snapshot_provider_states' \
  '^}'
require_absent_pattern \
  'B2-06 migration tests do not replace the production identity path with a helper clone' \
  'fn helper_load_or_register_vault_identity' \
  "$migration_file"
require_absent_pattern \
  'B2-06 migration tests do not swallow identity or migration results' \
  '\.ok\(\)' \
  "$migration_file"

require_pattern \
  'B2-06 tests fresh v4 scoped migration targets' \
  'fresh_sync_schema_v4_creates_vault_scoped_legacy_targets' \
  "$schema_file"
require_pattern \
  'B2-06 tests deterministic single-vault row and backup preservation' \
  'legacy_single_vault_migration_preserves_all_rows_and_backup' \
  "$migration_file"
require_min_pattern_count_in_range \
  'B2-06 deterministic migration snapshots raw source before and after' \
  'snapshot_legacy_state' \
  2 \
  "$migration_file" \
  'fn legacy_single_vault_migration_preserves_all_rows_and_backup' \
  '^    }'
require_pattern_in_range \
  'B2-06 deterministic migration inspects the scoped target state' \
  'snapshot_vault_scoped_sync_state' \
  "$migration_file" \
  'fn legacy_single_vault_migration_preserves_all_rows_and_backup' \
  '^    }'
require_pattern_in_range \
  'B2-06 deterministic migration inspects durable backup contents' \
  'snapshot_legacy_backup' \
  "$migration_file" \
  'fn legacy_single_vault_migration_preserves_all_rows_and_backup' \
  '^    }'
require_pattern_in_range \
  'B2-06 deterministic fixture uses a real registered identity mapping' \
  'insert_sync_vault_mapping' \
  "$migration_file" \
  'fn legacy_single_vault_migration_preserves_all_rows_and_backup' \
  '^    }'

require_pattern \
  'B2-06 tests exact production legacy DDL with two ordered deltas' \
  'production_legacy_schema_migrates_two_ordered_deltas_losslessly' \
  "$migration_file"
require_pattern_in_range \
  'B2-06 production fixture uses the real legacy update primary key' \
  'id INTEGER PRIMARY KEY AUTOINCREMENT' \
  "$migration_file" \
  'fn production_legacy_schema_migrates_two_ordered_deltas_losslessly' \
  '^    }'
require_pattern_in_range \
  'B2-06 production fixture uses the real legacy delta column' \
  'delta BLOB NOT NULL' \
  "$migration_file" \
  'fn production_legacy_schema_migrates_two_ordered_deltas_losslessly' \
  '^    }'
require_pattern_in_range \
  'B2-06 production fixture uses the real legacy timestamp column' \
  'timestamp INTEGER NOT NULL' \
  "$migration_file" \
  'fn production_legacy_schema_migrates_two_ordered_deltas_losslessly' \
  '^    }'
require_pattern_in_range \
  'B2-06 production fixture uses the real path_updated_at column' \
  'path_updated_at INTEGER NOT NULL' \
  "$migration_file" \
  'fn production_legacy_schema_migrates_two_ordered_deltas_losslessly' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 production fixture has at least two ordered legacy deltas' \
  'INSERT INTO crdt_updates' \
  2 \
  "$migration_file" \
  'fn production_legacy_schema_migrates_two_ordered_deltas_losslessly' \
  '^    }'
require_pattern_in_range \
  'B2-06 production fixture proves backup reconstructs every raw row' \
  'reconstruct_legacy_state_from_backup' \
  "$migration_file" \
  'fn production_legacy_schema_migrates_two_ordered_deltas_losslessly' \
  '^    }'

require_pattern \
  'B2-06 tests ambiguity with backup, bootstrap, and zero assignment' \
  'ambiguous_legacy_state_is_backed_up_and_requires_bootstrap_without_assignment' \
  "$migration_file"
require_min_pattern_count_in_range \
  'B2-06 ambiguous fixture registers two distinct durable vault mappings' \
  'insert_sync_vault_mapping' \
  2 \
  "$migration_file" \
  'fn ambiguous_legacy_state_is_backed_up_and_requires_bootstrap_without_assignment' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 ambiguity test proves raw legacy rows are unchanged' \
  'snapshot_legacy_state' \
  2 \
  "$migration_file" \
  'fn ambiguous_legacy_state_is_backed_up_and_requires_bootstrap_without_assignment' \
  '^    }'
require_pattern_in_range \
  'B2-06 ambiguity test inspects zero target assignment' \
  'snapshot_vault_scoped_sync_state' \
  "$migration_file" \
  'fn ambiguous_legacy_state_is_backed_up_and_requires_bootstrap_without_assignment' \
  '^    }'
require_pattern_in_range \
  'B2-06 ambiguity fixture includes an identifiable real cursor provider' \
  'sync_cursor_gdrive' \
  "$migration_file" \
  'fn ambiguous_legacy_state_is_backed_up_and_requires_bootstrap_without_assignment' \
  '^    }'
require_pattern_in_range \
  'B2-06 ambiguity test inspects durable provider states' \
  'snapshot_provider_states' \
  "$migration_file" \
  'fn ambiguous_legacy_state_is_backed_up_and_requires_bootstrap_without_assignment' \
  '^    }'
require_pattern_in_range \
  'B2-06 ambiguity test asserts the durable sync_state value' \
  'bootstrap_required' \
  "$migration_file" \
  'fn ambiguous_legacy_state_is_backed_up_and_requires_bootstrap_without_assignment' \
  '^    }'

require_pattern \
  'B2-06 tests underscore paths without inventing a provider' \
  'legacy_baseline_with_underscore_path_never_guesses_provider' \
  "$migration_file"
require_pattern_in_range \
  'B2-06 underscore-path regression uses the real provider-less legacy key' \
  'sync_hash_notes/my_file\.md' \
  "$migration_file" \
  'fn legacy_baseline_with_underscore_path_never_guesses_provider' \
  '^    }'
require_pattern_in_range \
  'B2-06 underscore-path regression inspects provider bootstrap state' \
  'snapshot_provider_states' \
  "$migration_file" \
  'fn legacy_baseline_with_underscore_path_never_guesses_provider' \
  '^    }'
require_pattern_in_range \
  'B2-06 provider-less baseline returns typed BootstrapRequired' \
  'LegacyMigrationDecision::BootstrapRequired' \
  "$migration_file" \
  'fn legacy_baseline_with_underscore_path_never_guesses_provider' \
  '^    }'

require_pattern \
  'B2-06 tests an existing provider row without corrupting unrelated fields' \
  'legacy_cursor_updates_existing_provider_without_overwriting_other_fields' \
  "$migration_file"
require_min_pattern_count_in_range \
  'B2-06 existing-provider regression compares full provider state' \
  'snapshot_provider_states' \
  2 \
  "$migration_file" \
  'fn legacy_cursor_updates_existing_provider_without_overwriting_other_fields' \
  '^    }'

require_pattern \
  'B2-06 tests identity mismatch before any legacy assignment' \
  'unregistered_or_mismatched_identity_cannot_claim_legacy_state' \
  "$migration_file"
require_min_pattern_count_in_range \
  'B2-06 identity-mismatch regression compares complete target state' \
  'snapshot_vault_scoped_sync_state' \
  2 \
  "$migration_file" \
  'fn unregistered_or_mismatched_identity_cannot_claim_legacy_state' \
  '^    }'

require_pattern \
  'B2-06 tests backup failure as full-state zero mutation' \
  'legacy_backup_failure_is_zero_mutation' \
  "$migration_file"
require_pattern_in_range \
  'B2-06 backup-failure test injects a real SQLite write failure' \
  'CREATE TRIGGER' \
  "$migration_file" \
  'fn legacy_backup_failure_is_zero_mutation' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 backup-failure test compares complete raw state' \
  'snapshot_legacy_state' \
  2 \
  "$migration_file" \
  'fn legacy_backup_failure_is_zero_mutation' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 backup-failure test compares complete target state' \
  'snapshot_vault_scoped_sync_state' \
  2 \
  "$migration_file" \
  'fn legacy_backup_failure_is_zero_mutation' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 backup-failure test compares migration marker state' \
  'snapshot_legacy_migration_state' \
  2 \
  "$migration_file" \
  'fn legacy_backup_failure_is_zero_mutation' \
  '^    }'

require_pattern \
  'B2-06 tests apply failure retains backup and source' \
  'legacy_apply_failure_preserves_committed_backup_and_source' \
  "$migration_file"
require_pattern_in_range \
  'B2-06 apply-failure test injects a real SQLite target failure' \
  'CREATE TRIGGER' \
  "$migration_file" \
  'fn legacy_apply_failure_preserves_committed_backup_and_source' \
  '^    }'
require_pattern_in_range \
  'B2-06 apply-failure test verifies committed backup contents' \
  'snapshot_legacy_backup' \
  "$migration_file" \
  'fn legacy_apply_failure_preserves_committed_backup_and_source' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 apply-failure test compares complete raw source state' \
  'snapshot_legacy_state' \
  2 \
  "$migration_file" \
  'fn legacy_apply_failure_preserves_committed_backup_and_source' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 apply-failure test compares complete target state' \
  'snapshot_vault_scoped_sync_state' \
  2 \
  "$migration_file" \
  'fn legacy_apply_failure_preserves_committed_backup_and_source' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 apply-failure test compares migration marker state' \
  'snapshot_legacy_migration_state' \
  2 \
  "$migration_file" \
  'fn legacy_apply_failure_preserves_committed_backup_and_source' \
  '^    }'

require_pattern \
  'B2-06 rejects conflicting target rows without marking migration complete' \
  'conflicting_scoped_target_aborts_without_marking_complete' \
  "$migration_file"
require_min_pattern_count_in_range \
  'B2-06 conflict regression compares target state before and after' \
  'snapshot_vault_scoped_sync_state' \
  2 \
  "$migration_file" \
  'fn conflicting_scoped_target_aborts_without_marking_complete' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 conflict regression compares migration marker before and after' \
  'snapshot_legacy_migration_state' \
  2 \
  "$migration_file" \
  'fn conflicting_scoped_target_aborts_without_marking_complete' \
  '^    }'

require_pattern \
  'B2-06 tests production reopen idempotence and stable vault identity' \
  'legacy_migration_reopen_is_idempotent_and_keeps_vault_identity' \
  "$migration_file"
require_min_pattern_count_in_range \
  'B2-06 reopen test calls the full registered identity path twice' \
  '[[:space:]=]load_or_register_vault_identity' \
  2 \
  "$migration_file" \
  'fn legacy_migration_reopen_is_idempotent_and_keeps_vault_identity' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 reopen test compares scoped durable state across both opens' \
  'snapshot_vault_scoped_sync_state' \
  2 \
  "$migration_file" \
  'fn legacy_migration_reopen_is_idempotent_and_keeps_vault_identity' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 reopen test compares raw vault metadata across both opens' \
  'read' \
  2 \
  "$migration_file" \
  'fn legacy_migration_reopen_is_idempotent_and_keeps_vault_identity' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 reopen test compares durable backup across both opens' \
  'snapshot_legacy_backup' \
  2 \
  "$migration_file" \
  'fn legacy_migration_reopen_is_idempotent_and_keeps_vault_identity' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 reopen test compares migration marker across both opens' \
  'snapshot_legacy_migration_state' \
  2 \
  "$migration_file" \
  'fn legacy_migration_reopen_is_idempotent_and_keeps_vault_identity' \
  '^    }'
require_pattern_in_range \
  'B2-06 reopen test starts with real non-empty legacy state' \
  'INSERT INTO crdt_updates|INSERT INTO crdt_documents' \
  "$migration_file" \
  'fn legacy_migration_reopen_is_idempotent_and_keeps_vault_identity' \
  '^    }'

require_pattern \
  'B2-06 tests BootstrapRequired reopen as a zero-mutation decision' \
  'bootstrap_required_reopen_is_idempotent' \
  "$migration_file"
require_min_pattern_count_in_range \
  'B2-06 bootstrap reopen compares durable backup state' \
  'snapshot_legacy_backup' \
  2 \
  "$migration_file" \
  'fn bootstrap_required_reopen_is_idempotent' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 bootstrap reopen compares full provider state' \
  'snapshot_provider_states' \
  2 \
  "$migration_file" \
  'fn bootstrap_required_reopen_is_idempotent' \
  '^    }'
require_min_pattern_count_in_range \
  'B2-06 bootstrap reopen compares migration marker state' \
  'snapshot_legacy_migration_state' \
  2 \
  "$migration_file" \
  'fn bootstrap_required_reopen_is_idempotent' \
  '^    }'

require_pattern \
  'B2-06 tests apply-failure retry without duplicate backup rows' \
  'apply_failure_retry_reuses_backup_without_duplicates' \
  "$migration_file"
require_min_pattern_count_in_range \
  'B2-06 apply retry compares backup before and after recovery' \
  'snapshot_legacy_backup' \
  2 \
  "$migration_file" \
  'fn apply_failure_retry_reuses_backup_without_duplicates' \
  '^    }'
require_pattern_in_range \
  'B2-06 apply retry proves backup still reconstructs exactly one source snapshot' \
  'reconstruct_legacy_state_from_backup' \
  "$migration_file" \
  'fn apply_failure_retry_reuses_backup_without_duplicates' \
  '^    }'

if (( failure_count > 0 )); then
  printf 'STATIC MANIFEST FAILED: %d required condition(s) missing; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'B2 versioned schema tests' "$app_root" cargo test db::schema::tests
run_gate 'B2 legacy migration tests' "$app_root" cargo test db::legacy_sync_migration::tests
run_gate 'B1 identity regression tests' "$app_root" cargo test sync::core::identity::tests
run_gate \
  'app all targets excluding documented unrelated search failure' \
  "$app_root" \
  cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'B2-CLOSURE-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'B2-CLOSURE-V1 PASSED. This accepts only Package B slice B2; Work package B remains open.\n'
