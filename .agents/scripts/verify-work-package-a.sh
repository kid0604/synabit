#!/usr/bin/env bash

set -u
set -o pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
app_root="${repo_root}/src-tauri"
server_root="${repo_root}/sync-server"
checkpoint_file="${repo_root}/docs/sync_implementation_plan.md"
oracle_file="${repo_root}/.agents/scripts/verify-work-package-a.sh"
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

printf 'Synabit Work package A frozen closure gate: A-CLOSURE-V1\n'

require_pattern \
  'AC-01 defines LegacySyncOperationV0' \
  'struct LegacySyncOperationV0' \
  "${app_root}/src/sync/adapter/gdrive.rs"
require_pattern \
  'AC-01 has frozen historical-layout regression test' \
  'gdrive_operation_codec_supports_frozen_historical_layouts' \
  "${app_root}/src/sync/adapter/gdrive.rs"
require_min_pattern_count_in_range \
  'AC-01 has trailing and malformed rejection evidence for all four layouts' \
  'is_err\(\)' \
  8 \
  "${app_root}/src/sync/adapter/gdrive.rs" \
  'async fn gdrive_operation_codec_supports_frozen_historical_layouts' \
  'async fn gdrive_ack_returns_unsupported_capability'
require_absent_pattern \
  'AC-01 removes never-emitted unprefixed envelope decode' \
  'decode_exact::<GDriveOpEnvelope>\(data\)' \
  "${app_root}/src/sync/adapter/gdrive.rs"

require_pattern \
  'AC-02 has active V5 producer-to-decoder regression test' \
  'production_v5_payloads_roundtrip_through_remote_decoder' \
  "${app_root}/src/sync"
require_pattern \
  'AC-02 defines the frozen shared production encoder' \
  'fn encode_sync_payload_v5' \
  "${app_root}/src/sync/core/change.rs"
require_absent_pattern_in_range \
  'AC-02 prepare_push_operations delegates V5 encoding instead of duplicating it' \
  'encrypt_v5' \
  "${app_root}/src/sync/core/change.rs" \
  '^pub fn prepare_push_operations' \
  '^}'
require_min_pattern_count_in_range \
  'AC-02 prepare_push_operations calls the shared production encoder' \
  'encode_sync_payload_v5' \
  1 \
  "${app_root}/src/sync/core/change.rs" \
  '^pub fn prepare_push_operations' \
  '^}'
require_absent_pattern_in_range \
  'AC-02 regression test uses the shared producer encoder instead of duplicating it' \
  'encrypt_v5' \
  "${app_root}/src/sync/coordinator.rs" \
  'async fn production_v5_payloads_roundtrip_through_remote_decoder' \
  'async fn authenticated_current_legacy_and_nested_trailing_bytes_are_rejected'
require_min_pattern_count_in_range \
  'AC-02 regression covers all three payload variants through the shared encoder' \
  'encode_sync_payload_v5' \
  3 \
  "${app_root}/src/sync/coordinator.rs" \
  'async fn production_v5_payloads_roundtrip_through_remote_decoder' \
  'async fn authenticated_current_legacy_and_nested_trailing_bytes_are_rejected'
require_min_pattern_count_in_range \
  'AC-02 regression proves V5 wire-version bytes for Upsert and Delete' \
  '\[0\].*0x05' \
  2 \
  "${app_root}/src/sync/coordinator.rs" \
  'async fn production_v5_payloads_roundtrip_through_remote_decoder' \
  'async fn authenticated_current_legacy_and_nested_trailing_bytes_are_rejected'
require_min_pattern_count_in_range \
  'AC-02 regression proves compression flags for Upsert and Delete' \
  '\[1\].*0x01' \
  2 \
  "${app_root}/src/sync/coordinator.rs" \
  'async fn production_v5_payloads_roundtrip_through_remote_decoder' \
  'async fn authenticated_current_legacy_and_nested_trailing_bytes_are_rejected'
require_pattern \
  'AC-03 has authenticated malformed-payload regression test' \
  'authenticated_current_legacy_and_nested_trailing_bytes_are_rejected' \
  "${app_root}/src/sync"

require_pattern \
  'AC-04 has Delete cursor regression test' \
  'unsupported_delete_prevents_cursor_advancement' \
  "${app_root}/src/sync/coordinator.rs"
require_pattern \
  'AC-04 has AssetReference cursor regression test' \
  'unsupported_asset_reference_prevents_cursor_advancement' \
  "${app_root}/src/sync/coordinator.rs"

require_pattern \
  'AC-05 has deterministic provider identity regression test' \
  'server_provider_id_stable_across_device_endpoint_and_reconnect_state' \
  "${app_root}/src/sync/adapter/server.rs"
require_pattern \
  'AC-05 defines a socket-free production identity component' \
  'struct ServerAdapterIdentity' \
  "${app_root}/src/sync/adapter/server.rs"
require_absent_pattern_in_range \
  'AC-05 identity does not store provider_id as mutable runtime state' \
  'provider_id' \
  "${app_root}/src/sync/adapter/server.rs" \
  'struct ServerAdapterIdentity' \
  '^}'
require_pattern_in_range \
  'AC-05 identity derives provider ID from the frozen provider constant' \
  'SERVER_PROVIDER_ID' \
  "${app_root}/src/sync/adapter/server.rs" \
  'impl ServerAdapterIdentity' \
  '^}'
require_absent_pattern_in_range \
  'AC-05 identity adapter_id cannot depend on mutable provider state' \
  'self\.provider_id' \
  "${app_root}/src/sync/adapter/server.rs" \
  'impl ServerAdapterIdentity' \
  '^}'
require_pattern_in_range \
  'AC-05 production adapter_id delegates to the identity component' \
  'self\.identity\.adapter_id\(\)' \
  "${app_root}/src/sync/adapter/server.rs" \
  'impl SyncAdapter for SynabitServerAdapter' \
  'async fn is_connected'
require_absent_pattern_in_range \
  'AC-05 regression does not assert constants or static helpers as a proxy' \
  'SERVER_PROVIDER_ID|adapter_id_static|GoogleDriveAdapter::for_testing_dummy' \
  "${app_root}/src/sync/adapter/server.rs" \
  'fn server_provider_id_stable_across_device_endpoint_and_reconnect_state' \
  'fn server_push_batch_preserves_all_three_entry_kinds'
require_min_pattern_count_in_range \
  'AC-05 regression constructs distinct socket-free identity states' \
  'ServerAdapterIdentity' \
  3 \
  "${app_root}/src/sync/adapter/server.rs" \
  'fn server_provider_id_stable_across_device_endpoint_and_reconnect_state' \
  'fn server_push_batch_preserves_all_three_entry_kinds'
require_min_pattern_count_in_range \
  'AC-05 regression exercises identity behavior for each state' \
  '\.adapter_id\(\)' \
  3 \
  "${app_root}/src/sync/adapter/server.rs" \
  'fn server_provider_id_stable_across_device_endpoint_and_reconnect_state' \
  'fn server_push_batch_preserves_all_three_entry_kinds'

require_pattern \
  'AC-06 app frame reader uses exact consumption' \
  'take_from_bytes' \
  "${app_root}/src/sync/protocol.rs"
require_pattern \
  'AC-06 app has trailing-frame regression test' \
  'read_message_rejects_trailing_postcard_bytes' \
  "${app_root}/src/sync/protocol.rs"
require_pattern \
  'AC-06 server frame reader uses exact consumption' \
  'take_from_bytes' \
  "${server_root}/src/protocol.rs"
require_pattern \
  'AC-06 server has trailing-frame regression test' \
  'read_message_rejects_trailing_postcard_bytes' \
  "${server_root}/src/protocol.rs"

require_absent_pattern \
  'A1 has no SyncTransport' \
  'SyncTransport' \
  "${app_root}/src/sync" "${server_root}/synabit-protocol/src"
require_absent_pattern \
  'A2 has no string entry kind on core/protocol path' \
  'entry_kind:[[:space:]]*String' \
  "${app_root}/src/sync" "${server_root}/synabit-protocol/src"
require_absent_pattern \
  'R6 has no legacy sync_inbox_operations path' \
  'sync_inbox_operations' \
  "${app_root}/src/db" "${app_root}/src/sync"

if (( failure_count > 0 )); then
  printf 'STATIC MANIFEST FAILED: %d required condition(s) missing; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'app GDrive adapter tests' "$app_root" cargo test sync::adapter::gdrive::tests
run_gate 'app coordinator tests' "$app_root" cargo test sync::coordinator::tests
run_gate 'app server adapter tests' "$app_root" cargo test sync::adapter::server::tests
run_gate 'app protocol tests' "$app_root" cargo test sync::protocol::tests
run_gate 'app schema tests' "$app_root" cargo test db::schema::tests
run_gate \
  'app all targets excluding documented unrelated search failure' \
  "$app_root" \
  cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

run_gate 'shared protocol cargo fmt --check' "$server_root" cargo fmt --check
run_gate 'shared protocol cargo check' "$server_root" cargo check -p synabit-protocol
run_gate 'shared protocol tests' "$server_root" cargo test -p synabit-protocol
run_gate 'mailbox server cargo check' "$server_root" cargo check -p synabit-sync-server
run_gate 'mailbox server tests' "$server_root" cargo test -p synabit-sync-server

if (( failure_count > 0 )); then
  printf 'A-CLOSURE-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'A-CLOSURE-V1 PASSED. Known unrelated search test was explicitly skipped, not reported as PASS.\n'
