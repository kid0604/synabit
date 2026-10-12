# Connector của Syn

**2026-10-08, viết lại 2026-10-09.** Nghiên cứu và đề xuất, chưa có dòng code nào.
**Cập nhật cùng ngày:** "executor" tách thành ba trục — transport, decoder, logic (§3.1) — sau
khi rà lại các hình thức kết nối còn thiếu: dữ liệu của thiết bị, email, đăng ký nhận sự kiện,
SSH, thư mục trên máy, các định dạng ngoài JSON. Thêm hình dạng `subscribe` (§3.2) và kịch bản
27–32.

Bản trước (commit `b7c64ed`) lớn lên theo từng kịch bản: mỗi ví dụ — Splunk, man-day từ Jira,
điền Sheet rồi gửi mail, điều tra một tài khoản — thêm vài cơ chế đặt tên theo đúng chỗ nó gãy.
Bản này làm ngược lại: **năm nguyên thuỷ** (§3), và kịch bản là bài kiểm ở phụ lục A. Chi tiết
của từng dịch vụ là nội dung của bản khai và skill (phụ lục B), không phải của thiết kế. Những
gì thuộc về *vòng agent* hơn là connector — hồ sơ cho việc dài, run con, dữ liệu nhạy cảm về
một người — đã tách sang `syn-long-work-2026-10-09.md`.

**Luật của tài liệu này:** một kịch bản mới phải đi qua được bằng năm nguyên thuỷ, cộng nội
dung bản khai/skill. Kịch bản nào cần một khái niệm mới thì ghi vào §9 (vấn đề mở), không vá
thẳng vào thiết kế.

---

## 0. Tóm tắt

**Câu hỏi:** làm sao để connector nhận được MCP, nhận được connector làm sẵn, và để Syn tự
tạo connector cho chính nó trong lúc chat — mà không phá những lời hứa an toàn đã xây ở P6,
Phase F và Phase G.

**Trả lời ngắn:**

- Syn tự tạo connector bằng cách **viết bản khai**, không viết code rồi chạy. Rust thực thi
  bản khai bằng ba danh sách cố định — **transport** (nói chuyện bằng gì: HTTP, MQTT, SQL, email,
  SSH, dữ liệu của thiết bị, tiến trình), **decoder** (đọc câu trả lời thế nào), **logic** (ai
  định nghĩa operation: bản khai, server MCP, hay script). Người dùng duyệt thứ Rust dựng từ bản
  khai.
- Năm nguyên thuỷ: **Instance** (đi tới đâu, bằng quyền của ai), **Operation** (một lần gọi và
  hiệu ứng của nó), **Result set** (dữ liệu mang về), **Pipeline** (tính trên dữ liệu đó một cách
  tất định), **Luồng ra** (dữ liệu được đi đâu tiếp).
- Mirror, recipe, routine, báo cáo, điều tra đều là **ghép** các nguyên thuỷ đó (§5), không
  phải tính năng riêng.
- Lát đầu tiên (§8) chứng minh cả năm nguyên thuỷ trên hai kịch bản, một cá nhân và một
  công việc, trước khi làm thêm bất kỳ thứ gì.

---

## 1. Hiện trạng

"Connector" hôm nay là đúng một thứ: một server MCP người dùng tự cấu hình.

| Phần | Ở đâu | Ghi chú |
| --- | --- | --- |
| Kiểu dữ liệu | `syn/connector/config.rs:37-123` | `TransportConfig::{Http, Stdio}`, `Server`, `ConnectorConfig` |
| Lưu | `{vault}/Syn/connectors.json` (sync), keychain (bí mật), `.synabit/connectors-trusted.json` (theo thiết bị) | Tách đúng giữa cái sync và cái không |
| Client MCP | `client.rs` | Tự viết, `2025-06-18`; chỉ `initialize`, `tools/list`, `tools/call`; không OAuth |
| Transport HTTP | `transport_http.rs:55` | Không theo redirect; cho IP nội bộ; không thêm được CA hay ghim cert |
| Đưa tool vào model | `provider.rs` | Tiền tố + cắt mô tả, `sanitize_schema`, chỉ `Surface::App` |
| Quyền | `consent.rs`, `gate.rs` | Đọc → `NetRead{server}`; còn lại → `NetWrite` hỏi từng tool |
| Gọi | `mod.rs:404` | Điền `{{safe:…}}`, xoá bí mật vọng lại, bọc ranh giới, **cắt 12.000 ký tự** (`mod.rs:68`) |
| Taint | `gate.rs:154` | Đọc bất cứ gì từ ngoài → mọi tool connector bị từ chối tới hết run |

Dùng lại nguyên vẹn: `Capability` + consent ledger + `gate::decide` + audit; trust theo thiết
bị bằng fingerprint; Safe và egress theo đích; bọc ranh giới; nhóm tool `connector:<slug>`;
tiền lệ "Syn viết, người duyệt" của `reflect.rs::SkillDraft`; `recipe.rs`; Rich Table; ngữ
pháp truy vấn (`query-grammar-2026-09-20.md`) — đã có hiện thực một phần trong `query.rs` +
`pipeline.rs` (`where`, `stats count by`, `sort`, `head`, `explode`, `seq` trên bảng ô chuỗi); `crunch`
mở rộng nó thay vì viết mới.

---

## 2. Nguyên tắc

Sáu câu. Mọi quyết định ở dưới phải truy về được một trong số chúng.

1. **Syn viết bản khai, không viết chương trình.** Bộ thực thi là Rust cố định, đã test. Một
   bản khai không Turing-complete — đúng ranh giới mà `recipe.rs` tự đặt cho mình.
2. **Rust suy ra quyền; bản khai chỉ được hạ, không được nâng.** Model không quyết định cái gì
   là "chỉ đọc", host nào được tới, hay chữ gì hiện trên card.
3. **Người duyệt thứ Rust dựng**, theo thiết bị, và duyệt lại khi bản khai đổi.
4. **Dữ liệu ngoài là dữ liệu.** Nó được bọc ranh giới, mang nguồn, và không bao giờ là căn cứ
   cho quyền.
5. **Không cắt lặng lẽ, không cộng nhẩm.** Model luôn biết nó đang thấy toàn bộ hay một phần;
   con số trong câu trả lời đến từ phép tính của máy.
6. **Tối thiểu cái được đi ra:** ra mạng, ra model, ra người khác — mỗi lối ra đều có chủ đích
   và thấy được.

---

## 3. Năm nguyên thuỷ

### 3.1 Instance — đi tới đâu, bằng quyền của ai

Một connector là một **bản khai** (sync, trong vault) và một hay nhiều **instance** (theo thiết
bị, không sync).

```text
Bản khai (node syn_connector trong SynConnectors/, khối ```connector)
├── id, name, version, provenance: user | bundled | syn
├── transport: http | mqtt | sql | mail | ssh | device | process
├── decoder: json | xml | csv | ics | vcard | rss | html | text    (theo operation)
├── logic: manifest | mcp | script
├── destinations: [host…]
├── auth: {type, secret slots}
├── operations: […]                 §3.2
└── skill: <syn_skill>              cách dùng, bản đồ dữ liệu, định nghĩa (§4.3)

Instance (theo thiết bị)
├── tài khoản ("Gmail cá nhân", "Jira công ty")
├── zone: personal | work:<tổ chức>
├── model_exposure: full | aggregates | local_only
├── fingerprint đã duyệt, cert đã ghim, loại IP đã duyệt
└── bí mật ở keychain / Safe; token OAuth theo issuer
```

`SynConnectors/` ở top-level, không trong `Syn/`, vì cái bẫy `is_in_unscanned_dir` mà `skill.rs`
đã ghi. Server MCP tiếp tục ở `connectors.json`.

**Bộ thực thi = ba trục, mỗi trục một danh sách cố định trong Rust.** Trước đây gọi chung là
"executor", và chính việc gộp đó che mất những hình thức kết nối chưa có: một danh sách phẳng
`mcp | http | sql | command | script` trộn *nói chuyện bằng gì*, *đọc câu trả lời thế nào* và *ai
định nghĩa các operation*.

**Transport — nói chuyện bằng gì:**

| Transport | Phủ | Chạy ở | Mức `read` chứng minh được bằng | Khi nào |
| --- | --- | --- | --- | --- |
| `http` | REST, GraphQL, API tìm kiếm; WebDAV / **CalDAV / CardDAV**; WebSocket và SSE (cho `subscribe`); HTTP qua **unix socket** (Docker, daemon cục bộ) | mọi nơi; unix socket chỉ desktop | `GET`/`HEAD`/`PROPFIND`/`REPORT`; GraphQL `query` | lát 1 |
| `mqtt` | Home Assistant, Zigbee2MQTT, thiết bị IoT | desktop | `subscribe` là `read`; `publish` là `write` | sau |
| `sql` | Postgres, MySQL, file SQLite | mọi nơi (TCP) | transaction `READ ONLY` / mở file `?mode=ro` | sau |
| `mail` | IMAP + SMTP, JMAP — mọi hộp thư, kể cả không có API (Proton qua Bridge, mail công ty) | mọi nơi; Bridge chỉ desktop | IMAP `EXAMINE` (mở hộp thư chỉ đọc) + `FETCH`/`SEARCH`; `STORE`/`MOVE`/`EXPUNGE` là `write`; gửi SMTP là `irreversible` | sau |
| `ssh` | lệnh cố định trên server từ xa | mọi nơi (SSH thuần Rust) | không — chỉ `read_attested` | sau gate ở §8 |
| `device` | dữ liệu của chính thiết bị, qua một danh sách cầu nối cố định (§3.1.1) | theo nền tảng | cầu nối chỉ mở API đọc, trừ khi khai ghi | sau |
| `process` | server MCP chạy bằng stdio; CLI trên máy | desktop | không — chỉ `read_attested` | stdio có rồi; CLI sau gate ở §8 |

Ghi chú theo transport:

- **`mail`:** người gửi là `free_text` cho tới khi được xác thực. Decoder đọc
  `Authentication-Results` do nhà cung cấp hộp thư ghi (DKIM/SPF/DMARC) thành một cột; một mirror
  "thư từ ngân hàng" lọc theo `dkim = pass` và tên miền, không theo dòng `From:`.
- **`ssh`:** lệnh đi qua shell *ở máy bên kia*, nên mọi giá trị điền vào được thoát theo shell
  POSIX, và lệnh là danh sách cố định trong bản khai, không phải chuỗi Syn viết. `read_attested`
  nên dựa trên một khoá có `command=` cố định trong `authorized_keys` của server — card gợi ý điều
  đó. Khoá lấy từ SSH agent của Safe.
- **`mqtt`, WebSocket, SSE:** là kết nối *đi ra* và giữ lâu, nên nhận được sự kiện tức thì mà
  không cần mở cổng cho bên ngoài gọi vào. Đó là lý do "cảnh báo tức thì" không còn nằm ngoài
  phạm vi (A.15).
- **`process`:** không shell; argv là template; file chạy do người dùng chọn bằng hộp thoại và được
  ghim hash.

**Decoder — đọc câu trả lời thế nào:** `json`, `xml`, `csv`, `ics`, `vcard`, `rss`/`atom`,
`html` (readability, như feed), `text`. Mặc định theo `Content-Type`, khai được cho từng
operation. Decoder là Rust có sẵn hoặc crate đã qua review; sau decoder, mọi thứ là cây dữ liệu
mà `pick` biến thành bảng (§3.3). Một định dạng mới là một decoder mới — không phải một transport.

**Logic — ai định nghĩa các operation:**

| Logic | Operation đến từ | Syn viết được? |
| --- | --- | --- |
| `manifest` | khai trong bản khai | có |
| `mcp` | server tự liệt kê (`tools/list`); bản khai chỉ thêm overlay đọc kết quả (§3.3). Chạy trên `http` hoặc `process` | chỉ đề xuất cấu hình |
| `script` | hàm JS trong QuickJS nhúng, gọi lại vào operation `manifest` của chính bản khai | có — sau gate ở §8 |

#### 3.1.1 `device`: cầu nối cố định

Dữ liệu quý nhất cho phần mềm cá nhân thường không nằm sau một API web, mà ở ngay trên máy.
Mỗi cầu nối là code Kotlin / Swift / Rust của app, có schema cố định, xin quyền qua hệ điều hành:

| Cầu nối | Nền tảng | Cho ra |
| --- | --- | --- |
| `android.notifications` | Android (notification listener) | app (package name do hệ thống ghi — không giả được), tiêu đề, nội dung, thời điểm. Thông báo biến động số dư của app ngân hàng, tin Zalo |
| `health` | Android Health Connect; iOS HealthKit | giấc ngủ, bước, nhịp tim… |
| `calendar`, `contacts` | Android; iOS / macOS EventKit | sự kiện, nhắc việc, danh bạ |
| `folder.watch` | desktop | file mới trong một thư mục người dùng chọn (sao kê CSV trong Downloads) |

- Syn chỉ *chọn* cầu nối và viết `pick`/lọc; không viết cầu nối mới.
- Nội dung thông báo là `free_text`; lọc theo package name là lọc theo một trường hệ thống ghi.
- Cầu nối chạy trên điện thoại phục vụ desktop qua mirror + sync — chiều ngược với `process`.
- **Đọc SMS bị loại**: chính sách Google Play gần như chỉ cho app SMS mặc định. Quyền đọc thông
  báo cũng phải đối chiếu chính sách Play trước khi làm (§9.8).

**Xác thực — danh sách cố định:** `bearer`, `header`, `query`, `basic`, `oauth2_code` (PKCE,
callback qua deep link `com.synabit.app`), `oauth2_client`, `sigv4`, `hmac`, `mtls`. Ký request là
code — nhưng là code *của app*. Không có đăng nhập bằng form web để lấy cookie.

**Mạng:**
- Công khai: chặn IP riêng.
- Mạng nội bộ / VPN / localhost: được khi host do người dùng gõ, hoặc card ghi rõ "mạng nội bộ".
  Loại IP được ghim lúc duyệt (chống DNS rebinding).
- Cert tự ký: ghim fingerprint lúc duyệt. Không có tuỳ chọn bỏ qua kiểm tra TLS.
- Không theo redirect sang host khác.

**`zone`** do người dùng đặt. Dữ liệu đi giữa hai vùng luôn hiện trên card (§3.5).

**`model_exposure`** quyết định dữ liệu của instance được tới model tới đâu:

| Giá trị | Model thấy |
| --- | --- |
| `full` | Dữ liệu sau `pick` |
| `aggregates` | Schema, số dòng, thống kê cột, và đầu ra *đã gộp* của pipeline; dòng thô không ra khỏi máy |
| `local_only` | Như `full`, nhưng chỉ khi provider là local; với provider hosted, operation của instance không được đưa ra |

Gợi ý mặc định: `aggregates` cho instance `work` khi provider là hosted.

**Trust:** fingerprint = hash của khối bản khai + các lựa chọn đã duyệt của instance. Đổi bất
kỳ thứ gì trong đó → instance tắt cho tới khi duyệt lại trên thiết bị này. Bản khai đồng bộ từ
máy khác hiện `NotTrustedHere`, như server MCP hiện nay.

Tool của vault (`VaultTools`) không phải connector và giữ nguyên.

### 3.2 Operation — một lần gọi, và hiệu ứng của nó

**Hiệu ứng có bốn mức:**

| Mức | Nghĩa | Ví dụ | Hỏi |
| --- | --- | --- | --- |
| `read` | Bộ thực thi **chứng minh** được là không ghi | `GET`/`HEAD`; SQL trong transaction `READ ONLY`; GraphQL `query`; endpoint chỉ-tìm-kiếm | một lần mỗi instance |
| `read_attested` | Về giao thức có thể ghi; người dùng **cam kết** token chỉ đọc | `POST /search`, SPL, ES `_search`, `readOnlyHint` của MCP | như `read`, khi người dùng đã bật trên card |
| `write` | Ghi, đảo ngược được | tạo task, ghi ô | mỗi operation, hoặc qua kế hoạch (§3.5) |
| `irreversible` | Gửi đi, không lấy lại được | gửi mail, chuyển tiền, xoá cứng | luôn qua kế hoạch; không bao giờ `Always` |

Rust tính **mức trần** từ transport và request: `POST` không thể là `read`; GraphQL có `mutation`
không thể là `read`. Bản khai chỉ hạ. Lint ngôn ngữ truy vấn (SPL `outputlookup|delete|sendemail`,
SQL ngoài `SELECT`) hạ một lần gọi xuống `write` — một lớp phòng thủ thêm, không phải căn cứ để
gọi thứ gì là `read`.

**Hình dạng một lần gọi** — mẫu của bộ thực thi, khai bằng tham số, **mỗi mẫu có trần cứng**:

| Mẫu | Tham số | Trần |
| --- | --- | --- |
| một request | method, path, query, header, body — template chỉ trong các vị trí này, giá trị mã hoá theo vị trí | timeout, kích thước |
| phân trang | `cursor`, `page`, `offset`, `link_header` | `max_pages` |
| job | submit → kiểm trạng thái tới `done_when` → lấy kết quả; `cancel` tuỳ chọn | `max_wait_s` |
| fan-out | đầu vào là **một cột của một result set đã có trong run**; theo lô nếu API nhận lô | `max_calls`, `concurrency` |
| subscribe | topic / kênh / bộ lọc sự kiện; mỗi sự kiện thành một dòng của một result set *dòng chảy*, hoặc kích hoạt một routine / mirror (§5) | số sự kiện mỗi phút, kích thước bộ đệm; chạy trên một thiết bị được chọn |

`subscribe` là `read`. Nó không chạy trong một run: Rust giữ kết nối, model không ở trong vòng
lặp, và một sự kiện chỉ *bắt đầu* một run khi người dùng đã gắn nó với một routine. Trên điện
thoại, hệ điều hành chặn kết nối nền, nên `subscribe` chạy trên desktop.

Chạm trần không phải lỗi: result set ghi phần thiếu (§3.3). Bộ thực thi tôn trọng `429`/`503` +
`Retry-After`, và giữ phần đã lấy khi lỗi giữa chừng.

**Operation truy vấn bằng ngôn ngữ** khai `language: sql | spl | promql | logql | jql | graphql | …`.
Câu truy vấn của mỗi lần gọi vào audit nguyên văn. Operation `role: discover` (danh sách index,
bảng, trường, metric) rẻ, đọc, cache theo thiết bị, và nuôi bản đồ dữ liệu trong skill (§4.3).

Template **không bao giờ** chạm host hay scheme. Một bản khai có một `base_url` cho mỗi
destination; path không nhận `/ .. @ # ?` từ giá trị điền vào.

### 3.3 Result set — dữ liệu mang về

Mọi kết quả của mọi operation, kể cả của MCP, đi một trong ba đường:

| Sau `pick` | Đi đâu | Model nhận |
| --- | --- | --- |
| nhỏ (≤ ~8.000 ký tự) | thẳng vào prompt, bọc ranh giới | dữ liệu (theo `model_exposure`) |
| lớn | **result set** trong `state.db` của thiết bị, có TTL | handle, số dòng, cột + kiểu, thống kê cột, vài dòng mẫu, và câu "đây là tóm tắt" |
| nhị phân | file trong cache của thiết bị | handle, loại, kích thước; đọc bằng tool file có sẵn |

Một result set là **một bảng có kiểu**:

- **Cột có kiểu:** `text`, `number`, `bool`, `time` (lưu UTC, kèm múi giờ gốc khi nguồn có),
  `json`. Cột văn bản tự do được đánh dấu `free_text` (§3.5).
- **Mỗi dòng có id ổn định** (`r12#4471`), để một khẳng định trỏ được tới đúng nó.
- **Mỗi bảng mang nguồn:** instance, operation, truy vấn nguyên văn, thời điểm.
- **Mỗi bảng mang phần thiếu:** "dừng ở trang 10", "37 đầu vào chưa gọi", "API chỉ nhúng 20
  mục con ở 3 dòng". Model không thể không thấy.

API trả dạng cột (`{"time": […], "rain": […]}`, như Open-Meteo) được đọc bằng `zip`: các danh sách
đặt cạnh nhau, mỗi vị trí một bản ghi. `pick` biến cây dữ liệu mà decoder trả về (từ JSON, XML, ICS, vCard, email…) thành bảng: chọn và **đặt tên** cột, đi vào mảng (`each`), tách mảng con
thành nhiều dòng (`explode`), và khai dấu hiệu API đã cắt mảng con. Với MCP, một `pick` phủ lên
tool (overlay) đọc `structuredContent` hay JSON trong text; overlay chỉ đổi cách đọc kết quả,
không đổi quyền.

Result set ở `state.db`, không ở vault: đó là bộ nhớ đệm của thiết bị (nguyên tắc của commit
`0541efa`; tránh D4 "kết quả tool sync nguyên"). Thành Rich Table trong vault khi người dùng bấm
lưu.

### 3.4 Pipeline — tính tất định trên result set

Một tool, `crunch(handle…, pipeline)`, một ngôn ngữ: tập con của `query-grammar-2026-09-20.md`
(lọc kiểu Lucene + ống dẫn kiểu Splunk). Chạy trong Rust, không mạng, không model. Đầu ra nhỏ
thì inline, lớn thì thành result set mới.

Mọi phép phân tích là **một hàm của ngữ pháp này**, không phải một khái niệm của thiết kế:

| Nhóm | Hàm |
| --- | --- |
| hình dạng | `where`, `fields`, `sort`, `head`, `explode` |
| tính | `eval` (số học, đổi đơn vị, `hour(_time, tz)`, `extract` theo regex) |
| gộp | `stats count|sum|avg|min|max|distinct|p50|p95|stdev by …`, `bin <thời gian>` |
| ghép | `join <handle> on …`, `union <handle>…` (cột `source` cho biết từ đâu) |
| so | `compare <cửa sổ> to <nền>` |

Nguồn ghép có thể là một truy vấn vault (người trong vault, task trong vault).

**Mỗi lần trả lời có số, Rust dựng một biên bản tính** từ run — truy vấn đã chạy, số dòng mỗi
bước, các pipeline, phần thiếu — hiện dưới câu trả lời, gập lại được. Bước đối chiếu số đã có
được mở rộng: số trong câu trả lời phải xuất hiện trong đầu ra pipeline của run.

Thứ tự hiện thực: bắt đầu với `where | stats | sort | head | eval | bin`, thêm hàm khi một kịch bản ở
phụ lục A cần. Thêm hàm không phải thêm khái niệm.

### 3.5 Luồng ra — dữ liệu được đi đâu tiếp

**Nguồn.** Run giữ tập nguồn đã đọc: `{web, feed, file, instance A, instance B, …}`. Mỗi giá trị
truyền vào một operation mang nguồn của nó khi Rust biết được (điền từ handle, từ lời người dùng,
từ node vault).

**Đọc tiếp:**

| Sau khi đã đọc từ… | Đọc cùng instance | Đọc instance khác | Ghi bất kỳ |
| --- | --- | --- | --- |
| không gì | theo grant | theo grant | theo grant / kế hoạch |
| instance A | được | hỏi `Once`, hiện tham số và nguồn — **trừ khi trong một grant phạm vi** | qua kế hoạch |
| web / feed / file | từ chối | từ chối | từ chối |

**Grant phạm vi:** người dùng duyệt một lần một tập instance cùng `zone` được đọc chéo cho nhau
— trong một run, hoặc trong một hồ sơ (`syn-long-work`). Chỉ áp cho `read`/`read_attested`. Không
bao giờ chứa web.

**Ghi đi qua kế hoạch.** Trước lần ghi đầu, Syn gọi `propose_writes`: danh sách request cụ thể —
đích, operation, tham số đã điền, nguồn của từng tham số. Rust dựng một card, mỗi dòng một request,
dòng `irreversible` tô riêng, bỏ chọn được từng dòng. Duyệt là duyệt **đúng những request đó**:
mỗi request được băm; lúc thực thi, request không khớp thì hỏi lại. Kế hoạch sống qua resume
(bài học R1). Operation có thể khai `undo` — bộ thực thi đọc trạng thái trước khi ghi — để
hoàn tác thật khi chuỗi dừng giữa chừng.

**Giá trị từ handle.** Tham số `{{result:r7}}` được Rust điền từ result set lúc thực thi. Model
không chép lại dữ liệu nó đã tính; card hiện đúng cái sẽ được gửi. Phần văn xuôi do model viết
đi qua đối chiếu số; số không khớp tô đỏ trên card.

**Tham số có `role`** chỉ nhận giá trị từ các nguồn được phép:

| `role` | Ví dụ | Nguồn được phép |
| --- | --- | --- |
| `recipient` | `to`, `cc`, kênh, số điện thoại | lời người dùng; node người do người dùng tạo |
| `destination` | id sheet, thư mục, URL webhook | lời người dùng; node vault; kết quả của một request trong cùng kế hoạch |
| `amount` | tiền, số lượng | lời người dùng |

Sai nguồn: cảnh báo đỏ; với `irreversible` thì từ chối. Node lấy từ mirror không tính là do
người dùng tạo.

**Đích có loại** (`sink: spreadsheet | html | markdown | plain`), và Rust vô hiệu nội dung chủ
động theo loại: bảng tính ghi `RAW` hoặc thoát `= + - @` ở đầu ô; HTML thoát và không nhúng tài
nguyên tự tải; Markdown qua sanitizer có sẵn.

**Vùng.** Dữ liệu từ instance `work` đi vào instance khác vùng là một dòng riêng trên card. Không
chặn — đây là phần mềm của một người — nhưng không bao giờ lặng lẽ.

**Cột `free_text`** (user agent, nội dung lệnh, tiêu đề, thân thư) có thể do người khác viết, kể
cả kẻ tấn công. Chúng không bao giờ là căn cứ *duy nhất* cho một khẳng định, và không bao giờ
điền vào tham số có `role`.

---

## 4. Syn tự tạo connector

### 4.1 Luồng

| Bước | Tool | Ghi chú |
| --- | --- | --- |
| Tìm | `connector_find(service)` | Bản làm sẵn → server MCP chính thức (registry MCP) → OpenAPI / GraphQL introspection → tài liệu qua `browse` |
| Viết | `connector_draft(manifest)` | Rust validate, lỗi trả về cụ thể. Nháp trơ |
| Thử | `connector_try(draft, op, args)` | Quyền như thật; mặc định chỉ thử `read` |
| Khám phá | các operation `discover` | Nuôi bản đồ dữ liệu trong skill |
| Đề xuất | `connector_propose(draft)` | Run dừng ở `AwaitingApproval`; card do Rust dựng |
| Duyệt | người dùng | Tạo instance, đặt `zone` và `model_exposure`, dán bí mật thẳng vào ô, ghim cert / loại IP |
| Sửa | `connector_draft` trên id có sẵn | Fingerprint đổi → tắt; card hiện diff; grant của operation đổi bị thu hồi |

Syn viết được tới đâu, theo transport:

| Transport | Syn viết | Người dùng nhập / chọn |
| --- | --- | --- |
| `http`, `mqtt` | toàn bộ bản khai | bí mật |
| `sql` | bản khai, truy vấn mẫu | chuỗi kết nối |
| `mail` | bộ lọc, `pick` | máy chủ, tài khoản, mật khẩu ứng dụng |
| `ssh` | danh sách lệnh cố định | host, khoá (từ Safe) |
| `device` | chọn cầu nối, lọc, `pick` | quyền của hệ điều hành; thư mục cho `folder.watch` |
| `process` | bản khai, argv mẫu | **file chạy**, bằng hộp thoại |
| logic `mcp` | URL + loại auth | — ; **không bao giờ** đề xuất server stdio kiểu `npx …` |

### 4.2 Bất biến

- Syn không tự duyệt. Chỉ lệnh Tauri gọi từ UI mới ghi trust.
- Syn không thấy giá trị bí mật hay token.
- Nháp trơ; viết nháp được cả khi run đã taint, vì người duyệt là chốt.
- Chỉ `Surface::App` tạo hay sửa connector.
- Mọi chữ trên card do Rust sinh; card không nằm trong vùng văn bản model điều khiển (S9).
- Mô tả operation là đầu vào không tin cậy: cùng giới hạn với mô tả MCP, và là một phần của
  fingerprint.

### 4.3 Skill đi kèm

Mỗi connector có thể trỏ tới một `syn_skill` — node, có version, người đọc được — chứa ba thứ:

1. **Cách dùng:** ngôn ngữ truy vấn của dịch vụ, operation nào cho việc gì.
2. **Bản đồ dữ liệu:** index, bảng, trường tuỳ biến, metric, quy tắc ghép định danh với hệ thống
   khác — tóm tắt từ `discover`, có ngày.
3. **Định nghĩa:** tên chỉ số → cách tính ("man-day" = giờ đã log theo người log, 8 giờ/ngày),
   tên nhóm → cách xác định ("team" = trường Team). Khi câu hỏi dùng một chỉ số chưa định nghĩa
   hoặc nhiều nghĩa, Syn hỏi lại bằng `NeedsChoice` với lựa chọn lấy từ `discover`, và lưu câu
   trả lời vào đây.

Nạp khi nhóm tool của connector bật; không tính vào giới hạn hai skill mỗi run.

### 4.4 Model nhỏ

Model mặc định (`gemma4:e2b`) khó viết bản khai từ tài liệu. Nên: Rust nhập OpenAPI / GraphQL /
schema SQL và liệt kê operation, model chỉ *chọn* và viết `pick`; bản làm sẵn cho dịch vụ phổ
biến; validate thất bại quá 3 vòng thì Syn nói thẳng là cần model mạnh hơn.

---

## 5. Ghép lại

Những thứ trông như tính năng thật ra là cách ghép năm nguyên thuỷ:

| Ghép | Gồm |
| --- | --- |
| **Hỏi một câu** | Operation → result set → pipeline → câu trả lời + biên bản tính |
| **Báo cáo** | Như trên, rồi một kế hoạch ghi với giá trị từ handle (§3.5) |
| **Recipe** | Một chuỗi cố định operation `read` + `crunch` + kế hoạch ghi, tham số hoá. `recipe.rs` chỉ cần cho phép bước gọi operation đọc |
| **Mirror** | Một operation `read` chạy theo lịch hoặc theo sự kiện của một `subscribe`, do Rust, không model → upsert node vault theo key, mang `origin: connector:<id>`. Đọc node mirror bật taint (bài học S5). Không mirror vào kiểu dữ liệu app tin để tính toán (bài học S4) — tài chính đi qua staging. Một thiết bị được chọn chạy mirror |
| **Routine** | Recipe theo lịch, chỉ operation đọc được liệt kê lúc tạo. Kế hoạch ghi được *soạn* rồi chờ người duyệt trong app (hết hạn sau 48 giờ) |
| **Phản ứng với sự kiện** | `subscribe` + bộ lọc → bắt đầu một routine (ví dụ: cửa mở khi không ai ở nhà → báo lên điện thoại). Đây chính là `Trigger::VaultEvent` đã có trong kế hoạch, mở rộng ra sự kiện bên ngoài. Báo cho người dùng là ghi vào vault, không phải ghi ra ngoài |
| **Điều tra** | Nhiều lượt hỏi trong một grant phạm vi, với hồ sơ — xem `syn-long-work-2026-10-09.md` |

---

## 6. MCP

MCP là một giá trị của trục logic, chạy trên transport `http` hoặc `process`. Mọi thứ ở §3 áp cho nó: kết quả thành result set qua overlay, `readOnlyHint`
là `read_attested`, ghi qua kế hoạch, cert ghim, mạng nội bộ.

Nâng cấp riêng:

- **OAuth** — mở khoá các server từ xa chính thức (Atlassian, Notion, Linear, GitHub…). Theo spec
  `2026-07-28`: Client ID Metadata Documents trước, DCR chỉ để tương thích; kiểm `iss` (RFC 9207);
  credential gắn theo issuer. Nhà cung cấp không nhận client tự đăng ký (Google, Microsoft) thì
  người dùng tự tạo client — bản làm sẵn đi kèm hướng dẫn (phụ lục B).
- **Spec `2026-07-28`** bỏ `initialize` và `Mcp-Session-Id`, thêm `server/discover`, mọi result có
  `resultType`, tasks thành extension. Thử `server/discover` trước, không được thì `initialize`.
  Giữ `sanitize_schema` bóc combinator dù spec đã nới.
- **Tìm server** qua registry trong `connector_find`, chỉ là đề xuất qua card.
- Hai connector cho cùng một dịch vụ là bình thường: MCP chính thức cho hỏi đáp và thao tác lẻ,
  `http` trực tiếp khi cần endpoint theo lô mà server MCP không có.

---

## 7. Surface và thiết bị

| Surface | Connector được làm gì |
| --- | --- |
| App | Mọi thứ ở trên |
| Routine | Operation đọc được liệt kê trong routine; routine được duyệt theo thiết bị (vá S8 trước); ghi chỉ bằng kế hoạch chờ người duyệt |
| Telegram | Operation đọc của instance được bật "dùng qua Telegram"; không ghi, không xin quyền qua Telegram; node mirror thì luôn đọc được |

Mỗi transport chạy ở đâu:

| Chỉ desktop | Chỉ điện thoại | Cả hai |
| --- | --- | --- |
| `process`, `mqtt`, `subscribe` (kết nối nền), HTTP qua unix socket, `folder.watch`, IMAP qua Proton Bridge | `android.notifications`, Health Connect, HealthKit trên iOS | `http`, `sql`, `mail`, `ssh` |

Transport không có trên máy hiện "cần desktop" / "cần điện thoại", không báo lỗi kết nối. **Mirror
nối hai chiều đó:** mirror chạy trên desktop phục vụ điện thoại qua sync, và mirror từ cầu nối
`device` chạy trên điện thoại phục vụ desktop. Một thiết bị được chọn chạy mỗi mirror.

---

## 8. Lát đầu tiên, rồi lộ trình

**Điều kiện trước:** Phase G/H đã đóng (review 09-26 §9), hoặc chấp nhận rõ ràng là làm song song.

### Lát 1 — chứng minh năm nguyên thuỷ

Nhỏ nhất có thể mà vẫn chạm cả năm:

- **Instance:** node `syn_connector`, instance theo thiết bị, `zone`, `model_exposure`, fingerprint,
  ghim cert (Splunk on-prem dùng cert tự ký).
- **Operation:** transport `http`, decoder `json`, logic `manifest`; chỉ một request + phân trang;
  bốn mức hiệu ứng trong `Capability`. Ba trục là ba trait riêng ngay từ đầu, dù mỗi trục mới có
  một giá trị — để thêm `csv` hay `mail` sau này không phải tách lại.
- **Result set:** trong `state.db`, có kiểu, có id dòng, có phần thiếu; kết quả MCP cũng đi qua đây
  (thay cho cắt 12.000 ký tự).
- **Pipeline:** `crunch` với `where | stats | sort | head | eval | bin`; biên bản tính; đối chiếu số.
- **Luồng ra:** nguồn theo instance thay cho `gate.rs:154` (chỉ cột "đọc cùng instance" và "ghi →
  hỏi"); chưa có kế hoạch ghi.
- Một bản làm sẵn cho một dịch vụ người dùng thực sự dùng.

**Gate lát 1:** hai kịch bản đầu-cuối — một cá nhân (A.20, sức khoẻ) và một công việc (A.3 qua MCP
Splunk) — với số khớp một bảng tính tay. Test tấn công: path traversal qua tham số, `POST` khai
`read`, redirect sang host khác, bí mật vọng lại, bản khai sync từ máy khác, kết quả lớn không bị
cắt lặng lẽ. Mỗi test phải thất bại khi tắt bản vá.

### Sau đó, theo thứ tự

| Bước | Thêm | Kịch bản mở khoá |
| --- | --- | --- |
| 2 | Kế hoạch ghi, giá trị từ handle, `role`, `sink`, `undo` | A.2, A.9, A.18 |
| 3 | Syn tự tạo (§4), skill đi kèm, nhập OpenAPI | A.1, A.2 do Syn tạo |
| 4 | Decoder `xml`, `csv`, `ics`, `vcard`, `rss`; WebDAV (CalDAV / CardDAV); mirror | A.23, A.25, A.28 |
| 5 | `device`: `android.notifications`, `health`, `calendar`, `folder.watch` — sau khi qua §9.8 | A.20 không cần token, A.27, A.32 |
| 6 | OAuth, spec MCP mới, job, fan-out, grant phạm vi | A.17, A.18, A.19 |
| 7 | `mail` (IMAP / SMTP / JMAP), cột xác thực người gửi | A.7 không cần OAuth, A.22, A.29 |
| 8 | `subscribe` (WebSocket, SSE, `mqtt`) + routine theo sự kiện | A.15, A.30 |
| 9 | Routine và Telegram (§7) | A.14 |
| 10 | `sql`, `sigv4` / `hmac` / `mtls` | A.4, A.11 |
| 11 | `process` cho CLI, `ssh`, logic `script` — **chỉ khi** bộ đếm `needs_logic` (validate từ chối vì cần logic hoặc lệnh) ghi được ít nhất ba trường hợp thật; đúng gate P8 | A.10, A.31 |

Thứ tự này xếp theo giá trị cho phần mềm *cá nhân*: dữ liệu trên máy, lịch và danh bạ, email đi
trước; những thứ chủ yếu cho công việc (`sql`, ký request, SSH) đi sau.

---

## 9. Vấn đề mở

Những chỗ mà kịch bản đòi một khái niệm chưa có. Ghi lại, chưa giải.

1. **Ghi tự động mức thấp.** "Tắt đèn khi tao ra khỏi nhà" (A.21) là một lần ghi không người nhìn,
   đảo ngược được, rủi ro thấp. Thiết kế hiện bắt mọi lần ghi của routine chờ người duyệt, nên
   tự động hoá nhà cửa không làm được. Cần một khái niệm "ghi được phép không người nhìn" mà không
   mở lại S8.
2. **Mirror hai chiều.** Sửa một sự kiện lịch đã mirror ngay trong app (A.23): hiện là "remote là
   nguồn sự thật, sửa qua operation". Trải nghiệm đó có chấp nhận được không, hay cần đồng bộ hai
   chiều với giải quyết xung đột?
3. **Trích xuất từ văn bản tự do trong mirror.** Thông báo giao dịch qua email (A.22) cần biến thân
   thư thành giao dịch. `extract` theo regex trong pipeline làm được khi mẫu ổn định; khi không,
   cần model — mà mirror được thiết kế là không model.
4. **Job dài hơn `max_wait_s`.** Run đỗ chờ job rồi tiếp tục, dùng chung cơ chế resume của routine?
5. **Nguồn theo từng chuỗi.** §3.5 theo dõi nguồn của giá trị Rust điền; giá trị model tự viết thì
   chưa. Đây là bước tiếp theo ở review 09-26 §7.3.
6. **Dùng connector desktop từ điện thoại** bằng lời gọi live qua iroh — chưa đề xuất.
7. **Dịch vụ không có API** (Zalo, Messenger, phần lớn ngân hàng Việt Nam). `android.notifications`
   lấy được *những gì hiện thành thông báo* — tin đến, biến động số dư — nhưng không có lịch sử,
   không có tin đã gửi, và chỉ trên Android. Bước xa hơn là một transport `page`: mở trang người
   dùng đã đăng nhập trong pane của Syn và trích bảng theo CSS selector, chỉ đọc, không click. Nó
   dễ vỡ khi trang đổi giao diện và có thể trái điều khoản của dịch vụ, nên **chưa đề xuất**.
8. **Chính sách cửa hàng ứng dụng.** Google Play gần như chỉ cho app SMS mặc định đọc SMS, nên SMS
   bị loại. Notification listener và Health Connect cũng có yêu cầu khai báo riêng. Phải đối chiếu
   với `android-google-play-readiness-reaudit-2026-08-27.md` và chính sách Play hiện hành trước
   bước 5; nếu không qua được, cầu nối đó chỉ có ở bản cài ngoài Play.

---

## 10. Không làm

- Không cho Syn viết rồi chạy server MCP hay chương trình nào nó tự chọn.
- Không chợ connector (review 09-05: ClawHub nhiễm malware).
- Không để mức hiệu ứng, chữ trên card, host hay người nhận do model quyết định cuối cùng.
- Không cắt kết quả lặng lẽ; không để model tự tính số khi pipeline làm được.
- Không có tuỳ chọn bỏ qua kiểm tra TLS.
- Không mirror vào kiểu dữ liệu app tin để tính toán mà không qua staging.
- Không thêm điều kiện hay vòng lặp vào bản khai `http`. Khi nó muốn có, đó là tín hiệu của bước 11.
- Không thêm khái niệm vào §3 để qua một kịch bản. Ghi vào §9.

---

## Phụ lục A — Bảng kiểm kịch bản

Cột "Mới?" là bài kiểm thật của thiết kế: **không** = đi qua bằng năm nguyên thuỷ + nội dung bản
khai/skill; số = vấn đề mở ở §9.

### Công việc

| # | Kịch bản | Nguyên thuỷ | Nội dung bản khai / skill | Mới? |
| --- | --- | --- | --- | --- |
| 1 | Thời tiết, tỉ giá | Operation `read` | — | không |
| 2 | Todoist / Notion: việc hôm nay, đánh dấu xong | `read`, `write` qua kế hoạch | OAuth hoặc token | không |
| 3 | Lỗi 5xx tăng ở host nào (Splunk / ES / Loki) | `read_attested`, job, result set, `stats` | Ngôn ngữ truy vấn, bản đồ index; cert ghim | không |
| 4 | Doanh thu theo vùng (Postgres công ty) | `sql` `read`, `discover`, pipeline | Schema | không |
| 5 | CPU tuần qua có bất thường (Prometheus) | `read`, `bin`, `compare` | PromQL | không |
| 6 | PR chờ review (GitHub) | `read`, phân trang | GraphQL hoặc MCP chính thức | không |
| 7 | Tóm tắt và trả lời thư (Gmail) | `read`, `irreversible` qua kế hoạch | OAuth tự tạo client | không |
| 9 | Tắt đèn phòng khách (Home Assistant) | `write`, mạng nội bộ | Long-lived token | không |
| 10 | `kubectl get pods` | `process`, `read_attested` | — | không (sau bước 11) |
| 11 | Bucket S3 nào public | `read`, `sigv4` | — | không |
| 13 | Lỗi Splunk tuần này đã có ticket Jira chưa | Đọc chéo instance → `Once` hoặc grant phạm vi | — | không |
| 15 | Cảnh báo tức thì khi có lỗi | `subscribe` (SSE / WebSocket của hệ thống cảnh báo) → routine | Kênh sự kiện của dịch vụ | không — kết nối đi ra, không cần nhận kết nối vào |
| 16 | Tải file trong vault lên Drive | `write`, `role: destination` | — | không |
| 17 | Phân bổ man-day theo người (Jira) | `read` (endpoint tìm kiếm), phân trang, fan-out theo lô, `explode`, `join`, `eval`, `stats` | Trường tuỳ biến; **định nghĩa** man-day và team | không |
| 18 | Điền Sheet và gửi mail cho sếp | Kế hoạch ghi, `{{result:…}}`, `sink: spreadsheet`, `role: recipient`, vùng | OAuth Google, scope nhỏ nhất | không |
| 31 | "Disk của web-03 còn bao nhiêu, service X có chạy không?" | `ssh`, lệnh cố định, `read_attested` | Danh sách lệnh; khoá có `command=` cố định | không (sau bước 11) |
| 19 | Một nhân viên làm việc lúc nửa đêm: tài khoản có bị chiếm? → reboot → metric Grafana | Grant phạm vi, `time` có múi giờ, `join` định danh, `union`, `compare`, id dòng làm bằng chứng, `free_text`, `aggregates` | Playbook (skill); quy tắc ghép định danh trong bản đồ dữ liệu | không — phần hồ sơ ở `syn-long-work` |

Số giữ nguyên theo bản trước. Dòng 8 (sao kê ngân hàng) thành 25; dòng 12 (trang không có API)
thành §9.7; dòng 14 chuyển xuống phần cá nhân.

### Cá nhân

| # | Kịch bản | Nguyên thuỷ | Nội dung bản khai / skill | Mới? |
| --- | --- | --- | --- | --- |
| 14 | Routine sáng: lịch hôm nay + việc + giấc ngủ | Routine đọc; lịch qua mirror | — | không |
| 20 | "Tuần này tao ngủ thế nào so với tháng trước?" (Oura / Garmin / Withings) | `read`, result set, `compare` | Token cá nhân | không |
| 21 | Tắt đèn khi tao ra khỏi nhà | Routine + `write` | — | **§9.1** |
| 22 | Giao dịch từ email thông báo của ngân hàng vào Tài chính | Mirror qua `mail` (lọc `dkim = pass` + tên miền), `extract`, staging | Mẫu thư của từng ngân hàng | **§9.3** |
| 23 | Lịch Google hai chiều | Mirror + `write` | OAuth | **§9.2** |
| 24 | Báo khi giá vé / giá hàng giảm dưới ngưỡng | Mirror + routine đọc + `where` | API của hãng nếu có | không (nếu có API); §9.7 nếu không |
| 25 | Sao kê CSV từ ngân hàng → chi tiêu theo nhóm | File → result set → `stats`; lưu thành giao dịch qua staging | — | không |
| 26 | Tóm tắt tin nhắn Zalo | Một phần: `android.notifications` lọc theo package Zalo | — | **§9.7** — chỉ tin đến đã hiện thành thông báo |
| 27 | "Tháng này tao tiêu bao nhiêu cho ăn uống?" từ thông báo biến động số dư của app ngân hàng | Mirror từ `android.notifications` (lọc package ngân hàng), `extract`, staging, `stats` | Mẫu thông báo của từng ngân hàng | **§9.3**, **§9.8** |
| 28 | Lịch và danh bạ iCloud / Fastmail / Nextcloud | `http` `PROPFIND`/`REPORT` (`read`), decoder `ics`/`vcard`, mirror | Địa chỉ CalDAV/CardDAV, mật khẩu ứng dụng | không |
| 29 | "Tuần này thư nào tao cần trả lời?" từ hộp thư không có API (Proton qua Bridge, mail công ty) | `mail` IMAP `EXAMINE` (`read`), result set, cột xác thực người gửi | Máy chủ IMAP | không |
| 30 | "Báo tao khi cửa mở lúc nhà không có ai" | `subscribe` (`mqtt` hoặc WebSocket của Home Assistant) → routine → thông báo | Topic / entity | không |
| 32 | "Có sao kê mới trong Downloads thì nhập vào" | `device` `folder.watch`, decoder `csv`, staging | Cột của từng ngân hàng | không |

---

## Phụ lục B — Mẫu bản khai

Chi tiết của từng dịch vụ sống ở đây, và rồi trong các bản làm sẵn — không ở thiết kế.

**Bản làm sẵn đầu tiên** là Thời tiết (Open-Meteo), trong `src-tauri/src/syn/connector/bundled.rs`:
một operation `GET /archive`, không cần khoá, `result.zip: $.daily`.

**Một request (Todoist):**

```connector
transport: http
decoder: json
logic: manifest
base_url: https://api.todoist.com/rest/v2
auth: { type: bearer, secret: todoist_token }
operations:
  - name: list_tasks
    effect: read
    call: { method: GET, path: /tasks, query: { filter: "{{filter}}" } }
    params: { filter: { type: string, description: "today | overdue | …" } }
    result: { each: "$", pick: { id: id, content: content, due: { path: due.date, type: time, tz: local } } }
```

**Job (Splunk REST):**

```yaml
  - name: search
    effect: read_attested
    language: spl
    call: { method: POST, path: /services/search/jobs, form: { search: "{{spl}}", output_mode: json } }
    job:
      id: "$.sid"
      status: { method: GET, path: "/services/search/jobs/{{job.id}}", query: { output_mode: json } }
      done_when: { path: "$.entry[0].content.isDone", equals: true }
      results: { method: GET, path: "/services/search/jobs/{{job.id}}/results", query: { output_mode: json, count: 0 } }
      poll: { every_s: 2, backoff: 1.5, max_wait_s: 300 }
```

Splunk on-prem dùng cert tự ký trên 8089 → ghim lúc duyệt. Splunk cũng có MCP Server chính thức
(`/services/mcp`, Bearer token, `splunk_run_query` giới hạn 1 phút / 1000 event).

**Tìm kiếm + phân trang + `explode` + fan-out theo lô (Jira):**

```yaml
  - name: search_issues
    effect: read                     # endpoint chỉ tìm kiếm
    language: jql
    call: { method: GET, path: /rest/api/3/search/jql, query: { jql: "{{jql}}", fields: "assignee,timeoriginalestimate,customfield_10001,worklog" } }
    paginate: { style: cursor, param: nextPageToken, next: "$.nextPageToken", max_pages: 20 }
    result:
      each: "$.issues[*]"
      pick: { key: key, assignee: fields.assignee.displayName, team: fields.customfield_10001.name }
      explode: { path: fields.worklog.worklogs, pick: { worklog_id: id, author: author.displayName, spent_s: timeSpentSeconds, started: { path: started, type: time } } }
      truncated_when: { total: fields.worklog.total, got: fields.worklog.maxResults }
  - name: worklogs_by_id
    effect: read_attested            # POST nhưng chỉ đọc
    from: { column: worklog_id }
    batch: 1000
    call: { method: POST, path: /rest/api/3/worklog/list, json: { ids: "{{batch}}" } }
    max_calls: 50
```

`/search/jql` dùng `nextPageToken` (endpoint `/search` cũ đã bị gỡ). Jira chỉ nhúng 20 worklog
mỗi issue. Chi tiết của `worklog/list` (tối đa 1000 id) **chưa được xác nhận** — đối chiếu tài
liệu Atlassian trước khi làm bản làm sẵn.

**Đích bảng tính + người nhận (Google):**

```yaml
  - name: write_values
    effect: write
    sink: spreadsheet
    undo: { read: { method: GET, path: "/v4/spreadsheets/{{sheet_id}}/values/{{range}}" } }
    call: { method: PUT, path: "/v4/spreadsheets/{{sheet_id}}/values/{{range}}", query: { valueInputOption: RAW }, json: { values: "{{values}}" } }
    params: { sheet_id: { role: destination }, values: { from_handle: true } }
  - name: send_mail
    effect: irreversible
    sink: html
    params: { to: { role: recipient }, cc: { role: recipient } }
```

Google không nhận client tự đăng ký: người dùng tự tạo OAuth client. Ở trạng thái **Testing**,
refresh token **hết hạn sau 7 ngày** — phải chuyển sang In production và đăng nhập lại. Scope nhỏ
nhất: `drive.file`, `gmail.send`. ("`gmail.send` là sensitive, không phải restricted" chưa được
xác nhận từ nguồn chính.)

**Cầu nối thiết bị (thông báo ngân hàng trên Android):**

```connector
transport: device
logic: manifest
operations:
  - name: bank_notifications
    effect: read
    bridge: android.notifications
    filter: { package: [com.vietcombank.app] }     # trường do hệ thống ghi, không giả được
    result:
      pick: { at: { path: posted_at, type: time }, text: { path: text, free_text: true } }
mirror:
  from: bank_notifications
  every: event
  pipeline: 'eval amount = extract(text, "[+-][\d.,]+ ?VND")'
  into: staging:transaction           # không vào giao dịch thật khi chưa duyệt
  runs_on: android
```

Package name ở trên là ví dụ, chưa kiểm.

**Đăng ký sự kiện (Home Assistant qua MQTT):**

```connector
transport: mqtt
logic: manifest
destinations: [mqtt://homeassistant.local:1883]    # mạng nội bộ, người dùng gõ
operations:
  - name: door_events
    effect: read
    subscribe: { topic: "zigbee2mqtt/front_door", max_per_minute: 30 }
    result: { pick: { open: { path: contact, type: bool }, at: { path: _received, type: time } } }
```

Một routine gắn với `door_events` và lọc `open = false and presence = away`. Báo lên điện thoại
là ghi vào vault, không phải ghi ra ngoài.

**Hộp thư IMAP (chỉ đọc):**

```connector
transport: mail
decoder: text
logic: manifest
destinations: [imap://127.0.0.1:1143]              # Proton Bridge trên desktop
operations:
  - name: recent_threads
    effect: read                                    # EXAMINE, không SELECT
    mail: { mailbox: INBOX, search: "SINCE {{since}}", max_messages: 500 }
    result:
      pick:
        from: { path: from, free_text: true }
        sender_auth: auth.dkim                       # pass | fail | none, từ Authentication-Results
        subject: { path: subject, free_text: true }
        at: { path: date, type: time }
```

---

## Nguồn

- Spec MCP `2026-07-28`, changelog: https://modelcontextprotocol.io/specification/2026-07-28/changelog
- Splunk MCP Server, tool và guardrail: https://help.splunk.com/en/splunk-enterprise/mcp-server-for-splunk-platform/mcp-server-tools
- Jira `/search/jql` và `nextPageToken`: https://confluence.atlassian.com/display/JIRAKB/Run+JQL+search+query+using+Jira+Cloud+REST+API
- Jira giới hạn 20 worklog nhúng: https://community.developer.atlassian.com/t/failed-to-get-more-than-the-maxresults-limit-for-worklogs-for-an-issue-through-rest-api/78720
- Google OAuth, refresh token 7 ngày ở Testing: https://developers.google.com/health/setup
- Cloudflare Code Mode: https://developers.cloudflare.com/agents/api-reference/codemode
- QuickJS-NG, giới hạn bộ nhớ và interrupt: https://www.mintlify.com/quickjs-ng/quickjs/advanced/performance
- Trong repo: `docs/syn-agent-review-2026-09-05.md` (P6, P6a, P8, §4.1),
  `docs/syn-agent-review-2026-09-26.md` (R1, R6, S4, S5, S7, S8, S9, D4, §7, §9),
  `docs/safe-2026-09-28.md`, `docs/query-grammar-2026-09-20.md`, `docs/rich-table-2026-10-05.md`,
  `src-tauri/src/syn/{connector,recipe.rs,skill.rs,reflect.rs,consent.rs,gate.rs,taint.rs,surface.rs}`.
