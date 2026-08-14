# Synabit Sync — Active implementation plan

> Trạng thái sản phẩm: **NO-GO**
>
> Đối tượng thực thi: AI coding agent làm việc trực tiếp trên source local hiện
> tại. Không dùng Git history để suy đoán intent.
>
> Đây là file kế hoạch và tiến độ duy nhất. Không tạo plan, report, ADR,
> checklist hoặc tài liệu con. Các phần roadmap chưa active chỉ để định tuyến;
> agent tuyệt đối không triển khai chúng trước khi external auditor phát hành
> contract và oracle tương ứng.

## 0. Active execution capsule

Agent phải đọc trọn mục `0` và đúng contract được chỉ định trước khi làm gì
khác. Không đọc Historical ledger để suy ra implementation. Mọi field bên dưới
dùng đúng enum/literal, không tự tạo biến thể trạng thái mới.

- Current milestone: `Package D — Delete, rename and conflict correctness`
- Current milestone status: `in_progress`
- Execution status: `awaiting_external_audit`
- Last internally completed batch: `D2-SERVER-TOMBSTONE-TRANSPORT`
- Internal QA result: `PASS_SELF_REVIEW_NOT_EXTERNAL`
- Internal repair loops: `0`
- Internal verification result: `PASS:D2-ORACLE-V1`
- Last externally accepted batch: `D1-TOMBSTONE-IDENTITY`
- External audit status: `not_started`
- External audit result: `D1_ACCEPTED_D2_PUBLISHED_RED_BASELINE`
- Closure contract ID: `D2-SERVER-TOMBSTONE-TRANSPORT-V1`
- Closure contract status: `frozen`
- External oracle version: `D2-ORACLE-V1`
- External oracle path: `.agents/scripts/verify-work-package-d2-server-tombstone-transport.sh`
- External oracle SHA-256: `ee33667a45c0e023a18b2cacf68b4e98b9c207ed095cae05e97860776f5992e5`
- External oracle result: `RED_BASELINE`
- Next batch ID: `D2-SERVER-TOMBSTONE-TRANSPORT`
- Next batch objective: `Preserve the non-empty encrypted typed tombstone byte-for-byte through Sync Server push, durable blob storage, quota, paged pull and legacy pull while rejecting corrupt or payloadless delete writes.`
- Stop boundary: `AWAITING_EXTERNAL_AUDIT_AFTER_D2_SERVER_TOMBSTONE_TRANSPORT`
- Reopen policy: `D1 is externally accepted. Execute only the frozen D2 server transport seam; do not start bootstrap, client filesystem apply, rename, conflict resolution, asset or general server durability work.`
- Known unrelated gate failure: `search::tests::test_unknown_type_filter_ignored`
- Workflow control SHA-256: `94fd9ed882e47f2a00d76117055cadca1f3dd1b62613bbf2a168dd608be9f11f`
- Execution command: `/sync-next milestone`

### 0.1. State ownership

Antigravity may update only:

- `Execution status` to `building`, `qa_failed`, `awaiting_external_audit` or
  `blocked` as permitted by the active contract;
- `Last internally completed batch`;
- `Internal QA result`;
- `Internal repair loops`;
- `Internal verification result`;
- one bounded entry in `Implementation log`.

Only Codex/external auditor may update:

- milestone/status and work-package checkboxes;
- last externally accepted batch;
- external audit fields and findings;
- contract ID/status/content;
- oracle version/path/digest/result or oracle file;
- next batch/objective, stop boundary and reopen policy.

Internal PASS never closes a package. Antigravity must stop at
`awaiting_external_audit`.

### 0.2. Current external findings

These are the complete D2 publication findings:

1. D1 is externally accepted: the typed `DeletePayload` is prepared durably,
   validated exactly and passes `D1-ORACLE-V2` independently on current source.
2. `MailboxHandler::handle_push` skips payload-hash verification for Delete,
   acknowledges an idempotent retry before checking the supplied bytes, stores
   the literal path `(tombstone)` instead of a blob and still charges byte quota.
3. `handle_pull_page` and `Database::pull_entries` replace every Delete
   ciphertext with `Vec::new()`. Page byte limits therefore measure zero and a
   missing tombstone blob cannot fail closed.
4. The payloadless legacy `PushDelete` route still creates a zero-hash Delete,
   so it can inject an entry that cannot contain D1's typed encrypted identity.
5. The D2 harness compiles when wired and all three tests fail against current
   production at exact storage, integrity and bounded-read assertions. Without
   wiring, the aggregate red baseline exits `1` only on the immutable inventory;
   accepted regressions remain green (`386` client, `4` protocol, `10` server).
6. External feasibility verification temporarily applied the contract behavior:
   exact `D2-ORACLE-V1` passed all three D2 tests and complete regressions, then
   the two production files were restored byte-for-byte to the red baseline.
7. Bootstrap currently depends on unfinished server tables and is explicitly
   deferred; D2 covers Push/PushBatch plus delta and legacy pull only.

### 0.3. Frozen contract

<!-- ACTIVE_CONTRACT_BEGIN -->

Contract `D2-SERVER-TOMBSTONE-TRANSPORT-V1` is the only executable scope. Builder must
implement it directly; do not create another plan, contract, oracle, report or
phase. Stop after the workflow reaches `awaiting_external_audit`.

Builder may edit only:

- `sync-server/src/mailbox.rs`;
- `sync-server/src/db.rs`;
- `.agents/runtime/sync-next-evidence.tsv` through the prescribed handoff.

Builder must not edit this contract, workflow controls, `.agents/oracles/**`,
the selected oracle script, shared protocol, client source, server schema or any
server production file outside the two listed files. In `mailbox.rs`, wire the
immutable harness exactly once as a top-level test-only module using:

```rust
#[cfg(test)]
#[path = "../../.agents/oracles/d2_server_tombstone_transport.rs"]
mod d2_server_tombstone_transport;
```

Do not copy the harness into Builder-owned tests or duplicate its decisions in a
test-only model. Existing mailbox tests whose empty Delete fixtures encode the
retired behavior must be updated to use non-empty payloads with matching hashes.

#### `D2ST-WRITE`

For `MailboxRequest::Push` and every `PushBatch` item whose kind is Delete,
require a non-empty `encrypted_payload` and verify `blake3(encrypted_payload)`
equals the supplied `payload_hash` before returning any idempotent success. A
hash mismatch, empty payload or same operation ID with changed bytes/hash/kind/
document must return an error without consuming a sequence, charging quota or
leaving a final/temp blob.

Persist a valid Delete ciphertext through the same atomic temp-write/rename
blob boundary as other operation payloads; record the real blob path and exact
byte length. An exact retry must return the original sequence and create no new
blob or quota charge. `MailboxRequest::PushDelete` cannot carry the D1 typed
payload and must return a protocol error with zero mutation; do not fabricate an
empty tombstone, hash or identity. Do not remove/reorder shared wire variants.

#### `D2ST-READ`

`MailboxHandler::handle_pull_page` and `Database::pull_entries` must read the
durable blob for Delete exactly as for other entry kinds and return the original
operation ID, kind, doc hash, source device, ciphertext and payload hash.
`max_bytes` must count the actual Delete ciphertext: when the first entry is too
large return `MailboxResponse::Error` without an advancing page; otherwise stop
before exceeding the bound and preserve normal `has_more` semantics.

A missing/unreadable Delete blob must fail the paged and legacy pull paths; it
must never become an empty successful entry. D2 must not implement or modify
bootstrap/checkpoint behavior, client filesystem apply, CRDT/path/baseline
state, GDrive, rename, conflict resolution, assets, schema/migrations, GC or
general transactional server redesign.

The immutable D2 acceptance resources are:

- `.agents/oracles/d2_server_tombstone_transport.rs` — three behavioral tests
  covering exact durable round trip/idempotency, invalid/legacy zero mutation,
  and bounded/missing-blob reads; SHA-256
  `3c3c84ffc502485ca8532243f64dd7124809eb7a50d625f3488a0939e2996523`;
- `.agents/scripts/verify-work-package-d2-server-tombstone-transport.sh` — pure
  aggregate oracle `D2-ORACLE-V1`, digest recorded in the capsule. It locks the
  complete client tree, shared protocol tree and server production outside
  `mailbox.rs`/`db.rs`, requires exactly three compiled D2 tests, and runs format,
  D2 behavior plus full client/protocol/server regressions.

Current aggregate red baseline exits `1` because the immutable module is not
wired. External temporary direct wiring compiled all three tests and each
failed on a distinct current production behavior: no tombstone blob, skipped
hash rejection and zero-byte bounded pull. A temporary compliant implementation
then produced `D2_ORACLE_V1_PASS`, including all regressions, before production
was restored byte-for-byte. Remove the red failures through production behavior;
do not alter the oracle or harness.

<!-- ACTIVE_CONTRACT_END -->

## 1. Product outcome

Sync is release-ready only when multiple devices converge through GDrive or the
Sync Server without cross-vault leakage, data loss, unbounded binary buffering,
cursor gaps, retry duplicates or misleading UI state.

Required end-state:

1. Vaults remain isolated even with the same recovery key.
2. Text, Markdown, JSON/canvas, rename, delete and binary assets sync end to end.
3. Retry/restart preserves operation identity and does not double-charge quota.
4. Remote data is durable locally before cursor commit and remote ACK.
5. Assets use bounded streaming/chunking outside operation payloads.
6. Bootstrap, quota, GC and device revocation survive server restart.
7. UI status, cancellation and errors reflect actual backend state.
8. Automated fault injection covers critical crash/network boundaries.

## 2. Target architecture

```text
UI trigger
  → acquire one run per (vault_id, provider_id)
  → load immutable vault context
  → provider identity preflight
  → recover durable outbox, inbox and ACK gaps
  → detect local changes and create/reuse outbox rows
  → upload required asset chunks
  → push ready operations
  → transactionally commit ACK + local baseline
  → pull one bounded page
  → transactionally stage page and ordered membership
  → apply contiguous entries
  → commit local provider cursor
  → ACK remote cursor
  → emit truthful result
```

Server push path:

```text
Hello/Auth
  → validate vault, device and capability
  → idempotency lookup by (vault_id, operation_id)
  → durable blob staging
  → one DB transaction: sequence + processed result + mailbox + head + quota
  → durable commit/finalization
  → per-operation result
```

## 3. Non-negotiable invariants

1. **Vault/provider isolation:** every cursor, baseline, path, CRDT, outbox,
   inbox, asset and bootstrap record is scoped by vault and provider as needed.
2. **Stable identities:** provider ID is stable (`gdrive`, `server`); connection,
   endpoint and device IDs are separate.
3. **Immutable run context:** a run never rereads a mutable global active vault.
4. **Durable outbox:** one logical change keeps one operation ID across retry and
   restart; baseline advances only after accepted commit.
5. **Durable inbox:** remote page is staged before apply; cursor never advances
   through failed, corrupt, unsupported or unavailable content.
6. **Local commit before remote ACK:** ACK failure is retryable without rolling
   back completed local work.
7. **Idempotency:** retry returns the original logical result and does not create
   a new remote sequence/blob/quota charge.
8. **Typed operations:** Upsert, Delete and AssetReference retain their kind end
   to end and use matching payload semantics.
9. **Asset bytes outside operations:** operations contain only asset references;
   bytes are bounded, chunked and integrity checked.
10. **Safe filesystem:** every remote path crosses one safe resolver; absolute,
    traversal and symlink escape are rejected.
11. **Bounded resources:** page, frame, plaintext, decompression, asset and chunk
    sizes have hard limits.
12. **Truthful capability/UI:** unsupported features return typed errors; partial,
    pending, cancelled and failed runs are never reported complete.

Forbidden on durability/integrity paths:

- `unwrap_or_default` or fabricated zero IDs/hashes/payloads;
- ignored errors or log-and-continue after a required durable mutation;
- success no-op for an advertised capability;
- test names, comments, string tokens or duplicated decisions as evidence.

## 4. Accepted foundation ledger

Accepted slices are closed regression boundaries, not executable instructions.
They may be reopened only when the active batch changes their production seam
and an external audit demonstrates a regression.

| Slice | Status | Immutable oracle SHA-256 | Accepted boundary |
|---|---|---|---|
| A | accepted | `5918897c035cc470b673cbd63c3655bb31bdcc65bf2fc0d4e22f51cfdeaa38f6` | One adapter/protocol contract, typed entry kinds, exact codecs |
| B1 | accepted | `7dff56a65d6b047a717e95c9b2bc7cbfcf428494124bba768ce5f96296c8aa98` | Atomic vault metadata and canonical registered identity |
| B2 | accepted | `ca8731ac9c4b08d994ad570e241f21d01149f19ae7312c8a0d1ec057956b2a17` | Versioned migration, backup and deterministic legacy handling |
| B3 | accepted | `b0152d58b9591387cc02b188258d226827a22428e094d2afb7056b86efc0c7d7` | Vault-scoped CRDT, path and baseline DAO/callers |
| B4 | accepted | `401b6f487a45ecceee4c31d9c87542e01a5b7ba20d88a737425343c3ba380e67` | Scoped provider state and preflight identity reconciliation |
| C1 | accepted | `b41ed416450130c5f5da23e8a4b413937c6cd3470aaf4e02b890d3319c940e16` | Durable outbox model, preparation, ACK commit and retry DAO |
| C2A | accepted | `50d9dde4ce21c5d3c8f5abc23d0b6d7504da59e6dc457ba81a4fceac55549e7c` | Durable inbox/page/membership ledger and atomic cursor commit DAO |
| C2B | accepted | `5b851fa715c22bb2c2ee86603a9ce8bf3098f93dc0be7967e291ce111b085888` | Restart-safe orchestration, exact outcomes, durable pull/cursor/ACK and strict provider schema |

Accepted production contracts relevant to current C2B:

- vault identity and provider state must be reconciled before push/pull;
- outbox conversion is fallible and has one authority in `db/sync_outbox.rs`;
- sent marking, accepted commit and batch retry are scoped transactional DAO
  operations;
- inbox page staging, membership ordering, safe-page evaluation and local cursor
  commit have one authority in `db/sync_inbox.rs`;
- C2B may orchestrate these APIs but may not clone their decisions.

## 5. Work-package roadmap

Only the active contract in section `0` is executable. Before each later
package, Codex must inspect current local code and replace its roadmap summary
with one small frozen acceptance slice and a red-baseline behavioral oracle.

### Package A — Protocol and adapter contract

- [x] Accepted.

### Package B — Vault identity, migration and scoped DAO

- [x] Accepted.

### Package C — Durable outbox, inbox and cursor

- [x] Accepted.

### Package D — Delete, rename and conflict correctness

- [ ] In progress; D1 typed tombstone identity is accepted and D2 server
  tombstone transport is the only active slice.
- Typed tombstone integrity, idempotent vault-scoped delete and safe-path apply.
- Stable-node rename that converges with concurrent remote edits.
- JSON/HLC and Markdown/CRDT conflict decisions must create reconciliation when
  the local value wins; validation failure cannot advance baseline/cursor.
- Exit scenarios: offline delete, retry tombstone, delete/recreate, rename/edit,
  local-winner reconciliation and malformed JSON.

### Package E — Asset pipeline

- [ ] Pending.
- One pending-asset implementation and postcard `AssetRef` contract.
- Streaming snapshot/hash/chunk with mutation detection and bounded sizes.
- Reference operation cannot become ready before every required chunk exists.
- Pull persists resumable asset jobs, verifies AAD/chunk/final integrity and uses
  safe atomic publication.
- Exit scenarios include 0-byte through 50 MiB, interrupted upload/download,
  text-only mode, tamper/AAD/final-hash failures and path escapes.

### Package F — Google Drive durability and onboarding

- [ ] Pending.
- Opaque remote vault namespace and encrypted create/join descriptor.
- Idempotent operation/chunk publication and stable source-device identity.
- Per-vault Changes token with bounded page staging and token-410 reconciliation.
- Exit: second device joins existing vault, same-key vaults remain separate,
  retries do not duplicate, corrupt objects are surfaced and backlog is bounded.

### Package G — Sync Server durability

- [ ] Pending.
- Versioned startup migration for vault, sequence, device, processed-operation,
  mailbox, document-head, asset-ref, bootstrap and quota state.
- Hello/Auth capability truthfulness and complete per-operation push results.
- One transactional idempotent push boundary covering sequence, blob metadata,
  ledger, mailbox, head, asset refs and quota.
- Exit: fresh server, first push/pull/ACK, restart, idempotent retry, mixed batch,
  delete, quota rollback and concurrent monotonic sequence.

### Package H — Bootstrap, GC, quota and device lifecycle

- [ ] Pending.
- Fixed-high-watermark resumable bootstrap followed by gap-free delta.
- Reference/pin-aware GC; no age-only deletion of live data.
- Transactional quota for document and asset bytes without retry double charge.
- Durable device status, revocation and secret/epoch handling.
- Exit: restartable bootstrap with concurrent push, GC pin safety, correct quota
  after restart/GC and revoked device denied future access.

### Package I — UI/UX and cancellation

- [ ] Pending.
- One run per `(vault_id, provider_id)` with backend cancellation token.
- UI timeout cancels and waits for a terminal backend event.
- One progress/event schema for phases, counts, bytes, pending, partial and error.
- No duplicate listener/component; UI commands must exist in Rust.
- Exit: trailing run, real cancellation, no stale writes, truthful partial/quota/
  error display and correct GDrive create/join UX.

### Package J — Release and fault-injection gate

- [ ] Pending.
- Local DB, fake adapter, server integration and provider conformance layers.
- Crash/error injection after outbox insert, chunk upload, server blob/DB commit,
  client response, inbox stage/apply, asset publication, cursor commit and every
  bootstrap page.
- End-to-end matrix covers multi-device/vault, delete/rename, large assets,
  restart, bootstrap/GC/quota/revoke and cancel.
- Exit: all repository format/check/test/clippy/frontend gates and behavioral
  scenario matrix pass with no undocumented skip.

## 6. Release Definition of Done

Sync may be called ready only when:

- [ ] Packages A–J are externally accepted.
- [ ] Fresh and migrated client/server databases work across restart.
- [ ] No unscoped sync/CRDT/path/cursor/baseline query remains.
- [ ] No mock/derived operation identity or advertised success no-op remains.
- [ ] Delete and asset reference are typed end to end.
- [ ] Cursor cannot pass failed, corrupt or unavailable content.
- [ ] Retry creates no logical duplicate and charges quota once.
- [ ] Large assets are bounded and cannot write outside the vault.
- [ ] GDrive join, server bootstrap, GC pinning and revocation scenarios pass.
- [ ] UI cancellation stops backend work and UI never reports false completion.
- [ ] All required quality gates and fault-injection scenarios pass.

## 7. Bounded handoff contract

Antigravity handoff for one active batch must contain only:

1. exact batch/contract/oracle ID and verified digest;
2. changed files;
3. one row per active criterion with production call, observable behavioral
   evidence and smallest counterfactual;
4. exact commands and exit codes from the current invocation;
5. internal checkpoint transition;
6. remaining blocker, if any.

It must not claim package completion. Prose such as “implemented all fixes” is
not evidence.

## 8. Bounded implementation log

This document was compacted on 2026-08-13 with user authorization. Detailed
historical Builder/repair narration was replaced by the accepted foundation
ledger in section `4`; no acceptance status or current blocker was discarded.

From this point:

- Antigravity appends at most one entry per internal batch.
- Each entry is at most 12 lines and follows section `7`.
- Keep at most the latest 12 entries.
- When the limit is reached, only external auditor may fold accepted entries
  into section `4` and remove superseded rejected narration.
- Active findings always live in section `0`, never only in this log.

### 2026-08-14 — C2B history compacted through closure 2

- Retired V2/V4 gates after semantic audits found proxy evidence, incomplete
  snapshots and a migration fixture that never rebuilt a historical TEXT table.
- V3 established accepted dispatcher, typed payload, durable pull, provider
  mapping and raw-snapshot behavior; V5 added real historical rebuild, child-FK,
  overflow, rollback and fail-closed schema evidence.
- Builder handoffs remained internally green but did not grant package
  acceptance. Detailed superseded narration was compacted by external auditor.
- Package C remained in progress and package D remained closed.

### 2026-08-14 — C2B architecture closure 3 publication

- External semantic audit rejected `C2B-ARCH-CLOSURE-2`: V5 was false-green on
  TEXT/10-byte identity storage and backward UPDATE rewriting `created_at`.
- Retired contradictory V4 test wiring and corrected two corrupt-data fixtures
  to bypass CHECK constraints explicitly; production behavior was not repaired.
- Published frozen `C2B-ARCH-CLOSURE-3-V1` and pure `C2B-ORACLE-V6` with three
  storage-class/length/backward-update tests while retaining V3 and V5 gates.
- Red baseline exits `1` only for zero compiled V6 tests; direct temporary wiring
  confirmed all three V6 tests fail on the current production loopholes.
- Full regression: 379 passed, 0 failed, 1 ignored, 1 filtered.
- Machine preflight target: `C2B-ARCH-CLOSURE-3`.

### 2026-08-14 — Internal handoff for C2B-ARCH-CLOSURE-3

- Contract: C2B-ARCH-CLOSURE-3-V1.
- Self-review: PASS_SELF_REVIEW_NOT_EXTERNAL.
- Criteria: C2BAC3-IDENTITY,C2BAC3-TIMESTAMP,C2BAC3-MIGRATION.
- Evidence TSV SHA-256: 2cce3e9dfee2a787448421b007206d6171b84b563d21416823833ac5a3590d28.
- Verification: C2B-ORACLE-V6 exited 0 with unchanged digest.
- Repair loops: 0.
- State: awaiting_external_audit; no package acceptance claimed.

### 2026-08-14 — C2B architecture closure 4 publication

- External semantic audit rejected `C2B-ARCH-CLOSURE-3`: fresh and rebuilt
  schemas still admitted disabled-provider TEXT identity when `last_error` was set.
- Published frozen `C2B-ARCH-CLOSURE-4-V2` and `C2B-ORACLE-V7R1` with the
  complete five-state INSERT/UPDATE matrix and corrected locked legacy fixture.
- V3, V5, V6 and full regression remain green; V7 red baseline exits `1` only
  because its two immutable tests are not yet wired.
- Temporary direct wiring confirmed both V7 tests fail at the exact production
  exception on fresh and rebuilt schemas.
- Full regression: 382 passed, 0 failed, 1 ignored, 1 filtered.
- V7R1 fixture revision was published after the first Builder run exposed a
  second intentional TEXT-corruption fixture without CHECK bypass.
- Machine preflight target: `C2B-ARCH-CLOSURE-4`.

### 2026-08-14 — Internal handoff for C2B-ARCH-CLOSURE-4

- Contract: C2B-ARCH-CLOSURE-4-V2.
- Self-review: PASS_SELF_REVIEW_NOT_EXTERNAL.
- Criteria: C2BAC4-NOEXCEPTION,C2BAC4-PRESERVE.
- Evidence TSV SHA-256: 03a1d66d4f9049520243735eb8a7fca2cee1c6d76a3d5e66b145799264a01e43.
- Verification: C2B-ORACLE-V7R1 exited 0 with unchanged digest.
- Repair loops: 0.
- State: awaiting_external_audit; no package acceptance claimed.

### 2026-08-14 — Package C acceptance and D1 publication

- External semantic audit accepted `C2B-ARCH-CLOSURE-4`: exact fresh/rebuilt
  identity constraints and migration preservation pass `C2B-ORACLE-V7R1`.
- Package C and C2B were added to the accepted foundation ledger.
- Source audit found unit delete payload, dropped provider-independent target
  identity and the explicit `UnsupportedDelete` validation branch.
- Published frozen `D1-TOMBSTONE-IDENTITY-V1` and `D1-ORACLE-V1` with two
  immutable behavioral tests plus shape/shortcut bans.
- D1 red baseline exits `1` on production shape and exact 0/2 inventory only;
  accepted client (`384/0/1`) and server/protocol (`10+4`) regressions pass.
- Server tombstone transport, filesystem apply, rename and conflict resolution
  remain closed for later contracts.

### 2026-08-14 — D1 V2 external contract repair

- External audit found V1 impossible because its new typed variant could not
  compile the locked C2B unit-delete fixtures.
- Preserved the historical accepted harness and published a complete 21-test
  typed-delete successor instead of mutating acceptance history.
- Removed Builder's ineffective empty compatibility const and switched only
  test wiring to the successor; no filesystem/server behavior was opened.
- Published `D1-TOMBSTONE-IDENTITY-V2` / `D1-ORACLE-V2`; direct oracle passes
  D1 2/2, C2B 21/21, client 386/0/1, protocol 4/4 and server 10/10.
- Builder repair count remains 0 because this was an external contract defect;
  resume the same batch at self-review and evidence handoff.

### 2026-08-14 — Internal handoff for D1-TOMBSTONE-IDENTITY

- Contract: D1-TOMBSTONE-IDENTITY-V2.
- Self-review: PASS_SELF_REVIEW_NOT_EXTERNAL.
- Criteria: D1TI-IDENTITY,D1TI-VALIDATION.
- Evidence TSV SHA-256: 23bd1f0d90181e87a60442981aee2f4b27c68df55a5e5b29fbf125650ffbe337.
- Verification: D1-ORACLE-V2 exited 0 with unchanged digest.
- Repair loops: 0.
- State: awaiting_external_audit; no package acceptance claimed.

### 2026-08-14 — D1 acceptance and D2 server transport publication

- External semantic audit accepted D1 on exact source behavior and independently
  reproduced `D1-ORACLE-V2` (`2/2` D1, `21/21` C2B successor, full regressions).
- Source audit found Delete hash verification skipped before idempotency, a
  literal `(tombstone)` path, ciphertext stripping in both pull paths and a
  payloadless legacy write route.
- Published frozen `D2-SERVER-TOMBSTONE-TRANSPORT-V1` / `D2-ORACLE-V1` with two
  criteria and a three-test immutable server harness.
- Aggregate red baseline exits `1` only for zero compiled D2 tests; temporary
  direct wiring compiled `3/3` and all failed at distinct production assertions.
- A temporary compliant source patch then produced full `D2_ORACLE_V1_PASS` and
  was reverted byte-for-byte, proving the frozen contract is executable.
- Existing accepted regressions remain green: client `386/0/1`, protocol `4/4`,
  server `10/10`.
- Bootstrap, filesystem apply, rename and conflict work remain scope-locked.

### 2026-08-14 — Internal handoff for D2-SERVER-TOMBSTONE-TRANSPORT

- Contract: D2-SERVER-TOMBSTONE-TRANSPORT-V1.
- Self-review: PASS_SELF_REVIEW_NOT_EXTERNAL.
- Criteria: D2ST-WRITE,D2ST-READ.
- Evidence TSV SHA-256: 54f592109cb33c6b8e59fa4416b9820de14ee86fa17232b4f9e40668d28ce816.
- Verification: D2-ORACLE-V1 exited 0 with unchanged digest.
- Repair loops: 0.
- State: awaiting_external_audit; no package acceptance claimed.
