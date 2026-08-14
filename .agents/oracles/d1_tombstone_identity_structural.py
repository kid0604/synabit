#!/usr/bin/env python3

"""Negative/shape checks for D1 typed tombstone identity.

Behavior is proved by d1_tombstone_identity.rs. These checks only prevent the
unit-delete wire shape and the previously explicit UnsupportedDelete shortcut.
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


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: d1_tombstone_identity_structural.py PROTOCOL CHANGE COORDINATOR")
        return 2

    protocol = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
    change = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
    coordinator = pathlib.Path(sys.argv[3]).read_text(encoding="utf-8")
    failures: list[str] = []

    delete_struct = braced_body(protocol, r"\bpub\s+struct\s+DeletePayload\b")
    fields = re.findall(r"^\s*pub\s+([a-z_][a-z0-9_]*)\s*:", delete_struct, re.M)
    if fields != ["node_id", "rel_path"]:
        failures.append(
            "DeletePayload must contain exactly node_id, rel_path in that order "
            f"(actual={fields})"
        )

    payload_enum = braced_body(protocol, r"\bpub\s+enum\s+SyncPayload\b")
    if re.search(r"\bDelete\s*\(\s*DeletePayload\s*\)", payload_enum) is None:
        failures.append("SyncPayload::Delete is not a typed DeletePayload variant")
    if re.search(r"^\s*Delete\s*,", payload_enum, re.M) is not None:
        failures.append("unit SyncPayload::Delete variant is still present")

    prepare = braced_body(change, r"\bpub\s+fn\s+prepare_durable_outbox_operations\b")
    if "SyncPayload::Delete(" not in prepare:
        failures.append("durable preparation does not construct a typed tombstone")

    validate = braced_body(
        coordinator, r"\bpub\s+fn\s+validate_and_parse_remote_entry\b"
    )
    if "UnsupportedDelete" in validate:
        failures.append("typed validation still rejects every valid tombstone as unsupported")
    if "SyncPayload::Delete(" not in validate:
        failures.append("typed validation does not match the structured Delete variant")

    if failures:
        for failure in failures:
            print(f"D1_STRUCTURAL_FAIL {failure}")
        return 1
    print("D1_STRUCTURAL_PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
