# Syn thành agent: đọc lại Synabit, và đường đi từ đây

**Ngày:** 2026-09-02
**Phạm vi:** toàn bộ Synabit — Rust core, vault, index, sync, front end, và Syn.
**Câu hỏi được trả lời:** để Syn trở thành một trợ lý có skill riêng, tự học,
tự viết skill, tự ghi nhớ, làm việc được với vault và với thế giới bên ngoài —
thì phải xây gì, theo thứ tự nào, và cái gì sẽ gãy trên đường đi.

---

## 0. Tóm tắt cho người bận

Synabit đang ở vị trí tốt hơn hầu hết các sản phẩm định làm cùng việc này, vì
một lý do mà chính codebase đã nói ra ở nhiều chỗ nhưng chưa ai gọi tên:

> **Vault đã là một agent runtime rồi. Nó chỉ chưa được dùng như một cái runtime.**

Một agent kiểu Hermes cần sáu thứ: nơi lưu trí nhớ, nơi lưu skill, phiên bản
để quay lui, tìm kiếm để nhớ lại, đồng bộ giữa máy, và một định dạng con người
đọc được để còn sửa khi nó học sai. Synabit đã có đủ cả sáu — dưới dạng
`nodes` + FTS5 + `node_edges` + CRDT log + `iroh` sync + Markdown/frontmatter.
Trong khi các framework agent khác phải bolt thêm một vector DB, một prompt
store, một cái state machine, thì ở đây **một memory là một node, một skill là
một node**, và cả hai thừa hưởng miễn phí: sync, versioning, undo, trash,
mã hoá khi truyền, và khả năng mở bằng bất cứ editor nào.

Khoảng cách thật không nằm ở lưu trữ. Nó nằm ở bảy chỗ:

| # | Khoảng cách | Chẩn đoán ngắn |
|---|---|---|
| 1 | **Không có khái niệm "run"** | `syn_send_message` là một lời gọi Tauri: nạp hội thoại → RAG → vòng lặp tool ≤12 lượt → lưu → trả về. Không checkpoint, không resume, không chạy nền, không sub-run. Đơn vị công việc của agent phải là *run*, không phải *message*. |
| 2 | **System prompt là một `format!` cứng trong Rust** | `rag.rs:949` dựng prompt bằng chuỗi tĩnh. Không có ngân sách token, không có cách chèn skill/memory động, và người dùng không xem được cái thực sự đã gửi đi. |
| 3 | **RAG-stuffing đang tranh chỗ với agentic search** | 1.777 dòng `syn/rag.rs` nhồi context vào system prompt. Chính repo đã dựng thí nghiệm A/B (`rag_vs_agentic`) mà chưa kết luận. Chưa chốt cái này thì không có chỗ trong prompt cho skill. |
| 4 | **Bộ tool là danh sách tĩnh, đã từng phình rồi bị cắt** | 20 → 12, nay 23. Commit `9d28f2d` nói rõ lý do cắt: token toll mỗi lượt và tỉ lệ chọn sai tăng theo độ dài danh sách. Thêm skill + memory + MCP + HTTP + scheduler sẽ đẩy nó về 60+. Cần *progressive disclosure*, không phải thêm tool. |
| 5 | **Mô hình an toàn hiện tại là "mọi hành động đều đảo ngược được"** | Đẹp và đủ cho một agent chỉ chạm vault. Vỡ ngay khi agent gửi được email, gọi được API, tiêu được tiền, hoặc chạy được code. |
| 6 | **Không có đường ra thế giới bên ngoài** | Không `tauri-plugin-shell`. Không MCP. `reqwest` chỉ dùng cho feeds/sync/LLM. Không có token store cho dịch vụ thứ ba (keychain hiện chỉ giữ E2EE key, app-lock hash, và API key của provider). |
| 7 | **Không có eval** | Có phôi thai: `gate_one`, `rag_vs_agentic`, `where_the_recall_goes` — các test `#[ignore]` in ra bảng. Cho agent tự viết skill mà không có eval là cách chắc chắn nhất để nó thoái hoá âm thầm. |

Và một ràng buộc cứng phải nói ngay, vì nó quyết định thiết kế skill:

> **Skill chạy code là một ngã ba, và Android quyết định hướng đi.**
> Không có shell plugin. Google Play cấm app tải-và-chạy code. iOS cấm JIT.
> Nên: **skill mặc định là văn bản + công thức tool khai báo được; skill chạy
> code là một tầng riêng, opt-in, chỉ desktop, trong sandbox WASM/QuickJS —
> không bao giờ là `sh -c`.** Và vì skill sync qua vault, một skill có code
> phải *suy biến* đàng hoàng trên máy không chạy được nó, chứ không phải vỡ.
> Đây đúng là chính sách `CLAUDE.md` đã viết cho Baseline Newly available,
> áp sang một lớp khác.

Lộ trình đề xuất: **bảy phase, mỗi phase một gate**, tổng cộng khoảng 5–8
tháng cho một người làm chính. Chi tiết ở mục 5. Ba tháng đầu ở mục 10.

---

## 1. Synabit hôm nay — bản đồ kiến trúc

### 1.1 Quy mô và hình dạng

| Lớp | Thực tế |
|---|---|
| Rust core | ~85.600 dòng, 198 lệnh Tauri, 1.378 test |
| Front end | ~98.400 dòng Vue 3 + TS, 109 file spec |
| Mini-apps | 12 (`shared/appRegistry.ts`), lazy-load từng chunk |
| Lưu trữ | Thư mục file Markdown/JSON/canvas, index bằng SQLite + FTS5 |
| Sync | `iroh` (QUIC) + Loro (CRDT) + XChaCha20-Poly1305, mailbox server không đọc được nội dung |
| AI | Ollama, hoặc bất cứ endpoint nào nói giọng OpenAI `/chat/completions` |

Điều đáng chú ý về mặt kiến trúc không phải là quy mô mà là **kỷ luật**:
gần như mọi quyết định khó đều có một doc comment giải thích *tại sao*, và
nhiều quyết định có một test canh chừng cho nó khỏi trôi. Ví dụ
`settings.rs::the_frontend_defaults_match_the_ones_a_fresh_vault_gets` đọc
thẳng file TypeScript để so default hai bên; `node.rs::the_types_the_frontend_can_write_are_all_types_the_backend_knows`
parse union type của TS để kiểm tra enum Rust. Đây là văn hoá cần giữ khi
thêm agent, vì agent là thứ dễ trôi nhất trong toàn bộ hệ thống.

### 1.2 Vault: cấu trúc dữ liệu thật

Một node = một file. Danh tính là đường dẫn tương đối (`id`), nhưng danh tính
*bền* là `properties.node_id` trong frontmatter — `NodeMetadata::stable_id()`
ở `models/node.rs`. Đây là chi tiết quan trọng cho agent: khi Syn ghi một
memory rồi file bị di chuyển, backlink phải bám vào `stable_id`, không phải
đường dẫn.

Các thư mục theo quy ước hiện có:

```
{vault}/
  Notes/          ghi chú
  Tasks/          task
  Projects/       project
  Schema/<kind>.md   hình dạng khai báo của một kind (type: schema, title = tên kind)
  Syn/
    settings.json    SynSettings — sync giữa máy, KHÔNG được chứa bí mật
    <uuid>.json      từng hội thoại
  Messages/       thẻ thông báo chủ động do chat_engine sinh ra
  .synabit/       identity của vault (không sync)
  .synabit_crdt/  shadow doc của Loro (không sync qua đường file)
```

`NodeType` (`models/node.rs`) có 19 loại "đã biết" cộng `Other(String)` —
và `Other` là một quyết định thiết kế chứ không phải fallback: một vault
được phép chứa type mà app chưa từng nghe, và app **không được** sửa nó.
Test `an_unknown_type_survives_the_round_trip_unchanged` canh chừng đúng
tính chất đó.

**Hệ quả cho agent:** thêm `type: memory`, `type: skill`, `type: run` vào
vault là thao tác *đã được hỗ trợ sẵn*, không cần migration, không cần
schema mới. Chúng sẽ hiện ngay trong Things, trong Nexus graph, trong search,
và sync ngay. Đây là món quà lớn nhất mà kiến trúc hiện tại tặng cho dự án này.

### 1.3 Index và ngôn ngữ truy vấn

SQLite giữ `nodes`, `node_blocks`, `node_edges`, một FTS5 `search_index`,
cộng các bảng riêng cho files/feeds/finance/sync. DB là *index*, xoá đi
quét lại là có — README nói đúng như vậy, và đó là lý do memory/skill nên
sống trong file chứ không phải trong bảng.

Ngôn ngữ truy vấn (`search.rs::ParsedQuery` + `db/node_query.rs`) là thứ
Syn đang dùng để "nhìn" vault:

```
type:task status:todo sort:due_date
type:book rating:>3 columns:title,rating
#work due_date:<2026-09-01 -status:done
"cụm chính xác" in:title limit:20
```

Chia việc rõ ràng: cấu trúc → SQL trên `nodes`, chữ tự do → FTS5.
`property_ranges` cho so sánh, `property_exclusions` tách khỏi `exclude_terms`
vì `-status:done` và `-done` là hai câu hỏi khác nhau — doc comment tại
`search.rs:15-26` ghi luôn chi phí đo được của việc không có nó (assistant
trả lời 7 và 0 cho một con số thật là 4).

`QueryResult.total` là *tổng thật*, không phải `rows.len()`. Comment ở
`node_query.rs:40-50` kể chuyện assistant đọc nhầm con số này và báo cho
người dùng "2 task trên tổng 126". Chi tiết này quan trọng: **agent tin
những gì tool trả về, nên tool nói dối là lỗi nặng hơn model nói dối.**

Không có embedding, không có vector index. Toàn bộ retrieval là BM25 + graph
1-hop. Điều này ổn ở quy mô vault cá nhân, và tôi khuyến nghị **không** thêm
vector store cho tới khi memory recall đo được là kém — xem mục 6.4.

### 1.4 Sync, CRDT, E2EE

`is_syncable_document` (`sync/utils.rs:56`) chỉ nhận `.md`, `.json`,
`.canvas`. Dotfile và dot-directory bị bỏ qua ở mọi cấp. Nghĩa là:

- `Skills/*.md` và `Memory/*.md` **sẽ tự động sync** — không phải làm gì thêm.
- `Syn/settings.json` cũng sync (và đó là lý do `SynSettings` cấm chứa
  credential; có hẳn test `no_secret_is_ever_written_into_the_vault`).
- Muốn một thứ *không* sync thì đặt trong dotfile hoặc trong keychain.

Đây là ranh giới thiết kế đã có sẵn cho agent: **skill và memory sync; token,
key và consent cho từng thiết bị thì không.**

CRDT log (`db/crdt.rs`, `commands/versions.rs`) giữ mọi lần lưu. Đó chính là
cái làm cho `list_versions`/`restore_version` khả thi, và là nền tảng của
mô hình an toàn hiện tại.

### 1.5 Syn hiện tại, từng mảnh một

```
src-tauri/src/syn/
  provider/mod.rs      trait ChatProvider — chat, chat_streaming, list_models, check_status
  provider/ollama.rs   465 dòng
  provider/openai.rs   995 dòng — reasoning_effort được "học" chứ không cấu hình
  engine.rs            815 dòng — vòng lặp tool, cancel theo conversation, prune history
  tools.rs             2.797 dòng — 23 tool
  rag.rs               1.777 dòng — trích từ khoá, FTS, mở rộng graph, nhồi prompt
  conversation.rs      424 dòng — JSON file trong {vault}/Syn/
  settings.rs          SynSettings ↔ {vault}/Syn/settings.json
```

**Vòng lặp hiện tại** (`engine.rs::send_message_with_tools`):

1. Prune history về `max_history_messages` (giữ system message — có test).
2. Gọi provider, kèm 23 tool definition.
3. Nếu có tool call → chạy tuần tự, emit `syn-tool-call`, đẩy kết quả vào
   `working_messages`.
4. Lặp tối đa `max_tool_iterations` (mặc định 12).
5. Hết lượt → gọi lại *không kèm tool* để model buộc phải trả lời bằng chữ,
   kèm `log::warn!` nói rằng câu trả lời dựa trên một điều tra bị cắt ngang.

Streaming được bật khi provider báo `streams_tool_calls()` — Ollama không,
nên nó vẫn đi đường blocking.

**Cái đã đúng và nên giữ nguyên:**

- Provider là trait. Bốn khác biệt wire-format giữa Ollama và OpenAI được xử
  lý đúng một chỗ duy nhất, ở biên. Đây là hạ tầng để sau này cắm Anthropic,
  hay `/v1/responses`, mà không đụng engine.
- Tool đã *generic theo node* chứ không theo mini-app. `create_node`/
  `update_node`/`query_nodes` chạm được cả những type mà app chưa từng nghe.
  Đây chính xác là tính chất một agent cần.
- Bốn tool cấu trúc (`rename_field`, `delete_field`, `rename_kind`,
  `delete_kind`) dùng cơ chế **hai bước**: gọi không có `confirm_nodes` thì
  chỉ báo kế hoạch và số file sẽ đụng; gọi lại với đúng con số đó mới thực
  thi; sai số thì bị từ chối kèm số thật. **Đây là ngôn ngữ thiết kế cho
  toàn bộ hệ thống permission sau này** — không cần phát minh thêm.
- Ghi vault của Syn phát `node:created/updated/deleted` vào Tauri event bus
  nên UI đang mở cập nhật ngay (sửa ở commit `545cfed`).

**Cái sẽ chặn đường:**

- Không có `ToolContext` nào mang theo *ai đang chạy*, *ngân sách còn bao
  nhiêu*, *đã được cho phép những gì*. `ToolContext { db, vault_path, app }` —
  đủ cho vault, không đủ cho thế giới.
- `execute_tool` là một `match` trên tên. Không đăng ký động được, nên MCP
  server hay skill-tool không có chỗ cắm vào.
- Không có transcript có cấu trúc. `tool_calls_log` được nhét vào
  `SynMessage` để hiển thị, không phải để máy đọc lại. Không thể replay,
  không thể debug một run cũ, không thể eval.

### 1.6 Những thứ đã có mà chưa ai gọi là "agent"

Ba cái này đáng nêu riêng, vì chúng tiết kiệm hàng tuần công việc:

**a) Kênh chủ động đã tồn tại.** `chat_engine.rs` chạy một tick 60 giây,
sinh `ChatMessage` (kiểu `system`, subtype `task_due`/`event_upcoming`) vào
`{vault}/Messages/`, và front end vẽ chúng bằng `NotificationCard.vue`.
Một agent chạy nền cần đúng cái ống này để báo cáo. Không cần UI mới.

**b) Lịch trên điện thoại đã giải xong.** `calendar/scheduler.rs` giải thích
rất rõ: desktop chạy vòng lặp được vì đóng cửa sổ chỉ là ẩn; điện thoại thì
không, nên kế hoạch một tuần được tính trước và giao cho OS scheduler. Agent
định kỳ trên mobile phải theo đúng mô hình này, không phải một background
service.

**c) Things + SchemaManager là cùng một dự án nhìn từ đầu kia.**
Things cho người dùng nắn *hình dạng dữ liệu* mà không cần code. Skill cho
người dùng (và Syn) nắn *hành vi* mà không cần code. `Schema/<kind>.md` —
một file khai báo, title là tên kind, `fields` trong frontmatter — chính là
khuôn mẫu cho `Skills/<name>.md`. Nên dùng chung từ vựng, chung màn hình
quản lý, chung cảm giác.

---

## 2. Bảy khoảng cách, chẩn đoán chi tiết

### 2.1 Đơn vị công việc sai

`syn_send_message` (`commands/syn.rs:143`) làm mười việc trong một lời gọi
async và trả về một `SynMessage`. Điều đó có nghĩa:

- Một "công việc" không tồn tại như một thực thể. Không có id, không có
  mục tiêu, không có trạng thái, không có ngân sách.
- Đóng app giữa chừng = mất. Không resume được.
- Không thể chạy khi không có ai nhìn (scheduled run).
- Không thể có sub-run (ví dụ: "đọc 40 bài feed rồi tóm tắt" nên là một run
  con với context riêng, để không làm ngộp hội thoại chính).
- Cancel là một `AtomicBool` theo `conversation_id` — đúng cho chat, sai cho
  một hàng đợi run.

**Cần:** một `Run` được persist. Xem 4.1.

### 2.2 Prompt là chuỗi cứng

`rag.rs::build_system_prompt` là một `format!` với một khối `tool_guidelines`
tĩnh dài ~40 dòng. Nó đã tốt (dạy *hình dạng vault* thay vì liệt kê tool —
đúng), nhưng:

- Không đo được nó tốn bao nhiêu token.
- Không chèn được skill/memory theo ngữ cảnh.
- Người dùng không xem được cái đã gửi. Khi một skill bắn nhầm, không ai
  debug được.
- Personality có 3 giá trị cứng (`casual`/`professional`/`auto`) và chỉ
  đổi giọng, không đổi hành vi.

### 2.3 RAG nhồi vs. để nó tự tìm — chưa chốt

`rag.rs:1253` đặt câu hỏi thẳng: "1.173 dòng này còn đáng không?" và dựng
sẵn thí nghiệm hai nhánh (`stuffed` vs `agentic`), cộng hai test offline đo
riêng phần retrieval (`what_retrieval_finds_for_each_question`,
`where_the_recall_goes`). **Đây là món nợ phải trả trước khi làm skill**,
vì hai lý do:

1. Context nhồi sẵn chiếm chỗ mà skill descriptions cần.
2. Nếu retrieval kém (và `where_the_recall_goes` gợi ý là có: `AND` giữa các
   từ khoá + ngưỡng relevance), thì memory recall xây trên cùng cơ chế đó
   cũng sẽ kém, và ta sẽ đổ lỗi nhầm cho memory.

Khuyến nghị ở 6.1.

### 2.4 Danh sách tool tĩnh và đã chạm trần một lần

Lịch sử: 20 tool (một tool cho mỗi loại dữ liệu) → 12 tool generic →
nay 23. Commit `9d28f2d` ghi rõ chi phí của danh sách dài: token mỗi lượt
của mọi hội thoại, và xác suất model chọn nhầm tăng theo độ dài.

Nếu thêm ngây thơ: memory (3–4 tool), skill (3–4), MCP (mỗi server 5–30),
HTTP (2–3), scheduler (3–4), sandbox (1–2) → 60–90 tool. Sẽ hỏng.

**Cần:** progressive disclosure. Một core nhỏ ổn định + cơ chế nạp theo yêu
cầu. Xem 4.3 và 4.5.

### 2.5 Mô hình an toàn hết hạn

Commit `545cfed` nói thẳng: *"Nothing in that loop asks permission, so the
guard is that every act reverses."* Với `trash_node` + `list_trash` +
`restore_node` + `list_versions` + `restore_version`, đó là một mô hình
an toàn thật sự chặt chẽ, và nó là lý do Syn được phép ghi vault mà không hỏi.

Nó hết hiệu lực ngay khi có một trong bốn thứ: gửi đi (email/HTTP POST),
tiêu tiền (API trả phí), lộ dữ liệu (gửi nội dung note tới bên thứ ba),
hoặc chạy code. Không cái nào trong bốn cái đó undo được.

**Cần:** một tầng capability + consent. Và nó **phải đi trước** egress,
không phải đi sau. Xem 4.6.

Một điều nữa cần nói thẳng trong UI: **chọn provider OpenAI-compat nghĩa là
note rời khỏi máy.** README hứa "AI on your own machine". Khi Syn thành agent
mạnh, người ta sẽ chọn model hosted vì model local 8B không kham nổi — và
lúc đó lời hứa đó cần một dòng chữ trung thực ngay tại chỗ chọn provider,
không phải trong footer.

### 2.6 Không có đường ra

Kiểm kê thực tế:

- Không `tauri-plugin-shell`, không `Command` spawn ở đâu cả.
- `reqwest` có, dùng cho: LLM provider, feed fetcher, sync adapter, updater.
- CSP của webview rất chặt, nhưng **không liên quan** — mọi HTTP của agent
  sẽ đi qua Rust, không qua webview.
- Keychain (`secrets.rs`) giữ: E2EE password/key, app-lock hash + config,
  và API key của Syn provider theo `slot`. Cơ chế `slot` đã tổng quát sẵn —
  thêm `slot` cho từng dịch vụ ngoài là chuyện nhỏ.
- `feed_engine/` đã có sẵn `fetcher`, `readability`, `sanitizer`, `scrape`.
  Nghĩa là **"đọc một trang web và trả về text sạch" đã tồn tại**, chỉ chưa
  được phơi ra cho Syn.

### 2.7 Không có eval

Có phôi thai và nó tốt: `gate_one` chạy một job nhiều bước trên vault tạm với
setting thật của người dùng; `rag_vs_agentic` in bảng so sánh; các test dùng
substring thay vì judge model ("checkable, cheap, và một câu trả lời sai mà
tình cờ chứa đúng chuỗi thì vẫn nhìn thấy trong transcript").

Nhưng đó là các test `#[ignore]` chạy tay. Với một agent tự viết skill,
cần một bộ eval chạy được theo yêu cầu, có seed vault cố định, có điểm số
so sánh được giữa hai lần. Xem mục 8.

---

## 3. Sáu nguyên tắc ràng buộc thiết kế

Không phải sở thích. Đây là những ràng buộc mà chính sản phẩm đã tự đặt ra,
và vi phạm cái nào cũng là phá một lời hứa đã bán cho người dùng.

**N1 — Mọi thứ Syn học được đều phải sống trong vault, dưới dạng file người
đọc được.** Nếu một memory chỉ tồn tại trong SQLite hoặc trong một blob, đó
là bước lùi so với lời hứa trung tâm ("Your vault is a folder of files").
Người dùng phải mở được, sửa được, xoá được, và commit vào git được.

**N2 — Học là một tài liệu, không phải một trọng số.** ADR
`adr-measuring-the-slope-2026-08-29.md` đã giải bài toán tương đương: khi
không được telemetry, thì *đếm tại chỗ và cho người dùng xem con số*. Áp
sang đây: Syn không "tự tinh chỉnh" theo cách vô hình. Nó viết ra cái nó
học được, ở nơi người dùng đọc và sửa được. Đó vừa là cơ chế học vừa là
cơ chế kiểm soát.

**N3 — Không telemetry, kể cả cho eval.** Eval chạy local, kết quả hiện cho
người dùng nếu họ muốn xem. Không có endpoint thu thập.

**N4 — Bí mật không bao giờ vào vault.** Đã có test canh
(`no_secret_is_ever_written_into_the_vault`). Token OAuth, API key của dịch
vụ ngoài, consent theo thiết bị → keychain. Skill và memory → vault.

**N5 — Suy biến thay vì gãy.** Chính sách `CLAUDE.md` viết cho tính năng
web áp nguyên xi cho skill: một skill dùng năng lực mà thiết bị này không
có (chạy code, mạng, MCP stdio) phải *mất tính năng đó* chứ không được làm
hỏng cả run. Vault sync giữa desktop và điện thoại; một skill viết trên Mac
sẽ có mặt trên Android trong vài giây.

**N6 — Mọi hành động không đảo ngược được phải hỏi, và câu hỏi phải nói ra
được phạm vi.** Mở rộng đúng cơ chế `confirm_nodes` hai bước đã có. Không
phát minh mô hình permission thứ hai.

---

## 4. Kiến trúc đích

### 4.1 `Run` thay cho `Message`

Đây là thay đổi cấu trúc lớn nhất và là nền của mọi thứ sau nó.

```rust
// src-tauri/src/syn/run.rs  (mới)

pub struct Run {
    pub id: String,
    /// Hội thoại sinh ra nó, hoặc None nếu là run nền/định kỳ.
    pub conversation_id: Option<String>,
    /// Câu người dùng nói, nguyên văn. Không diễn giải lại.
    pub goal: String,
    pub trigger: Trigger,          // User | Schedule | VaultEvent | External
    pub state: RunState,           // Planning|Working|AwaitingConsent|Paused|Done|Failed|Cancelled
    pub budget: Budget,            // tool_calls, tokens, wall_clock, money_cents
    pub spent: Budget,
    /// Những gì đã được cho phép *trong run này*.
    pub grants: Vec<Grant>,
    pub steps: Vec<Step>,          // transcript có cấu trúc, máy đọc được
    pub created_at: String,
    pub updated_at: String,
}

pub struct Step {
    pub index: u32,
    pub kind: StepKind,            // Thought|ToolCall|ToolResult|ConsentAsked|ConsentGiven|Message
    pub tool: Option<String>,
    pub args: Option<Value>,
    pub result_digest: Option<String>,  // hash, để phát hiện replay lệch
    pub result_preview: String,
    pub tokens: Option<u64>,
    pub ms: u64,
}
```

**Lưu ở đâu:** `{vault}/Syn/runs/<id>.json` cho transcript đầy đủ, cộng một
node tóm tắt `type: run` để nó tìm được bằng `query_nodes` và hiện trong
Nexus graph. Lý do tách: transcript có thể vài trăm KB và không nên nằm
trong `nodes` index; nhưng "run nào đã đụng vào note nào" là một câu hỏi
graph, và `node_edges` trả lời được nó miễn phí.

**Đổi gì trong code hiện có:**

- `engine.rs::send_message_with_tools` → `engine.rs::drive(run: &mut Run)`.
  Vòng lặp giữ nguyên logic, nhưng mỗi vòng ghi một `Step` và kiểm tra
  `budget` trước khi gọi tiếp.
- `STOP_FLAGS` chuyển từ khoá theo `conversation_id` sang khoá theo `run_id`.
  Cancel một chat = cancel run đang gắn với nó.
- `syn_send_message` trở thành: tạo Run → `drive` → trả về message cuối.
  Giao diện Tauri không đổi ở bước này, để không phải sửa front end ngay.
- Thêm `syn_list_runs`, `syn_get_run`, `syn_resume_run`, `syn_cancel_run`.

**Lợi ích tức thì, trước cả khi có skill:** debug được. Hiện nay khi Syn làm
sai, không có gì để đọc lại ngoài `log::info!`.

### 4.2 Lắp ráp context có ngân sách

Thay `build_system_prompt(context, personality) -> String` bằng:

```rust
pub struct PromptPlan {
    pub sections: Vec<Section>,   // mỗi cái có tên, ưu tiên, và chi phí token ước tính
    pub budget_tokens: usize,
    pub dropped: Vec<String>,     // cái bị cắt vì hết ngân sách — hiện cho người dùng
}

pub enum Section {
    Identity,          // "You are Syn…" — không bao giờ cắt
    Personality,
    Today,             // ngày + thứ
    ToolShape,         // hình dạng vault, cú pháp query — không bao giờ cắt
    Memory(Vec<MemoryRef>),      // profile luôn có + memory truy hồi theo ngữ cảnh
    SkillIndex(Vec<SkillRef>),   // CHỈ name + description + when_to_use
    SkillBody(String),           // nạp theo yêu cầu, tối đa 1–2 skill mỗi lượt
    VaultContext(String),        // RAG stuffing — nếu quyết định giữ
    Custom(String),              // custom_system_prompt của người dùng
}
```

Kèm một lệnh `syn_preview_prompt(conversation_id)` và một nút trong Syn
settings: **"Xem Syn thực sự nhận được gì"**. Không có cái này thì skill
không debug được, và người dùng sẽ nghĩ Syn bị ngu chứ không nghĩ là skill
của họ chưa được nạp.

Ước lượng token: dùng heuristic ký tự/4 trước, không kéo thêm tokenizer.
Sai số ~15% là chấp nhận được cho việc cắt ngân sách; đo chính xác là việc
sau và chỉ khi cần.

### 4.3 Tool registry động

```rust
pub trait ToolProvider: Send + Sync {
    fn definitions(&self, ctx: &RunContext) -> Vec<ToolDefinition>;
    fn execute(&self, ctx: &RunContext, name: &str, args: &Value) -> AppResult<ToolOutcome>;
    /// Cái này quyết định có phải hỏi người dùng không.
    fn capability(&self, name: &str) -> Capability;
}

pub struct ToolOutcome {
    pub content: String,
    /// Đảo ngược được không, và bằng cách nào. Dùng để quyết định có hỏi.
    pub reversal: Reversal,   // Automatic{how} | Manual{how} | None
    pub side_effects: Vec<SideEffect>,
}
```

Các provider: `VaultTools` (23 tool hiện có), `MemoryTools`, `SkillTools`,
`WebTools`, `McpTools` (một per server), `SandboxTools`.

`definitions(ctx)` nhận context nên **danh sách tool có thể co giãn theo
run**: một run chỉ đọc vault không cần thấy `create_transaction`; một run
được kích hoạt skill "review PR" thì mới thấy tool GitHub.

`Reversal` là trường quan trọng nhất trong struct này. Nó cho phép giữ đúng
lời hứa hiện tại: **cái gì tự đảo ngược được thì không hỏi; cái gì không thì
hỏi.** Người dùng sẽ không thấy dialog spam, vì phần lớn việc Syn làm là ghi
vault, và ghi vault vẫn `Reversal::Automatic`.

### 4.4 Memory: ba tầng, một định dạng

**Tầng 1 — hội thoại.** Đã có. `{vault}/Syn/<uuid>.json`, prune theo
`max_history_messages`.

**Tầng 2 — episodic.** Chính là `Run` ở 4.1. "Lần trước tôi nhờ anh làm cái
này, anh làm sao?" trả lời được bằng cách query run.

**Tầng 3 — semantic. Đây là cái đang thiếu.**

```markdown
---
type: memory
node_id: 01J...
kind: fact | preference | instruction | relationship | project
subject: "Minh"              # ai/cái gì memory này nói về
confidence: 0.8
source_run: run_01J...       # run nào sinh ra nó
source_nodes: [Notes/abc.md] # bằng chứng
first_seen: 2026-09-02
last_confirmed: 2026-09-02
review_after: 2026-12-02
pinned: false                # true = luôn nằm trong prompt
---

Minh thích họp buổi sáng, không họp sau 16h.

**Vì sao ghi:** anh ấy đã dời ba cuộc họp chiều trong tháng 8.
```

Vì sao là node Markdown chứ không phải bảng SQLite: N1. Và vì thế nó tự
động có version history, có trash, có sync, có FTS, có graph edge tới
`source_nodes`, có mặt trong Things — **không viết thêm một dòng hạ tầng nào.**

**Ghi vào bộ nhớ theo hai đường:**

1. Tool `remember(kind, subject, body, confidence, source_nodes)` — model
   gọi khi người dùng nói điều đáng nhớ. Có bằng chứng là bắt buộc: một
   memory không chỉ được nguồn thì không được ghi.
2. Reflection cuối run — sau khi run xong, một lượt gọi phụ, rẻ, hỏi "có gì
   đáng nhớ lâu dài không?" Đề xuất được xếp hàng, **không tự ghi**, và hiện
   trong một khay "Syn muốn nhớ những điều này" để người dùng duyệt. Cho tới
   khi có bằng chứng là nó chính xác, tự-ghi là cách nhanh nhất để vault đầy
   rác.

**Nhớ lại:**

- `pinned: true` → luôn nằm trong prompt. Đây là "profile": tên, múi giờ,
  cách xưng hô, ràng buộc cố định. Giới hạn cứng, ví dụ 800 token; vượt thì
  bắt người dùng chọn bỏ.
- Còn lại → truy hồi qua đúng cơ chế `query_nodes`/FTS đang có, cộng graph
  1-hop từ các node đang được nhắc tới. Không cần cơ chế mới.
- Thêm tool `recall(query)` để model chủ động tìm khi nó nghi là có.

**Quản trị — phần hay bị bỏ và là phần sẽ quyết định sản phẩm sống hay chết:**

- Màn hình Memory. Dùng lại `SchemaManager`/Things: cột `kind`, `subject`,
  `confidence`, `last_confirmed`, `loose`. Sửa tại chỗ, xoá là `trash_node`.
- Mâu thuẫn: khi `remember` ghi một fact cùng `subject` + `kind` với một
  memory đang có mà nội dung khác → không ghi đè. Tạo bản mới, đánh dấu
  `supersedes`, và hỏi. Một agent âm thầm đổi ý về bạn là thứ đáng sợ.
- Suy giảm: `review_after`. Quá hạn mà chưa được xác nhận lại thì hạ
  `confidence` và tụt ưu tiên truy hồi. Không tự xoá bao giờ.
- Quên: `forget(id)` = `trash_node`. Đảo ngược được, đúng mô hình cũ.

### 4.5 Skill: một node, ba tầng

```markdown
---
type: skill
node_id: 01J...
name: weekly-review
description: Tổng kết tuần từ task, calendar và note, viết vào một note mới.
when_to_use: Người dùng nói "tổng kết tuần", "review tuần này", hoặc chạy định kỳ chiều thứ Sáu.
tier: prose | recipe | code
tools: [query_nodes, get_node, create_node]
version: 3
author: user | syn
enabled: true
---

## Các bước

1. `query_nodes` với `type:task status:done updated_at:>{tuần trước}`
2. `query_nodes` với `type:event date:this-week`
3. …

## Định dạng đầu ra
…

## Bài học
- Lần đầu tôi liệt kê cả task của project đã archive. Người dùng không muốn thế.
```

**Ba tầng, cố ý:**

| Tier | Là gì | Chạy ở đâu | Rủi ro |
|---|---|---|---|
| `prose` | Chỉ là hướng dẫn được nạp vào prompt | Mọi nơi | Không |
| `recipe` | Một chuỗi tool call khai báo, có tham số, chạy tuần tự bởi Rust, model chỉ điền tham số | Mọi nơi | Thấp — chỉ dùng được tool đã có |
| `code` | JS/WASM trong sandbox | **Chỉ desktop**, opt-in | Cao — xem 4.9 |

`recipe` là tầng bị các framework khác bỏ qua và ở đây nó đáng giá nhất:
nó cho phép skill *xác định* (chạy giống nhau mỗi lần, test được, không tốn
lượt inference) mà không cần sandbox nào cả. Phần lớn "skill" thật của một
app năng suất — tổng kết tuần, dọn inbox, chuẩn bị họp — là recipe, không
phải code.

**Progressive disclosure, cụ thể:**

- Trong system prompt chỉ có bảng: `name` + `description` + `when_to_use`.
  ~25 token/skill. 40 skill ≈ 1.000 token. Chấp nhận được.
- Một tool `load_skill(name)` trả về body. Model tự quyết định nạp.
- Tối đa 2 skill body trong một run, để không nổ context.
- Skill `enabled: false` không xuất hiện trong bảng. Người dùng tắt được.

**Syn tự viết skill — vòng lặp cụ thể:**

1. Sau một run mà model phải lặp lại một chuỗi tool nhiều bước, reflection
   nhận ra hình mẫu, đề xuất `propose_skill(...)`.
2. Skill được ghi vào `{vault}/Skills/<name>.md` với `enabled: false` và
   `author: syn`.
3. **Chạy eval trước khi bật:** so kết quả có/không có skill trên chính
   run vừa rồi cộng 2–3 case từ bộ eval. Kết quả in ra.
4. Người dùng duyệt trong màn hình Skills, đọc được nguyên văn, sửa được,
   rồi mới bật.
5. Skill có `version`. Mỗi lần sửa là một version mới, và `list_versions`/
   `restore_version` đã cho quay lui miễn phí.

Bước 3 và 4 không được bỏ. Một agent tự bật skill của chính nó là một
agent tự thay đổi hành vi mà không ai biết — đó chính xác là điều N2 cấm.

### 4.6 Capability và consent

Mở rộng cơ chế `confirm_nodes` hai bước thành mô hình chung.

```rust
pub enum Capability {
    /// Đọc vault. Không hỏi bao giờ.
    VaultRead,
    /// Ghi vault. Không hỏi, vì đảo ngược được — giữ nguyên hành vi hôm nay.
    VaultWrite,
    /// Đụng nhiều file cùng lúc. Hai bước, đã có.
    VaultStructural,
    /// Đọc mạng. Hỏi một lần cho mỗi domain, nhớ lựa chọn.
    NetRead { domain: String },
    /// Ghi ra ngoài. Hỏi mỗi lần, trừ khi được cấp "always" cho đúng
    /// (server, tool) đó.
    NetWrite { domain: String, tool: String },
    /// Tiêu tiền. Luôn hỏi, luôn hiện số tiền ước tính.
    Spend { cents_estimate: u32 },
    /// Chạy code. Luôn hỏi, hiện nguyên văn code sẽ chạy.
    Execute,
}
```

**Consent ledger** — `{vault}/.synabit/consent.json` (dotfile ⇒ **không**
sync, đúng N4: cho phép trên Mac không có nghĩa là cho phép trên điện thoại).
Ghi: capability, phạm vi, cấp lúc nào, hết hạn khi nào, ai cấp.

**Ba mức trong UI:** "Lần này" / "Luôn cho X" / "Không bao giờ".

**Dry run.** Một run có thể chạy ở chế độ `plan_only`: mọi tool có
`Reversal != Automatic` trả về mô tả thay vì thực thi. Người dùng đọc kế
hoạch rồi bấm chạy. Đây là `confirm_nodes` mở rộng ra toàn hệ thống.

**Ngân sách.** Mỗi run có trần: số tool call, số token, thời gian, và tiền
(khi provider có giá). Chạm trần thì dừng và hỏi, không im lặng cắt như
`max_tool_iterations` hiện tại.

**Audit.** Mọi capability không phải `VaultRead`/`VaultWrite` ghi một dòng
vào transcript của run *và* vào một log gộp mà người dùng xem được.

### 4.7 Thế giới bên ngoài: MCP là câu trả lời, không phải N tích hợp

Sai lầm dễ mắc nhất ở đây là viết tích hợp Gmail, rồi Slack, rồi Notion,
rồi GitHub. Mỗi cái là OAuth riêng, refresh token riêng, rate limit riêng,
API đổi liên tục, và không bao giờ đủ.

**Đề xuất: Synabit làm MCP client.** Một lần làm, và mọi MCP server đang có
(và sẽ có) đều cắm được. Người dùng thêm server trong settings như thêm
một feed.

Ba tầng, theo thứ tự làm:

**Tầng 1 — đọc web (tuần 1–2 của phase).** Đã có gần hết trong `feed_engine/`:
`fetcher` + `readability` + `sanitizer`. Phơi ra hai tool:
- `fetch_url(url)` → text sạch, cắt theo ngân sách ký tự.
- `search_web(query)` → cần một provider; hoặc bỏ qua, hoặc để người dùng
  cấu hình (SearXNG self-host là lựa chọn hợp với triết lý sản phẩm).
Capability `NetRead{domain}`, hỏi một lần cho mỗi domain.

**Tầng 2 — MCP client.**
- Transport HTTP/SSE: chạy được ở **mọi** nền tảng, kể cả Android. Làm cái
  này trước.
- Transport stdio: cần spawn process. Không có shell plugin, nên phải thêm
  `tokio::process::Command` trực tiếp trong Rust (không cần
  `tauri-plugin-shell` — plugin đó phơi shell ra cho *webview*, thứ ta
  không muốn). **Chỉ desktop.** Trên Android thì server stdio đơn giản là
  không khả dụng, và UI phải nói thế chứ không phải báo lỗi kết nối (N5).
- Tool của MCP server được namespace: `mcp:<server>:<tool>`, và
  **không** vào system prompt mặc định. Người dùng bật từng server, và
  bật rồi thì mới có trong `definitions(ctx)`.
- Token của server → keychain, theo `slot` đã có ở `secrets.rs`.

**Tầng 3 — ghi ra ngoài.** Chỉ sau khi 4.6 xong. Mọi `NetWrite` mặc định
hỏi. Cân nhắc chế độ "soạn nhưng không gửi": Syn viết email vào vault dưới
dạng draft node, người dùng bấm gửi. Với một app local-first, đó có lẽ là
điểm dừng đúng chứ không phải là bước đệm.

### 4.8 Chủ động và định kỳ

Ba loại trigger:

| Trigger | Nguồn đã có | Ghi chú |
|---|---|---|
| Thời gian | `chat_engine.rs` tick 60s (desktop); `calendar/scheduler.rs` (mobile) | Mobile phải theo mô hình "lên kế hoạch trước, giao cho OS" |
| Sự kiện vault | `watcher.rs` → `vault:file-modified` | Desktop-only; mobile quét lại khi resume |
| Bên ngoài | feed refresh, MCP notification | Sau cùng |

Kết quả run nền đi vào `{vault}/Messages/` dưới dạng `ChatMessage` — **ống
đã có, không cần UI mới.** `NotificationCard.vue` cần thêm một biến thể cho
"run đã xong, đây là kết quả, mở transcript".

Ràng buộc phải viết vào code ngay từ đầu: **run nền không được có
capability nào cần hỏi.** Vì không có ai ở đó để trả lời. Nếu một run định
kỳ chạm phải consent, nó dừng ở `AwaitingConsent` và gửi một thẻ vào
Messages nói "tôi cần cho phép để làm tiếp", chứ không được tự cho phép.

### 4.9 Sandbox (tuỳ chọn, làm sau cùng, có thể không làm)

Nếu tới đây và vẫn cần skill chạy code:

- **QuickJS nhúng** (`rquickjs`) là lựa chọn đúng hơn WASM cho trường hợp
  này: nhẹ, không có I/O mặc định, và host quyết định hoàn toàn cái gì được
  phơi ra. Skill code chỉ gọi được các host function tương ứng với tool đã
  được cấp.
- Không filesystem. Không network. Không `eval` của host.
- Trần CPU và bộ nhớ; hết là kill.
- Desktop-only. Trên mobile, skill tier `code` hiện là "không chạy được
  trên thiết bị này", và nếu nó có phần `prose` thì phần đó vẫn dùng được.
- Google Play: skill là dữ liệu người dùng chứ không phải code tải về, và
  trên Android nó không chạy — nên không rơi vào điều khoản cấm. Đây là lý
  do tier `code` phải desktop-only *về mặt thực thi*, không chỉ về mặt UI.

**Khuyến nghị thẳng: đừng làm phase này cho tới khi có bằng chứng là
`recipe` không đủ.** Tôi ngờ rằng nó đủ cho 90% trường hợp thật.

---

## 5. Lộ trình theo phase

Mỗi phase có một gate. Gate không đạt thì không sang phase sau — đúng văn
hoá đã có trong repo (`gate_one`, và ADR về gate trước P4).

### P0 — Nền móng (4–6 tuần)

**Vì sao trước:** cả năm thứ sau đều phụ thuộc vào Run và context assembly.
Làm skill trước Run là xây nhà trên cát.

**Làm gì:**
1. `syn/run.rs` — struct Run/Step/Budget, persist vào `{vault}/Syn/runs/`,
   node tóm tắt `type: run`.
2. `engine.rs::drive(run)` thay cho `send_message_with_tools`. Giữ nguyên
   logic vòng lặp; thêm ghi Step và kiểm tra budget.
3. `syn/prompt.rs` — `PromptPlan`, sections có ưu tiên và ngân sách.
4. `syn/registry.rs` — trait `ToolProvider`, `VaultTools` bọc 23 tool hiện
   có, `execute_tool` giữ nguyên bên trong.
5. `RunContext` thay `ToolContext`: thêm `run_id`, `budget`, `grants`.
6. Lệnh `syn_preview_prompt`, `syn_list_runs`, `syn_get_run`,
   `syn_cancel_run`.
7. UI: một tab "Runs" trong Messages app, và nút "Xem prompt" trong
   SynSettings.

**Không làm:** không thêm tool mới nào. Đây là phase refactor.

**Gate P0:**
- Một run bị huỷ giữa chừng, mở lại app, transcript vẫn đọc được đầy đủ.
- `gate_one` vẫn xanh sau refactor, với cùng model.
- Người dùng đọc được nguyên văn prompt đã gửi, và tổng token của từng
  section hiện ra.

**Rủi ro:** đây là refactor động vào đường nóng của tính năng đang chạy.
Giữ `syn_send_message` nguyên chữ ký để front end không phải đổi cùng lúc.

---

### P1 — Chốt món nợ RAG (1–2 tuần)

**Vì sao ở đây:** phải biết còn bao nhiêu chỗ trong prompt trước khi phân
bổ cho memory và skill.

**Làm gì:**
1. Chạy `rag_vs_agentic` và `where_the_recall_goes` với ít nhất 3 model
   (một local, hai hosted). Ghi kết quả vào một ADR.
2. Sửa lỗi recall đã lộ ra: `AND` giữa các từ khoá cho câu hỏi (đã có cờ
   `match_any`, cần bật đúng chỗ), và ngưỡng relevance.
3. Quyết định: giữ RAG stuffing, giảm nó xuống một "gợi ý ngắn", hay bỏ hẳn.
   **Dự đoán của tôi:** giảm xuống ~2.000 ký tự và chỉ khi câu hỏi trông
   giống câu hỏi về vault; agentic search thắng ở model đủ tốt, còn model
   local yếu vẫn cần được nhồi.
4. Nếu bỏ/giảm: `syn/rag.rs` từ 1.777 dòng xuống còn phần `format_context`
   và `build_system_prompt` được PromptPlan gọi.

**Gate P1:** một ADR trong `docs/` với bảng số, và ngân sách prompt còn
lại được ghi thành hằng số có tên.

---

### P2 — Memory (4–6 tuần)

**Làm gì:**
1. `type: memory` + `Memory/` + template frontmatter ở 4.4.
2. Tool `remember`, `recall`, `forget` (`forget` = `trash_node`, chỉ là
   tên dễ gọi hơn cho model).
3. Reflection cuối run → hàng đợi đề xuất, **không tự ghi**.
4. Truy hồi: pinned luôn có (trần 800 token) + FTS/graph theo ngữ cảnh.
5. Màn hình Memory, dựng trên `SchemaManager`/`TableView` đã có.
6. Phát hiện mâu thuẫn (`supersedes`) và `review_after`.
7. i18n: `en.json` + `vi.json`.

**Gate P2:** ba tiêu chí, hai định lượng một định tính.
- Trên bộ eval 20 câu (mục 8), phiên bản có memory tốt hơn phiên bản không
  có ở ≥5 câu và không kém ở câu nào.
- Người dùng tìm và sửa được một memory bất kỳ trong dưới 30 giây, không
  cần hướng dẫn.
- Sau 2 tuần dùng thật: số memory rác < 20% tổng số. Nếu cao hơn, cơ chế
  ghi sai chứ không phải cơ chế nhớ sai.

**Rủi ro lớn nhất của phase này:** vault đầy rác. Đó là lý do bước 3 bắt
buộc duyệt. Nếu sau 2 tuần tỉ lệ duyệt-đồng-ý > 90%, hãy cân nhắc tự-ghi
cho `kind: preference`; đừng cho `kind: fact`.

---

### P3 — Skill, tier `prose` và `recipe` (6–8 tuần)

**Làm gì:**
1. `type: skill` + `Skills/` + frontmatter ở 4.5.
2. `SkillIndex` trong PromptPlan; tool `load_skill`.
3. Recipe runner: một executor Rust chạy chuỗi tool call khai báo, có tham
   số, có điều kiện đơn giản. **Không phải Turing-complete.** Nếu thấy mình
   đang thêm vòng lặp và nhánh vào định dạng recipe, đó là dấu hiệu nó nên
   là tier `code` — dừng lại.
4. Màn hình Skills: liệt kê, bật/tắt, sửa, xem version, xem lần chạy gần
   nhất. Dùng lại `SchemaManager` + `KindDesigner` làm khuôn.
5. `propose_skill` + quy trình duyệt 5 bước ở 4.5.
6. Skill chạy được thủ công (từ màn hình Skills) chứ không chỉ khi model
   tự chọn — người dùng cần thấy nó làm gì trước khi tin nó.

**Gate P3:**
- Một skill người dùng tự viết đổi được hành vi trên một tác vụ lặp lại,
  và người viết không phải hỏi ai.
- Một skill Syn tự đề xuất được duyệt và bật, và eval cho thấy nó không
  làm tệ đi tác vụ khác.
- Bật 20 skill vào không làm giảm điểm eval nền — nếu giảm, progressive
  disclosure chưa đủ và cần cắt bảng index xuống.

---

### P4 — Capability và consent (3–4 tuần)

**Vì sao ở đây và không muộn hơn:** P5 mở cửa ra ngoài. Cửa phải có khoá
trước khi mở.

**Làm gì:**
1. `Capability`, `Reversal`, `Grant` trong registry.
2. `{vault}/.synabit/consent.json`, ba mức, không sync.
3. Chế độ `plan_only` cho run.
4. Budget cứng: tool call, token, thời gian, tiền.
5. UI consent: một thẻ trong chat, không phải modal chặn — Syn dừng và hỏi
   ngay trong dòng hội thoại, giống cách nó báo tool call.
6. Audit log xem được.

**Gate P4:** một run thử với capability giả lập (một tool `send_test` không
làm gì) phải: hỏi đúng một lần, nhớ đúng lựa chọn, dừng đúng khi hết ngân
sách, và hiện đúng trong audit log. Chưa qua thì không sang P5.

---

### P5 — Ra thế giới (6–8 tuần)

**Làm gì, theo thứ tự:**
1. `fetch_url` từ `feed_engine` (1–2 tuần). Capability `NetRead{domain}`.
2. MCP client, transport HTTP/SSE (3 tuần). Settings thêm server, keychain
   giữ token, tool namespace `mcp:<server>:<tool>`.
3. MCP stdio, desktop-only (1–2 tuần). `tokio::process` trực tiếp.
4. `NetWrite` sau cùng, và cân nhắc dừng ở "soạn draft vào vault".

**Gate P5:** Syn hoàn thành một công việc bắc cầu — ví dụ "đọc trang này,
đối chiếu với ghi chú của tôi về nó, viết một note khác biệt" — với luồng
consent đọc được và không có bước nào khiến người dùng phải đoán chuyện gì
đang xảy ra.

---

### P6 — Chủ động (3–4 tuần)

1. Trigger thời gian, dùng `chat_engine` tick trên desktop và mô hình
   "kế hoạch trước" của `calendar/scheduler.rs` trên mobile.
2. Trigger sự kiện vault qua `watcher.rs`.
3. Run nền báo cáo vào `Messages/`.
4. Ràng buộc: run nền không được chạm capability cần hỏi; chạm thì dừng ở
   `AwaitingConsent` và gửi thẻ.

**Gate P6:** một run định kỳ chạy một tuần và người dùng không tắt nó.
Đó là tiêu chí thật; mọi tiêu chí khác cho tính năng chủ động đều là tự dối.

---

### P7 — Sandbox (tuỳ chọn, 4–6 tuần)

Chỉ làm nếu P3 cho thấy `recipe` không đủ, và có ít nhất ba ví dụ cụ thể
về skill không viết được bằng recipe. Chi tiết ở 4.9.

---

## 6. Bảy quyết định phải chốt trước khi gõ dòng code đầu tiên

### 6.1 RAG stuffing: giữ, giảm, hay bỏ?

**Khuyến nghị: giảm, có điều kiện.** Giữ một khối ngắn (~2.000 ký tự) và
chỉ khi câu hỏi trông giống câu hỏi về vault; bỏ hoàn toàn khi model đủ
mạnh (phát hiện qua `num_ctx` và provider). Lý do: model local 8B trên cửa
sổ 8K vẫn cần được nhồi, và sản phẩm hứa chạy được offline. Nhưng phải đo
trước — thí nghiệm đã dựng sẵn, chỉ cần chạy.

### 6.2 Model tier: nói thật với người dùng

Một agent tự chọn skill, tự viết skill, tự quyết định nhớ gì — **không chạy
được trên llama3.2:3b.** Cần nói ra, trong UI, tại chỗ chọn model:

| Tier | Model ví dụ | Syn làm được gì |
|---|---|---|
| Cơ bản | 3B–8B local | Chat, tìm, tạo/sửa node. Không skill, không memory tự động. |
| Đủ dùng | 20–30B local, hoặc hosted rẻ | Thêm skill và memory. |
| Đầy đủ | Hosted mạnh | Tự viết skill, run nhiều bước, MCP. |

Và kèm câu nói thẳng: chọn tier 3 nghĩa là note rời khỏi máy. Đây là chỗ
duy nhất trong sản phẩm mà lời hứa privacy có ngoại lệ, và ngoại lệ phải
được viết ra ở đúng nơi người dùng quyết định.

### 6.3 Ngôn ngữ

`build_system_prompt` hiện có ba personality với hướng dẫn tiếng Việt
(tao/mày, tôi/bạn). Skill và memory sẽ được viết bằng tiếng Việt. Câu hỏi:
skill do Syn viết nên viết bằng ngôn ngữ nào?

**Khuyến nghị:** ngôn ngữ của người dùng, và `description`/`when_to_use`
song ngữ nếu skill được chia sẻ. Model xử lý tiếng Việt trong prompt tốt;
vấn đề là FTS5 — `search.rs` đã xử lý dấu và `đ` (`utils/diacritics.ts` và
`search_fold.rs`), nên recall tiếng Việt ổn. Đừng ép tiếng Anh.

### 6.4 Embedding: chưa

Không thêm vector store ở P2. Lý do: vault cá nhân hiếm khi quá 20–50k
node, BM25 + graph đủ, và thêm embedding nghĩa là thêm một model để chạy,
một bảng để đồng bộ, một thứ để rebuild khi index hỏng. **Đo trước:** nếu
gate P2 thất bại vì recall chứ không phải vì chất lượng memory, lúc đó mới
thêm — và khi đó Ollama đã có embedding endpoint, làm cục bộ được.

### 6.5 Skill có sync không?

**Có.** `.md` trong vault ⇒ tự sync. Nhưng: skill tier `code` sync sang
điện thoại rồi không chạy được. Đó là N5, và UI phải nói "skill này cần
desktop" chứ không phải im lặng bỏ qua.

Câu hỏi phụ: có chợ skill không? **Chưa.** Skill từ người lạ là code từ
người lạ, kể cả khi nó chỉ là prose (prompt injection). Nếu làm, phải có
ký số và review — đó là một sản phẩm riêng, không phải một tính năng.

### 6.6 Vị trí của Syn trong app

Hiện Syn nằm trong mini-app "Messages", cùng chỗ với thẻ thông báo. Khi Syn
thành agent, nó không còn là một mini-app ngang hàng với 11 cái kia — nó là
thứ chạy *xuyên qua* tất cả.

**Khuyến nghị:** giữ Messages làm nơi hội thoại và báo cáo, nhưng thêm một
lối vào Syn ở mọi màn hình (một thanh lệnh, hoặc mở rộng QuickEntry đã có
hotkey toàn cục). `QuickEntry.vue` đã là một cửa sổ nhỏ nổi lên trên công
việc đang làm và biến mất — đó chính xác là hình dạng đúng cho "hỏi Syn một
câu ngay tại đây".

### 6.7 Thương mại hoá

Có license-server và `LicenseModal`. Agent là chỗ tự nhiên để đặt ranh giới
trả phí, nhưng cẩn thận: nếu tier miễn phí không có memory, sản phẩm sẽ bị
đánh giá qua một Syn hay quên. Đề xuất ranh giới: **memory và skill do
người dùng viết là miễn phí; skill do Syn tự viết, MCP, và run định kỳ là
trả phí.** Cái miễn phí giữ được lời hứa; cái trả phí là cái tốn tiền vận
hành và tốn model mạnh.

---

## 7. Những gì không nên làm

- **Đừng làm multi-agent / fleet.** Một app cá nhân không cần một dàn agent.
  Cái cần là sub-run có context riêng cho việc dài, và cái đó là một trường
  `parent_run_id` chứ không phải một kiến trúc.
- **Đừng viết tích hợp riêng cho từng dịch vụ.** MCP một lần.
- **Đừng cho agent sửa code của app.** Nó sửa *dữ liệu* và *hành vi*
  (skill), không sửa binary. Ranh giới này giữ cho app còn kiểm chứng được.
- **Đừng thêm vector DB trước khi đo.**
- **Đừng cho Syn tự bật skill của chính nó.**
- **Đừng biến `max_tool_iterations` thành vô hạn.** Trần là thứ ngăn một
  model đã quyết định tìm mãi tiêu tiền của người dùng. Comment ở
  `models/syn.rs` nói đúng; giữ nguyên tinh thần đó, chỉ chuyển nó vào
  `Budget`.
- **Đừng bỏ chế độ chỉ-local.** Nó là lý do sản phẩm tồn tại.

---

## 8. Đo lường và eval

Xây một lần ở P0/P1, dùng cho mọi phase sau.

**Vault seed cố định** — mở rộng `gate_one::seed` thành một module dùng lại
được: ~200 node, đủ nhiễu, có type do người dùng bịa ra, có tiếng Việt lẫn
tiếng Anh, có ngày tháng cần suy luận.

**Bộ câu hỏi 20–30 case**, mỗi case gồm: câu hỏi, các chuỗi *bắt buộc có*,
các chuỗi *cấm có*, và số tool call tối đa hợp lý. Dùng substring thay vì
judge model — đúng lý do `rag.rs:1290` đã nêu: kiểm được, rẻ, và câu trả
lời sai mà tình cờ đúng chuỗi vẫn nhìn thấy trong transcript.

**Bốn con số cho mỗi lần chạy:**
1. Tỉ lệ đúng (bao nhiêu case qua).
2. Số tool call trung bình (hiệu quả).
3. Token vào/ra trung bình (chi phí).
4. Tỉ lệ "bịa" — bao nhiêu lần xuất hiện chuỗi trong danh sách cấm.

**Chạy khi:** đổi prompt, thêm/sửa tool, bật skill mới, đổi model, và
trước mỗi gate. `cargo test --lib syn_eval -- --ignored --nocapture`, in
bảng, không assert — giữ đúng khuôn đã có.

**Và theo N3:** không gửi đi đâu cả. Nếu muốn biết người dùng thật ra sao,
áp ADR đã có: đếm tại chỗ, hiện cho họ xem. Ví dụ trong Syn settings:
*"Bạn có 12 skill, 4 cái đang bật. Syn đã dùng chúng 31 lần trong 30 ngày
qua."* Con số đó vừa là phép đo vừa là tính năng — nó dạy người dùng rằng
skill tồn tại.

---

## 9. Rủi ro, xếp theo mức độ

| Rủi ro | Vì sao đáng lo | Giảm thiểu |
|---|---|---|
| **Vault đầy rác** | Memory và run transcript sinh ra file liên tục. Một vault 5.000 note thành 15.000 file trong ba tháng, và người dùng mất niềm tin vào chính thư mục của mình. | Duyệt trước khi ghi memory; transcript run vào `Syn/runs/` (không phải `nodes`) và tự dọn sau N ngày; đếm và hiện số. |
| **Prompt phình, chất lượng giảm** | Mỗi phase thêm một section. Đến P5 thì system prompt 6.000 token và model bắt đầu bỏ sót. | `PromptPlan` có ngân sách cứng từ P0; eval chạy mỗi lần thêm section. |
| **Agent làm điều không đảo ngược được** | Xảy ra đúng một lần là mất người dùng. | P4 trước P5, không đảo thứ tự. Dry run. `Reversal` là trường bắt buộc trên mọi tool. |
| **Prompt injection từ nội dung** | Syn đọc feed article, trang web, file PDF, và sau này là output của MCP server. Tất cả đều là văn bản do người khác viết. | Đánh dấu rõ ranh giới nội dung không tin cậy trong prompt; không bao giờ để nội dung đọc được cấp capability; consent luôn hỏi người, không hỏi model. |
| **Model local không kham nổi** | Sản phẩm hứa chạy offline. Agent thật cần model mạnh. | Tier hoá công khai (6.2). Đừng giả vờ 3B làm được. |
| **Refactor P0 làm hỏng tính năng đang chạy** | `syn_send_message` đang hoạt động và có người dùng. | Giữ chữ ký lệnh; `gate_one` là bài kiểm tra hồi quy; làm P0 trên nhánh riêng và merge một lần. |
| **Android tụt lại** | Nhiều năng lực chỉ có trên desktop. Nếu không thiết kế suy biến từ đầu, bản Android thành phiên bản hỏng chứ không phải phiên bản nhỏ hơn. | N5 ngay từ P3. Mỗi skill/tool khai báo yêu cầu nền tảng; UI nói ra, không báo lỗi. |
| **Tự viết skill làm giảm chất lượng** | Skill sai nằm im trong prompt và làm hỏng mọi thứ khác. | Eval trước khi bật; người duyệt; version + `restore_version`; nút tắt hàng loạt. |

---

## 10. Ba tháng đầu, cụ thể

**Tháng 1 — P0.**
Tuần 1–2: `Run`/`Step`/`Budget`, persist, `drive()`. Tuần 3: `PromptPlan`
và `syn_preview_prompt`. Tuần 4: `ToolProvider` registry + `RunContext`,
`VaultTools` bọc 23 tool cũ. Chạy `gate_one` xác nhận không hồi quy.

**Tháng 2 — P1 rồi bắt đầu P2.**
Tuần 5: chạy `rag_vs_agentic` trên ba model, viết ADR, sửa recall.
Tuần 6–8: `type: memory`, tool `remember`/`recall`/`forget`, reflection
sinh đề xuất, hàng đợi duyệt.

**Tháng 3 — hoàn tất P2, dựng eval.**
Tuần 9–10: màn hình Memory, mâu thuẫn, `review_after`, i18n.
Tuần 11: bộ eval 20 case + vault seed dùng lại được.
Tuần 12: chạy gate P2. Dùng thật hai tuần trước khi sang P3.

Điểm dừng có thể ship: **cuối tháng 3.** Một Syn có Run xem được, có memory
người dùng sửa được, và một bộ eval nói được nó có tốt lên không. Đó đã là
một sản phẩm khác hẳn hôm nay, và nó không phá bất cứ lời hứa nào.

---

## Phụ lục A — 23 tool hiện có

| Nhóm | Tool |
|---|---|
| Đọc node | `query_nodes`, `get_node`, `list_schemas`, `get_linked_nodes` |
| Ghi node | `create_node`, `update_node`, `trash_node` |
| Hoàn tác | `list_trash`, `restore_node`, `list_versions`, `restore_version` |
| Cấu trúc (hai bước) | `rename_field`, `delete_field`, `rename_kind`, `delete_kind` |
| Files | `search_files`, `read_file_text` |
| Feeds | `search_feed_articles`, `update_feed_article` |
| Finance | `get_finance_summary`, `search_finance`, `create_transaction`, `get_transactions` |

## Phụ lục B — File cần đụng vào, theo phase

| Phase | Sửa | Thêm mới |
|---|---|---|
| P0 | `syn/engine.rs`, `syn/tools.rs`, `commands/syn.rs`, `lib.rs` | `syn/run.rs`, `syn/prompt.rs`, `syn/registry.rs`, `mini-apps/messages/components/RunPanel.vue` |
| P1 | `syn/rag.rs`, `search.rs` | `docs/adr-rag-vs-agentic-*.md` |
| P2 | `models/node.rs` (NodeType), `syn/registry.rs`, i18n | `syn/memory.rs`, `shared/views/MemoryManager.vue` |
| P3 | `syn/prompt.rs`, `shared/views/SchemaManager.vue` (khuôn) | `syn/skill.rs`, `syn/recipe.rs`, `shared/views/SkillManager.vue` |
| P4 | `syn/registry.rs`, `syn/run.rs`, `secrets.rs` | `syn/consent.rs`, `mini-apps/messages/components/ConsentCard.vue` |
| P5 | `feed_engine/fetcher.rs` (tái dùng), `Cargo.toml` | `syn/web.rs`, `syn/mcp/` |
| P6 | `chat_engine.rs`, `calendar/scheduler.rs`, `watcher.rs` | `syn/trigger.rs` |
