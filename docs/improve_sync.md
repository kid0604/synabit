# Synabit Sync — Kế hoạch hoàn thiện

## 1. Mục đích tài liệu

Tài liệu này là execution plan độc lập để đội dev đưa toàn bộ tính năng đồng bộ
của Synabit từ trạng thái development hiện tại lên mức có thể phát hành
production. Mọi quyết định kiến trúc, contract, thứ tự triển khai, acceptance
criteria và release gate cần thiết đều nằm trong file này. Plan bao phủ:

- Sync core trên client;
- local database và migration;
- document identity, CRDT, delete/conflict;
- binary/large asset;
- Synabit Sync Server, protocol, bootstrap và durability;
- Google Drive provider;
- device lifecycle và secret storage;
- UI/UX, metrics và error reporting;
- automated test, fault injection, rollout và release gate.

Dev không cần đọc thêm một plan phụ nào để thực thi. Nếu thay đổi một quyết định
trong file này, PR phải cập nhật trực tiếp file này cùng code.

## 2. Kết quả cần đạt

Sau khi hoàn thành plan:

1. Mỗi vault có identity và state sync độc lập; không có dữ liệu, cursor, CRDT,
   pending job hoặc provider namespace dùng chung ngoài ý muốn.
2. Mỗi local change được ghi vào durable outbox trước khi gửi và chỉ được đánh
   dấu hoàn thành sau remote ACK.
3. Retry cùng một operation là idempotent; một lần edit/delete/recreate mới luôn
   có operation ID mới.
4. Cursor chỉ tiến sau khi remote entry đã được apply hoặc staged bền vững ở
   local database.
5. Synabit Server có handshake/capability rõ ràng, paged pull bounded, bootstrap
   resumable và durable ACK.
6. Asset chỉ được publish reference sau khi toàn bộ chunks tồn tại; download
   được verify và atomic apply.
7. Google Drive không dùng wall-clock timestamp làm correctness cursor.
8. Conflict, partial success, pending asset, quota và provider connection được
   hiển thị đúng theo trạng thái backend.
9. Device revocation không đưa ra cam kết bảo mật vượt quá implementation thực
   tế.
10. Bộ test tái hiện được restart, retry, corruption, concurrent edit,
    multi-vault và backlog lớn.

## 3. Phạm vi và nguyên tắc triển khai

### 3.1. Trong phạm vi

- Breaking local DB migration có backup và rollback path.
- Breaking remote protocol nếu có explicit version negotiation.
- Layout Google Drive mới theo từng vault.
- Giữ khả năng đọc dữ liệu legacy trong một chu kỳ phát hành.
- Tắt hoặc ẩn feature chưa hoàn thiện thay vì trả success giả.
- Thêm bảng/journal cần thiết để correctness không phụ thuộc filesystem scan.

### 3.2. Ngoài phạm vi

- Real-time collaborative editing ở cấp ký tự qua server.
- Peer-to-peer direct transport v2. Direct P2P v1 phải bị tắt trong release
  build; chỉ managed Sync Server và Google Drive nằm trong release scope này.
- Cross-vault deduplication.
- Tự động xóa dữ liệu đã tồn tại trên thiết bị bị revoke.
- Delta encoding bên trong binary/video.
- Upload nhiều chunks song song. Bản production đầu tiên dùng concurrency 1 để
  giữ memory và retry semantics đơn giản.

### 3.3. Quy tắc bắt buộc

- Không merge một PR làm thay đổi wire format nếu chưa có fixture compatibility
  test.
- Không `unwrap`, `expect` hoặc `todo!()` trên request path, file từ remote hoặc
  dữ liệu có thể hỏng.
- Không swallow lỗi migration, cursor, ACK, decrypt, deserialize, file write
  hoặc pending queue.
- Không update acknowledged hash/path/cursor trước durable commit.
- Không fallback protocol âm thầm. Fallback chỉ được phép nếu handshake xác
  nhận server hỗ trợ legacy và client đang ở migration mode có chủ đích.
- Một test fail hoặc warning bị nâng thành error là release blocker, không được
  đánh dấu ignored để qua gate.

## 4. Invariant thiết kế

Mọi implementation và review phải kiểm tra các invariant này trước khi đánh
dấu ticket hoàn thành.

### S1 — Vault isolation

Mọi key local phải có `vault_id`. Mọi namespace remote phải derive từ cả vault
identity và secret:

```text
remote_vault_id = keyed_blake3(
    derive_key("synabit-remote-vault-id-v1", vault_master_key),
    local_vault_uuid
)
```

Không derive remote vault chỉ từ master key. Hai vault dùng cùng recovery key
vẫn phải tạo hai namespace khác nhau.

### S2 — Durable local intent

Filesystem change phải được chuyển thành row trong `sync_outbox` trước khi gửi.
Outbox chứa operation ID, payload metadata, source content hash và trạng thái
retry. App restart không được làm mất intent.

### S3 — Event identity

`operation_id` nhận diện một event, không nhận diện content:

- Tạo UUID/random 128-bit mới khi phát hiện một local event mới.
- Retry tái sử dụng operation ID đã lưu trong outbox.
- Content hash nằm ở trường riêng.
- Delete lần hai và recreate cùng content phải có ID mới.

### S4 — Durable remote ACK

Server chỉ ACK sau khi blob và metadata transaction đã durable. Google Drive
chỉ ACK local operation sau khi file/manifest đã được Drive xác nhận và operation
có thể được đọc lại bằng stable ID.

### S5 — Durable local receipt

Client chỉ tiến cursor sau một trong hai điều kiện:

- Entry đã apply và local DB/filesystem commit thành công; hoặc
- Entry đã được ghi vào durable inbox/pending table đủ dữ liệu để resume.

### S6 — Upload before reference

Asset reference chỉ được push sau khi mọi chunk đã được upload và xác nhận tồn
tại. Nếu một chunk fail, reference operation vẫn ở outbox và không được ACK.

### S7 — Safe apply

Mọi remote path, bao gồm document và asset, phải qua cùng một
`resolve_safe_path`. Final write dùng temp file, verify, optional fsync và atomic
rename. Không overwrite file tốt nếu verification fail.

### S8 — Bounded resource

- Pull bounded bởi cả số entry và tổng bytes.
- Bootstrap bounded theo page.
- Asset memory bounded theo chunk.
- Decompression có maximum plaintext size.
- Không gom toàn bộ backlog hoặc toàn bộ asset vào một `Vec`.

### S9 — Convergence

Khi local thắng conflict, local winner phải được publish lại hoặc vẫn ở dirty
state. Không được cập nhật acknowledged hash làm mất trigger reconciliation.

### S10 — Explicit capability

Client chỉ sử dụng asset, paged pull, bootstrap hoặc checkpoint khi server
handshake công bố capability tương ứng. Unsupported capability trả typed error
và UI giải thích cần update server/app.

### S11 — Safe garbage collection

Không xóa mailbox blob, document head, bootstrap item hoặc asset chunk nếu còn
được pin bởi:

- document head hiện tại;
- active bootstrap session;
- active device chưa ACK;
- pending local/remote reference.

Khi reference graph chưa hoàn chỉnh, ưu tiên không GC hơn là xóa theo tuổi.

### S12 — Honest UI state

UI phân biệt rõ:

- `idle`;
- `checking`;
- `pulling`;
- `applying`;
- `pushing`;
- `waiting_for_assets`;
- `partial`;
- `success`;
- `offline`;
- `error`;
- `upgrade_required`.

`lastSuccessfulSync` chỉ đổi khi không còn lỗi bắt buộc hoặc operation bị bỏ
qua. `lastAttemptedSync` được lưu riêng.

### 4.13. Asset contract đã chốt

#### Giới hạn

```rust
const ASSET_CHUNK_SIZE: usize = 4 * 1024 * 1024;
const INLINE_PAYLOAD_LIMIT: u64 = 8 * 1024 * 1024;
const MAX_SYNC_ASSET_SIZE: u64 = 2 * 1024 * 1024 * 1024;
```

Một file đi asset pipeline khi:

- không phải UTF-8 hợp lệ;
- thuộc nhóm attachment/binary theo MIME hoặc extension;
- hoặc lớn hơn `INLINE_PAYLOAD_LIMIT`.

File lớn hơn `MAX_SYNC_ASSET_SIZE` trả typed error, giữ trong local dirty state
và không được làm document nhỏ khác ngừng sync.

#### Payload

```rust
pub struct AssetSyncPayloadV2 {
    pub node_id: String,
    pub rel_path: String,
    pub asset: AssetRef,
}

pub struct AssetRef {
    pub version: u8,
    pub asset_id: [u8; 32],
    pub plaintext_hash: [u8; 32],
    pub plaintext_size: u64,
    pub chunk_size: u32,
    pub chunks: Vec<AssetChunkRef>,
    pub mime_type: Option<String>,
}

pub struct AssetChunkRef {
    pub index: u32,
    pub chunk_id: [u8; 32],
    pub plaintext_hash: [u8; 32],
    pub plaintext_size: u32,
}
```

Plaintext path, MIME và hashes chỉ nằm trong encrypted payload. Provider chỉ
nhìn thấy opaque remote vault ID, asset/chunk ID, ciphertext size và timing.

#### ID và key derivation

```text
asset_id_key = derive_key("synabit-asset-id-v1", vault_master_key)
asset_id = keyed_blake3(asset_id_key, plaintext_file_hash)

chunk_id_key = derive_key("synabit-chunk-id-v1", vault_master_key)
chunk_id = keyed_blake3(
    chunk_id_key,
    asset_id || chunk_index || plaintext_chunk_hash
)

asset_content_key = derive_key(
    "synabit-asset-content-v1",
    vault_master_key || asset_id
)
```

Không tái sử dụng mailbox token, remote vault ID hoặc cùng domain key cho các
mục đích trên.

#### Encryption và snapshot

- Hash file bằng streaming read.
- Mỗi chunk dùng XChaCha20-Poly1305 với nonce ngẫu nhiên.
- Nonce được prefix vào ciphertext.
- AAD chứa format version, asset ID, chunk index, chunk count và full plaintext
  size.
- Không compress asset chunk.
- Verify AEAD và plaintext chunk hash trước khi ghi.
- Sau khi ghép, verify lại full size và full plaintext hash.
- Trong memory chỉ giữ tối đa một upload chunk và một download chunk trên mỗi
  asset.
- Trước/sau các pass phải so sánh size + mtime và verify final hash. Nếu file
  thay đổi trong lúc chuẩn bị/upload, hủy reference operation và retry từ local
  dirty state.

#### Adapter API đích

```rust
#[async_trait]
pub trait SyncAdapter {
    async fn has_asset_chunk(
        &self,
        chunk_id: [u8; 32],
    ) -> AppResult<bool>;

    async fn push_asset_chunk(
        &self,
        chunk_id: [u8; 32],
        data: bytes::Bytes,
    ) -> AppResult<()>;

    async fn pull_asset_chunk(
        &self,
        chunk_id: [u8; 32],
    ) -> AppResult<Option<bytes::Bytes>>;
}
```

`Bytes` chỉ chứa một chunk, không bao giờ chứa cả asset.

#### Push sequence

1. Push document nhỏ độc lập.
2. Tạo/khôi phục asset job từ outbox.
3. Tính manifest bằng streaming read.
4. Check từng chunk trên provider.
5. Upload các chunk còn thiếu.
6. Re-check toàn bộ chunk tồn tại.
7. Chỉ lúc này chuyển reference operation sang `ready`.
8. Push reference.
9. Chỉ sau remote ACK mới commit acknowledged file hash.

#### Pull sequence

1. Decrypt và validate `AssetRef`.
2. Resolve safe target path.
3. Ghi durable `sync_pending_assets`.
4. Cursor có thể tiến sau khi pending row commit.
5. Download từng chunk vào `.synabit/tmp/<operation-id>.part`.
6. Verify AEAD/hash trong lúc ghi.
7. Verify full file.
8. Flush/fsync nếu platform hỗ trợ.
9. Atomic rename.
10. Update mapping/index/acknowledged state.
11. Mark pending row `applied`.

Operation mới hơn cùng document/asset đánh dấu job cũ `superseded` và dọn temp
file an toàn.

### 4.14. Wire compatibility đã chốt

- Rename payload hiện tại thành `DocSyncPayloadV1`.
- Client mới vẫn đọc legacy V1 inline binary nhưng không tạo thêm.
- Asset-aware payload dùng marker mã hóa mới; marker phải được kiểm tra trước
  deserialize.
- Client cũ gặp marker mới phải fail closed với `UpgradeRequired`, không ghi
  manifest bytes thành file.
- Protocol enum variant mới chỉ được append.
- Mỗi wire version có golden fixture encode/decode.
- Server hello công bố protocol version và capability; không suy ra capability
  từ server/app version string.
- Maximum document frame, pull page, compressed plaintext và asset chunk được
  enforce ở cả client lẫn server.

### 4.15. Server data model đã chốt

Server database tối thiểu gồm:

#### `vaults`

- `vault_hash`/remote vault ID;
- random `incarnation_id`;
- `compacted_through_seq`;
- `committed_bytes`;
- `bootstrap_state`: `not_ready`, `seeding`, `ready`, `degraded`;
- checkpoint generation.

#### `vault_sequences`

Một row bắt buộc cho mỗi vault. Sequence allocation và mailbox insert nằm trong
cùng `BEGIN IMMEDIATE` transaction.

#### `devices`

- composite key `(vault_hash, device_id)`;
- state: `bootstrap_required`, `bootstrapping`, `active`, `revoked`;
- per-device credential hash;
- last ACK/bootstrap sequence;
- joined/last-seen/stale/revoked timestamps.

#### `processed_operations`

Ledger unique `(vault_hash, operation_id)` lưu original sequence/result. Ledger
không bị xóa cùng mailbox history vì retry cũ vẫn phải idempotent.

#### `blob_objects`

Lưu blob ID, storage path, size, reference count/state và timestamps. Blob path
nên tương đối với configured data root để có thể move/restore server.

#### `mailbox_entries`

Lưu ordered delta metadata theo vault sequence. Payload tham chiếu blob object,
không duplicate raw data vào SQLite.

#### `document_heads`

Latest opaque full snapshot hoặc delete tombstone theo document hash. Server
không decrypt content.

#### `document_asset_refs`

Opaque reference graph từ document head tới asset chunks. Update cùng
transaction thay head.

#### `bootstrap_sessions` và `bootstrap_items`

Session có random ID, vault/device, `base_seq`, state, expiry và progress.
Items materialize đúng document heads của snapshot tại `base_seq`.

#### `blob_gc_queue`

Composite key `(vault_hash, blob_id)`, có retry count, next attempt và last
error. Chỉ remove row sau physical delete thành công.

#### `schema_migrations`

Migration version/name/applied timestamp. Chỉ ghi applied sau transaction thành
công; không ignore `ALTER`/`execute_batch` error.

### 4.16. Bootstrap state machine đã chốt

```text
bootstrap_required
        |
        v
BeginBootstrap -> downloading -> materializing
        |                |              |
        |                +----resume----+
        |
        +----expiry/abort----> bootstrap_required
                                     
materializing -> CompleteBootstrap -> active
                       |
                       v
             pull delta > base_seq
```

Rules:

- `base_seq` và head snapshot được chốt trong cùng DB transaction.
- Push concurrent commit trước snapshot nằm trong heads; commit sau snapshot có
  sequence lớn hơn `base_seq`.
- Client resume active session thay vì luôn tạo session mới.
- Keep-alive gia hạn bounded expiry.
- Mỗi page có stable position và payload hash.
- Client không giữ DB mutex qua decrypt/file apply.
- Complete chỉ xảy ra sau toàn bộ items durable/apply.
- Sau complete, client pull delta từ `base_seq` tới fixed high watermark.
- Incarnation mismatch buộc bỏ cursor và bootstrap lại.

### 4.17. Server cleanup và quota contract

- Safe GC chỉ chạy khi vault bootstrap state là `ready`.
- Mailbox entry chỉ GC tới watermark của active device ACK và không vượt
  checkpoint/bootstrap safety boundary.
- Document head, active bootstrap item và referenced asset luôn pin blob.
- Stale device chuyển `bootstrap_required` trước khi bị loại khỏi ACK quorum.
- Revoked device không tham gia quorum.
- `committed_bytes` update transactionally; duplicate operation/chunk không
  charge lần hai.
- Blob ghi thành công nhưng DB fail trở thành orphan có thể scan/recover sau
  grace period.
- Không age-GC asset nếu reference graph chưa được xác minh.
- Config retention không được tồn tại dưới dạng no-op.

## 5. Target architecture

```text
Filesystem watcher / manual sync
              |
              v
       change detector
              |
              v
   durable per-vault outbox
              |
              v
        SyncCoordinator
       /               \
      v                 v
document operations   asset jobs
      |                 |
      |          upload/verify chunks
      |                 |
      +---------> publish references
              |
              v
    capability-aware adapter
       /               \
      v                 v
GDrive Changes API   Sync Server V3
      |                 |
      v                 v
per-vault layout    durable blob + metadata
      \                 /
       \               /
        v             v
     paged remote inbox
              |
              v
    apply / pending assets
              |
              v
 ACK + cursor after durable commit
```

Local state được tổ chức thành ba lớp rõ ràng:

1. **Observed state**: hash/path/content hiện có trên filesystem.
2. **Pending state**: outbox, inbox, pending asset và retry metadata.
3. **Acknowledged state**: remote version/cursor/hash đã được provider xác nhận.

Không dùng một field `sync_hash` cho cả ba ý nghĩa.

## 6. Mô hình thực thi đề xuất

### 6.1. Nhân sự

Khuyến nghị tối thiểu:

- Dev A — client sync core, local DB, CRDT/apply;
- Dev B — protocol và Synabit Server;
- Dev C — asset và Google Drive adapter;
- Dev D — frontend/UI, integration harness và QA automation;
- QA có thể tham gia từ giữa Sprint 1.

Nếu chỉ có hai dev, giữ nguyên dependency/order nhưng không chạy song song các
ticket cùng sửa `coordinator.rs`, shared protocol hoặc schema.

### 6.2. Thời lượng

Khuyến nghị ba sprint, mỗi sprint hai tuần:

| Sprint | Release gate | Trọng tâm |
|---|---|---|
| 1 | Core correctness | Vault isolation, schema, journal, safe apply |
| 2 | Provider durability | Server V3, bootstrap, assets, bounded pull |
| 3 | Product readiness | Google Drive V2, UI contract, security, hardening |

Không phát hành production giữa các sprint. Có thể phát internal build sau mỗi
gate để chạy migration/fault-injection.

### 6.3. Quy tắc chia PR

- Mỗi PR phải giữ build xanh và có migration/test đi kèm.
- Shared protocol changes đi trước server/client implementation nhưng phải giữ
  old decode fixtures.
- Mỗi DB migration là một PR riêng hoặc commit độc lập có rollback notes.
- Không gộp UI polish với correctness change trong cùng PR.
- PR description bắt buộc ghi invariant nào bị ảnh hưởng.

---

## 7. Sprint 1 — Core correctness và vault isolation

### SYNC-001 — Tạo `VaultSyncContext`

**Mục tiêu:** mọi sync run mang một vault identity bất biến.

**Files chính:**

- `src-tauri/src/commands/sync_core.rs`
- `src-tauri/src/sync/coordinator.rs`
- `src-tauri/src/sync/core/types.rs`
- `src-tauri/src/db/schema.rs`
- `src/stores/useAppStore.ts`

**Implementation:**

1. Tạo `local_vault_id` là UUID 128-bit khi vault được tạo hoặc lần đầu được
   mở sau migration.
2. Lưu ID trong metadata bên trong vault, ví dụ `.synabit/vault.json`, và một
   mapping cache trong app DB.
3. Tạo struct:

   ```rust
   pub struct VaultSyncContext {
       pub vault_id: Uuid,
       pub vault_root: PathBuf,
       pub provider_id: String,
       pub remote_vault_id: [u8; 32],
       pub device_id: String,
   }
   ```

4. `sync_run`, coordinator, change detection, apply và adapter đều nhận context;
   không tự đọc “active vault” từ global state ở giữa một run.
5. Canonicalize vault root khi bắt đầu run. Nếu UI đổi vault, run cũ phải kết
   thúc trên context cũ hoặc bị cancel, không được chuyển target giữa chừng.

**Acceptance criteria:**

- Hai vault dùng cùng E2EE key có `vault_id` và `remote_vault_id` khác nhau.
- Đổi active vault trong lúc sync không làm operation apply sang vault mới.
- Không còn sync key nào chỉ dựa trên `provider_id` hoặc path tương đối.

**Tests:**

- Unit test remote ID derivation.
- Integration test mở A, sync, chuyển B, sync; B không phát tombstone cho A.
- Test symlink/canonical path của vault root.

### SYNC-002 — Local schema V2 có namespace theo vault

**Mục tiêu:** tách hoàn toàn CRDT, identity, cursor và pending state.

**Files chính:**

- `src-tauri/src/db/schema.rs`
- `src-tauri/src/db/sync.rs`
- `src-tauri/src/db/metrics.rs`
- các DAO đang dùng `document_paths`, `kv_store`, CRDT tables.

**Schema đích tối thiểu:**

```sql
CREATE TABLE sync_vaults (
    vault_id          TEXT PRIMARY KEY,
    canonical_path    TEXT NOT NULL,
    created_at        INTEGER NOT NULL,
    schema_version    INTEGER NOT NULL
);

CREATE TABLE sync_provider_state (
    vault_id          TEXT NOT NULL,
    provider_id       TEXT NOT NULL,
    remote_vault_id   BLOB NOT NULL,
    cursor_blob       BLOB,
    incarnation_id    BLOB,
    last_attempt_at   INTEGER,
    last_success_at   INTEGER,
    PRIMARY KEY(vault_id, provider_id)
);
```

Thêm `vault_id` vào primary/unique key của:

- `document_paths`;
- `crdt_documents`;
- `crdt_updates`;
- `sync_pending_assets`;
- bootstrap sessions/items;
- sync metrics nếu metrics cần phân tách vault;
- mọi acknowledged hash/version.

Không tiếp tục nhét structured sync state vào global `kv_store`.

**Migration:**

1. Bắt đầu transaction migration.
2. Tạo backup database có timestamp trước khi thay schema.
3. Xác định vault hiện tại. Dữ liệu legacy chỉ được gán vào vault này khi mapping
   chắc chắn; nếu không chắc, đặt provider state thành `bootstrap_required`.
4. Tạo bảng mới, copy, verify row counts và foreign keys.
5. Swap table.
6. Chỉ ghi schema version sau commit thành công.
7. Nếu bất kỳ bước nào lỗi, rollback và giữ DB cũ.

**Acceptance criteria:**

- Migration chạy lại không làm duplicate/corrupt.
- Không drop/recreate bảng `nodes` như một side effect của DB open.
- Mở vault mới không nhìn thấy document paths/cursor của vault cũ.
- Schema bootstrap và DAO dùng chính xác cùng tập column.

**Tests:**

- Fixture DB trước migration.
- Fresh DB.
- Migration bị kill/fault giữa từng bước.
- Hai vault có cùng relative path và node ID không collision.

### SYNC-003 — Durable outbox và operation event ID

**Mục tiêu:** không mất local change khi app/provider lỗi.

**Files chính:**

- `src-tauri/src/sync/core/change.rs`
- `src-tauri/src/sync/coordinator.rs`
- `src-tauri/src/db/sync.rs`
- `src-tauri/src/sync/core/types.rs`

**Schema:**

```sql
CREATE TABLE sync_outbox (
    vault_id           TEXT NOT NULL,
    provider_id        TEXT NOT NULL,
    operation_id       BLOB NOT NULL,
    doc_id              TEXT NOT NULL,
    entry_kind          TEXT NOT NULL,
    rel_path            TEXT,
    source_hash         BLOB,
    payload_blob        BLOB,
    payload_hash        BLOB,
    state               TEXT NOT NULL,
    retry_count         INTEGER NOT NULL DEFAULT 0,
    next_retry_at       INTEGER,
    last_error          TEXT,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    PRIMARY KEY(vault_id, provider_id, operation_id)
);
```

States:

```text
prepared -> uploading_assets -> ready -> sent -> acknowledged
                                      \-> failed -> ready
```

**Implementation:**

1. Tạo random operation ID khi observed content khác acknowledged content.
2. Persist outbox row trước network I/O.
3. Retry dùng lại row và operation ID.
4. Chỉ cập nhật acknowledged hash/path sau ACK tương ứng.
5. Xóa/compact acknowledged rows theo retention; không xóa ngay trong cùng
   transaction để còn debug/audit.
6. Không clone toàn bộ encrypted payload để lookup ACK; dùng metadata map hoặc
   query outbox theo operation ID.

**Acceptance criteria:**

- Kill app sau prepare, run sau vẫn push operation.
- Retry không tạo operation ID mới.
- Edit A → delete → recreate A tạo ba operation ID khác nhau.
- Provider reject không làm acknowledged hash tiến.

### SYNC-004 — Durable inbox, page transaction và cursor

**Mục tiêu:** cursor không thể vượt qua dữ liệu chưa apply/stage.

**Schema:**

```sql
CREATE TABLE sync_inbox (
    vault_id           TEXT NOT NULL,
    provider_id        TEXT NOT NULL,
    remote_position    TEXT NOT NULL,
    operation_id       BLOB NOT NULL,
    doc_hash           BLOB NOT NULL,
    entry_kind         TEXT NOT NULL,
    encrypted_payload  BLOB,
    payload_hash       BLOB,
    source_device      TEXT,
    state               TEXT NOT NULL,
    last_error          TEXT,
    received_at         INTEGER NOT NULL,
    applied_at          INTEGER,
    PRIMARY KEY(vault_id, provider_id, operation_id)
);
```

**Implementation:**

- Pull từng page, verify outer bounds/hash và persist inbox trong một DB
  transaction.
- Apply từ inbox; một bad entry không làm mất các entry còn lại.
- Cursor page chỉ commit khi mọi entry đã `applied`, `pending_asset` hoặc
  `superseded` bền vững.
- Unknown delete vào `pending_delete`, không log rồi bỏ.
- Không gom mọi page vào một `Vec`.

**Acceptance criteria:**

- 100.000 remote entries không làm RAM tăng tuyến tính.
- Crash sau persist page nhưng trước apply sẽ resume từ inbox.
- Malformed entry không làm cursor tiến qua entry đó.
- Pull page network error trả partial/degraded result, không trả success.

### SYNC-005 — Sửa apply, conflict và safe path

**Files chính:**

- `src-tauri/src/sync/core/apply.rs`
- `src-tauri/src/sync/core/crdt.rs`
- `src-tauri/src/sync/core/identity.rs`

**Tasks:**

1. Dùng một `resolve_safe_path()` cho document, asset, temp file và rename.
2. Reject absolute path, `..`, invalid component và symlink escape.
3. Temp filename chứa random operation ID; không dùng tên `.tmp` cố định.
4. Verify rồi optional fsync file, atomic rename, optional fsync directory.
5. Chỉ update document mapping/index/hash sau final rename thành công.
6. JSON local-winner giữ dirty/outbox state để publish reconciliation.
7. Markdown write failure phải trả error; không ghi success rồi cập nhật hash.
8. Unknown tombstone được lưu durable cho tới khi resolve hoặc được chứng minh
   superseded.
9. Thay timestamp string comparison bằng typed HLC/version structure.
10. Validate embedded node ID đúng UUID; identity injection dùng atomic write.

**Acceptance criteria:**

- Remote `../../x` không thể ghi ngoài vault.
- File tốt không bị overwrite khi decrypt/hash/write fail.
- Hai device hội tụ sau JSON conflict.
- Delete tới trước create/path mapping vẫn được áp dụng sau khi mapping có.
- Rename/update mapping là một transaction logic, không để stale path.

### Sprint 1 release gate

- Multi-vault isolation tests pass.
- Local migration tests pass trên fresh và legacy fixtures.
- Delete/recreate regression pass.
- JSON/Markdown two-device convergence pass.
- Cursor crash-resume tests pass.
- Không còn update acknowledged state trước ACK.
- Server/GDrive production toggle vẫn giữ off nếu Sprint 2/3 chưa hoàn tất.

---

## 8. Sprint 2 — Synabit Server, bootstrap và asset durability

### PROTO-001 — Version/capability handshake

**Files chính:**

- `sync-server/synabit-protocol/src/lib.rs`
- `sync-server/src/mailbox.rs`
- `src-tauri/src/sync/adapter/server.rs`

**Protocol response tối thiểu:**

```rust
pub struct ServerHello {
    pub protocol_version: u16,
    pub server_incarnation: [u8; 16],
    pub capabilities: Vec<Capability>,
    pub max_message_bytes: u64,
    pub max_page_bytes: u64,
    pub max_asset_chunk_bytes: u64,
}
```

Capabilities:

```text
PagedPull
BootstrapV1
AssetChunksV1
DurableIdempotency
DeviceLifecycleV1
QuotaV1
```

**Rules:**

- Client không gọi request nếu capability thiếu.
- Version không tương thích trả `UpgradeRequired`.
- Không fallback legacy chỉ vì `GetSyncPlan` lỗi.
- Mọi request có deadline và cancellation.
- Wire enum variants chỉ append; có fixture encode/decode.

### SERVER-001 — Migration framework và fresh-vault initialization

**Tasks bắt buộc:**

- Migration chạy trong transaction và propagate error.
- Không đánh dấu applied nếu `execute_batch` fail.
- Khi register vault, cùng transaction phải tạo:
  - random `incarnation_id`;
  - `vault_sequences` row với sequence 0;
  - quota counters;
  - bootstrap state.
- Kiểm tra schema/query khớp bằng integration test trên empty DB.
- Sửa `pull_page_metadata` để đọc đúng bảng/column.

**Acceptance criteria:**

- Fresh server + first vault + first push thành công.
- Restart giữ sequence và idempotency.
- DB migration lỗi không khởi động server ở trạng thái schema nửa vời.

### SERVER-002 — Durable push transaction

**Implementation order:**

1. Validate request size, payload hash, operation ID và quota.
2. Check processed-operation ledger.
3. Ghi ciphertext vào temp blob.
4. Flush, atomic rename và fsync directory nếu platform hỗ trợ.
5. `BEGIN IMMEDIATE`.
6. Allocate sequence.
7. Insert operation ledger, mailbox entry và document head.
8. Update asset refs và committed quota.
9. Commit.
10. Chỉ sau commit mới ACK và notify.

Nếu DB commit fail sau final blob rename, blob trở thành recoverable orphan.
Startup/cleanup phải scan và quarantine/delete orphan sau grace period.

Duplicate operation trả original result/sequence, không trả current head
sequence và không charge quota lần hai.

### SERVER-003 — Paged pull và bootstrap resumable

**Tasks:**

- Page clamp theo entry count và total encrypted bytes.
- `has_more` dựa trên existence query sau last returned sequence, không suy luận
  từ số byte đã dùng.
- Missing blob là corruption error; không thay bằng empty payload.
- Bootstrap session cố định `base_seq` và materialize document heads trong
  snapshot nhất quán.
- Client resume active session sau restart.
- Implement keep-alive, complete, abort và expiry.
- Bootstrap items có payload hash và stable position.
- Apply bootstrap không giữ DB mutex qua file/decrypt/apply call.
- Chỉ complete local và remote sau mọi item durable/apply thành công.
- Sau bootstrap, pull delta `(base_seq, fixed_high_watermark]`.
- Chuẩn hóa `entry_kind`: `full_snapshot`, `delete_tombstone`; không so với
  string `"delete"` rời rạc.

**Acceptance criteria:**

- Bootstrap 50.000 document không tăng RAM tuyến tính.
- Server/client restart ở từng page vẫn resume.
- Concurrent push trong bootstrap không tạo gap.
- Delete head được bootstrap thành delete, không decode như document.

### ASSET-001 — Hoàn thiện server chunk API

**Files chính:**

- `src-tauri/src/sync/adapter/server.rs`
- `sync-server/src/mailbox.rs`
- `sync-server/src/db.rs`
- `sync-server/src/cleanup.rs`

**Tasks:**

- Thay mọi `todo!()` trong asset request path bằng typed result.
- `has_asset_chunk`, `push_asset_chunk`, `pull_asset_chunk` hoạt động theo
  `(remote_vault_id, chunk_id)`.
- Enforce maximum encrypted chunk size.
- Push idempotent và không double-charge quota.
- Blob write dùng temp + verify + atomic rename.
- Coordinator dừng reference operation nếu bất kỳ chunk nào fail.
- Sau upload, verify toàn bộ chunk tồn tại trước publish reference.
- Protocol truyền outer opaque asset/chunk refs đủ để server pin reference mà
  không thấy plaintext path/hash.

**Acceptance criteria:**

- Không asset request nào panic.
- Inject failure ở chunk N không publish reference.
- Retry chỉ upload missing chunks.
- Missing/corrupt chunk không overwrite local file.

### ASSET-002 — Client asset snapshot, queue và apply

**Tasks:**

- Classification dùng UTF-8 validation + MIME/extension + size.
- Giữ file snapshot ổn định hoặc verify size, mtime và final hash sau pass hai.
- Pending asset key có `vault_id`, provider và remote operation.
- `INSERT ... ON CONFLICT DO UPDATE`; job failed có thể retry.
- States gồm `pending`, `downloading`, `failed`, `applied`, `superseded`.
- Temp path nằm trong `.synabit/tmp`, không nằm tùy ý ở vault root.
- Rename asset update mapping và cleanup path cũ đúng transaction logic.
- Metrics tính riêng asset bytes.

### SERVER-004 — Device lifecycle, cleanup và quota

**Tasks:**

- Auth sử dụng bảng `devices`; revoked device bị từ chối trước request dispatch.
- `last_acked_seq` chỉ tăng đơn điệu.
- Stale device chuyển `bootstrap_required`, không bị xóa cursor im lặng.
- GC watermark dựa trên active devices và bootstrap pins.
- `document_asset_refs` được cập nhật cùng transaction document head.
- Tắt age-based asset GC cho tới khi reference graph được test.
- GC queue key và delete dùng đầy đủ `(vault_hash, blob_id)`.
- Chỉ xóa queue row sau physical delete thành công; nếu fail thì retry.
- Quota dùng một transactional committed counter cho document + asset.
- `max_entry_age` hoặc được implement đúng invariant, hoặc bỏ khỏi config để
  tránh tạo cảm giác đang có retention.

### Sprint 2 release gate

- Không còn `todo!()`/panic trên mailbox request path.
- Fresh vault push/pull pass.
- Server restart/crash fault tests pass.
- Paged pull và bootstrap bounded/resumable pass.
- Asset 500 MiB qua server có bounded memory.
- Retry operation/chunk không double-charge quota.
- GC không xóa document head/asset đang được reference.
- Protocol incompatibility fail closed với thông báo rõ ràng.

---

## 9. Sprint 3 — Google Drive, UI/UX, security và hardening

### GDRIVE-001 — Namespace layout theo vault

Layout mới:

```text
Synabit Vault/
  vaults/
    <remote_vault_id_hex>/
      control/
      ops/
      assets/
      snapshots/
```

**Rules:**

- Folder ID được lưu theo `(vault_id, Google account)`.
- Không tìm folder chỉ bằng display name ở mỗi sync.
- Nếu find/create race tạo duplicate folder, chọn canonical ID trong control
  record và báo migration warning.
- Chunk filename deterministic theo opaque chunk ID.
- Legacy `.sync_log`/`.sync_assets` chỉ read/import; client mới không ghi thêm.

### GDRIVE-002 — Thay timestamp cursor bằng Drive change token

**Implementation:**

1. Lấy và lưu Drive `startPageToken`.
2. Dùng Changes API để đọc biến động theo page.
3. Filter theo canonical per-vault folder/file IDs.
4. Dedupe bằng durable operation ID trong local inbox.
5. Chỉ commit `newStartPageToken` sau page đã durable.
6. Nếu token invalid/expired, chạy bounded reconciliation scan:
   - list operation metadata theo page;
   - dedupe bằng operation ID;
   - lấy token mới sau khi scan hoàn thành.
7. Timestamp/HLC chỉ dùng conflict metadata, không dùng làm pull cursor.

**Acceptance criteria:**

- Hai operation cùng millisecond đều được nhận.
- Device clock lệch 24 giờ không mất operation.
- Late upload vẫn được nhận.
- Crash giữa changes pages resume không mất/duplicate apply.

### GDRIVE-003 — Idempotency, snapshot và growth control

**Tasks:**

- Một operation ID ánh xạ một immutable remote file ID.
- Retry tìm operation record trước khi tạo file mới.
- Validate downloaded size/hash trước cursor commit.
- Không advance cursor qua malformed file; đưa vào quarantine/error state.
- Tạo encrypted snapshot manifest định kỳ để bootstrap bounded.
- Trong release đầu, không destructive-GC history nếu chưa có all-device ACK
  proof. Có thể cảnh báo storage usage thay vì xóa không an toàn.
- Mọi API call có timeout, cancellation và exponential backoff có jitter.

### UI-001 — Một sync event contract duy nhất

**Files chính:**

- `src-tauri/src/sync/progress.rs`
- `src-tauri/src/commands/sync_core.rs`
- `src/composables/useSync.ts`
- `src/shared/components/SyncErrorPanel.vue`
- `src/shared/components/SyncConflictToast.vue`

**Contract đề xuất:**

```ts
type SyncPhase =
  | 'checking'
  | 'pulling'
  | 'applying'
  | 'pushing'
  | 'assets'
  | 'complete'
  | 'error';

interface SyncProgressEvent {
  runId: string;
  vaultId: string;
  provider: 'gdrive' | 'server';
  phase: SyncPhase;
  completedItems: number;
  totalItems?: number;
  bytesTransferred: number;
  totalBytes?: number;
  currentFile?: string;
}
```

Error, conflict và quota dùng typed payload riêng, camelCase thống nhất tại
Tauri boundary. Tạo frontend contract tests bằng serialized Rust fixtures hoặc
JSON fixtures dùng chung.

**Acceptance criteria:**

- Backend thực sự emit mọi event UI đang nghe.
- Không còn field name/enum case mismatch.
- Conflict toast chỉ mount một lần.
- Error panel được mount và có keyboard/screen-reader semantics.

### UI-002 — Sync state machine và truthful status

**Tasks:**

- Thay các boolean rời rạc bằng một state machine.
- File change trong lúc sync đặt `syncAgain=true`; run hiện tại xong sẽ chạy
  thêm đúng một trailing sync.
- Tách `lastAttemptedSync` và `lastSuccessfulSync`.
- `SyncResult.errors`, pending asset và rejected operation tạo trạng thái
  `partial`, không phải success.
- Hiển thị offline, retry time, upgrade required và provider disconnected.
- Nút Sync Now không bị treo vô hạn; hỗ trợ timeout/cancel.
- Startup reconnect dựa trên live `sync_status`, không dựa vào persisted provider.

### UI-003 — Provider connect/disconnect

**Tasks:**

- `useGDrive.finishConnect` truyền đúng provider/context vào `sync_run`.
- Connect thành công mới set `activeSyncProvider`.
- Connect phải kiểm tra token thực sự dùng được, không chỉ kiểm tra có token.
- Switching provider là transaction UI:
  1. stop/cancel provider cũ;
  2. persist provider mới;
  3. connect;
  4. rollback UI state nếu connect fail.
- Server “Connected” dựa trên authenticated live session.
- Bỏ fixed two-second connecting state.
- Official server chỉ hiển thị available sau health/capability check.

### UI-004 — Mobile policy và metrics

**Tasks:**

- Enforce cả `off`, `text_only`, `all`.
- `text_only` không upload/download asset nhưng vẫn giữ pending state rõ ràng.
- Sửa shape response `sync_metrics`.
- Tính document bytes và asset bytes.
- Không quảng cáo background sync nếu chưa có worker thật.
- Nếu triển khai background worker:
  - dùng cùng coordinator lock/outbox;
  - tuân theo cellular/battery policy;
  - có OS deadline;
  - không giữ secret trong job payload;
  - có integration test foreground/background race.

### SEC-001 — Secret storage

**Tasks:**

- iOS dùng Keychain; không ghi master/token plaintext vào
  `synabit_secrets.json`.
- Desktop keyring load error phân biệt `not found`, `locked`, `corrupt` và
  `permission denied`; không trả default im lặng.
- Android JNI loại bỏ `unwrap` trên input/platform failure.
- Secret log redaction tests.
- Xác định rõ desktop OAuth client là public/native client; không coi embedded
  client secret là bí mật bảo mật.

### SEC-002 — Device revoke có nghĩa rõ ràng

Release này chốt phạm vi revoke là **chặn quyền truy cập Sync Server từ thời
điểm revoke**, không tuyên bố xóa dữ liệu cũ trên thiết bị.

**Tasks:**

- Mỗi device có credential/token riêng, không dùng một mailbox token chung làm
  credential trực tiếp.
- Server kiểm tra device state trên mỗi new session.
- Revoke invalidates credential và đóng session đang mở.
- Device không thể tự đăng ký lại chỉ bằng device ID mới.
- UI copy đổi thành mô tả chính xác.
- Ẩn pairing/list/remove UI nếu backend command tương ứng chưa implement.
- Full cryptographic vault-key rotation và re-encrypt mọi head/asset phải là
  project riêng; không dùng increment epoch giả để tuyên bố đã re-encrypt.

### OBS-001 — Logging, metrics và supportability

**Structured fields:**

- run ID;
- redacted vault tag;
- provider;
- phase;
- operation counts;
- page/sequence range;
- pending counts;
- bytes tx/rx;
- retry count;
- typed error code.

Không log:

- plaintext path nếu không cần;
- recovery phrase/master key/token;
- encrypted payload;
- full remote vault ID.

Thêm health metrics server:

- active connections;
- push/pull errors;
- DB transaction latency;
- blob write latency;
- bootstrap sessions;
- pending GC;
- committed bytes;
- corruption/orphan count.

### Sprint 3 release gate

- Google Drive clock-skew/late-entry tests pass.
- GDrive multi-vault isolation pass.
- UI contract tests pass.
- Không UI nào báo connected/success dựa trên state cũ hoặc partial result.
- iOS/Android/desktop secret storage review pass.
- Revoked server device không authenticate lại.
- Full build, lint, clippy và tests sạch.

---

## 10. Test strategy bắt buộc

### 10.1. Unit tests

- Stable vault ID/remote ID derivation.
- Operation ID uniqueness và retry stability.
- HLC ordering.
- Markdown CRDT merge.
- JSON LWW local/remote winner.
- Identity injection atomicity và UUID validation.
- Safe path: absolute, `..`, symlink escape, Unicode edge cases.
- Asset boundaries, AEAD/AAD, corrupt chunk, final hash.
- Decompression size limit.
- Protocol encode/decode fixtures.

### 10.2. Database tests

- Fresh schema.
- Legacy local DB migration.
- Legacy server DB migration.
- Migration rollback khi statement fail.
- Foreign key/integrity check.
- Outbox/inbox crash resume.
- First vault/first push sequence.
- Duplicate operation returns original sequence.
- Bootstrap session resume/expiry.
- GC pin/reference behavior.

### 10.3. Adapter contract suite

Cùng một suite phải chạy cho:

- in-memory fake adapter;
- Synabit Server adapter với real temporary server;
- mocked/fake Google Drive API.

Cases:

- Push/pull text operation.
- Duplicate operation.
- Delete.
- Pagination.
- Timeout/retry.
- Malformed payload.
- Asset missing/upload/pull/corruption.
- Quota.
- Provider upgrade required.

### 10.4. Two-device end-to-end matrix

| Scenario | Kỳ vọng |
|---|---|
| A tạo note, B pull | Hai bên hội tụ |
| A/B sửa Markdown offline | CRDT merge và hội tụ |
| A/B sửa JSON offline | LWW winner được reconcile |
| Delete rồi recreate cùng content | Recreate xuất hiện trên cả hai |
| Delete tới trước create/mapping | Delete nằm pending rồi apply |
| Rename document/asset | Không duplicate hoặc stale path |
| App kill trước/giữa/sau ACK | Resume, không mất operation |
| Server restart giữa blob và DB | Không ACK giả, orphan recoverable |
| 100k backlog | Pull bounded |
| Asset 500 MiB | RAM bounded, resume theo chunk |
| Corrupt remote chunk | Không overwrite file tốt |
| Hai vault cùng path/key | Không trộn state |
| Device revoked | Không tạo session mới |
| Clock lệch trên GDrive | Không mất operation |

### 10.5. Soak và fault injection

- 24 giờ, hai device, mỗi phút edit/create/delete.
- Network disconnect ngẫu nhiên.
- Duplicate/reorder response.
- Disk full/read-only.
- SQLite busy/transaction rollback.
- Blob missing/corrupt.
- Drive 429/5xx/token expiry.
- Server/client version mismatch.
- Process kill ở các checkpoint có nhãn trong code.

Mỗi fault phải có expected typed error và recovery path; không chấp nhận chỉ
“không crash”.

## 11. CI và quality gate

Mỗi PR sync phải chạy:

```bash
npm run type-check
npm run lint
npm run test:unit -- --run
npm run build

cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets

cd ../sync-server
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Bổ sung CI jobs:

- protocol compatibility fixtures;
- local/server DB migration fixtures;
- real temporary mailbox integration;
- multi-vault test;
- two-client convergence test;
- asset memory test;
- fault-injection test subset.

Không được release khi server test count vẫn bằng 0 hoặc sync core test không
compile.

## 12. Migration và rollout

### 12.1. Local database

1. Preflight disk space.
2. Tạo backup.
3. Transactional migration.
4. `PRAGMA integrity_check`.
5. Nếu mapping legacy không chắc chắn, đánh dấu provider
   `bootstrap_required`; không đoán.
6. Giữ backup ít nhất một app version và cung cấp support path để export.

### 12.2. Synabit Server

1. Deploy schema-compatible server trước.
2. Health check schema version và capabilities.
3. Seed/checkpoint existing vaults.
4. Chỉ set bootstrap `ready` sau verification.
5. Deploy client có protocol negotiation.
6. Bật asset capability sau server asset contract tests.
7. Bật safe GC cuối cùng.

### 12.3. Google Drive

1. Tạo per-vault V2 folder.
2. Scan/import legacy log theo page vào local inbox.
3. Ghi V2 only sau import checkpoint.
4. Giữ legacy data read-only trong ít nhất một chu kỳ release.
5. Không xóa legacy files tự động trong rollout đầu.

### 12.4. Feature flags

Feature flags tối thiểu:

- `sync_server_v3`;
- `sync_server_assets`;
- `sync_server_bootstrap`;
- `sync_gdrive_v2`;
- `sync_safe_gc`.

Flags phải fail closed. Production default chỉ bật khi release gate tương ứng
đã pass. Không dùng flag để chạy legacy unsafe direct P2P trong release build.

## 13. Definition of Done toàn bộ tính năng

Tính năng sync chỉ được coi là hoàn thiện khi:

- [ ] Mọi local/remote sync state được namespace theo vault.
- [ ] Local outbox/inbox survive restart.
- [ ] Operation ID là event ID và retry idempotent.
- [ ] Cursor chỉ tiến sau durable apply/stage.
- [ ] Không còn silent protocol fallback.
- [ ] Không còn `todo!()`/panic trên input hoặc request path.
- [ ] Fresh Synabit Server vault push/pull được.
- [ ] Bootstrap resumable, bounded và không có gap.
- [ ] Server ACK chỉ sau durable transaction.
- [ ] Server/GDrive đều hoàn thiện chunk asset contract.
- [ ] Reference không publish trước chunks.
- [ ] Document/asset path đều chống traversal.
- [ ] Google Drive không dùng timestamp làm correctness cursor.
- [ ] JSON/Markdown/delete/recreate hội tụ trong two-device tests.
- [ ] Device revoke chặn server access và UI mô tả đúng giới hạn.
- [ ] iOS/Android/desktop secret storage đạt platform standard.
- [ ] UI event/command contracts khớp backend.
- [ ] UI phân biệt partial, pending, offline, error và success.
- [ ] Server có test thực, không còn zero-test pass.
- [ ] Lint, clippy, build và toàn bộ test pass.
- [ ] Soak/fault-injection không tạo data loss hoặc success giả.
- [ ] Runbook migration, rollback, backup và support được viết.

## 14. Ticket dependency map

```text
SYNC-001 Vault context
    |
    v
SYNC-002 Namespaced schema
    |
    +----> SYNC-003 Outbox/event IDs
    |          |
    |          v
    |      ASSET-001/002
    |
    +----> SYNC-004 Inbox/cursor
               |
               +----> SYNC-005 Safe apply/convergence
               |
               +----> SERVER-003 Bootstrap

PROTO-001
    |
    v
SERVER-001 Migration/fresh vault
    |
    +----> SERVER-002 Durable push
    +----> SERVER-003 Paged pull/bootstrap
    +----> SERVER-004 Lifecycle/GC/quota

SYNC-001/002 ----> GDRIVE-001
SYNC-004 --------> GDRIVE-002/003

Backend contracts stable
    |
    v
UI-001/002/003/004
    |
    v
OBS-001 + release hardening
```

## 15. Quy tắc cắt scope khi chậm tiến độ

Có thể cắt:

- Parallel asset upload.
- UI progress từng chunk.
- Destructive GC.
- Background mobile worker.
- Advanced MIME detection.
- Automatic cleanup legacy Google Drive layout.

Không được cắt:

- Vault isolation.
- Durable outbox/inbox.
- Event operation ID.
- Cursor-after-durable-apply.
- Upload-before-reference.
- Safe path.
- Protocol negotiation.
- Durable server ACK.
- Bounded paging/bootstrap.
- Google Drive non-timestamp cursor.
- Two-device convergence tests.
- Honest UI success/error state.

Nếu một mục “không được cắt” chưa hoàn thành, provider liên quan phải tiếp tục
ở trạng thái experimental/disabled trong production thay vì hạ acceptance
criteria.
