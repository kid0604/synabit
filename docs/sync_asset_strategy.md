# Sync Asset Strategy — One-Sprint Implementation Plan

## 1. Mục tiêu

Trong một sprint, thay toàn bộ đường đồng bộ binary/large file hiện tại bằng
luồng asset theo chunk, để:

- Không đọc, serialize, compress, encrypt hoặc clone toàn bộ asset vào RAM.
- Không nhét binary vào `DocSyncPayload.snapshot` hay document batch.
- Một asset lớn hoặc lỗi không chặn việc đồng bộ các document nhỏ khác.
- Upload/download có thể retry theo chunk và không truyền lại các chunk đã có.
- Cả Sync Server và Google Drive dùng cùng một contract ở coordinator.
- Giữ khả năng đọc dữ liệu binary inline do phiên bản cũ đã tạo.

Thời lượng dự kiến: **8–10 dev-days**, dành cho một Rust developer đã quen
codebase. QA trên thiết bị thật có thể chạy song song trong hai ngày cuối.

## 2. Phạm vi sprint

### Trong phạm vi

- Binary file và file lớn đi qua asset pipeline mới.
- Chunk size cố định 4 MiB.
- E2EE độc lập cho từng chunk.
- Asset manifest/reference nhỏ nằm trong document sync operation.
- Sync Server lưu chunk theo content-addressed opaque ID.
- Google Drive lưu mỗi chunk thành một file độc lập.
- Retry/resume theo danh sách chunk còn thiếu.
- Download vào file tạm, verify rồi atomic rename.
- Hàng đợi local cho asset pull đang pending/failed.
- Progress và partial-error rõ ràng trên UI.
- Decode fallback cho binary inline từ payload cũ.
- Tests cho chunking, corruption, retry, ordering và legacy payload.

### Ngoài phạm vi

- Raw streaming trên một QUIC stream kéo dài.
- Google Drive resumable-upload session API.
- Cross-vault hoặc global deduplication.
- Delta sync bên trong binary.
- Preview/thumbnail generation.
- Tối ưu upload song song theo băng thông.
- GC hoàn hảo cho asset đã bị thay thế/xóa.
- Thay đổi CRDT algorithm hoặc conflict resolution của document text.

Trong sprint này, không GC các asset chunk đã vào provider vì server không thể
phân biệt chunk đã được encrypted reference sử dụng với orphan chunk. Chỉ temp
file chưa vào DB/provider được dọn. Đây là lựa chọn cố ý để ưu tiên an toàn dữ
liệu; age-based asset GC hiện tại phải được tắt. Storage reclamation chính xác
không thuộc sprint này.

## 3. Quyết định thiết kế đã chốt

### 3.1. Giới hạn

```rust
const ASSET_CHUNK_SIZE: usize = 4 * 1024 * 1024;       // 4 MiB
const INLINE_PAYLOAD_LIMIT: u64 = 8 * 1024 * 1024;     // 8 MiB
const MAX_SYNC_ASSET_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB
```

Một file đi qua asset pipeline nếu:

- Không phải UTF-8 hợp lệ; hoặc
- Thuộc nhóm binary/attachment; hoặc
- Kích thước lớn hơn `INLINE_PAYLOAD_LIMIT`, bất kể extension.

Không tiếp tục phân loại chỉ bằng prefix `assets/` hoặc `Files/`. Markdown,
JSON/canvas lớn hơn ngưỡng cũng đi asset pipeline và dùng whole-file/LWW
semantics trong sprint này, không dùng CRDT merge.

File lớn hơn `MAX_SYNC_ASSET_SIZE` không được upload. Sync trả lỗi typed, giữ
document/text sync tiếp tục chạy và UI phải hiển thị file bị bỏ lại.

### 3.2. Data model

Để giữ scope trong một sprint, small Markdown/JSON tiếp tục dùng payload hiện
tại với `is_binary == false`. Chỉ asset dùng payload version mới; không refactor
schema của document text trong sprint này:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSyncPayloadV2 {
    pub node_id: String,
    pub rel_path: String,
    pub asset: AssetRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRef {
    pub version: u8,
    pub asset_id: [u8; 32],
    pub plaintext_hash: [u8; 32],
    pub plaintext_size: u64,
    pub chunk_size: u32,
    pub chunks: Vec<AssetChunkRef>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetChunkRef {
    pub index: u32,
    pub chunk_id: [u8; 32],
    pub plaintext_hash: [u8; 32],
    pub plaintext_size: u32,
}
```

`asset_id` phải opaque với provider:

```text
vault_asset_id_key = derive_key("synabit-asset-id-v1", vault_e2ee_key)
asset_id = keyed_blake3(vault_asset_id_key, plaintext_hash)
```

Không gửi raw plaintext hash ra server/GDrive filename. `plaintext_hash` chỉ
nằm bên trong E2EE document payload.

`chunk_id` cũng phải opaque:

```text
vault_chunk_id_key = derive_key("synabit-chunk-id-v1", vault_e2ee_key)
chunk_id = keyed_blake3(
    vault_chunk_id_key,
    asset_id || chunk_index || plaintext_chunk_hash
)
```

Asset encryption key phải được domain-separate tương tự:

```text
asset_key = derive_key("synabit-asset-content-v1", vault_e2ee_key || asset_id)
```

Không dùng mailbox token, server vault hash hoặc một key derivation context
chung cho các mục đích trên.

Tạo manifest bằng pass đọc đầu tiên: ghi nhận hash/size từng chunk và hash toàn
file. Pass thứ hai chỉ encrypt/upload các chunk provider còn thiếu. Trước và sau
hai pass phải so sánh size + mtime; nếu file thay đổi thì hủy job và retry.

### 3.3. Compatibility

- Giữ struct payload hiện tại dưới tên `DocSyncPayloadV1`.
- Pull kiểm tra encrypted wire marker: V6 decode `AssetSyncPayloadV2`; V5 và
  format cũ decode `DocSyncPayloadV1`.
- V1 có `is_binary == true` vẫn được apply như hiện tại để đọc lịch sử cũ.
- Client mới không tạo thêm V1 inline binary.
- Asset-aware payload dùng encrypted wire marker mới `FORMAT_V6`.
- Client cũ gặp `FORMAT_V6` phải fail closed với lỗi “sync upgrade required”,
  không được ghi manifest bytes thành file.
- Shared mailbox protocol thêm `MAILBOX_PROTOCOL_VERSION = 2`; server phải
  validate `Hello.version`. Asset request chỉ được chạy với protocol 2.
- Các enum variant mới chỉ được append vào shared `synabit-protocol`, không
  chèn giữa các variant cũ.

Không yêu cầu rolling compatibility để client cũ tiếp tục sync asset mới.
Release note phải yêu cầu cập nhật tất cả device trong cùng vault. Yêu cầu bắt
buộc là client cũ dừng an toàn, không làm hỏng dữ liệu.

### 3.4. E2EE và chunk format

- Hash plaintext file bằng streaming read, không `fs::read` toàn file.
- Derive một asset key từ vault key và `asset_id`.
- Mỗi chunk dùng XChaCha20-Poly1305 với nonce ngẫu nhiên riêng.
- Nonce được prefix trong ciphertext chunk.
- AAD phải chứa:
  - format version;
  - `asset_id`;
  - chunk index;
  - chunk count;
  - plaintext file size.
- Không compress asset chunk. JPEG/PNG/PDF/ZIP/video hầu như không hưởng lợi và
  compression tạo thêm allocation.
- Mỗi chunk được verify bằng AEAD và plaintext chunk hash trong encrypted
  `AssetRef`.
- Sau khi ghép file, verify lại `plaintext_size` và `plaintext_hash`.

Tại mọi thời điểm chỉ được giữ một số chunk bounded trong memory. Mặc định chỉ
một chunk upload và một chunk download cho mỗi asset.

### 3.5. Adapter contract

Trong sprint này, mỗi “asset” ở adapter level là **một chunk**, không phải toàn
file. Tận dụng storage path hiện có và thay contract thành:

```rust
#[async_trait]
pub trait SyncAdapter {
    // Existing document methods remain.

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

`Bytes` ở đây vẫn là in-memory buffer, nhưng bị giới hạn tối đa một chunk 4 MiB.
Không adapter nào được nối các chunk thành một `Vec` toàn file.

Không thêm begin/commit asset state ở provider trong sprint này. Một asset được
coi là upload hoàn chỉnh khi tất cả `chunk_id` trong `AssetRef` đã được provider
xác nhận; chỉ sau đó coordinator mới publish reference operation. Cách này tận
dụng bảng asset hiện có và giữ implementation trong một sprint.

### 3.6. Thứ tự push

Coordinator phải tách change thành:

1. Small document operations.
2. Asset upload jobs.
3. Asset-reference document operations.

Thứ tự chạy:

1. Push small document operations trước.
2. Với mỗi asset:
   1. Stream file để tính hash và tạo `AssetRef`.
   2. Gọi `has_asset_chunk` cho từng chunk.
   3. Chỉ đọc/encrypt/upload các chunk còn thiếu.
   4. Sau khi tất cả chunk được xác nhận mới push document operation chứa
      `AssetRef`.
3. Chỉ commit local `sync_hash` sau khi tất cả chunk tồn tại và reference
   operation được ACK.

Nếu file thay đổi size/mtime trong lúc hash hoặc upload:

- Hủy reference operation.
- Không commit local hash.
- Đưa file về pending để retry ở lần sync kế tiếp.

Asset lỗi phải được ghi vào `SyncResult.errors`, nhưng không được rollback ACK
của document nhỏ đã push thành công.

### 3.7. Thứ tự pull và pending queue

Khi pull gặp `AssetRef`:

1. Validate path bằng `resolve_safe_path`.
2. Ghi operation vào `sync_pending_assets` trong local SQLite.
3. Sau khi pending record đã durable, coordinator được phép tiếp tục xử lý
   remote entries tiếp theo.
4. Asset worker tải từng chunk vào file tạm
   `.synabit_tmp/<asset_id>.part`.
5. Verify AEAD, chunk hash, final size và plaintext hash.
6. `fsync` file tạm nếu platform hỗ trợ.
7. Atomic rename sang path cuối.
8. Update `document_paths`, node/search index và local sync hash.
9. Xóa pending record.

Schema tối thiểu:

```sql
CREATE TABLE IF NOT EXISTS sync_pending_assets (
    provider_id    TEXT NOT NULL,
    remote_seq     INTEGER NOT NULL,
    asset_id       BLOB NOT NULL,
    node_id        TEXT NOT NULL,
    rel_path       TEXT NOT NULL,
    asset_ref_blob BLOB NOT NULL,
    status         TEXT NOT NULL,
    retry_count    INTEGER NOT NULL DEFAULT 0,
    last_error     TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY(provider_id, remote_seq, asset_id)
);
```

Các status hợp lệ:

```text
pending -> downloading -> applied
                      \-> failed -> pending
```

Không advance trạng thái document thành applied chỉ vì đã enqueue. UI phải phân
biệt `transport received` và `asset applied`.

Nếu có operation mới hơn cho cùng `node_id`, pending asset cũ được đánh dấu
`superseded` và file tạm được dọn an toàn.

Không tạo background daemon mới trong sprint. Mỗi `sync()`:

1. Staging toàn bộ asset references vào pending table trong lúc vẫn apply các
   document nhỏ.
2. Sau khi duyệt remote entries, drain pending assets tuần tự.
3. Pending job chưa xong được giữ lại và retry ở lần `sync()` kế tiếp.

## 4. Triển khai theo module

### 4.1. Core client

Files chính:

- `src-tauri/src/sync/core/types.rs`
- `src-tauri/src/sync/core/change.rs`
- `src-tauri/src/sync/core/crypto.rs`
- Tạo `src-tauri/src/sync/core/asset.rs`

Tasks:

- Thêm V1 document payload/V2 asset payload và decode fallback.
- Viết streaming file hash.
- Viết chunk iterator đọc tối đa 4 MiB.
- Viết encrypt/decrypt chunk với AAD.
- Tạo `AssetRef` mà không giữ cả file trong RAM.
- Bỏ đường tạo inline binary mới trong `prepare_push_operations`.
- Không clone encrypted payload để map ACK; dùng map
  `operation_id -> local commit metadata` không chứa payload.

### 4.2. Adapter trait và coordinator

Files chính:

- `src-tauri/src/sync/adapter/mod.rs`
- `src-tauri/src/sync/coordinator.rs`
- `src-tauri/src/sync/core/apply.rs`

Tasks:

- Thay asset API `Vec<u8>` bằng chunk API.
- Partition document jobs và asset jobs.
- Push document nhỏ trước.
- Chỉ publish `AssetRef` sau khi tất cả chunk được xác nhận tồn tại.
- Thêm pending pull queue.
- Apply asset bằng temp file + verify + atomic rename.
- Không để asset failure ngăn apply document nhỏ ở sequence sau khi pending
  record đã durable.
- `SyncResult` thêm:

```rust
pub assets_pushed: u32,
pub assets_pulled: u32,
pub assets_pending: u32,
pub asset_bytes_tx: u64,
pub asset_bytes_rx: u64,
```

### 4.3. Shared protocol và Sync Server

Files chính:

- `sync-server/synabit-protocol/src/lib.rs`
- `sync-server/src/mailbox.rs`
- `sync-server/src/db.rs`
- `sync-server/src/cleanup.rs`

Append protocol requests/responses:

```text
HasAsset { chunk_id } / AssetExists { encrypted_size } / AssetNotFound
```

Giữ `PushAsset`, `PullAsset`, `AssetOk`, `AssetData` hiện có trên wire để giảm
scope, nhưng từ protocol v2 trường `asset_hash` được hiểu là opaque `chunk_id`
và payload bắt buộc không lớn hơn một encrypted chunk. Client trait đổi tên để
không còn hiểu nhầm đây là toàn file.

Tận dụng bảng `assets` hiện có; mỗi row lưu một encrypted chunk theo
`(vault_hash, chunk_id)`. Không thêm `asset_uploads`/`asset_chunks` schema trong
sprint này.

Server invariants:

- `HasAsset` lookup theo `(vault_hash, chunk_id)`.
- `PushAsset` idempotent: nếu chunk ID đã tồn tại thì trả `AssetOk` mà không
  charge quota hoặc overwrite blob.
- Reject payload lớn hơn `ASSET_CHUNK_SIZE + encryption overhead`.
- Ghi blob vào temp path rồi atomic rename trước khi commit DB.
- Quota check dùng bytes thực; retry cùng chunk không charge hai lần.
- Không age-GC bất kỳ asset row nào trong sprint này vì server không nhìn thấy
  encrypted reference graph. Chấp nhận storage tăng để tránh data loss.
- Giữ protocol variants cũ để đọc request từ client cũ trong thời gian rollout.

### 4.4. Google Drive adapter

Files chính:

- `src-tauri/src/sync/adapter/gdrive.rs`
- `src-tauri/src/gdrive/api.rs`

Drive layout:

```text
Synabit Vault/
  .sync_log/
  .sync_assets/
    <chunk_id_hex>.chunk
```

Rules:

- `has_asset_chunk` query exact deterministic filename.
- Upload từng file chunk bằng multipart helper hiện tại; mỗi request tối đa
  khoảng 4 MiB nên memory bounded.
- Tên chunk deterministic, không tạo duplicate cho retry.
- `pull_asset_chunk` download đúng một chunk.
- Download response được giới hạn kích thước; reject chunk lớn hơn
  `ASSET_CHUNK_SIZE + encryption overhead`.
- Không dùng `drive_download_file` để đọc toàn asset.
- Không cần Google resumable-upload session trong sprint này: mỗi chunk là một
  retryable upload unit.

### 4.5. UI/UX

Files chính:

- `src/composables/useSync.ts`
- Các component sync status/progress hiện tại.

Yêu cầu:

- Hiển thị document progress và asset progress riêng.
- Hiển thị filename, bytes transferred/total và trạng thái pending retry.
- Partial success không được đổi thành “Synced successfully”.
- Error copy tối thiểu:
  - File vượt giới hạn 2 GiB.
  - Asset upload/download bị gián đoạn, sẽ retry.
  - Asset bị hỏng hoặc verify thất bại.
  - Device/app quá cũ để đọc asset payload V2.
- Không thêm màn hình settings mới trong sprint.

## 5. Lịch thực hiện trong một sprint

| Ngày | Deliverable |
|---|---|
| 1 | Chốt V2 types, constants, protocol `HasAsset`, local DB migration và test fixtures |
| 2 | Streaming hash, chunk iterator, chunk crypto, V1/V2 decode tests |
| 3 | Server Has/Push/Pull chunk trên asset storage hiện có |
| 4 | Server idempotency, quota, atomic blob write và tests |
| 5 | GDrive chunk implementation và mocked API tests |
| 6 | Coordinator push partition, ACK ordering và loại bỏ full-payload clone |
| 7 | Pending pull queue, temp-file apply, verify và atomic rename |
| 8 | Progress events, UI partial status, compatibility/error handling |
| 9 | Integration/fault-injection tests, 50–500 MiB manual tests |
| 10 | Fix regression, run full checks, documentation và release gate |

Nếu trễ tiến độ, cắt theo thứ tự:

1. MIME detection nâng cao — dùng UTF-8 + size + extension cơ bản.
2. Parallel chunk upload — giữ sequential.
3. UI progress chi tiết từng chunk — giữ progress theo bytes.

Không được cắt:

- Bounded chunk memory.
- Upload-before-reference.
- AEAD/hash verification.
- Temp-file atomic apply.
- Retry/idempotency.
- V1 read compatibility.
- Asset failure không chặn document nhỏ.

## 6. Test plan

### Unit tests

- Chunk boundaries: 0 byte, 1 byte, đúng 4 MiB, 4 MiB + 1.
- Streaming hash bằng hash của cùng buffer.
- Encrypt/decrypt từng chunk.
- Sai index/AAD phải decrypt fail.
- Bit-flip ciphertext phải fail.
- V2 asset encode/decode round trip.
- V1 inline binary vẫn decode/apply được.
- File > 8 MiB được classify thành asset.
- File > 2 GiB trả typed error mà không tạo operation.

### Adapter contract tests

Chạy cùng một test suite cho in-memory adapter, server adapter và mocked GDrive:

- `has_asset_chunk` trả false trước upload và true sau upload.
- Upload cùng chunk ID hai lần thành công idempotently.
- Retry chunk không tăng quota/storage usage lần hai.
- Pull chunk không tồn tại trả `None`.
- Pull chunk đã upload trả đúng ciphertext.
- Chunk vượt hard limit bị reject.

### Integration tests

- Đồng bộ asset 50 MiB và document nhỏ trong cùng run; document nhỏ hoàn thành
  kể cả khi asset được fault-inject fail.
- Interrupt sau chunk thứ N; lần sau không upload lại chunk 0..N.
- Corrupt một chunk trên provider; không tạo/overwrite file cuối.
- Pull asset 200 MiB vào temp file rồi atomic rename.
- Rename và delete asset không tạo inline binary operation.
- Legacy inline binary từ log cũ vẫn được apply.
- Pending asset cũ bị supersede bởi operation mới hơn cho cùng node.

### Manual release checks

- Asset 500 MiB trên desktop qua Sync Server.
- Asset 200 MiB qua Google Drive.
- Theo dõi RSS: mức tăng phải bounded và không tăng tuyến tính theo file size;
  target không quá 128 MiB trên baseline khi concurrency mặc định là 1.
- Tắt mạng giữa upload/download rồi bật lại.
- Provider quota exceeded.
- Hai device cùng nhận một asset.
- Một device cũ gặp V2 phải dừng an toàn và hiện yêu cầu update.

## 7. Definition of Done

Sprint chỉ được coi là hoàn thành khi:

- Không còn code path mới đưa binary/large file vào
  `DocSyncPayloadV1.snapshot`.
- Không asset adapter API nào nhận/trả `Vec` toàn file.
- Server và GDrive đều implement asset contract; không có success stub.
- Asset 500 MiB qua server và 200 MiB qua GDrive không làm RAM tăng theo kích
  thước toàn file.
- Interrupted transfer resume từ chunk còn thiếu.
- Reference operation không bao giờ được publish trước khi tất cả chunk tồn tại.
- Corrupt/missing chunk không overwrite file local tốt.
- Text/document nhỏ vẫn sync khi một asset fail.
- V1 inline binary vẫn đọc được.
- `cargo test` cho app và sync-server pass.
- Frontend type-check, unit tests và production build pass.
- Không có ignored regression test mới.
- UI không báo full success khi còn pending/failed asset.

## 8. Pull request checklist

- [ ] Shared protocol version và variants được cập nhật ở một crate duy nhất.
- [ ] DB migration an toàn, idempotent.
- [ ] Asset classification không chỉ dựa vào path prefix.
- [ ] Không `fs::read` toàn asset.
- [ ] Không `postcard` serialize toàn asset.
- [ ] Không compress toàn asset.
- [ ] Không clone encrypted asset payload.
- [ ] Chunk AEAD dùng đúng AAD và verify trước khi write.
- [ ] Server blob write dùng temp + atomic rename.
- [ ] Retry chunk không charge quota hoặc tạo blob duplicate.
- [ ] GDrive chunk names deterministic và retry không duplicate.
- [ ] Pending pull queue survives app restart.
- [ ] Final file write atomic.
- [ ] Metrics/progress tính asset bytes riêng.
- [ ] Legacy V1 fixture test pass.
- [ ] Fault-injection và interruption tests pass.
