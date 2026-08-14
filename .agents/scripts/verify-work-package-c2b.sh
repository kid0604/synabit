#!/usr/bin/env bash

# Immutable external oracle for C2B durable inbox runtime cutover.
# Antigravity may read and execute this file but must never modify it.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
checkpoint_file="$repo_root/docs/sync_implementation_plan.md"
oracle_file="$repo_root/.agents/scripts/verify-work-package-c2b.sh"
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

printf 'Synabit C2B: C2B-DURABLE-INBOX-RUNTIME-CUTOVER-V1 / C2B-ORACLE-V1\n'

check_digest 'accepted A oracle immutable' "$repo_root/.agents/scripts/verify-work-package-a.sh" '5918897c035cc470b673cbd63c3655bb31bdcc65bf2fc0d4e22f51cfdeaa38f6'
check_digest 'accepted B1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b1.sh" '7dff56a65d6b047a717e95c9b2bc7cbfcf428494124bba768ce5f96296c8aa98'
check_digest 'accepted B2 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b2.sh" 'ca8731ac9c4b08d994ad570e241f21d01149f19ae7312c8a0d1ec057956b2a17'
check_digest 'accepted B3 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b3.sh" 'b0152d58b9591387cc02b188258d226827a22428e094d2afb7056b86efc0c7d7'
check_digest 'accepted B4 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-b4.sh" '401b6f487a45ecceee4c31d9c87542e01a5b7ba20d88a737425343c3ba380e67'
check_digest 'accepted C1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-c1.sh" 'b41ed416450130c5f5da23e8a4b413937c6cd3470aaf4e02b890d3319c940e16'
check_digest 'historical C2A V1 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-c2a.sh" 'b9d82c6862c9df3cc9ad9a4755c2a6a46bd83b1234b7d46d9f9e460e054b6a3e'
check_digest 'accepted C2A V2 oracle immutable' "$repo_root/.agents/scripts/verify-work-package-c2a-repair1.sh" '50d9dde4ce21c5d3c8f5abc23d0b6d7504da59e6dc457ba81a4fceac55549e7c'

check_digest 'scope lock: accepted schema v6' "$app_root/src/db/schema.rs" '542c97ba149ac5809444f0b3572e995e8f7ead6cdd338ed461d85437c0d946de'
check_digest 'scope lock: accepted outbox' "$app_root/src/db/sync_outbox.rs" '9174a0eacf972732fecb52f256890d02f798bc211f289f906c22c83de4be0574'
check_digest 'scope lock: accepted provider state' "$app_root/src/db/sync_provider_state.rs" '202035fb064777915f23319726eca774974bd2b66686b285a36d18b559785325'
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

def code_only(source):
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)

remote_entry = code_only(braced_body(adapter, r"\bstruct\s+RemoteEntry\s*\{"))
require(remote_entry, "C2B-01 RemoteEntry model missing")
for token in ("remote_position", "String", "remote_seq", "Option<u64>", "operation_id", "entry_kind"):
    require(token in remote_entry, f"C2B-01 RemoteEntry lacks {token!r}")
require(re.search(r"\bseq\s*:\s*u64", remote_entry) is None,
        "C2B-01 RemoteEntry retains ambiguous mandatory numeric seq")

for source_name, source, seq_shape in (
    ("server", server, "Some"),
    ("gdrive", gdrive, "None"),
):
    require("remote_position" in source, f"C2B-01 {source_name} adapter does not emit provider-native remote_position")
    require("remote_seq" in source and seq_shape in source,
            f"C2B-01 {source_name} adapter does not emit required remote_seq shape {seq_shape}")

convert = code_only(fn_body(coordinator, "remote_entry_to_inbox_entry"))
require(convert, "C2B-02 remote_entry_to_inbox_entry production converter missing")
for token in (
    "InboxEntryToStage", "remote_position", "remote_seq", "operation_id", "doc_hash",
    "entry_kind", "encrypted_payload", "payload_hash", "source_device",
):
    require(token in convert, f"C2B-02 remote converter lacks {token!r}")

failure_kind = code_only(braced_body(coordinator, r"\benum\s+InboxApplyFailureKind\s*\{"))
require(failure_kind, "C2B-03 typed InboxApplyFailureKind missing")
for token in ("Corrupt", "Retryable", "PendingAsset", "UnsupportedDelete"):
    require(token in failure_kind, f"C2B-03 failure kind lacks {token!r}")

verified_own = code_only(fn_body(coordinator, "is_verified_own_operation"))
require(verified_own, "C2B-04 is_verified_own_operation seam missing")
for token in ("source_device", "device_id", "get_outbox_by_id", "vault_id", "provider_id", "operation_id"):
    require(token in verified_own, f"C2B-04 own-operation verification lacks {token!r}")

validate_remote = code_only(fn_body(coordinator, "validate_and_parse_remote_entry"))
require(validate_remote, "C2B-04 remote validation seam missing")
require("return Ok(None)" not in validate_remote,
        "C2B-04 validation still short-circuits an entry as own before durable verification")

process = code_only(fn_body(coordinator, "process_staged_inbox_page"))
require(process, "C2B-03 process_staged_inbox_page production seam missing")
for token in (
    "get_inbox_page_entries", "transition_inbox_state", "InboxState::Pending",
    "InboxState::Applying", "InboxState::Failed", "InboxState::PendingAsset",
    "InboxState::Quarantined", "InboxState::Applied", "InboxState::IgnoredOwnOperation",
    "mark_inbox_page_applied_if_safe", "commit_applied_inbox_page_cursor",
    "is_verified_own_operation",
):
    require(token in process, f"C2B-03 staged-page processor lacks {token!r}")

resume = code_only(fn_body(coordinator, "resume_durable_inbox_before_pull"))
require(resume, "C2B-03 resume_durable_inbox_before_pull seam missing")
for token in ("get_inbox_page", "InboxPageState::Staged", "InboxPageState::Applied", "process_staged_inbox_page", "commit_applied_inbox_page_cursor"):
    require(token in resume, f"C2B-03 resume seam lacks {token!r}")

ack_gap = code_only(fn_body(coordinator, "retry_remote_ack_gap"))
require(ack_gap, "C2B-05 retry_remote_ack_gap seam missing")
for token in ("ack_cursor", "adapter.ack", "mark_sync_provider_cursor_acked_cas", "UnsupportedCapability"):
    require(token in ack_gap, f"C2B-05 ACK-gap seam lacks {token!r}")

pull = code_only(fn_body(coordinator, "pull_pages_durable"))
require(pull, "C2B-02 pull_pages_durable missing")
for token in (
    "retry_remote_ack_gap", "resume_durable_inbox_before_pull", "remote_entry_to_inbox_entry",
    "stage_inbox_page", "process_staged_inbox_page", "adapter.pull_page",
):
    require(token in pull, f"C2B-02 durable pull lacks {token!r}")
require("page_processor(page.entries)" not in pull,
        "C2B-02 durable pull still applies transient page entries before staging")

sync_body = code_only(fn_body(coordinator, "sync"))
require("pull_pages_durable" in sync_body, "C2B-02 active SyncCoordinator::sync does not use durable pull runtime")
require("advance_sync_provider_cursor_cas" not in sync_body,
        "C2B-02 active sync still directly advances provider cursor outside page-ledger commit")

snapshot = code_only(fn_body(all_sources, "snapshot_c2b_runtime_raw"))
require(snapshot, "C2B-06 complete raw runtime snapshot helper missing")
for token in (
    "rusqlite::types::Value", "sync_provider_state", "sync_inbox", "sync_inbox_pages",
    "sync_inbox_page_entries", "sync_outbox", "ORDER BY", "ack_cursor", "remote_position",
    "remote_seq", "operation_id", "state", "last_error",
):
    require(token in snapshot, f"C2B-06 runtime snapshot lacks {token!r}")

required_tests = {
    "c2b_server_and_gdrive_positions_are_provider_native": (
        "remote_position", "remote_seq", "server", "gdrive", "assert_eq!",
    ),
    "c2b_page_is_staged_before_apply_and_local_commit_before_ack": (
        "pull_pages_durable", "stage", "apply", "commit", "ack", "snapshot_c2b_runtime_raw", "assert_eq!",
    ),
    "c2b_restart_resumes_staged_page_before_new_pull": (
        "stage_inbox_page", "resume_durable_inbox_before_pull", "pull", "snapshot_c2b_runtime_raw", "assert_eq!",
    ),
    "c2b_applying_crash_state_reapplies_without_duplicate_terminal_transition": (
        "InboxState::Applying", "process_staged_inbox_page", "Applied", "snapshot_c2b_runtime_raw", "assert_eq!",
    ),
    "c2b_corrupt_middle_entry_blocks_cursor_ack_and_later_page": (
        "Corrupt", "Quarantined", "ack", "pull", "snapshot_c2b_runtime_raw", "assert_eq!",
    ),
    "c2b_verified_own_operation_requires_device_or_scoped_outbox_evidence": (
        "is_verified_own_operation", "IgnoredOwnOperation", "sync_outbox", "v1", "v2", "assert_eq!",
    ),
    "c2b_unverified_source_is_validated_and_applied": (
        "is_verified_own_operation", "validate_and_parse_remote_entry", "Applied", "assert_eq!",
    ),
    "c2b_ack_failure_preserves_local_commit_and_restart_retries_gap_before_pull": (
        "retry_remote_ack_gap", "cursor_committed", "ack_cursor", "pull", "snapshot_c2b_runtime_raw", "assert_eq!",
    ),
    "c2b_two_updates_same_document_apply_in_page_order": (
        "page_ordinal", "operation_id", "Applied", "assert_eq!",
    ),
    "c2b_asset_and_delete_block_page_in_durable_typed_states": (
        "PendingAsset", "UnsupportedDelete", "Failed", "snapshot_c2b_runtime_raw", "assert_eq!",
    ),
    "c2b_empty_advancing_page_commits_and_acks": (
        "stage_inbox_page", "entry_count", "cursor_committed", "ack_cursor", "assert_eq!",
    ),
}

test_bodies = []
for name, tokens in required_tests.items():
    body = code_only(fn_body(all_sources, name))
    require(body, f"C2B-06 evidence missing regression {name}")
    if body:
        test_bodies.append(body)
        for token in tokens:
            require(token in body, f"C2B-06 {name} lacks executable evidence {token!r}")
        require("assert!(true)" not in body and "assert_eq!(1, 1)" not in body,
                f"C2B-06 {name} contains tautological evidence")

raw_tests = "\n".join(test_bodies)
require("oracle token" not in raw_tests.lower() and "references for oracle" not in raw_tests.lower(),
        "C2B-06 tests contain explicit oracle-token filler")
require(re.search(r"let\s+_c2b", raw_tests) is None,
        "C2B-06 tests contain unused C2B token variables")

if errors:
    for error in errors:
        print(f"FAIL  {error}")
    print(f"STATIC MANIFEST FAILED: {len(errors)} condition(s) missing")
    raise SystemExit(1)

print("PASS  C2B production and executable-evidence manifest")
PY
then
  pass 'C2B static manifest'
else
  fail 'C2B static manifest'
fi

if (( failure_count > 0 )); then
  printf 'C2B RED/STATIC FAILURE: %d gate(s) failed; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'accepted C1 outbox regressions' "$app_root" cargo test db::sync_outbox::tests
run_gate 'accepted C2A inbox regressions' "$app_root" cargo test db::sync_inbox::tests
run_gate 'adapter regressions' "$app_root" cargo test sync::adapter::
run_gate 'C2B focused regressions' "$app_root" cargo test c2b_
run_gate 'coordinator regressions' "$app_root" cargo test sync::coordinator::tests
run_gate 'full app suite excluding documented unrelated search failure' "$app_root" cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'C2B-DURABLE-INBOX-RUNTIME-CUTOVER-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'C2B-DURABLE-INBOX-RUNTIME-CUTOVER-V1 / C2B-ORACLE-V1 PASSED. Await external audit; do not self-accept Work package C.\n'
