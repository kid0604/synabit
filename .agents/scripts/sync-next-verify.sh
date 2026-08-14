#!/usr/bin/env bash

set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
plan_file="$repo_root/docs/sync_implementation_plan.md"

sha256_file() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

bash "$script_dir/sync-next-preflight.sh"

oracle_rel="$(awk -F'`' '/^- External oracle path: `/{print $2; exit}' "$plan_file")"
expected_digest="$(awk -F'`' '/^- External oracle SHA-256: `/{print $2; exit}' "$plan_file")"
oracle_path="$repo_root/$oracle_rel"

if [[ -z "$oracle_rel" || -z "$expected_digest" || ! -f "$oracle_path" ]]; then
  printf 'SYNC_NEXT_VERIFICATION_FAILED: oracle metadata is incomplete\n'
  exit 12
fi

actual_before="$(sha256_file "$oracle_path")"
if [[ "$actual_before" != "$expected_digest" ]]; then
  printf 'BLOCKED_EXTERNAL_ORACLE_MUTATED expected=%s actual=%s\n' "$expected_digest" "$actual_before"
  exit 13
fi

set +e
bash "$oracle_path"
oracle_exit=$?
set -e

actual_after="$(sha256_file "$oracle_path")"
if [[ "$actual_after" != "$expected_digest" ]]; then
  printf 'BLOCKED_EXTERNAL_ORACLE_MUTATED expected=%s actual=%s\n' "$expected_digest" "$actual_after"
  exit 13
fi

if [[ "$oracle_exit" -ne 0 ]]; then
  printf 'SYNC_NEXT_VERIFICATION_FAILED oracle_exit=%s\n' "$oracle_exit"
  exit "$oracle_exit"
fi

printf 'SYNC_NEXT_VERIFICATION_PASS oracle_exit=0 digest=%s\n' "$actual_after"
