# Synabit Sync Workspace Rules

## Authority

- docs/sync_implementation_plan.md section 0 is the only active execution input.
- Sections 1 through 8 provide invariants, accepted boundaries and roadmap
  context; they are not executable unless copied into the marked active
  contract by the external auditor.
- Review current local source. Ignore Git history and preserve unrelated edits.
- Do not create planning/report/checklist documents.
- Workflow controls under .agents/workflows, .agents/rules, .agents/skills and
  sync-next control scripts are external-owned. Antigravity may read and run
  them but may not edit, rename, delete, bypass or regenerate them.
- `.agents/runtime/sync-next-evidence.tsv` is the only Builder-writable workflow
  artifact. It is machine evidence, not a plan/report, and may contain only the
  current criterion matrix in the required TSV schema.
- The active oracle and its checkpoint metadata are external-owned and immutable.
- An external oracle is a pure source/behavior gate. It must not depend on the
  current execution status, the Builder evidence TSV or implementation-log
  text, so the external auditor can reproduce it after internal handoff.

## Exact state vocabulary

Execution status must be exactly one of:

- blocked
- ready_for_builder
- building
- qa_failed
- awaiting_external_audit

External audit status must be exactly one of:

- not_started
- repair_required
- awaiting_external_audit
- accepted
- testability_or_architecture_blocker

Closure contract status must be exactly one of:

- not_published
- frozen
- accepted
- retired

Do not add suffixes or batch IDs to state values. Put detail in the neighboring
result, batch and stop-boundary fields.

## Machine gates

- Every invocation begins with:
  bash .agents/scripts/sync-next-preflight.sh
- Exit 10 means STOP_BLOCKED. Exit 11 means
  STOP_AWAITING_EXTERNAL_AUDIT. Any other nonzero exit is a configuration or
  oracle-integrity blocker. Do not edit source after a nonzero preflight.
- Exit 13 means the external oracle changed. Exit 14 means a workflow control
  file changed without external publication. Both are terminal blockers.
- Source editing is authorized only when preflight exits 0.
- Internal checkpoint transitions must use:
  bash .agents/scripts/sync-next-checkpoint.sh MODE
- Final verification must use:
  bash .agents/scripts/sync-next-verify.sh
- Completion must supply the matrix artifact:
  bash .agents/scripts/sync-next-checkpoint.sh complete .agents/runtime/sync-next-evidence.tsv
- Never claim a command ran unless its current invocation completed.
- Never report PASS for a nonzero command.
- A final verifier PASS and a direct external-oracle PASS must be reproducible
  separately. The verifier owns preflight/digest/checkpoint mechanics; the
  oracle owns production behavior only.

## Scope and correctness

- Execute one frozen active contract only. The contract must be enclosed by
  ACTIVE_CONTRACT_BEGIN and ACTIVE_CONTRACT_END markers and contain one to eight
  criterion IDs.
- Do not read historical log entries to reconstruct required behavior.
- Do not broaden scope, start a roadmap package, or reopen an accepted boundary
  without an active external finding.
- Preserve vault/provider isolation, durable-before-ACK ordering, idempotency,
  typed operation semantics, safe paths and bounded resource limits.
- On durability/integrity paths, forbid fabricated defaults, ignored errors,
  success no-ops, unchecked numeric conversion and log-then-continue recovery.
- Use a production seam for clocks, adapters, filesystems, DB failures and apply
  effects. A test-only copy of a decision is not evidence.
- Test names, comments, source tokens, inventory counts and compilation are not
  semantic evidence.
- Zero-mutation claims compare complete deterministic durable state.
- Ordering claims record ordered effects or payloads, not only call counts.
- Restart claims create a second production invocation over durable state.
- Isolation claims use distinct colliding vault/provider fixtures.
- A positive token-presence check in an oracle cannot prove a semantic
  criterion. If the active oracle relies on one as its only evidence, stop with
  BLOCKED_ORACLE_CHANGE_REQUIRED.

## Review and repair

- Internal QA is same-context self-review, not an independent trust boundary.
- Audit exactly every active criterion once per pass and return all failures.
- Each row must contain criterion ID, production call, observed behavior,
  smallest counterfactual and PASS or FAIL.
- Missing or proxy evidence is FAIL even when tests and oracle are green.
- Evidence that names a nonexistent compiled test/filter, points at an import,
  attribute, blank/comment line or contradicts the observed durable type/state
  is FAIL even if its TSV shape passes.
- At most two repair loops are allowed after the initial Builder pass.
- If self-review still fails after two repairs, transition to blocked. Do not
  weaken tests, change the oracle, add token padding or self-declare success.

## State ownership and completion

Antigravity may modify only these checkpoint fields through the checkpoint
script:

- Execution status
- Last internally completed batch
- Internal QA result
- Internal repair loops
- Internal verification result

It may also replace `.agents/runtime/sync-next-evidence.tsv` for the current
batch. No other workflow/control artifact is writable.

Antigravity must not modify milestone, checkboxes, external audit fields,
accepted batch, findings, contract, oracle, next batch, stop boundary or reopen
policy.

A batch may stop only as:

- awaiting_external_audit after self-review PASS and verification PASS;
- blocked after preflight/configuration/oracle/repair-budget failure;
- unchanged awaiting_external_audit when external review is already pending.

External audit is always required before package acceptance or advancement.
