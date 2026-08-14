#!/usr/bin/env bash

# Immutable external acceptance oracle for Work package B, final slice B4.
# Antigravity may read and execute this file but must never modify it.

set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
app_root="$repo_root/src-tauri"
checkpoint_file="$repo_root/docs/sync_implementation_plan.md"
oracle_file="$repo_root/.agents/scripts/verify-work-package-b4.sh"

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

printf 'Synabit Work package B final slice: B4-CLOSURE-V1 / B4-ORACLE-V4\n'

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

if python3 - \
  "$app_root/src/sync/coordinator.rs" \
  "$app_root/src/sync/adapter/mod.rs" \
  "$app_root/src/sync/adapter/server.rs" \
  "$app_root/src/sync/adapter/gdrive.rs" \
  "$app_root/src/db/sync_provider_state.rs" \
  "$app_root/src/db/crdt.rs" <<'PY'
import pathlib
import re
import sys

coordinator_path, adapter_path, server_path, gdrive_path, provider_path, crdt_path = map(pathlib.Path, sys.argv[1:])
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

coordinator = read(coordinator_path)
adapter = read(adapter_path)
server = read(server_path)
gdrive = read(gdrive_path)
provider = read(provider_path)
crdt = read(crdt_path)

sync_body = fn_body(coordinator, "sync") or ""
require("sync_cursor_" not in sync_body, "B4-01 coordinator still constructs a legacy global KV cursor key")
require("get_kv(" not in sync_body and "set_kv(" not in sync_body, "B4-01 coordinator still reads/writes cursor through KV")
for symbol in (
    "ensure_sync_provider_state",
    "get_sync_provider_state",
    "advance_sync_provider_cursor_cas",
    "mark_sync_provider_cursor_acked_cas",
):
    require(symbol in coordinator, f"B4-01 active coordinator does not use {symbol}")

require(
    "sync_cursor_" not in "\n".join((coordinator, adapter, server, gdrive)),
    "B4-01 active runtime still contains legacy sync_cursor_<provider> persistence",
)
require(
    "pub fn ensure_sync_provider_state" in provider,
    "B4-01 missing provider-state ensure DAO",
)
require(
    "pub fn reconcile_sync_provider_plan" in provider,
    "B4-03 missing atomic provider plan/identity reconciliation DAO",
)

reconcile_body = fn_body(provider, "reconcile_sync_provider_plan") or ""
require(
    "unchecked_transaction" in reconcile_body or "transaction(" in reconcile_body,
    "B4-R1 reconcile still performs its read/write decision without a SQLite transaction",
)
require("tx.commit" in reconcile_body, "B4-R1 reconcile transaction is not explicitly committed")
require(
    "tx.prepare" in reconcile_body or "tx.query_row" in reconcile_body,
    "B4-R1 reconcile read is not performed through the same transaction as its write",
)

preflight_body = fn_body(coordinator, "preflight_provider_state") or ""
require(preflight_body, "B4-R2 missing production coordinator preflight seam")
for symbol in (
    "ensure_sync_provider_state",
    "get_sync_provider_state",
    "get_sync_plan",
    "reconcile_sync_provider_plan",
):
    require(symbol in preflight_body, f"B4-R2 production preflight does not use {symbol}")
require(
    "preflight_provider_state" in sync_body,
    "B4-R2 SyncCoordinator::sync does not call the production preflight seam",
)
preflight_pos = sync_body.find("preflight_provider_state")
detect_pos = sync_body.find("detect_local_changes")
push_pos = sync_body.find(".push(")
require(
    preflight_pos >= 0 and detect_pos >= 0 and preflight_pos < detect_pos,
    "B4-R2 provider preflight is not before local change detection",
)
require(
    preflight_pos >= 0 and push_pos >= 0 and preflight_pos < push_pos,
    "B4-R2 provider preflight is not before adapter push",
)

plan_match = re.search(r"pub\s+struct\s+AdapterSyncPlan\s*\{(.*?)\n\}", adapter, re.S)
plan_body = plan_match.group(1) if plan_match else ""
require(plan_match is not None, "B4-02 AdapterSyncPlan missing")
require("incarnation_id" in plan_body, "B4-02 AdapterSyncPlan drops provider incarnation")
require("remote_vault_id" in plan_body, "B4-02 AdapterSyncPlan drops remote vault identity")

trait_plan = re.search(r"async\s+fn\s+get_sync_plan\s*\((.*?)\)\s*->", adapter, re.S)
trait_args = trait_plan.group(1) if trait_plan else ""
require("client_incarnation_id" in trait_args, "B4-02 SyncAdapter::get_sync_plan does not accept stored client incarnation")

build_plan_body = fn_body(server, "build_get_sync_plan_request") or ""
require("client_incarnation_id" in build_plan_body, "B4-02 server plan request does not carry stored incarnation")
require("client_incarnation_id: None" not in build_plan_body, "B4-02 server plan request still hardcodes missing incarnation")

map_plan_body = fn_body(server, "map_sync_plan_response") or ""
require("incarnation_id" in map_plan_body, "B4-02 server response mapping discards incarnation")
server_get_plan = fn_body(server, "get_sync_plan") or ""
gdrive_get_plan = fn_body(gdrive, "get_sync_plan") or ""
require("incarnation_id" in gdrive_get_plan and "remote_vault_id" in gdrive_get_plan, "B4-02 GDrive plan does not explicitly report absent provider metadata")

ensure_body = fn_body(provider, "ensure_sync_provider_state") or ""
require("ON CONFLICT" in ensure_body and "DO NOTHING" in ensure_body, "B4-01 provider ensure can overwrite existing bootstrap/runtime fields")
require("'ready'" in ensure_body, "B4-01 new provider-state row is not initialized as ready")
require(
    "bootstrap_required" not in (fn_body(crdt, "ensure_provider_state") or ""),
    "B4-01 baseline FK ensure still creates every new provider as bootstrap_required",
)

required_tests = {
    "provider_state_ensure_preserves_existing_bootstrap_and_all_fields": (
        "ensure_sync_provider_state", "bootstrap_required", "before", "after", "assert_eq!"
    ),
    "two_vault_runtime_cursor_and_ack_are_isolated": (
        "vault_a", "vault_b", "advance_sync_provider_cursor_cas", "mark_sync_provider_cursor_acked_cas", "before_b", "after_b", "assert_eq!"
    ),
    "cursor_cas_failure_prevents_ack_and_next_page_with_real_provider_state": (
        "advance_sync_provider_cursor_cas", "ack_calls", "pull_calls", "is_err", "assert_eq!"
    ),
    "server_sync_plan_roundtrips_incarnation_and_remote_vault_identity": (
        "client_incarnation_id", "server_incarnation_id", "vault_hash", "build_get_sync_plan_request", "finalize_server_sync_plan_response", "assert_eq!"
    ),
    "incarnation_or_remote_vault_mismatch_requires_bootstrap_without_cursor_advance": (
        "incarnation_db", "remote_db", "reconcile_sync_provider_plan", "bootstrap_required", "cursor", "ack_cursor", "incarnation_after", "remote_after", "assert_eq!"
    ),
    "bootstrap_required_provider_state_stops_before_local_push_or_pull": (
        "bootstrap_required", "preflight_provider_state", "get_sync_provider_state", "before", "after", "push_calls", "pull_calls", "assert_eq!"
    ),
    "reconcile_bootstrap_reason_and_idempotence_are_durable": (
        "reconcile_sync_provider_plan", "bootstrap_required", "last_error", "first", "second", "assert_eq!"
    ),
}

test_sources = "\n".join((coordinator, adapter, server, gdrive, provider, crdt))
for name, tokens in required_tests.items():
    body = fn_body(test_sources, name)
    require(body is not None, f"B4-05 missing production-dependent regression {name}")
    if body is not None:
        for token in tokens:
            require(token in body, f"B4-05 {name} lacks evidence token {token!r}")
        if name in (
            "provider_state_ensure_preserves_existing_bootstrap_and_all_fields",
            "incarnation_or_remote_vault_mismatch_requires_bootstrap_without_cursor_advance",
        ):
            require(body.count("assert_eq!") >= 8, f"B4-05 {name} does not compare complete provider state")

ensure_test = fn_body(test_sources, "provider_state_ensure_preserves_existing_bootstrap_and_all_fields") or ""
require(
    ensure_test.count("ensure_sync_provider_state") >= 2,
    "B4-R3 provider ensure preservation test does not call the real ensure twice",
)
require(
    re.search(r"assert_eq!\s*\(\s*after\s*,\s*before\s*\)", ensure_test) is not None,
    "B4-R3 provider ensure preservation test does not exact-compare the complete ten-field record",
)

isolation_test = fn_body(test_sources, "two_vault_runtime_cursor_and_ack_are_isolated") or ""
for token in ("before_a", "after_a", "expected_a", "before_b", "after_b"):
    require(token in isolation_test, f"B4-R3 two-vault test lacks complete-state fixture {token}")
require(
    re.search(r"assert_eq!\s*\(\s*after_a\s*,\s*expected_a\s*\)", isolation_test) is not None,
    "B4-R3 two-vault test does not exact-compare vault A with its complete expected record",
)
require(
    re.search(r"assert_eq!\s*\(\s*after_b\s*,\s*before_b\s*\)", isolation_test) is not None,
    "B4-R3 two-vault test does not exact-compare vault B before/after",
)

cas_test = fn_body(test_sources, "cursor_cas_failure_prevents_ack_and_next_page_with_real_provider_state") or ""
for token in ("competing_state", "after"):
    require(token in cas_test, f"B4-R3 CAS failure test lacks final durable-state evidence {token}")
require(
    re.search(r"assert_eq!\s*\(\s*after\s*,\s*competing_state\s*\)", cas_test) is not None,
    "B4-R3 CAS failure test does not exact-compare the surviving competing-writer record",
)

server_test = fn_body(server, "server_sync_plan_roundtrips_incarnation_and_remote_vault_identity") or ""
require(server_test, "B4-R3 named server identity test is not located beside the production server adapter seam")
finalize_plan_body = fn_body(server, "finalize_server_sync_plan_response") or ""
require(finalize_plan_body, "B4-R3 missing testable production server plan finalizer")
require(
    "map_sync_plan_response" in finalize_plan_body and "remote_vault_id" in finalize_plan_body and "vault_hash" in finalize_plan_body,
    "B4-R3 server plan finalizer does not map the wire response and attach the exact vault hash",
)
require(
    "finalize_server_sync_plan_response" in server_get_plan,
    "B4-R3 production server get_sync_plan bypasses the tested plan finalizer",
)
require(
    "remote_vault_id" not in server_get_plan,
    "B4-FR3 production server get_sync_plan duplicates remote-vault mapping after the tested finalizer",
)

bootstrap_test = fn_body(coordinator, "bootstrap_required_provider_state_stops_before_local_push_or_pull") or ""
require(bootstrap_test, "B4-R3 bootstrap stop test is not located on the production coordinator preflight seam")
require(
    "pull_pages_durable" not in bootstrap_test,
    "B4-R3 bootstrap stop test still substitutes the pull helper for coordinator preflight",
)
require(
    "durable bootstrap reason" in bootstrap_test,
    "B4-FR1 bootstrap preflight test does not seed an actionable durable reason",
)
require(
    re.search(r"assert_eq!\s*\(\s*after\s*,\s*before\s*\)", bootstrap_test) is not None,
    "B4-FR1 bootstrap preflight test does not exact-compare the complete provider record",
)

reconcile_test = fn_body(provider, "reconcile_bootstrap_reason_and_idempotence_are_durable") or ""
for token in ("before_first", "after_first", "expected_first"):
    require(token in reconcile_test, f"B4-FR2 reconcile test lacks first-metadata fixture {token}")
require(
    re.search(r"assert_eq!\s*\(\s*after_first\s*,\s*expected_first\s*\)", reconcile_test) is not None,
    "B4-FR2 reconcile test does not exact-compare the first metadata persistence result",
)
require(
    reconcile_test.count("reconcile_sync_provider_plan") >= 4,
    "B4-FR2 reconcile test does not exercise first metadata, identical repeat, preserved marker and plan-requested bootstrap",
)
first_read = reconcile_test.find("before_first")
first_call = reconcile_test.find("reconcile_sync_provider_plan", first_read + 1)
first_after = reconcile_test.find("after_first", first_call + 1)
second_call = reconcile_test.find("reconcile_sync_provider_plan", first_after + 1)
require(
    first_read >= 0 and first_call > first_read and first_after > first_call and second_call > first_after,
    "B4-FR2 first-metadata fixture does not read-before, reconcile, read-after, then repeat",
)

mismatch_test = fn_body(provider, "incarnation_or_remote_vault_mismatch_requires_bootstrap_without_cursor_advance") or ""
require(
    mismatch_test,
    "B4-FR3 mismatch regression is not colocated with and calling the real reconciliation DAO",
)
require(
    fn_body(coordinator, "incarnation_or_remote_vault_mismatch_requires_bootstrap_without_cursor_advance") is None,
    "B4-FR3 stale coordinator mismatch test still uses adapter metadata omission instead of distinct reconciliation identities",
)
for token in (
    "incarnation_db", "incarnation_before", "incarnation_after", "incarnation_expected",
    "remote_db", "remote_before", "remote_after", "remote_expected", "last_error",
):
    require(token in mismatch_test, f"B4-FR3 mismatch test lacks complete distinct-identity evidence {token}")
require(
    "preflight_provider_state" not in mismatch_test,
    "B4-FR3 mismatch test substitutes preflight metadata omission for direct distinct identity reconciliation",
)
require(
    mismatch_test.count("reconcile_sync_provider_plan") >= 4,
    "B4-FR3 mismatch test does not independently seed and mismatch both incarnation and remote-vault identity",
)
require(
    re.search(r"assert_eq!\s*\(\s*incarnation_after\s*,\s*incarnation_expected\s*\)", mismatch_test) is not None,
    "B4-FR3 incarnation mismatch fixture lacks complete expected-record equality",
)
require(
    re.search(r"assert_eq!\s*\(\s*remote_after\s*,\s*remote_expected\s*\)", mismatch_test) is not None,
    "B4-FR3 remote-vault mismatch fixture lacks complete expected-record equality",
)

if errors:
    for error in errors:
        print(f"FAIL  {error}")
    print(f"STATIC MANIFEST FAILED: {len(errors)} required condition(s) missing")
    raise SystemExit(1)

print("PASS  B4 scoped provider runtime and evidence-shape manifest")
PY
then
  pass 'B4 static manifest'
else
  fail 'B4 static manifest'
fi

if (( failure_count > 0 )); then
  printf 'B4-CLOSURE-V1 BASELINE/STATIC FAILURE: %d gate(s) failed; cargo gates were not run.\n' "$failure_count"
  exit 1
fi

run_gate 'app cargo fmt --check' "$app_root" cargo fmt --check
run_gate 'app cargo check' "$app_root" cargo check
run_gate 'accepted B1 identity regressions' "$app_root" cargo test sync::core::identity::tests
run_gate 'accepted B2 schema regressions' "$app_root" cargo test db::schema::tests
run_gate 'accepted B2 migration regressions' "$app_root" cargo test db::legacy_sync_migration::tests
run_gate 'accepted B3 CRDT/path regressions' "$app_root" cargo test db::crdt::tests
run_gate 'B4 provider-state DAO regressions' "$app_root" cargo test db::sync_provider_state::tests
run_gate 'B4 provider ensure preservation' "$app_root" cargo test provider_state_ensure_preserves_existing_bootstrap_and_all_fields
run_gate 'B4 two-vault runtime cursor isolation' "$app_root" cargo test two_vault_runtime_cursor_and_ack_are_isolated
run_gate 'B4 real CAS failure stops ACK/pull' "$app_root" cargo test cursor_cas_failure_prevents_ack_and_next_page_with_real_provider_state
run_gate 'B4 server plan identity roundtrip' "$app_root" cargo test server_sync_plan_roundtrips_incarnation_and_remote_vault_identity
run_gate 'B4 identity mismatch forces bootstrap safely' "$app_root" cargo test incarnation_or_remote_vault_mismatch_requires_bootstrap_without_cursor_advance
run_gate 'B4 bootstrap state stops push and pull' "$app_root" cargo test bootstrap_required_provider_state_stops_before_local_push_or_pull
run_gate 'B4 reconcile preserves bootstrap reason and idempotence' "$app_root" cargo test reconcile_bootstrap_reason_and_idempotence_are_durable
run_gate \
  'app all targets excluding documented unrelated search failure' \
  "$app_root" \
  cargo test --all-targets -- --skip search::tests::test_unknown_type_filter_ignored

if (( failure_count > 0 )); then
  printf 'B4-CLOSURE-V1 FAILED: %d gate(s) failed.\n' "$failure_count"
  exit 1
fi

printf 'B4-CLOSURE-V1 / B4-ORACLE-V4 PASSED. Work package B is eligible for external acceptance.\n'
