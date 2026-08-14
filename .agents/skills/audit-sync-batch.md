# Skill: Same-Context Audit of One Active Sync Contract

## Trust model

This review runs inside the Antigravity workflow and may share context with the
Builder. It is a defensive self-review, not independent QA and never external
acceptance.

Do not use Builder summary, implementation log, test names, comments, source
tokens, test counts or a green oracle as evidence.

## Audit manifest

Read only:

1. section 0 of docs/sync_implementation_plan.md;
2. the marked active contract;
3. current allowed production/test files;
4. direct production callers/types;
5. immutable oracle behavior.

Enumerate every criterion ID inside the active markers before evaluating any
criterion. Missing markers, duplicate IDs, zero criteria or more than eight
criteria is a configuration FAIL.

## Required audit per criterion

For each criterion:

1. Trace the production path from input to durable/observable outcome.
2. Identify ordering, transaction, retry, restart, corruption, isolation and
   bounds risks relevant to that path.
3. Name the smallest production counterfactual that should make evidence fail.
4. Confirm a behavioral test actually reaches that decision and observes the
   result.
5. Classify PASS only if both implementation and counterfactual-dependent
   evidence are complete.

Automatic FAIL conditions:

- proxy or empty fixture;
- comment/token/constant assertion;
- duplicated production decision in test code;
- only a call count for an ordering/payload claim;
- restart claim without a second invocation;
- zero-mutation claim without complete deterministic raw state;
- failure injection that does not reach the claimed statement;
- expected values copied from actual output;
- missing malformed, rejected, duplicate or boundary case required by the
  criterion;
- positive source-token presence used as semantic proof.

## Whole-contract review

After per-criterion review:

- inspect changed source for duplicate authorities and forbidden fallbacks;
- verify all active caller paths use the intended production seam;
- ensure no allowed-file edit regressed an accepted boundary named in section 4;
- run focused behavioral tests needed to falsify uncertain claims;
- return all findings together.

Do not inspect or fail future roadmap packages.

## Result format

Return exactly:

QA_RESULT: PASS or QA_RESULT: FAIL

CRITERION_MATRIX:
- CRITERION_ID | PASS/FAIL | production: file:line/function | observation:
  durable state/effect | counterfactual: exact regression | evidence:
  behavioral test/command | gap: none or concrete gap

Include exactly one row per active criterion.

Before returning PASS, write the same matrix to
`.agents/runtime/sync-next-evidence.tsv` with exactly this tab-separated header:

criterion_id<TAB>status<TAB>production<TAB>observation<TAB>evidence<TAB>counterfactual

Rules for this machine artifact:

- one data row per active criterion, in active-contract order;
- status is exactly PASS;
- production contains a real source path and line;
- observation names the durable state or ordered effect actually asserted;
- evidence names the behavioral test/harness and current command result;
- any named test exists in the current compiled `cargo test -- --list` output,
  or the evidence names the exact oracle gate/filter and exit code instead;
- counterfactual names the concrete production regression that makes the
  evidence fail;
- production `path:line` resolves to the decision body being claimed, never an
  import, module declaration, attribute, blank line or comment;
- no multiline cells and no `none`, `n/a`, `token`, `test name` or `count only`.

Do not write the artifact on FAIL. The completion gate validates its schema,
criterion set and non-placeholder fields before changing checkpoint state.

When FAIL, also include:

FINDINGS:
- severity | criterion | file:line | failure mode | required behavior

REPAIR_COUNT:
- current value from checkpoint

## Repair transition

On FAIL, run:

bash .agents/scripts/sync-next-checkpoint.sh qa-fail SHORT_REASON

Then return to Builder and repair all findings together. After two repair loops,
one further FAIL must transition to blocked:

bash .agents/scripts/sync-next-checkpoint.sh block REPAIR_BUDGET_EXHAUSTED

Do not weaken the oracle or tests.

On PASS, continue to the verifier. Do not update checkpoint manually and do not
claim the review was independent.
