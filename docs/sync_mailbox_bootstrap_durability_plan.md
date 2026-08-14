# Mailbox Bootstrap & Durability — Implementation Plan

## 1. Mục tiêu

Thiết kế lại phần persistence và replay của Synabit Sync Server để đạt các
invariant sau:

1. Một push chỉ được ACK sau khi blob và metadata đã durable.
2. Retry cùng `operation_id` luôn trả lại cùng kết quả, kể cả mailbox log đã GC.
3. Sequence tăng đơn điệu và sequence allocation + mailbox insert nằm trong
   cùng một SQLite transaction.
4. Device mới hoặc device offline quá lâu luôn có thể bootstrap từ encrypted
   state hiện tại rồi replay delta mà không có khoảng trống.
5. Pull có pagination và giới hạn bytes, không load toàn bộ lịch sử vào RAM.
6. Cleanup không được xóa dữ liệu cần cho document head, bootstrap session,
   active device hoặc referenced asset.
7. Server vẫn là zero-knowledge mailbox: không decrypt document/asset content.
8. Server restart, process crash, disk error và client retry không tạo
   divergent state hoặc ACK giả.
9. Restore từ backup cũ được client nhận biết và bắt buộc bootstrap lại.

Plan này bao gồm cả server, shared protocol và phần client tối thiểu để sử dụng
bootstrap/paged pull. Không thay đổi CRDT merge algorithm hoặc UI provider
selection.

## 2. Hiện trạng và lỗi cần giải quyết

### 2.1. Push chưa durable theo một transaction

Hiện tại `sync-server/src/mailbox.rs`:

1. Ghi encrypted blob trực tiếp vào final path.
2. Sau đó gọi `Database::push_entry`.
3. `push_entry` tăng `vault_sequences.seq`.
4. Sau đó mới insert mailbox row.

Các bước filesystem và DB không có crash-recovery contract. Sequence update và
mailbox insert cũng chưa nằm trong cùng transaction. Failure giữa các bước có
thể tạo:

- Orphan blob không có DB row.
- Sequence gap.
- Blob tồn tại nhưng operation chưa được ghi nhận.
- Retry tạo entry mới thay vì hoàn tất operation cũ.

### 2.2. Idempotency bị mất khi mailbox log bị GC

`operation_id` hiện nằm trực tiếp trên mailbox row. Khi cleanup xóa mailbox row,
server quên operation đã xử lý. Retry cũ có thể được nhận thành operation mới.

### 2.3. Không có authoritative encrypted checkpoint

Server chỉ có append-only mailbox entries. Khi:

- toàn bộ active devices đã ACK và log bị GC;
- hard TTL xóa entry;
- device mới tham gia;
- device cũ mất local DB/cursor;

server không còn một encrypted full state để đưa device về trạng thái hiện tại.

Payload hiện tại là encrypted full document snapshot, nên server có thể giữ
“latest opaque payload per document” mà không cần đọc plaintext. Đây sẽ là
document head/checkpoint.

### 2.4. Pull không bounded

`Database::pull_entries` query toàn bộ rows có `seq > cursor`, sau đó đọc tất cả
blob bằng `std::fs::read` và gom vào một `Vec<MailboxEntry>`.

Một device offline lâu ngày có thể làm:

- Tăng RAM theo toàn bộ backlog.
- Block async runtime bằng filesystem read đồng bộ.
- Vượt protocol frame limit.
- Mất toàn bộ page nếu connection rớt ở cuối response.

### 2.5. Cleanup có thể gây permanent divergence

Cleanup hiện:

- Xóa entries đã được `MIN(cursor)` ACK.
- Xóa entries quá hard TTL bất kể ACK.
- Xóa asset quá TTL mà không biết encrypted document head còn tham chiếu.

Không có bootstrap checkpoint nên hard TTL có thể khiến device không bao giờ
catch up được.

### 2.6. Cursor chưa mô hình hóa device lifecycle

Một row trong `cursors` vừa là membership, last-seen và ACK state. Không phân
biệt:

- Device mới cần bootstrap.
- Active device đang catch up.
- Device stale được loại khỏi GC quorum.
- Device bị revoke.
- Device đang bootstrap.

Xóa cursor khi revoke cũng không phải một cryptographic security boundary; một
client còn mailbox token có thể đăng ký lại bằng device ID khác. Plan này chỉ
chuẩn hóa lifecycle phục vụ bootstrap/GC, không tuyên bố giải quyết hoàn toàn
device revocation.

## 3. Target architecture

```text
Client push
    |
    v
validate hash/size/idempotency
    |
    v
write temp blob -> fsync -> atomic rename -> fsync directory
    |
    v
BEGIN IMMEDIATE
  allocate seq
  insert processed_operation
  insert mailbox entry
  upsert document head
  update opaque asset references
  update vault usage
COMMIT
    |
    v
ACK client + notify subscribers


New/stale device
    |
    v
GetSyncPlan
    |
    +-- cursor usable ------> paged delta pull to fixed high watermark
    |
    +-- bootstrap needed --> materialized encrypted heads
                              + current trash metadata
                              + pinned asset references
                              |
                              v
                         ACK base sequence
                              |
                              v
                         paged delta > base sequence
```

### Core checkpoint rule

`document_heads` luôn giữ opaque full snapshot/tombstone mới nhất của mỗi
`doc_hash`.

Server không cần biết payload là Markdown, JSON hay asset reference. Protocol
phải khai báo outer `payload_kind`:

```text
FullSnapshot
DeleteTombstone
```

Chỉ `FullSnapshot` được phép thay document head. Nếu tương lai thêm CRDT delta,
delta không được thay head cho tới khi client gửi một full checkpoint mới.

## 4. Invariants bắt buộc

### D1 — Durable ACK

`PushOk`/accepted batch result chỉ được gửi sau:

- Blob đã được flush và atomic rename thành công.
- SQLite transaction đã commit.
- Mailbox row, operation ledger và document head cùng nhìn thấy operation.

### D2 — Stable idempotency

`(vault_hash, operation_id)` là unique vĩnh viễn hoặc ít nhất bằng lifetime của
vault. Duplicate trả lại original `seq` và result; không cấp sequence mới,
không charge quota lần hai và không ghi blob lần hai.

### D3 — Snapshot + delta không có gap

Bootstrap session có `base_seq`. State của session là document heads tại đúng
transaction snapshot đó. Mọi push concurrent:

- Commit trước bootstrap transaction: nằm trong snapshot.
- Commit sau bootstrap transaction: có `seq > base_seq` và nằm trong delta.

### D4 — Pull bounded

Mỗi page bị clamp bởi cả:

- `max_entries`, mặc định 128, tối đa 512.
- `max_bytes`, mặc định 8 MiB, tối đa 16 MiB.

Không page nào gom toàn bộ backlog.

### D5 — Cursor chỉ tiến sau durable local receipt/apply

Client chỉ ACK:

- Delta page sau khi entries trong page đã apply hoặc được lưu vào durable local
  inbox/pending table.
- Bootstrap `base_seq` sau khi toàn bộ bootstrap items đã durable và apply xong.

### D6 — GC luôn giữ đường bootstrap

Một mailbox entry có thể bị GC nhưng blob của latest document head không được
xóa. Active bootstrap session pin đúng blob/asset versions mà session đã
materialize.

### D7 — Restore detection

Mỗi vault có `incarnation_id` ngẫu nhiên. Client lưu incarnation cùng cursor.
Nếu server restore/reset và incarnation thay đổi, client bỏ cursor cũ và chạy
bootstrap.

### D8 — Zero knowledge

Server chỉ thấy:

- vault hash;
- document hash;
- operation/device IDs;
- sequence/timestamps/sizes;
- opaque asset chunk IDs;
- ciphertext.

Không đưa plaintext path, MIME, title, document content hoặc plaintext hashes
ra outer protocol.

## 5. Schema đích

Migrations phải dùng `schema_migrations` hoặc `PRAGMA user_version`; bỏ cách
`ALTER TABLE` rồi ignore error.

### 5.1. Vault state

```sql
ALTER TABLE vaults ADD COLUMN incarnation_id BLOB;
ALTER TABLE vaults ADD COLUMN compacted_through_seq INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vaults ADD COLUMN committed_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vaults ADD COLUMN bootstrap_state TEXT NOT NULL DEFAULT 'not_ready';
ALTER TABLE vaults ADD COLUMN checkpoint_generation BLOB;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version     INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    applied_at  INTEGER NOT NULL
);
```

`incarnation_id` là 16 random bytes, được tạo khi register/backfill. Không derive
từ vault key hoặc vault hash.

`committed_bytes` là quota counter transactional, không tính lại bằng nhiều
`SUM()` ngoài transaction trên hot path.

`compacted_through_seq = N` nghĩa là mailbox entries có `seq <= N` có thể đã bị
xóa. Một delta cursor chỉ usable khi `cursor >= N`. Cách định nghĩa này tránh
off-by-one của tên “minimum retained sequence”.

`bootstrap_state` chỉ nhận `not_ready`, `seeding`, `ready` hoặc `degraded`.
Safe GC chỉ chạy khi vault ở `ready`.

### 5.2. Device lifecycle

```sql
CREATE TABLE IF NOT EXISTS devices (
    vault_hash         TEXT NOT NULL REFERENCES vaults(vault_hash),
    device_id          TEXT NOT NULL,
    state              TEXT NOT NULL CHECK (
        state IN ('bootstrap_required', 'bootstrapping', 'active', 'revoked')
    ),
    last_acked_seq     INTEGER NOT NULL DEFAULT 0,
    last_bootstrap_seq INTEGER NOT NULL DEFAULT 0,
    joined_at          INTEGER NOT NULL,
    last_seen          INTEGER NOT NULL,
    stale_at           INTEGER,
    revoked_at         INTEGER,
    PRIMARY KEY(vault_hash, device_id)
);

CREATE INDEX IF NOT EXISTS idx_devices_gc
    ON devices(vault_hash, state, last_acked_seq);
```

Rules:

- Device mới: `bootstrap_required`, không tham gia active ACK quorum.
- `BeginBootstrap`: `bootstrapping`.
- `CompleteBootstrap` + ACK base: `active`.
- Active device quá stale threshold: `bootstrap_required`, bị loại khỏi ACK
  quorum; khi quay lại phải bootstrap.
- Revoked device: `revoked`; không tham gia quorum.
- `last_acked_seq` chỉ tăng bằng `MAX(existing, new)`.

### 5.3. Processed operation ledger

```sql
CREATE TABLE IF NOT EXISTS processed_operations (
    vault_hash    TEXT NOT NULL REFERENCES vaults(vault_hash),
    operation_id  BLOB NOT NULL,
    seq           INTEGER NOT NULL,
    doc_hash      TEXT NOT NULL,
    result_kind   TEXT NOT NULL,
    payload_hash  TEXT,
    created_at    INTEGER NOT NULL,
    PRIMARY KEY(vault_hash, operation_id)
);

CREATE INDEX IF NOT EXISTS idx_processed_operations_seq
    ON processed_operations(vault_hash, seq);
```

Không FK `processed_operations.seq` sang mailbox vì mailbox row được phép GC.
Ledger nhỏ hơn blob/log nhiều lần và được giữ lâu dài.

### 5.4. Blob objects

```sql
CREATE TABLE IF NOT EXISTS blob_objects (
    vault_hash    TEXT NOT NULL REFERENCES vaults(vault_hash),
    blob_id       TEXT NOT NULL,
    blob_path     TEXT NOT NULL,
    blob_size     INTEGER NOT NULL,
    payload_hash  TEXT NOT NULL,
    state         TEXT NOT NULL CHECK (state IN ('ready', 'corrupt')),
    created_at    INTEGER NOT NULL,
    PRIMARY KEY(vault_hash, blob_id)
);
```

`blob_id` là content-addressed opaque ID, tối thiểu:

```text
blob_id = hex(blake3(vault_hash || payload_hash))
```

Blob path không còn dựa vào document path. Một blob có thể được mailbox log và
document head cùng tham chiếu mà chỉ charge quota một lần.

Mailbox table cần `blob_id`, `entry_kind` và unique operation:

```sql
CREATE UNIQUE INDEX IF NOT EXISTS idx_mailbox_operation
    ON mailbox(vault_hash, operation_id)
    WHERE operation_id IS NOT NULL;
```

Trong migration có thể giữ `blob_path/blob_size` cũ trong một release để
rollback, nhưng code mới phải đọc qua `blob_objects`.

### 5.5. Document heads

```sql
CREATE TABLE IF NOT EXISTS document_heads (
    vault_hash     TEXT NOT NULL REFERENCES vaults(vault_hash),
    doc_hash       TEXT NOT NULL,
    head_seq       INTEGER NOT NULL,
    operation_id   BLOB NOT NULL,
    entry_kind     TEXT NOT NULL CHECK (
        entry_kind IN ('full_snapshot', 'delete_tombstone')
    ),
    blob_id        TEXT,
    payload_hash   TEXT NOT NULL,
    source_device  TEXT NOT NULL,
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY(vault_hash, doc_hash)
);

CREATE INDEX IF NOT EXISTS idx_document_heads_seq
    ON document_heads(vault_hash, head_seq);
```

For `delete_tombstone`, `blob_id` là `NULL`. Tombstone head được giữ để device
re-bootstrap trên một local vault cũ không làm sống lại file đã xóa.

Upsert head chỉ xảy ra trong cùng transaction với mailbox insert:

```text
replace head only when excluded.head_seq > current.head_seq
```

Legacy mailbox row không có `operation_id` phải nhận synthetic deterministic ID
khi backfill head:

```text
legacy_operation_id =
    blake3("synabit-legacy-operation-v1" ||
           vault_hash || seq || doc_hash || payload_hash)[0..16]
```

Không generate random ID khi retry migration vì migration phải idempotent.

### 5.6. Opaque asset references

Để bootstrap head có thể kéo đúng asset và GC không xóa chunk đang được dùng,
outer push metadata cần thêm:

```rust
pub asset_refs: Vec<[u8; 32]>
```

Các ID này là keyed/opaque chunk IDs từ asset strategy, không phải plaintext
hash.

```sql
CREATE TABLE IF NOT EXISTS document_asset_refs (
    vault_hash  TEXT NOT NULL,
    doc_hash    TEXT NOT NULL,
    head_seq    INTEGER NOT NULL,
    chunk_id    TEXT NOT NULL,
    PRIMARY KEY(vault_hash, doc_hash, chunk_id)
);

CREATE INDEX IF NOT EXISTS idx_document_asset_chunk
    ON document_asset_refs(vault_hash, chunk_id);
```

Trong head transaction:

1. Xóa refs cũ của document.
2. Insert refs của full snapshot mới.
3. Delete tombstone để refs rỗng.

Giới hạn số refs trên một operation, ví dụ tối đa 4096, để tránh protocol/DB
abuse.

### 5.7. Bootstrap sessions

```sql
CREATE TABLE IF NOT EXISTS bootstrap_sessions (
    session_id      BLOB PRIMARY KEY,
    vault_hash      TEXT NOT NULL,
    device_id       TEXT NOT NULL,
    base_seq        INTEGER NOT NULL,
    incarnation_id  BLOB NOT NULL,
    item_count      INTEGER NOT NULL,
    total_bytes     INTEGER NOT NULL,
    state           TEXT NOT NULL CHECK (
        state IN ('active', 'completed', 'expired')
    ),
    created_at      INTEGER NOT NULL,
    expires_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS bootstrap_items (
    session_id      BLOB NOT NULL REFERENCES bootstrap_sessions(session_id),
    position        INTEGER NOT NULL,
    doc_hash        TEXT NOT NULL,
    head_seq        INTEGER NOT NULL,
    operation_id    BLOB NOT NULL,
    entry_kind      TEXT NOT NULL,
    blob_id         TEXT,
    payload_hash    TEXT NOT NULL,
    source_device   TEXT NOT NULL,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY(session_id, position),
    UNIQUE(session_id, doc_hash)
);

CREATE TABLE IF NOT EXISTS bootstrap_item_assets (
    session_id  BLOB NOT NULL,
    position    INTEGER NOT NULL,
    chunk_id    TEXT NOT NULL,
    PRIMARY KEY(session_id, position, chunk_id)
);
```

Bootstrap session materialize:

- All current `document_heads`.
- Current encrypted `trash_meta`.
- Opaque asset refs corresponding to each materialized head.

Nếu không muốn trộn trash vào `bootstrap_items`, thêm
`bootstrap_trash_items(session_id, position, ...)` và page riêng. Không được bỏ
trash metadata khỏi bootstrap contract.

### 5.8. Deferred blob deletion

```sql
CREATE TABLE IF NOT EXISTS blob_gc_queue (
    vault_hash   TEXT NOT NULL,
    blob_id      TEXT NOT NULL,
    blob_path    TEXT NOT NULL,
    not_before   INTEGER NOT NULL,
    attempts     INTEGER NOT NULL DEFAULT 0,
    last_error   TEXT,
    PRIMARY KEY(vault_hash, blob_id)
);
```

DB transaction chỉ enqueue blob không còn reference. Background worker:

1. Xóa file.
2. Nếu thành công hoặc file không tồn tại, xóa `blob_objects` và queue row.
3. Nếu lỗi, tăng attempts và retry.

Không dùng pattern hiện tại “delete DB rows rồi best-effort xóa file” mà không
có durable retry record.

### 5.9. Checkpoint seed cho dữ liệu legacy

Server không thể tái tạo payload đã bị cleanup trước migration. Backfill
`document_heads` từ mailbox còn lại chỉ là best effort; nó không chứng minh
server có full state của vault.

Thêm seed tables:

```sql
CREATE TABLE IF NOT EXISTS checkpoint_seeds (
    seed_id          BLOB PRIMARY KEY,
    vault_hash       TEXT NOT NULL,
    device_id        TEXT NOT NULL,
    start_seq        INTEGER NOT NULL,
    inventory_count  INTEGER NOT NULL DEFAULT 0,
    state            TEXT NOT NULL CHECK (
        state IN ('active', 'completed', 'aborted')
    ),
    created_at       INTEGER NOT NULL,
    expires_at       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS checkpoint_seed_items (
    seed_id    BLOB NOT NULL REFERENCES checkpoint_seeds(seed_id),
    doc_hash   TEXT NOT NULL,
    PRIMARY KEY(seed_id, doc_hash)
);
```

Protocol:

```text
BeginCheckpointSeed
AddCheckpointInventoryPage
CompleteCheckpointSeed
AbortCheckpointSeed
```

Seed flow:

1. User/operator chọn một trusted healthy device có local vault đầy đủ.
2. `BeginCheckpointSeed` capture `start_seq`, set vault `seeding`.
3. Client full-scan vault và upload inventory `doc_hash` theo page.
4. Client push full snapshot/tombstone mới cho mọi inventory item. Các operation
   dùng normal durable push path và phải có `seq > start_seq`.
5. Server verify:
   - số inventory items;
   - mỗi item có document head mới hơn `start_seq`;
   - mọi non-delete head có ready blob;
   - referenced asset chunks tồn tại hoặc có trạng thái pending rõ ràng.
6. `CompleteCheckpointSeed` set random `checkpoint_generation`, set
   `bootstrap_state = ready` và bump incarnation để mọi client lập SyncPlan mới.

Seed là thao tác authoritative và không được tự chạy từ device bất kỳ. UI/CLI
phải cảnh báo local-only data trên device khác có thể cần merge trước.

Head cũ không nằm trong authoritative inventory không được âm thầm gửi lại như
live document. Chọn một implementation và khóa bằng test:

- Recommended: materialize thành `delete_tombstone`/retired heads trong seed
  completion, với server-generated deterministic operation IDs; hoặc
- Bootstrap manifest được đánh dấu authoritative và client quarantine local
  extra files thay vì tự xóa.

Không được đơn giản bỏ head cũ khỏi snapshot mà không có client reconciliation
rule, vì stale local file có thể sống lại và được push lại.

Vault mới trên protocol v3 cũng hoàn tất một seed, kể cả inventory rỗng, trước
khi safe GC được phép chạy.

## 6. Durable push algorithm

### 6.1. Preflight

1. Validate protocol version, operation ID, payload size và asset ref count.
2. Verify `blake3(encrypted_payload) == payload_hash`.
3. Query `processed_operations`.
4. Nếu duplicate:
   - Verify `doc_hash/payload_hash/result_kind` khớp request cũ.
   - Trả original `seq`.
   - Nếu metadata không khớp, trả `OperationIdConflict`.

### 6.2. Blob preparation

1. Compute deterministic `blob_id`.
2. Nếu `blob_objects` đã có `ready` row và file hợp lệ, reuse.
3. Nếu chưa có:
   - Tạo temp file trong cùng filesystem/directory.
   - Write all ciphertext.
   - `sync_all` temp file.
   - Atomic rename temp → final.
   - `fsync` parent directory trên platform hỗ trợ.

Không ACK ở bước này.

### 6.3. SQLite transaction

Dùng `BEGIN IMMEDIATE`:

1. Re-check `processed_operations` để xử lý race.
2. Re-check quota bằng `vaults.committed_bytes`.
3. Insert/reuse `blob_objects`.
4. Atomically increment `vault_sequences`.
5. Insert mailbox row.
6. Insert processed operation ledger.
7. Upsert document head.
8. Replace `document_asset_refs`.
9. Increase `committed_bytes` nếu đây là physical blob mới.
10. Commit.

Batch push tiếp tục có per-item atomicity và trả per-item result. Không yêu cầu
toàn batch rollback nếu một item lỗi.

### 6.4. Post-commit

1. Notify subscribers.
2. Trả `PushOk` hoặc accepted batch result.
3. Nếu response bị mất, client retry và operation ledger trả original result.

### 6.5. Failure handling

- File write/rename fail: không tạo DB state; xóa temp best effort.
- DB transaction fail sau final blob: giữ orphan final blob; startup/periodic
  reconciler xóa sau grace period.
- Crash sau DB commit trước response: retry lấy result từ operation ledger.
- Race cùng operation ID: một transaction thắng; transaction còn lại đọc
  original result.
- Race quota: `BEGIN IMMEDIATE` + committed counter đảm bảo không overcommit.

## 7. Paged delta protocol

Thêm shared protocol version mới, ví dụ `MAILBOX_PROTOCOL_VERSION = 3`.

Server hiện yêu cầu message đầu tiên là `Auth`, nên không đổi đột ngột thành
`Hello -> Auth`. Append một variant mới ở cuối shared enum:

```rust
AuthV3 {
    version: u32,
    capabilities: u64,
    vault_hash: [u8; 32],
    mailbox_token: [u8; 32],
    device_id: String,
}
```

Server mới chấp nhận cả legacy `Auth` và `AuthV3` làm first message. Chỉ session
đã auth bằng `AuthV3` mới được dùng SyncPlan/bootstrap/paged pull. Không chèn
variant mới vào giữa postcard enum và phải có golden-byte compatibility
fixtures.

### 7.1. Sync plan

```rust
GetSyncPlan {
    client_incarnation_id: Option<[u8; 16]>,
    cursor: u64,
}

SyncPlan {
    incarnation_id: [u8; 16],
    head_seq: u64,
    compacted_through_seq: u64,
    mode: SyncMode,
}

enum SyncMode {
    Delta { until_seq: u64 },
    BootstrapRequired,
}
```

Server trả `BootstrapRequired` nếu:

- Incarnation mismatch.
- Vault `bootstrap_state != ready`; trong lúc `seeding/degraded`, trả typed
  `BootstrapUnavailable` thay vì tạo session không đầy đủ.
- Device state không phải active.
- Cursor nhỏ hơn `compacted_through_seq`.
- Cursor lớn hơn server `head_seq`.
- Server/operator đánh dấu vault cần rebuild.

### 7.2. Delta page

```rust
PullPage {
    after_seq: u64,
    until_seq: u64,
    max_entries: u16,
    max_bytes: u32,
}

PullPageResult {
    entries: Vec<MailboxEntryV3>,
    next_seq: u64,
    until_seq: u64,
    has_more: bool,
}
```

Rules:

- `until_seq` lấy từ `SyncPlan`, cố định trong cả pull cycle.
- Query: `seq > after_seq AND seq <= until_seq ORDER BY seq LIMIT ...`.
- DB chỉ trả row metadata; handler đọc blob bằng async filesystem I/O.
- Dừng page trước khi tổng ciphertext vượt `max_bytes`.
- Cho phép một entry đơn lẻ vượt default page bytes nhưng không vượt hard
  protocol entry limit.
- `next_seq` là sequence cuối thực sự có trong page, không phải `until_seq`.
- Empty page với gaps được phép; nếu không còn row đến `until_seq`, trả
  `next_seq = until_seq`.
- Include `operation_id` và `entry_kind` trong remote entry để client durable
  inbox có stable identity.

Client persist `next_seq` chỉ sau khi page đã durable/apply. Nếu connection rớt,
request lại cùng page và dedupe bằng `operation_id`.

## 8. Bootstrap protocol

### 8.1. Begin

```rust
BeginBootstrap {
    page_max_entries: u16,
    page_max_bytes: u32,
}

BootstrapStarted {
    session_id: [u8; 16],
    incarnation_id: [u8; 16],
    base_seq: u64,
    item_count: u64,
    total_bytes: u64,
    expires_at: i64,
}
```

Trong một SQLite transaction:

1. Capture `base_seq = vault_sequences.seq`.
2. Create session.
3. Copy all `document_heads` vào `bootstrap_items`.
4. Copy matching `document_asset_refs`.
5. Copy current trash metadata.
6. Set device state `bootstrapping`.
7. Commit.

Materialization là bắt buộc. Chỉ page trực tiếp từ mutable
`document_heads` sẽ bị race khi document update giữa các page.

Session TTL đề xuất: 24 giờ. Client hoạt động có thể gửi `KeepBootstrapAlive`,
server chỉ gia hạn tối đa một absolute lifetime, ví dụ 72 giờ.

### 8.2. Page

```rust
PullBootstrapPage {
    session_id: [u8; 16],
    after_position: u64,
    max_entries: u16,
    max_bytes: u32,
}

BootstrapPage {
    items: Vec<BootstrapItem>,
    next_position: u64,
    has_more: bool,
}
```

Page rules giống delta: bounded entries + bytes, async blob read, stable
position.

### 8.3. Client staging/apply

Client thêm local tables:

```sql
sync_bootstrap_sessions
sync_bootstrap_items
sync_inbox_operations
```

Flow:

1. Persist server session metadata.
2. Download page.
3. Verify payload hashes.
4. Persist encrypted items/local inbox transactionally.
5. Persist next position.
6. Sau khi đủ `item_count`, apply bằng core apply path hiện tại.
7. Asset references tiếp tục dùng `sync_pending_assets`; durable enqueue được
   coi là receipt, nhưng bootstrap chỉ complete khi policy đã chốt:
   - Recommended: document metadata apply xong; asset chunks có thể pending.
   - UI phải hiển thị asset pending, không full success.
8. Persist local `incarnation_id` và `base_seq`.
9. Gửi `CompleteBootstrap`.
10. Pull delta `seq > base_seq`.

Không clear local unsynced change chỉ vì bootstrap. Trước bootstrap, client phải:

- Detect/stage local changes vào durable outbox; hoặc
- Apply remote head qua conflict-safe merge.

Không được overwrite trực tiếp vault bằng snapshot server.

### 8.4. Complete

```rust
CompleteBootstrap {
    session_id: [u8; 16],
    applied_base_seq: u64,
}

BootstrapCompleted {
    next_delta_from: u64,
}
```

Server transaction:

- Validate session/device/vault/incarnation.
- Require `applied_base_seq == session.base_seq`.
- Set device `active`.
- Set `last_bootstrap_seq` và `last_acked_seq` ít nhất bằng base.
- Mark session completed.

Session items chỉ bị GC sau grace period để duplicate Complete/retry vẫn
idempotent.

## 9. GC và retention policy mới

### 9.1. Bỏ hard TTL destructive

Không gọi `gc_old_entries` theo kiểu xóa mọi unACKed row quá tuổi. Thay bằng:

1. Mark stale device thành `bootstrap_required`.
2. Loại device đó khỏi active ACK quorum.
3. Chỉ GC log khi checkpoint/head coverage đã tồn tại.

### 9.2. Active GC boundary

```text
active_min_ack =
    MIN(last_acked_seq của devices state='active')

safe_log_seq =
    MIN(active_min_ack, current_head_seq)
```

Nếu không có active device, server vẫn có thể GC log cũ sau minimum log
retention vì document heads giữ bootstrap state.

Recommended policy:

- Minimum log retention: 7 ngày kể cả đã ACK.
- Device stale threshold: 30 ngày không seen.
- Bootstrap session TTL: 24 giờ.
- Completed session grace: 24 giờ.
- Processed operation ledger: giữ suốt lifetime vault.
- Delete tombstone heads: giữ cho tới khi có explicit vault compaction
  generation; không age-delete trong implementation đầu.

### 9.3. Mailbox row GC transaction

Trong transaction:

1. Chọn rows `seq <= safe_log_seq` và cũ hơn minimum log retention.
2. Xóa mailbox rows.
3. Với mỗi referenced blob, kiểm tra còn được dùng bởi:
   - mailbox rows khác;
   - document_heads;
   - active/completed-grace bootstrap_items.
4. Nếu không còn reference, enqueue `blob_gc_queue`.
5. Update `compacted_through_seq` thành sequence lớn nhất vừa compact.
6. Commit.

### 9.4. Asset GC

Một asset chunk chỉ được GC nếu:

- Không có `document_asset_refs`.
- Không có `bootstrap_item_assets`.
- Cũ hơn orphan/replace grace period, đề xuất 30 ngày.

Trong thời gian rollout chưa backfill/ref tracking hoàn chỉnh, asset GC phải
disabled.

### 9.5. Device state transition

Cleanup không tự xóa device row. Nó chuyển:

```text
active + last_seen quá stale threshold -> bootstrap_required
```

Lần auth tiếp theo, `GetSyncPlan` bắt bootstrap. Đây là cơ chế bounded retention
mà không gây permanent divergence.

## 10. Startup reconciliation và repair

Server phải chạy reconciliation trước khi readiness trả success.

### 10.1. Temp/orphan files

- Xóa `.tmp_*` cũ hơn một giờ.
- Scan final blob files không có `blob_objects` row.
- Chỉ xóa orphan final blob sau grace period, ví dụ 24 giờ.
- Không xóa file mới chỉ vì một push concurrent chưa commit.

### 10.2. Missing/corrupt referenced blob

Với mỗi `blob_objects state='ready'` đang được head/bootstrap/mailbox tham chiếu:

- File phải tồn tại.
- Size phải khớp.
- Payload hash có thể verify theo incremental/budgeted scan.

Nếu thiếu/hỏng:

- Mark `state='corrupt'`.
- Readiness/metrics báo degraded.
- Pull trả typed `BlobUnavailable`, không trả empty payload.
- Không advance client cursor qua entry lỗi.
- Không tự xóa head hoặc giả lập tombstone.

### 10.3. Usage and sequence repair

- Recompute `vaults.committed_bytes` từ unique ready blob/assets.
- Đảm bảo `vault_sequences.seq >= MAX(mailbox.seq, document_heads.head_seq,
  processed_operations.seq, device.last_acked_seq)`.
- Sequence gap cũ được chấp nhận; sequence chỉ cần monotonic, không cần
  contiguous.
- Rebuild missing `document_heads` từ mailbox latest-per-doc trong migration
  trước khi bật GC.

### 10.4. Operator repair commands

Thêm CLI hoặc admin subcommands offline:

```text
sync-server repair verify
sync-server repair rebuild-heads
sync-server repair reconcile-blobs
sync-server repair recompute-usage
sync-server repair bump-incarnation
```

Commands mặc định dry-run; mutation yêu cầu explicit flag.

## 11. Backup và restore contract

SQLite DB và blob directory là một logical backup unit.

### Backup

Recommended:

1. Quiesce writes hoặc bật maintenance/read-only mode.
2. Dùng SQLite backup API/WAL checkpoint để tạo DB snapshot.
3. Copy/hardlink immutable ready blobs.
4. Ghi backup manifest gồm:
   - schema version;
   - backup timestamp;
   - per-vault incarnation/head seq;
   - DB checksum;
   - blob count/total bytes.
5. Resume writes.

### Restore

Sau restore từ một point-in-time cũ:

1. Run migrations/reconciliation.
2. Bump `incarnation_id` cho vault được restore.
3. Start server.
4. Client thấy incarnation mismatch và bootstrap.

Không giữ incarnation cũ khi server sequence đã rollback; nếu giữ, client có
cursor cao hơn server và sẽ bỏ qua dữ liệu.

## 12. Protocol compatibility và rollout

### Rollout flags

```text
mailbox_v3_accept
mailbox_v3_bootstrap
mailbox_v3_paged_pull
mailbox_v3_safe_gc
```

### Thứ tự deploy

1. **Safety patch**
   - Disable hard-TTL mailbox deletion.
   - Disable asset age GC.
   - Add metrics cho backlog/storage.
2. **Server additive schema**
   - Apply versioned migrations.
   - Backfill operation ledger, heads, devices, usage.
   - Keep old protocol behavior.
3. **Server v3 protocol**
   - Durable push path.
   - SyncPlan, paged pull, bootstrap endpoints.
   - Safe GC vẫn off.
4. **Client release**
   - Store incarnation.
   - Paged delta.
   - Bootstrap staging/apply/retry.
5. **Checkpoint seed**
   - Chọn trusted device cho từng legacy vault.
   - Full seed và verify.
   - Chỉ vault `bootstrap_state=ready` được canary GC.
6. **Observe**
   - Head coverage 100%.
   - No missing blobs.
   - Bootstrap integration tests pass in production-like environment.
7. **Enable safe GC**
   - Canary vaults first.
   - Expand gradually.

### Rollback

- Trước khi safe GC bật: rollback server/client code, schema additive giữ lại.
- Sau khi safe GC bật: old clients không được quay lại unbounded pull path nếu
  cursor dưới `compacted_through_seq`; server phải trả explicit
  upgrade/bootstrap required.
- Không drop old columns/tables trong cùng release triển khai v3.

## 13. Migration plan

### Migration 001 — Version ledger và additive tables

- Create `schema_migrations`.
- Add vault incarnation/usage/compaction/bootstrap fields.
- Create devices, processed operations, blob objects, document heads,
  bootstrap/seed tables và GC queue.

### Migration 002 — Backfill

Trong maintenance mode hoặc theo bounded batches:

1. `processed_operations` từ mailbox rows có operation ID.
   Legacy row không có operation ID vẫn được dùng để backfill head với synthetic
   deterministic ID; không thêm vào processed ledger nếu chưa từng nhận client
   operation ID.
2. `devices` từ cursors:
   - Existing recently seen devices → active.
   - Old devices → bootstrap_required.
3. `blob_objects` từ unique current mailbox/assets paths.
4. `document_heads` từ latest mailbox row per `(vault, doc_hash)`.
5. `committed_bytes` từ unique physical blobs/assets.
6. `incarnation_id` cho mọi vault.
7. Mọi legacy vault bắt đầu ở `bootstrap_state = not_ready`, kể cả khi backfill
   tìm thấy heads; chỉ explicit checkpoint seed mới chuyển `ready`.

### Migration 003 — Verification

Không bật v3/safe GC nếu:

- Có live mailbox row trỏ file thiếu.
- Một non-delete head không có ready blob.
- Head seq lớn hơn vault sequence.
- Duplicate operation ID có metadata khác nhau.
- Usage counter không khớp scan.

Migration phải log count/bytes, hỗ trợ resume và không dùng một transaction rất
lớn cho toàn bộ multi-vault database.

## 14. Work breakdown để giao dev

Ước lượng tổng: khoảng **29–35 dev-days**, phù hợp hai sprint với 3–4 dev làm
song song. Không nên ép toàn bộ feature vào một dev/sprint vì bootstrap phải
được chứng minh bằng crash/failure tests trước khi bật destructive GC.

| Ticket | Nội dung | Owner gợi ý | Estimate | Depends on |
|---|---|---:|---:|---|
| MBD-00 | Safety patch: tắt hard TTL/asset GC, thêm backlog metrics | Server dev | 0.5d | — |
| MBD-01 | Shared protocol v3 types, limits, compatibility fixtures | Protocol dev | 1.5d | — |
| MBD-02 | Versioned migrations + additive schema | DB dev | 2.5d | — |
| MBD-03 | Backfill/verification + legacy checkpoint seed tooling | DB/client dev | 3d | MBD-02 |
| MBD-04 | Durable blob writer + startup orphan reconciliation | Server dev | 2.5d | MBD-02 |
| MBD-05 | Transactional push/idempotency/quota/head update | Server/DB dev | 3d | MBD-02, MBD-04 |
| MBD-06 | Paged delta repository + protocol handlers | Server dev | 2d | MBD-01, MBD-05 |
| MBD-07 | Bootstrap materialization + page handlers | Server/DB dev | 3d | MBD-01, MBD-05 |
| MBD-08 | Client SyncPlan + paged delta + durable inbox | Client Rust dev | 3d | MBD-01, MBD-06 |
| MBD-09 | Client bootstrap staging/apply/resume | Client Rust dev | 4d | MBD-07, MBD-08 |
| MBD-10 | Device lifecycle + safe GC + blob GC queue | Server/DB dev | 3d | MBD-03, MBD-07 |
| MBD-11 | Opaque asset ref tracking + safe asset GC | Asset/server dev | 2d | MBD-05, asset strategy |
| MBD-12 | Observability, health/readiness, repair commands | Server dev | 2d | MBD-04, MBD-10 |
| MBD-13 | Fault-injection/integration/load test suite | QA + devs | 4d | MBD-05–MBD-12 |
| MBD-14 | Backup/restore runbook + restore drill | Infra/server dev | 1.5d | MBD-12 |

### Team split gợi ý

- **Dev A — Persistence:** MBD-02, 03, 04, hỗ trợ 05.
- **Dev B — Mailbox server:** MBD-00, 05, 06, 07, 10.
- **Dev C — Client Rust:** MBD-01, 08, 09.
- **Dev D/QA — Asset/operations:** MBD-11, 12, 13, 14.

MBD-10 safe GC chỉ được merge behind disabled flag cho tới khi MBD-13 pass.

## 15. Test strategy

### 15.1. Database unit tests

- Sequence allocation + mailbox insert rollback cùng nhau.
- Concurrent pushes tạo strictly increasing unique seq.
- Duplicate operation trả original seq.
- Duplicate operation với payload khác trả conflict.
- Head luôn trỏ latest seq.
- Delete tạo tombstone head.
- Quota counter không double-count reused blob.
- Device state transition và active min ACK.
- Backfill deterministic và idempotent.

### 15.2. Blob durability tests

Dùng failpoint tại:

1. Sau temp write.
2. Sau file `sync_all`.
3. Sau atomic rename.
4. Sau DB transaction begin.
5. Sau sequence allocation.
6. Sau mailbox insert.
7. Sau head upsert.
8. Sau DB commit, trước response.

Sau mỗi injected crash:

- Restart server.
- Run reconciliation.
- Retry operation.
- Assert đúng một processed operation/result.
- Assert head đúng.
- Assert không ACK state thiếu blob.
- Assert orphan được dọn hoặc reuse đúng.

### 15.3. Bootstrap consistency tests

- Empty vault.
- 10.000 document heads, nhiều page.
- Concurrent update trước/giữa/sau `BeginBootstrap`.
- Delete trong lúc bootstrap.
- Asset head bị thay thế trong lúc bootstrap.
- Client crash sau từng page và resume.
- Session expiry giữa chừng.
- Duplicate `CompleteBootstrap`.
- Bootstrap xong rồi delta replay hội tụ đúng state.
- Local unsynced edit không bị overwrite âm thầm.
- Legacy vault thiếu history không được đánh dấu ready trước checkpoint seed.
- Seed thiếu một inventory head phải fail.
- Trusted seed completion bump generation/incarnation đúng một lần.

### 15.4. Paged pull tests

- Entry count limit.
- Byte limit.
- Một entry lớn hơn default page nhưng dưới hard limit.
- Sequence gaps.
- Empty page tới high watermark.
- Connection drop giữa page.
- Duplicate page application idempotent.
- Backlog lớn không làm memory tăng theo toàn backlog.

### 15.5. GC tests

- Active device chưa ACK giữ log.
- Stale device chuyển bootstrap_required rồi log được GC.
- Revoked device không giữ quorum.
- Document head blob không bị GC sau mailbox deletion.
- Active bootstrap session pin old head/blob khi document đã update.
- Expired session release pin.
- Referenced asset chunk không bị xóa.
- Unreferenced asset chỉ xóa sau grace.
- Blob delete failure được retry qua queue.

### 15.6. Restore tests

- Restore consistent backup cùng incarnation/head.
- Restore point-in-time cũ rồi bump incarnation.
- Client cursor lớn hơn restored head bắt bootstrap.
- Missing blob làm readiness degraded và không advance cursor.

### 15.7. Load targets

Tối thiểu:

- 100.000 mailbox entries/vault trong test database.
- 10.000 current document heads.
- 10 concurrent devices/vault.
- 128-entry/8-MiB pages.
- Server RSS không tăng theo toàn backlog.
- Push p95 không bị `SUM(blob_size)` scan toàn vault.

Không đặt latency SLO tuyệt đối trước khi có baseline; PR phải ghi lại benchmark
trước/sau trên cùng fixture.

## 16. Observability

### Metrics

- `mailbox_push_total{result}`
- `mailbox_idempotency_hit_total`
- `mailbox_operation_conflict_total`
- `mailbox_push_transaction_seconds`
- `mailbox_pull_page_entries`
- `mailbox_pull_page_bytes`
- `mailbox_pull_page_seconds`
- `mailbox_backlog_entries{vault_tag}`
- `mailbox_device_lag_seq{vault_tag,device_tag}`
- `mailbox_bootstrap_active`
- `mailbox_bootstrap_started_total`
- `mailbox_bootstrap_completed_total`
- `mailbox_bootstrap_expired_total`
- `mailbox_bootstrap_bytes`
- `mailbox_gc_entries_total`
- `mailbox_gc_blobs_pending`
- `mailbox_orphan_blobs`
- `mailbox_missing_blobs`
- `mailbox_corrupt_blobs`
- `mailbox_storage_committed_bytes`
- `mailbox_storage_quota_rejections_total`

Vault/device labels phải là short hash tag, không raw identifier.

### Structured logs

Mọi push/pull/bootstrap/GC record:

- request/run ID;
- redacted vault/device tag;
- operation/session ID;
- sequence/base/high watermark;
- page count/bytes;
- result/error class;
- duration.

Không log mailbox token, encrypted payload, plaintext path hoặc asset plaintext
hash.

### Readiness

Readiness false khi:

- Migration chưa hoàn thành.
- Startup critical reconciliation chưa xong.
- Có referenced blob missing/corrupt vượt configured threshold.
- DB không ghi được.
- Data directory không writable.

Health/liveness vẫn true nếu process còn chạy để orchestrator không restart loop
vô hạn vì một vault corrupt.

## 17. Definition of Done

Feature chỉ được coi là hoàn thành khi:

- Sequence allocation, mailbox insert, operation ledger và head update cùng một
  SQLite transaction.
- Response chỉ gửi sau blob durable + DB commit.
- Duplicate operation vẫn idempotent sau mailbox GC.
- Mỗi live/delete document có một durable head.
- Legacy vault đã chạy verified checkpoint seed trước khi bật safe GC.
- Device mới bootstrap từ materialized session rồi replay delta không gap.
- Pull/bootstrap đều bounded theo entry count và bytes.
- Client resume được sau crash giữa bootstrap/page.
- Hard TTL không còn xóa unACKed entry một cách mù quáng.
- Stale device quay lại được ép bootstrap.
- GC không xóa head blob, session-pinned blob hoặc referenced asset.
- Startup reconciliation phát hiện orphan/missing/corrupt blobs.
- Restore cũ bump incarnation và ép client bootstrap.
- Server/client protocol compatibility fixtures pass.
- Fault-injection suite pass ở mọi durability boundary.
- Load fixture chứng minh memory không tăng theo toàn backlog.
- Backup/restore drill được thực hiện ít nhất một lần.
- Safe GC vẫn có kill switch và được canary trước khi bật mặc định.

## 18. PR review checklist

- [ ] Không có sequence update ngoài transaction chứa mailbox insert.
- [ ] Không ACK trước DB commit.
- [ ] Operation ledger không FK vào GC-able mailbox row.
- [ ] Duplicate operation trả original seq/result.
- [ ] Head upsert và asset refs cùng transaction với operation.
- [ ] Delete luôn tạo/giữ tombstone head.
- [ ] Bootstrap materialize immutable items tại một `base_seq`.
- [ ] Concurrent push nằm hoặc snapshot hoặc delta, không rơi giữa.
- [ ] Page limit được clamp phía server.
- [ ] Blob read dùng async/bounded path, không gom toàn backlog.
- [ ] Cursor/incarnation được persist cùng nhau phía client.
- [ ] Bootstrap staging survives process restart.
- [ ] GC query kiểm tra mọi reference/pin.
- [ ] File deletion có durable retry queue.
- [ ] Asset GC disabled cho tới khi opaque refs verified.
- [ ] Migration/backfill có resume và verification.
- [ ] Restore runbook bump incarnation.
- [ ] Logs/metrics không lộ secret hoặc plaintext metadata.
- [ ] Failure-path tests tồn tại, không chỉ happy path.
