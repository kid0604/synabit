#!/usr/bin/env python3

"""Negative/structural gates for C2B-ORACLE-V3.

This file deliberately does not prove behavior from source-token presence.
Behavior is owned by the compiled Rust acceptance module. These checks only
ban known shortcuts or validate an exact test-only record shape.
"""

from __future__ import annotations

import pathlib
import re
import sys


def braced_body(source: str, pattern: str) -> str:
    match = re.search(pattern, source, re.S)
    if match is None:
        return ""
    start = source.find("{", match.start())
    if start < 0:
        return ""
    depth = 0
    for index in range(start, len(source)):
        char = source[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    return ""


def code_without_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)


def fail_if(condition: bool, message: str, failures: list[str]) -> None:
    if condition:
        failures.append(message)


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: c2b_oracle_v3_structural.py MODE COORDINATOR_RS")
        return 2

    mode = sys.argv[1]
    coordinator_path = pathlib.Path(sys.argv[2])
    coordinator = coordinator_path.read_text(encoding="utf-8")
    production = coordinator.split("#[cfg(test)]\n#[derive", 1)[0]
    failures: list[str] = []

    if mode == "dispatcher":
        fail_if(
            re.search(r"\b(?:pub\s+)?fn\s+outbox_record_to_sync_operation\b", production)
            is not None,
            "coordinator still owns a duplicate outbox converter",
            failures,
        )
        dispatch = braced_body(
            production, r"\bpub\s+async\s+fn\s+dispatch_durable_outbox_at\b"
        )
        fail_if(not dispatch, "durable dispatcher seam is missing", failures)
        fail_if(
            "unwrap_or_default" in dispatch,
            "dispatcher still fabricates missing durable values",
            failures,
        )
        fail_if(
            dispatch.count("schedule_outbox_retry") > 1,
            "dispatcher contains repeated single-row retry scheduling",
            failures,
        )
        sync_body = braced_body(production, r"\bpub\s+async\s+fn\s+sync\b")
        fail_if(
            re.search(r"\btx_bytes\s*:\s*0\b", sync_body) is not None,
            "active sync still initializes its final tx_bytes as a fabricated zero",
            failures,
        )

    elif mode == "typed":
        validate = braced_body(
            production, r"\bpub\s+fn\s+validate_and_parse_remote_entry\b"
        )
        process = braced_body(
            production, r"\bpub\s+fn\s+process_staged_inbox_page\b"
        )
        fail_if(not validate or not process, "typed inbox production seams are missing", failures)
        fail_if(
            "unwrap_or_default" in validate,
            "typed payload validation still fabricates legacy bytes",
            failures,
        )
        fail_if(
            "unwrap_or_default" in process,
            "inbox processing still aliases missing payload/hash to defaults",
            failures,
        )

    elif mode == "proxy":
        banned_patterns = {
            "count-only apply proof": r"assert_eq!\(\s*applier\.apply_calls",
            "provider-position constructor proxy": r"let\s+entry\s*=\s*RemoteEntry\s*\{",
            "pull-page marker": r"let\s+pull_page\s*=\s*\"pull_page\"",
            "quarantine marker": r"let\s+q_token\s*=",
            "vault/provider string markers": r"let\s+[vp]_str\s*=",
            "missing/unknown string markers": r"let\s+(?:missing|unknown)_token\s*=",
            "retry trigger string markers": r"let\s+(?:trigger_sql|net_err|injected)\s*=",
        }
        legacy_tests = coordinator.split("#[cfg(test)]\nmod tests", 1)
        test_source = legacy_tests[1] if len(legacy_tests) == 2 else coordinator
        for label, pattern in banned_patterns.items():
            fail_if(
                re.search(pattern, test_source) is not None,
                f"legacy Builder-writable tests retain {label}",
                failures,
            )

    elif mode == "snapshot":
        inbox_struct = braced_body(coordinator, r"\bpub\s+struct\s+InboxRow\b")
        if not inbox_struct:
            failures.append("test-only InboxRow is missing")
        else:
            fields = re.findall(r"^\s*pub\s+([a-z_][a-z0-9_]*)\s*:", inbox_struct, re.M)
            expected = [
                "vault_id",
                "provider_id",
                "page_cursor",
                "remote_position",
                "remote_seq",
                "operation_id",
                "doc_hash",
                "entry_kind",
                "encrypted_payload",
                "payload_hash",
                "source_device",
                "state",
                "last_error",
                "received_at",
                "updated_at",
                "applied_at",
            ]
            fail_if(
                fields != expected,
                f"InboxRow fields/order differ: expected={expected} actual={fields}",
                failures,
            )
        snapshot = braced_body(
            coordinator, r"\bpub\s+fn\s+snapshot_c2b_runtime_raw\b"
        )
        fail_if(not snapshot, "raw C2B snapshot seam is missing", failures)
        for alias_pattern, label in (
            (r"if\s+op_blob\.len\(\)\s*==", "operation ID zero alias"),
            (r"if\s+doc_blob\.len\(\)\s*==", "document hash zero alias"),
            (r"payload_hash_blob\.and_then", "payload hash None alias"),
        ):
            fail_if(
                re.search(alias_pattern, snapshot) is not None,
                f"raw snapshot retains {label}",
                failures,
            )

    elif mode == "hygiene":
        banned_patterns = {
            "commented fake interface": r"//\s*trait\s+InboxEntryApplier",
            "applier token marker": r"\b_applier_token\b",
            "transition token marker": r"\btransition_marker\b",
            "snapshot position token marker": r"\btoken_pos\b",
            "snapshot sequence token marker": r"\btoken_seq\b",
        }
        for label, pattern in banned_patterns.items():
            fail_if(
                re.search(pattern, coordinator) is not None,
                f"source retains {label}",
                failures,
            )

    else:
        print(f"unknown structural mode: {mode}")
        return 2

    if failures:
        for failure in failures:
            print(f"STRUCTURAL_FAIL[{mode}] {failure}")
        return 1

    print(f"STRUCTURAL_PASS[{mode}]")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
