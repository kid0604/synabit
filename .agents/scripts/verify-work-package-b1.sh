#!/usr/bin/env bash

set -u
set -o pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
app_root="${repo_root}/src-tauri"
checkpoint_file="${repo_root}/docs/sync_implementation_plan.md"
oracle_file="${repo_root}/.agents/scripts/verify-work-package-b1.sh"
identity_file="${app_root}/src/sync/core/identity.rs"
server_command_file="${app_root}/src/commands/sync.rs"
gdrive_command_file="${app_root}/src/gdrive/sync.rs"
coordinator_file="${app_root}/src/sync/coordinator.rs"
stale_command_file="${app_root}/src/commands/sync_core.rs"
failure_count=0

expected_oracle_hash="$(awk -F'`' '/^- External oracle SHA-256: `/{ print $2; exit }' "$checkpoint_file")"
if command -v shasum >/dev/null 2>&1; then
  actual_oracle_hash="$(shasum -a 256 "$oracle_file" | awk '{ print $1 }')"
else
  actual_oracle_hash="$(sha256sum "$oracle_file" | awk '{ print $1 }')"
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

require_file_absent() {
  local label="$1"
  local file="$2"
  if [[ -e "$file" ]]; then
    fail "$label"
  else
    pass "$label"
  fi
}

require_pattern() {
  local label="$1"
  local pattern="$2"
  shift 2
  if command -v rg >/dev/null 2>&1; then
    if rg -q --glob '*.rs' "$pattern" "$@"; then
      pass "$label"
    else
      fail "$label"
    fi
  else
    if grep -r -q --include='*.rs' -E "$pattern" "$@"; then
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
    if rg -q --glob '*.rs' "$pattern" "$@"; then
      fail "$label"
    else
      pass "$label"
    fi
  else
    if grep -r -q --include='*.rs' -E "$pattern" "$@"; then
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

printf 'Synabit Work package B slice B1 frozen gate: B1-CLOSURE-V1 / B1-ORACLE-V3\n'

require_pattern \
  'B1-01 defines the only supported vault metadata schema version' \
  'VAULT_METADATA_SCHEMA_VERSION[^=]*=[[:space:]]*1' \
  "$identity_file"
require_pattern \
  'B1-01 serializes metadata with exact camelCase field names' \
  'serde\(rename_all[[:space:]]*=[[:space:]]*"camelCase"' \
  "$identity_file"
require_pattern \
  'B1-01 defines the shared metadata model' \
  'struct VaultMetadata' \
  "$identity_file"
require_pattern_in_range \
  'B1-01 metadata contains schema_version' \
  'schema_version[[:space:]]*:[[:space:]]*u32' \
  "$identity_file" \
  'struct VaultMetadata' \
  '^}'
require_pattern_in_range \
  'B1-01 metadata contains a UUID vault_id' \
  'vault_id[[:space:]]*:[[:space:]]*(uuid::)?Uuid' \
  "$identity_file" \
  'struct VaultMetadata' \
  '^}'
require_pattern \
  'B1-01 exposes a single production load/register seam' \
  'fn load_or_register_vault_identity' \
  "$identity_file"

require_pattern \
  'B1-02 defines an atomic metadata writer' \
  'fn write_vault_metadata_atomically' \
  "$identity_file"
require_pattern_in_range \
  'B1-02 atomic writer uses exclusive temporary-file creation' \
  'create_new\(true\)' \
  "$identity_file" \
  'fn write_vault_metadata_atomically' \
  '^}'
require_pattern_in_range \
  'B1-02 atomic writer writes through an explicit file handle' \
  'write_all' \
  "$identity_file" \
  'fn write_vault_metadata_atomically' \
  '^}'
require_min_pattern_count_in_range \
  'B1-02 atomic writer durably syncs file and parent directory' \
  'sync_all' \
  2 \
  "$identity_file" \
  'fn write_vault_metadata_atomically' \
  '^}'
require_pattern_in_range \
  'B1-02 atomic writer uses a no-clobber publication primitive' \
  'hard_link' \
  "$identity_file" \
  'fn write_vault_metadata_atomically' \
  '^}'
require_absent_pattern_in_range \
  'B1-02 atomic writer never falls back to replace-existing rename' \
  'std::fs::rename' \
  "$identity_file" \
  'fn write_vault_metadata_atomically' \
  '^}'
require_absent_pattern_in_range \
  'B1-02 atomic writer does not swallow filesystem failures' \
  'let _ =|\.ok\(\)|unwrap\(|unwrap_or_default' \
  "$identity_file" \
  'fn write_vault_metadata_atomically' \
  '^}'
require_pattern \
  'B1-02 handles concurrent publication without overwriting an existing identity' \
  'AlreadyExists' \
  "$identity_file"
require_pattern \
  'B1-02 exposes a typed metadata publication outcome' \
  'enum VaultMetadataPublishOutcome' \
  "$identity_file"
require_pattern_in_range \
  'B1-02 typed publication outcome distinguishes a newly published identity' \
  'Published' \
  "$identity_file" \
  'enum VaultMetadataPublishOutcome' \
  '^}'
require_pattern_in_range \
  'B1-02 typed publication outcome distinguishes the existing winner' \
  'Existing' \
  "$identity_file" \
  'enum VaultMetadataPublishOutcome' \
  '^}'
require_pattern_in_range \
  'B1-02 writer determines an outcome before its common durability epilogue' \
  'let outcome[[:space:]]*=[[:space:]]*match' \
  "$identity_file" \
  'fn write_vault_metadata_atomically' \
  '^}'
require_pattern_in_range \
  'B1-02 writer returns the typed outcome after parent-directory durability' \
  'Ok\(outcome\)' \
  "$identity_file" \
  'fn write_vault_metadata_atomically' \
  '^}'

require_pattern_in_range \
  'B1-03 production identity canonicalizes the vault root' \
  'canonicalize' \
  "$identity_file" \
  'fn load_or_register_vault_identity' \
  '^}'
require_pattern_in_range \
  'B1-03 production identity registers through sync_vaults DAO' \
  'insert_sync_vault_mapping' \
  "$identity_file" \
  'fn load_or_register_vault_identity' \
  '^}'
require_absent_pattern_in_range \
  'B1-03 production identity does not swallow mapping or parse failures' \
  'let _ =|\.ok\(\)|unwrap\(|unwrap_or_default' \
  "$identity_file" \
  'fn load_or_register_vault_identity' \
  '^}'
require_pattern_in_range \
  'B1-03 production identity consumes the typed existing-winner outcome' \
  'VaultMetadataPublishOutcome::Existing' \
  "$identity_file" \
  'fn load_or_register_vault_identity' \
  '^}'
require_absent_pattern_in_range \
  'B1-03 production identity does not infer publication outcome from error text' \
  'contains\("AlreadyExists"\)' \
  "$identity_file" \
  'fn load_or_register_vault_identity' \
  '^}'

require_pattern \
  'B1-04 P2P entrypoint uses the registered production identity' \
  'load_or_register_vault_identity' \
  "$server_command_file"
require_pattern \
  'B1-04 GDrive entrypoint uses the registered production identity' \
  'load_or_register_vault_identity' \
  "$gdrive_command_file"
require_pattern \
  'B1-04 coordinator consumes VaultIdentity instead of inventing identity' \
  'VaultIdentity' \
  "$coordinator_file"
require_absent_pattern_in_range \
  'B1-04 coordinator sync no longer accepts a raw vault_path string' \
  'vault_path:[[:space:]]*&str' \
  "$coordinator_file" \
  'pub async fn sync' \
  '^    }'
require_file_absent \
  'B1-04 removes the uncompiled duplicate sync_core command path' \
  "$stale_command_file"
require_absent_pattern \
  'B1-04 active entrypoints do not duplicate VaultMetadata definitions or direct vault.json writes' \
  'struct VaultMetadata|std::fs::write\([^\n]*vault\.json' \
  "$server_command_file" "$gdrive_command_file"

require_pattern \
  'B1-05 has atomic create/reopen behavior test' \
  'vault_metadata_atomic_create_and_reopen_is_stable' \
  "$identity_file"
require_pattern \
  'B1-05 has corrupt metadata zero-replacement behavior test' \
  'corrupt_vault_metadata_is_actionable_and_not_replaced' \
  "$identity_file"
require_pattern \
  'B1-05 has unsupported schema-version zero-replacement behavior test' \
  'unsupported_vault_metadata_version_is_rejected_and_not_replaced' \
  "$identity_file"
require_pattern \
  'B1-06 has canonical alias plus real DB registration behavior test' \
  'canonical_alias_registers_one_vault_mapping' \
  "$identity_file"
require_pattern_in_range \
  'B1-06 canonical alias regression uses a syntactically distinct parent-path alias' \
  'join\("\.\."\)' \
  "$identity_file" \
  'fn canonical_alias_registers_one_vault_mapping' \
  '^    }'
require_pattern \
  'B1-06 has cross-root vault-ID collision and zero-DB-mutation behavior test' \
  'same_vault_id_for_two_roots_is_rejected_without_db_mutation' \
  "$identity_file"
require_pattern \
  'B1-06 defines a full-row sync_vaults snapshot helper' \
  'fn snapshot_sync_vault_rows' \
  "$identity_file"
require_pattern_in_range \
  'B1-06 full-row snapshot reads all five durable mapping columns' \
  'SELECT vault_id, canonical_root, metadata_version, created_at, updated_at' \
  "$identity_file" \
  'fn snapshot_sync_vault_rows' \
  '^    }'
require_min_pattern_count_in_range \
  'B1-06 collision regression compares full rows before and after' \
  'snapshot_sync_vault_rows' \
  2 \
  "$identity_file" \
  'fn same_vault_id_for_two_roots_is_rejected_without_db_mutation' \
  '^    }'

require_pattern \
  'B1-02/B1-03 has concurrent identity convergence regression' \
  'concurrent_identity_creation_converges_on_one_published_id' \
  "$identity_file"
require_pattern_in_range \
  'B1-02/B1-03 concurrent regression coordinates competing creators' \
  'Barrier' \
  "$identity_file" \
  'fn concurrent_identity_creation_converges_on_one_published_id' \
  '^    }'
require_pattern_in_range \
  'B1-02/B1-03 concurrent regression uses the full production identity seam' \
  'load_or_register_vault_identity' \
  "$identity_file" \
  'fn concurrent_identity_creation_converges_on_one_published_id' \
  '^    }'
require_pattern_in_range \
  'B1-02/B1-03 concurrent regression inspects the published metadata file' \
  'read_to_string' \
  "$identity_file" \
  'fn concurrent_identity_creation_converges_on_one_published_id' \
  '^    }'
require_pattern_in_range \
  'B1-02/B1-03 concurrent regression inspects the registered DB mapping' \
  'get_sync_vault_by_canonical_root' \
  "$identity_file" \
  'fn concurrent_identity_creation_converges_on_one_published_id' \
  '^    }'

if (( failure_count > 0 )); then
  printf 'STATIC MANIFEST FAILED: %d required condition(s) missing; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'B1 production identity tests' "$app_root" cargo test sync::core::identity::tests
run_gate 'B1 sync_vault DAO collision tests' "$app_root" cargo test db::sync_vault::tests
run_gate \
  'app all targets excluding documented unrelated search failure' \
  "$app_root" \
  cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'B1-CLOSURE-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'B1-CLOSURE-V1 PASSED. This accepts only Package B slice B1; Work package B remains open.\n'
