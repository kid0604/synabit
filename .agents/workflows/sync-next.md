---
description: Execute the one externally published Synabit sync contract through Builder, self-review and immutable verification
---

# /sync-next [milestone|expected-batch-id]

This command is a thin state-machine driver. It does not plan scope, invent
criteria, accept packages or select future roadmap work.

## Stage 0 — Mandatory machine preflight

The first action must be:

bash .agents/scripts/sync-next-preflight.sh

Do not inspect implementation history, create a task plan or edit source before
this command completes.

Handle its exit code exactly:

- 0: continue with the batch and criterion IDs printed by preflight.
- 10: stop immediately with SYNC_NEXT_BLOCKED and the preflight reason.
- 11: stop immediately with SYNC_NEXT_AWAITING_EXTERNAL_AUDIT.
- 12: stop with SYNC_NEXT_INVALID_CHECKPOINT.
- 13: stop with BLOCKED_EXTERNAL_ORACLE_MUTATED.
- 14: stop with BLOCKED_WORKFLOW_CONTROL_MUTATED.
- any other nonzero: stop with SYNC_NEXT_PREFLIGHT_FAILED and actual output.

A user argument may only assert the expected batch ID. If it differs from the
preflight batch, stop; it cannot narrow, broaden or override the checkpoint.
The milestone argument means execute the one published contract and stop at its
external-audit boundary. It never crosses into another contract.

## Stage 1 — Builder

Act as @sync-builder and execute
.agents/skills/implement-sync-batch.md.

Rules:

- If preflight reports ready_for_builder, call:
  bash .agents/scripts/sync-next-checkpoint.sh begin
- If it reports building or qa_failed, resume the same batch and preserve the
  repair counter.
- Keep a private in-memory checklist only; never create or pause at a plan
  artifact.
- Modify only contract-allowed production/tests.
- Do not modify workflow controls, the plan's external-owned fields or any
  oracle.
- Do not stop after Builder handoff.

## Stage 2 — Same-context self-review

Act as @sync-qa and execute
.agents/skills/audit-sync-batch.md.

This is explicitly not an independent reviewer. Do not call it fresh,
independent or external QA.

Before deciding PASS:

1. enumerate the exact active criterion IDs reported by preflight;
2. produce one complete matrix row per ID;
3. identify a production counterfactual for every row;
4. inspect test bodies and fixtures rather than names/counts;
5. fail proxy, token, count-only, copied-expected or non-reaching evidence even
   when every command is green.

On FAIL:

1. run the qa-fail checkpoint command from the audit skill;
2. return all findings to Builder together;
3. repair all findings in the same loop;
4. repeat the complete matrix.

At most two repair loops are permitted. If QA fails after both repairs, run the
block command and stop. Do not weaken criteria, tests or oracle to fit the
budget.

## Stage 3 — Immutable verification and checkpoint transition

Only after CRITERION_MATRIX is entirely PASS, act as @sync-verifier and run:

bash .agents/scripts/sync-next-checkpoint.sh complete .agents/runtime/sync-next-evidence.tsv

The complete command itself:

- reruns machine preflight;
- verifies the oracle digest;
- runs the exact immutable oracle;
- requires exit 0;
- verifies the digest again;
- validates an exact one-row-per-criterion behavioral evidence TSV;
- updates only internal checkpoint fields;
- appends one bounded internal log entry.

Do not run a hand-written substitute gate. Do not manually edit checkpoint
fields around a failed command.

Oracle exit 0 is a necessary mechanical gate. It does not grant semantic
external acceptance.

The selected oracle must be lifecycle-independent. It may inspect source,
immutable resources and runtime behavior, but must not read checkpoint state,
the Builder evidence TSV or implementation-log prose. `sync-next-verify.sh`
wraps that pure oracle with preflight and digest checks during Builder
execution; Codex must be able to invoke the oracle directly after the
checkpoint reaches `awaiting_external_audit` and receive the same behavioral
result.

## Terminal output

Return to the user only in one of these states:

- SYNC_NEXT_BLOCKED
- SYNC_NEXT_AWAITING_EXTERNAL_AUDIT
- SYNC_NEXT_INVALID_CHECKPOINT
- BLOCKED_EXTERNAL_ORACLE_MUTATED
- BLOCKED_WORKFLOW_CONTROL_MUTATED
- BLOCKED_ORACLE_CHANGE_REQUIRED
- BLOCKED_AFTER_2_REPAIRS
- INTERNAL_QA_PASSED_AWAITING_EXTERNAL_AUDIT

For a successful internal run, the final response contains only:

- batch and contract ID;
- changed files;
- criterion matrix result;
- exact oracle command/result;
- internal repair count;
- INTERNAL_QA_PASSED_AWAITING_EXTERNAL_AUDIT;
- explicit request for Codex external audit.

Never say the work package or milestone is complete. Never start a later
roadmap package in the same invocation.
