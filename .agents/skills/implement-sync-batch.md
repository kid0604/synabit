# Skill: Implement One Active Sync Contract

## Input boundary

Use only:

1. docs/sync_implementation_plan.md section 0;
2. content between ACTIVE_CONTRACT_BEGIN and ACTIVE_CONTRACT_END;
3. source/test files explicitly allowed by that contract;
4. the immutable oracle selected by the checkpoint;
5. direct callers/types required to understand the allowed production seam.

Do not use roadmap summaries or implementation-log narration as requirements.

## Procedure

### 1. Machine preflight

Run:

bash .agents/scripts/sync-next-preflight.sh

Do not edit anything unless it exits 0. Preserve its reported batch, contract,
criteria, oracle and execution mode.

If execution is ready_for_builder, transition with:

bash .agents/scripts/sync-next-checkpoint.sh begin

If execution is building or qa_failed, resume the same batch without resetting
the repair counter.

### 2. Build an in-memory criterion map

For every active criterion, record:

- production decision and caller;
- Given/When/Then behavior;
- durable state or ordered effect to observe;
- failure injection/counterfactual;
- test or immutable harness that exercises it.

Do not create a plan artifact. If a criterion lacks a testable production seam,
extract that seam inside the allowed scope before writing its test.

### 3. Implement production behavior

- Complete one production responsibility at a time.
- Reuse accepted DAO/protocol helpers; do not create a second authority with the
  same name or behavior.
- Convert and validate complete input before the first irreversible/network
  effect.
- Preserve original causal errors when durable recovery also fails.
- Use typed outcomes and checked conversions.
- Delete superseded paths after callers move.
- Never add a string/comment/variable merely to satisfy an oracle search.

### 4. Add diagnostic tests

Builder-owned tests may speed debugging but cannot replace immutable behavioral
acceptance tests.

When applicable:

- failure fixtures must reach the intended boundary;
- restart uses a new invocation over persisted state;
- call-order assertions store an ordered event log;
- payload-order assertions store the actual payload identity;
- zero-mutation assertions compare complete raw snapshots;
- protocol response fixtures cover unique, duplicate, missing, unknown,
  accepted and rejected outcomes independently.

### 5. Builder self-check

- Format changed code.
- Run focused compile/tests.
- Search the changed surface for forbidden fallbacks, duplicate authorities,
  token padding and silent errors.
- For every active criterion, confirm the evidence fails under its named
  counterfactual.
- Do not run the final checkpoint completion command yet.

### 6. Handoff to self-review

Provide an in-conversation BUILDER_HANDOFF containing:

- batch/contract/oracle IDs;
- changed files;
- one row per active criterion;
- focused commands and actual exit codes;
- unresolved concerns.

Immediately execute audit-sync-batch.md. Do not send a user-facing completion
message between phases.

## Builder completion

Builder completion means only that the source is ready for same-context
self-review. It is not QA PASS, external acceptance or package completion.
