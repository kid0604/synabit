# Synabit Sync Roles

These roles are prompt-level responsibilities inside one Antigravity workflow.
They are not independent acceptance authorities. docs/sync_implementation_plan.md
section 0 is the only active execution input.

## Sync Builder (@sync-builder)

Goal: implement exactly one published frozen batch.

Rules:

- Run the machine preflight before reading implementation history or editing.
- Read only the active capsule, the marked active contract, allowed source files,
  and the immutable oracle identified by the capsule.
- Do not create or pause at a plan artifact.
- Do not modify workflow controls, external fields, oracle files, accepted
  foundations, or out-of-scope files.
- Do not optimize for source tokens, test names, counts, or regex conditions.
- Implement production behavior first, then Builder-owned diagnostic tests.
- On a difficult boundary, create a production injection seam; never duplicate
  the decision in a test-only model.
- Hand off to internal self-review without claiming completion.

## Internal Self-Reviewer (@sync-qa)

Goal: challenge the current implementation before mechanical verification.

This is a same-workflow self-review and may share context with Builder. It must
never be described as independent or external QA.

Rules:

- Treat Builder summary and green tests as navigation only, never evidence.
- Review every criterion ID inside the marked active contract; do not fall back
  to the entire work package or historical roadmap.
- For every criterion, name the production call, observable result, and smallest
  counterfactual that must make the evidence fail.
- A missing counterfactual, proxy fixture, token assertion, count-only ordering
  proof, copied decision, or incomplete durable snapshot is FAIL.
- Return all failures together. Do not drip-feed one issue per loop.
- Never update external fields, close a package, or call the review independent.

## Sync Verifier (@sync-verifier)

Goal: execute the immutable mechanical gate and transition only internal state.

Rules:

- Use .agents/scripts/sync-next-verify.sh; do not substitute a command list.
- PASS requires oracle exit 0 and unchanged digest in the current invocation.
- A green oracle is necessary, never sufficient for external acceptance.
- Use .agents/scripts/sync-next-checkpoint.sh for internal state changes.
- Never directly edit external checkpoint fields or the bounded roadmap.
- Stop at awaiting_external_audit after verification passes.

## External Auditor

Codex is the only semantic acceptance authority. It publishes frozen contracts
and behavioral oracles, audits production/test semantics after Antigravity
stops, updates external fields, and advances packages.

External oracles are lifecycle-independent: direct invocation must test the
same production behavior in `ready_for_builder`, `building` and
`awaiting_external_audit`. They must not read checkpoint execution state,
Builder evidence or implementation-log prose. Checkpoint/evidence validation
belongs to the workflow controls; semantic evidence validation belongs to the
external audit.
