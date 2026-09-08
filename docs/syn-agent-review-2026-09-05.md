# Syn từ trợ lý sang cộng sự — kiểm toán 2026-09-05 và đường đi tiếp

**Ngày:** 2026-09-05
**Phạm vi:** toàn bộ Synabit, nhìn qua lăng kính "Syn có thể trở thành cái gì".
**Nối tiếp:** `docs/syn-agent-roadmap-2026-09-02.md`, mà bốn phase đầu của nó
đã được xây xong trong ba ngày kể từ khi nó được viết.
**Câu hỏi được trả lời:** đứng ở đây, cách một Jarvis còn bao xa, phần nào là
tính năng chưa có, phần nào là tính năng đã có mà chưa ai dùng, và cái gì
trong thế giới bên ngoài đáng học — cũng như cái gì đáng từ chối.

---

## 0. Tóm tắt cho người bận

Roadmap ngày 02-09 ước lượng 5–8 tháng cho bảy phase. **P0 đến P4 đã xong
trong ba ngày.** Module `syn/` đi từ khoảng 7.500 dòng lên 20.779. Có Run có
transcript, có PromptPlan có ngân sách, có registry với `Reversal`, có memory
là node, có skill hai tier với recipe runner chạy trong Rust, có consent
ledger ba mức không sync, có audit log, có màn hình quản trị năm tab. Từng
mảnh một đều được viết ở mức mà rất ít codebase đạt tới: mỗi quyết định khó
có một doc comment giải thích *tại sao*, và phần lớn có một test canh chừng.

Và đây là phát hiện quan trọng nhất của bản kiểm toán này, mà chính repo đã
tự tìm ra hai lần rồi chưa gọi tên:

> **Vấn đề không còn là "cái này có chạy không". Vấn đề là "cái này có bao
> giờ chạy không".**

Hai con số, cả hai đều do chính công cụ đo của repo sinh ra:

| Cơ chế | Được xây | Đã kích hoạt trong đời thực |
| --- | --- | --- |
| `recall` — đường duy nhất tới memory chưa pinned | 2026-09-04 | **0 / 15 run** (`adr-memory-shape`) |
| `repeated_chain` — trình phát hiện để Syn đề xuất skill | 2026-09-04 | **0 / 17 run** (commit `3677f56`) |

Đây không phải hai lỗi rời rạc. Đây là **cùng một hình dạng thất bại**: mỗi
năng lực mới đều được gắn vào đúng một đường vào — model tự quyết định gọi
tool — và một model đang trả lời câu hỏi bình thường không tự nhiên nghi ngờ
rằng có thứ nó chưa được kể. `adr-memory-shape` đã sửa chuyện này cho memory
(nhồi tất cả vào prompt, `recall` hạ xuống đường tràn). `skill.rs:1160` viết
thẳng rằng skill có thể hỏng y hệt và không gate nào trong roadmap kiểm tra
điều đó. Nó đúng, và nó vẫn đang chưa được kiểm tra.

Khoảng cách tới Jarvis, tính từ hôm nay, là **bốn** thứ chứ không phải bảy:

| # | Khoảng cách | Một câu chẩn đoán |
| --- | --- | --- |
| **G1** | **Firing** — không biết cái đã xây có được dùng không | Không có một con số nào trong app trả lời "Syn đã dùng memory/skill bao nhiêu lần trong 30 ngày". Chỉ có test `#[ignore]` chạy tay. |
| **G2** | **Presence** — Syn bị nhốt trong một mini-app | Syn là app thứ hai trong mười hai. `QuickEntry.vue` đã có hotkey toàn cục và **không** chạm tới Syn. Không có "bôi đen rồi hỏi Syn" ở bất cứ đâu. |
| **G3** | **Trần công việc dài** | Không có sub-run, không có compaction. `build_pruned_history` cắt cụt theo số message chứ không tóm tắt. Một việc kiểu "đọc 40 bài feed rồi tổng hợp" sẽ làm ngộp hội thoại chính. |
| **G4** | **Reach & Initiative** — P5 và P6, chưa có gì | Không MCP, không `fetch_url`. `Trigger` có đúng một arm là `User` (`run.rs:88`). |

Và một cảnh báo từ thế giới bên ngoài, nói ngay vì nó quyết định thứ tự làm:

> **OpenClaw là bằng chứng sống cho việc "với tới mọi thứ" mà không có kỷ luật
> sẽ đi tới đâu.** Command injection (CVE-2026-24763), SSRF (CVE-2026-26322),
> path traversal (CVE-2026-26329), prompt-injection dẫn tới RCE
> (CVE-2026-30741), một lỗ 1-click account-takeover-to-RCE tháng 01/2026,
> 40.214 instance phơi ra internet với 35–63% bị đánh giá là vulnerable, và
> hàng trăm skill trong registry chứa malware — trong đó có Atomic Stealer
> thu hoạch API key.
>
> Synabit đang có chính xác thứ OpenClaw thiếu: `Reversal` khai báo trên mọi
> tool, consent ledger không sync, dotfile cho bí mật, không có chợ skill,
> không có shell. **Đừng đánh đổi cái đó lấy tốc độ.** Nó không phải là gánh
> nặng làm chậm dự án; nó là sản phẩm.

Khuyến nghị lộ trình, và nó **đảo thứ tự so với roadmap cũ**: chèn một phase
ngắn **P4.5 — "cho cái đã có được dùng"** trước P5. Xây thêm reach lên trên
một tầng chưa ai chạm là nhân đôi lượng rác chứ không nhân đôi giá trị.

---

## 1. Ba ngày vừa qua: bảng đối chiếu với các gate

### 1.1 Quy mô, đo lại

| Lớp | 02-09 | 05-09 |
| --- | ---: | ---: |
| Rust core | ~85.600 dòng | **97.958** |
| Rust test | 1.378 | **1.549** |
| Lệnh Tauri | 198 | **240** |
| Front end | ~98.400 dòng | **101.282** |
| File spec | 109 | **118** |
| `syn/` (kể cả command + model) | ~7.500 | **20.779** |
| Tool | 23 | **27** |

Gần như toàn bộ mức tăng của Rust nằm trong `syn/`. Ba ngày, 13.000 dòng, và
chất lượng không tụt — đó là điều đáng nói trước khi nói bất cứ điều gì khác.

### 1.2 Từng phase, và gate của nó

**P0 — Nền móng. Xong.**

- `syn/run.rs` (1.034 dòng). `Run` có `id`, `goal`, `trigger`, `state`,
  `budget`, `spent`, `steps`. Transcript được flush *từng bước*
  (`save_run_best_effort` sau mỗi step) chính xác vì lý do đúng: trường hợp
  cần transcript nhất là trường hợp process không chạy tới cuối.
- `KEEP_RUNS = 200`, có lý giải gắn với chi phí của `list_runs`.
- `LIVE_RUNS` khoá theo `run_id` chứ không theo `conversation_id`, và
  `is_live()` phân biệt được run đang chạy thật với run bị bỏ lại khi app
  đóng — một chi tiết nhỏ mà phần lớn hệ thống bỏ sót.
- `syn/prompt.rs` (915 dòng). Chín `SectionKind`, ngân sách
  `DEFAULT_BUDGET_CHARS` được **dẫn xuất từ đo đạc** chứ không phải một số
  tròn: `FIXED_SECTIONS_CHARS + DEFAULT_CONTEXT_CHARS + HEADROOM_CHARS`, có
  test `the_fixed_sections_still_cost_what_the_budget_assumes` canh khi tiền
  đề trôi.
- `syn_preview_prompt` có, và có tab "prompt" trong `RunInspector.vue`.

**Gate P0 đạt.** Ba tiêu chí (đọc lại transcript sau khi huỷ, không hồi quy,
người dùng xem được prompt) đều có cơ chế đứng sau.

**P1 — Món nợ RAG. Xong, và là phần việc tốt nhất trong repo.**

`docs/adr-rag-vs-agentic-2026-09-03.md` không chỉ trả lời câu hỏi, nó còn
**bác bỏ dự đoán của chính bản nháp đầu tiên của nó** và ghi lại chuyện đó.
Kết luận ngược với roadmap: stuffing ở lại, vì nó hoà về độ chính xác và
thắng về chi phí (724 ký tự prompt mua lại 1,1 tool call mỗi câu). Trên
đường đi nó tìm ra một bug thật: `retrieve_context` đặt `match_any`,
`tool_query_nodes` thì không — nửa app tìm bằng `OR`, nửa kia bằng `AND`.
Sửa xong, nhánh agentic đi từ 4/5 lên 15/15.

Và nó ghi lại bốn lần cái thước đo bị sai trước khi đúng, trong đó có lần
chấm một câu trả lời tiếng Việt đúng thành sai vì marker là tiếng Anh. Đó là
loại trung thực rất hiếm.

**P2 — Memory. Xong, rồi bị viết lại vì một phép đo.**

`syn/memory.rs` (1.806 dòng). `type: syn_memory`, thư mục `SynMemory/`,
frontmatter có `kind`/`subject`/`confidence`/`source_run`/`review_after`/
`pinned`. `adr-memory-shape-2026-09-04` đảo quyết định đọc: **mọi memory đều
vào prompt**, `pinned` từ "có tồn tại hay không" trở thành "cái gì sống sót
khi hết ngân sách". Lý do là số: ở độ dài memory thật (43 ký tự), 3.200 ký tự
chứa được năm mươi cái, nên sự phân biệt pinned/unpinned không mua được gì
cho tới memory thứ năm mươi — và nó đang làm hỏng cả tính năng.

Gate P2 nửa model đã chạy: 20 case, 40 lời gọi, nhóm pinned 9/14 tốt hơn,
nhóm unpinned 4/4 tốt hơn, hai control không đổi. Có người đọc từng cặp,
không dùng judge model — đúng bài học của ADR trước.

**P3 — Skill, tier `prose` và `recipe`. Xong.**

`syn/skill.rs` (1.268) + `syn/recipe.rs` (929). `type: syn_skill`, thư mục
`SynSkills/`, `INDEX_BUDGET_CHARS = 4.200`, `BODIES_PER_RUN = 2`,
`LOAD_TOOL = "load_skill"`. Recipe là YAML trong fence ` ```recipe ` **trong
thân skill chứ không trong frontmatter**, vì "một thủ tục người ta không đọc
được cạnh lời giải thích của nó là thủ tục người ta sẽ không kiểm".

`NOT_IN_A_RECIPE` chặn bốn tool cấu trúc, và lý do được nêu rất sắc: nhờ chặn
chúng mà `run_recipe` khai báo được **một** capability trung thực là
`VaultWrite`, thay vì phải khai báo thứ mạnh nhất nó có thể chạm tới.

`repeated_chain()` phát hiện mẫu lặp bằng **số học chứ không bằng model** —
rẻ, test được offline, và không thể bịa ra một mẫu không có. Đây là một cải
tiến so với roadmap (roadmap giao việc này cho reflection).

**P4 — Capability và consent. Xong, và gate đóng đúng cách.**

`syn/consent.rs` (578) + `syn/audit.rs` (298). Bảy `Capability`, ba `Answer`
(`Once`/`Always`/`Never`), `ALWAYS_LASTS_DAYS = 90`. Ledger ở
`{vault}/.synabit/consent.json` — dotfile, **không sync**, và sự bất đối xứng
được nêu rõ: `Syn/declined.json` *có* sync, vì "một lời từ chối về chính mình
thì đi theo người, một lời cho phép thì không".

`SendTest` là một `ToolProvider` chỉ tồn tại dưới `cfg(test)` — đúng thứ gate
P4 yêu cầu, và nó không được đưa vào `Registry::for_chat` vì "một tool trong
prompt tốn token mỗi lượt của mọi hội thoại".

`ConsentCard.vue` là **card trong hội thoại, không phải modal**, với lý do
viết ngay trong `useSynConsent.ts:1-13`: modal huấn luyện người ta bấm nút
làm nó biến mất.

**P5, P6, P7 — chưa bắt đầu.** Không MCP, không `fetch_url`, không
`search_web`, `Trigger` một arm, không sandbox. Đúng như kế hoạch.

### 1.3 Bảy chỗ đã xây nhưng chưa hoàn tất — kiểm tra kỹ mới thấy

Đây là phần giá trị nhất của việc đọc lại code, vì mỗi mục là một tính năng
đã trả tiền rồi mà chưa thu về.

**a) `plan_only` là một tính năng vô hình.**
`run.plan_only` tồn tại (`run.rs:379`), engine tôn trọng nó
(`engine.rs:485`), và nó **chỉ được đặt thành `true` ở đúng một chỗ:
`engine.rs:1690`, trong một test.** Không lệnh Tauri nào bật nó, không nút
nào trong UI. Dry run — thứ roadmap gọi là "`confirm_nodes` mở rộng ra toàn
hệ thống" — đã được xây và chưa ai chạy được.

**b) `Registry::definitions(ctx)` nhận `ctx` rồi bỏ qua nó.**
`VaultTools::definitions` gọi `get_tool_definitions()` không tham số
(`registry.rs:213`). Cái seam đã cắt đúng chỗ, nhưng **danh sách tool vẫn
tĩnh 27 cái**. Theo `adr-memory-shape`, schema tool tốn **16.803 ký tự mỗi
lượt** so với system prompt 20.940 — nghĩa là mô tả tool đang chiếm 44% của
mọi lượt trước khi hội thoại nói gì. Progressive disclosure đang có ở tầng
skill, chưa có ở tầng tool. Khi MCP tới, đây là chỗ vỡ trước.

**c) Consent hỏi xong thì run chết.**
`LoopEnd::NeedsConsent` kết thúc run ở `AwaitingConsent`
(`engine.rs:603-613`). `syn_answer_consent` ghi grant, xoá `pending_consent`,
lưu — **và không tiếp tục gì cả**. Lý do được viết ra và cho giai đoạn này là
hợp lý: "cách tự nhiên để nói 'làm tiếp' là nói ra". Nhưng nó **sẽ là lỗi**
ngay khi có run nền ở P6, nơi không có ai đang gõ. Cần ghi nhận đây là một
quyết định có hạn dùng, không phải một quyết định vĩnh viễn.

**d) Không có ngân sách token, không có ngân sách tiền.**
`Budget::from_settings` đặt `tokens: None` (`run.rs:307`), và không có trường
tiền. Lý do trung thực (provider báo token không nhất quán; bảng giá là thứ
app sẽ phải giữ cho đúng). Nhưng roadmap liệt kê cả bốn trần trong gate P4,
và hai trong bốn đang trống. Với model hosted, đây là khoảng cách giữa "Syn
tốn bao nhiêu" và "không ai biết".

**e) Dấu vết run chỉ được đóng lên memory.**
`ToolContext.run_id` tồn tại và được dùng **đúng một chỗ**:
`tools.rs:887`, trong `memory::frontmatter`. Một note mà Syn tạo hay sửa
không mang dấu vết nào về run nào đã làm việc đó. `run.rs` từ chối biến run
thành node — quyết định đúng, lý giải kỹ — nhưng hệ quả của quyết định đó
chưa được trả: câu "run nào đã sửa note này" hiện không trả lời được. Memory
có `source_run`; phần còn lại của vault thì không.

**f) Không có `{vault}/SYN.md`.**
`custom_system_prompt` nằm trong `Syn/settings.json`. Nghịch lý: skill là
file Markdown người dùng đọc và sửa được, memory là file Markdown người dùng
đọc và sửa được, còn **chỉ dẫn thường trực** — thứ ảnh hưởng tới mọi câu trả
lời — thì nằm trong một trường JSON. Đây là `CLAUDE.md` / `AGENTS.md` của
Synabit và nó đang thiếu. Nó là thay đổi rẻ nhất trong toàn bộ tài liệu này.

**g) Cửa thứ ba của memory vẫn chưa mở.**
`adr-memory-shape` §4 liệt kê ba đường ghi: explicit, reflection, và
**correction** — khi người dùng sửa Syn ("không phải, tao..."), tín hiệu có
giá trị cao nhất mà app từng nhận được. Nó vẫn chưa sinh ra gì. ADR cũng liệt
kê bốn việc của "quên" (decline phải ở lại đã decline, supersedes phải được
đề xuất, review phải nổi lên, decay chỉ hạ bậc) — cần kiểm lại xem bao nhiêu
trong số đó đã làm.

---

## 2. Phát hiện trung tâm: một ngôi nhà đã xây xong mà chưa ai ở

### 2.1 Hai con số, một hình dạng

`recall`: 0/15 run. `repeated_chain`: 0/17 run. Cả hai đều được tìm ra bởi
chính công cụ đo của repo, không phải bởi người dùng phàn nàn. Đó là văn hoá
đúng, và nó đã hoạt động hai lần.

Nhưng hãy nhìn hình dạng chung:

```
tính năng mới  →  một tool  →  model tự quyết định gọi  →  không bao giờ gọi
```

`adr-memory-shape` gọi tên chính xác: *"một model đang trả lời một câu hỏi
bình thường không tự nhiên nghi ngờ rằng thứ nó chưa từng được kể có tồn
tại."* Cách sửa cho memory là **bỏ hẳn cửa hẹp**: nhồi tất cả vào prompt.

Skill không dùng được cách đó — bốn mươi thủ tục không nhét vừa chỗ của bốn
mươi câu. Nên `skill.rs:1150-1165` viết thẳng rằng nó có thể hỏng y hệt và
"phản ứng đúng là đo xem cái này có được gọi không, chứ không phải giả định
rằng nó sẽ được gọi". Cái đo đó chưa tồn tại như một con số trong app.

Và commit `3677f56` cho thấy một tầng nữa của cùng vấn đề: trong 17 run
thật, **16 run có ít hơn 6 lời gọi tool thành công** — dưới sàn để một chuỗi
ba bước xuất hiện hai lần. Không phải trình phát hiện tồi. Là **Syn chưa bao
giờ được giao việc đủ lớn để có mẫu.** Cả hệ thống skill đang chờ một loại
công việc mà cách dùng hiện tại không sinh ra.

### 2.2 Hệ quả cho thứ tự làm việc

Nếu ba tháng tới bỏ vào P5 (MCP, web), kết quả gần như chắc chắn là: thêm
mười lăm tool, thêm 20% chi phí prompt mỗi lượt, thêm một bề mặt tấn công —
và ba cơ chế nữa chưa từng chạy nằm cạnh hai cái đã có.

**Vì vậy: đo trước, mở rộng sau.** Chi tiết ở mục 7.

### 2.3 Ba câu hỏi phải có số trả lời

1. Trong 30 ngày qua, bao nhiêu run có section Memory khác rỗng, và trong
   đó bao nhiêu run mà memory thật sự đổi câu trả lời?
2. `load_skill` đã được gọi bao nhiêu lần? `run_recipe` bao nhiêu lần?
3. Bao nhiêu run chạm trần, trần nào, và phân bố số round là gì?

Cả ba đều đọc được từ `Syn/runs/*.json` mà không cần thêm hạ tầng nào.
`adr-measuring-the-slope-2026-08-29` đã giải đúng bài toán này rồi: **không
có telemetry thì đếm tại chỗ và cho người dùng xem con số.** Ở đây, con số
đó vừa là phép đo cho người xây vừa là tính năng cho người dùng — nó dạy họ
rằng skill tồn tại.

---

## 3. Kiểm toán từng mặt

### 3.1 Triết lý — mạnh nhất, và có đúng một chỗ đang căng

Luận điểm trung tâm vẫn đúng và ngày càng đúng hơn: **vault là agent runtime,
và một memory là một node, một skill là một node.** Điều này không phải khẩu
hiệu — nó đem lại miễn phí: version history, trash, sync E2EE, FTS5, graph
edge, và khả năng mở bằng bất cứ editor nào. Hermes phải tự dựng SQLite+FTS5
để có "holographic memory"; Synabit đã có nó từ trước khi có agent.

Sáu nguyên tắc N1–N6 của roadmap vẫn được giữ trong code, và giữ đúng chỗ khó:

- N1 (mọi thứ học được sống trong vault dưới dạng file người đọc được) — giữ,
  trừ `custom_system_prompt` (mục 1.3f).
- N4 (bí mật không vào vault) — giữ, có test canh
  `no_secret_is_ever_written_into_the_vault`, và consent ledger nằm dotfile.
- N6 (không đảo ngược được thì phải hỏi) — giữ, và `reversal_of()` biến nó
  từ "đúng khi soi kỹ" thành "đúng theo khai báo".

**Chỗ đang căng: README hứa "AI on your own machine".**

Mọi phép đo nghiêm túc trong hai ADR đều chạy trên `gpt-5.6-luna` qua provider
OpenAI-compat. Model local `gemma4:e4b` **chậm 7,6× và thua 3 điểm**, và điều
làm nó thua không phải chất lượng ngôn ngữ — mà là *nó không soạn một truy vấn
thứ hai, có cấu trúc, sau khi truy vấn thứ nhất trả về rỗng*. ADR còn thử một
câu prompt để sửa và đo thấy nó không sửa được gì, rồi revert.

Đó là ranh giới năng lực thật, và nó chưa được nói với người dùng ở chỗ họ
chọn model. Roadmap §6.2 đề xuất bảng ba tier; nó vẫn chưa được làm. **Đây là
món nợ trung thực, không phải món nợ kỹ thuật**, và nó là loại nợ mà một sản
phẩm bán bằng lời hứa privacy không được để lâu.

### 3.2 Kiến trúc — đúng chỗ, thiếu bốn seam

**Đã đúng:**
- Provider là trait; khác biệt wire-format bị nhốt ở biên.
- Tool generic theo node chứ không theo mini-app — chạm được cả type app chưa
  từng nghe. Đây chính xác là tính chất "universal node" mà một agent cần.
- Consent nằm *trước* vòng lặp tool, trước cả ngân sách skill, với lý do
  đúng: "một lời từ chối không nên bị trừ vào hạn mức".
- Run là đơn vị công việc, không phải message.

**Còn thiếu:**

**(a) Sub-run.** Không có `parent_run_id`. Roadmap nói đây là "một trường, chứ
không phải một kiến trúc" — và nó đúng, nhưng trường đó chưa có. Hệ quả: mọi
việc dài đều chia sẻ một context với hội thoại chính. Claude Code giải bài
này bằng subagent có cửa sổ context riêng, tool riêng, chỉ dẫn riêng, và chỉ
trả về kết luận. Đó là mô hình đúng cho "đọc 40 bài feed rồi tóm tắt".

**(b) Compaction.** `build_pruned_history` cắt theo `max_history_messages` —
tức là **vứt** chứ không **nén**. OpenClaw tóm tắt các lượt cũ thành entry
nén khi sắp tràn context. Với run 12 round và mỗi kết quả tool vài trăm token,
đây là trần cứng của công việc dài, và nó đang không được nhìn thấy vì chưa ai
giao cho Syn việc đủ dài.

**(c) Tool chạy tuần tự.** `for tc in &reply.tool_calls` (`engine.rs:401`).
Ba `get_node` độc lập trong một round = ba lượt khoá DB nối tiếp. Với model
hosted, đây là khác biệt có thể đo được giữa 36s và khoảng 20s cho cùng một
công việc, và nó rẻ để sửa cho các tool `VaultRead` (những tool không đổi gì
thì thứ tự không quan trọng).

**(d) Không có tầng cưỡng chế mà người dùng viết được.** Mọi ràng buộc hành vi
hiện nằm trong section `Rules` của prompt — tức là *gợi ý*, model có thể lơ.
Claude Code phân biệt rõ hai thứ: `CLAUDE.md` là chỉ dẫn (có thể lơ), hook là
kiến trúc (không thể lơ, một hook chặn tool call thì không lý luận vòng qua
được). Synabit có consent — đó là cưỡng chế thật, và tốt. Nhưng người dùng
không có chỗ nào để viết luật của riêng họ ("đừng bao giờ sửa note trong
`Archive/`", "hỏi trước khi tạo quá 5 node"). Đó là khoảng trống giữa
`custom_system_prompt` (gợi ý) và `Capability` (do app định nghĩa).

### 3.3 Cấu trúc dữ liệu — mạnh nhất trong toàn bộ dự án

`NodeType` có 19 loại đã biết cộng `Other(String)`, với test
`an_unknown_type_survives_the_round_trip_unchanged` canh chừng. Việc thêm
`syn_memory` và `syn_skill` **không cần migration nào** và tự động có mặt
trong Things, Nexus, search, sync. Roadmap gọi đây là "món quà lớn nhất mà
kiến trúc hiện tại tặng cho dự án này", và ba ngày qua đã chứng minh: cả
memory lẫn skill đều ship được trong một ngày mỗi cái, phần lớn nhờ chuyện này.

Quyết định đặt tên `syn_memory`/`syn_skill` thay vì `memory`/`skill` là một
quyết định nhỏ mà rất đúng, và lý do được ghi ở `skill.rs:40-46`: từ không
tiền tố thuộc về người dùng, ai đó theo dõi *kỹ năng họ đang học* có toàn
quyền dùng kind tên `skill`.

Quyết định `SkillFolder = "SynSkills"` ở top-level chứ không dưới `Syn/` cũng
vậy: `is_in_unscanned_dir` bỏ qua `Syn`, nên skill viết ở đó sẽ được index bởi
chính lần ghi tạo ra nó rồi biến mất ở lần quét đầy đủ tiếp theo — "đúng trên
đĩa và không với tới được". Comment nói cái bẫy đó đã tốn một buổi chiều.

**Hai chỗ hở:**

1. **Provenance của hành động** (mục 1.3e). Đề xuất rẻ: khi Syn ghi một node,
   đóng `properties.syn_run: <run_id>` vào frontmatter. Nó biến "tại sao note
   này thay đổi" thành một cú nhấp, và nó dùng lại `run_id` đã có sẵn trong
   `ToolContext`.
2. **Không có nơi cho chỉ dẫn thường trực dạng file** (mục 1.3f).

### 3.4 Source code — chất lượng cao, một chỗ sắp quá tải

Văn hoá comment ở đây là thứ tôi hiếm khi thấy. Vài ví dụ đáng giữ:

- `tools.rs:3180-3196`: test kiểm tra danh sách tool **không** bằng cách đếm,
  mà bằng cách bắt mọi tool phải thuộc về một trong hai nhóm có lý do —
  "một con số cho bạn biết danh sách đã đổi, và không cho biết gì về việc thay
  đổi đó có phải loại làm hỏng nó không".
- `run.rs:_every_variant_is_listed`: một hàm `#[allow(dead_code)]` tồn tại chỉ
  để trình biên dịch từ chối build khi có variant mới mà `RunState::ALL` chưa
  cập nhật — "một test không làm được việc này; chỉ compiler biết hết".
- `registry.rs:29-31`: từ chối thêm arm enum chưa có ai sinh ra, vì "một arm
  không có producer là một lời hứa code không giữ được".

**Rủi ro tập trung: `engine.rs::drive_inner`.** Vòng lặp này giờ gánh:
prune history, streaming vs blocking, cancel, budget, consent, audit, skill
budget, plan_only, ghi step, emit event. Nó vẫn đọc được — nhưng nó là chỗ mà
P5, P6 và mọi thứ sau đó sẽ chèn tiếp vào.

Đề xuất trước khi P5: tách "quyết định phải làm gì với một tool call trước khi
chạy nó" ra thành một **hàm thuần** trả về enum
(`Run | Refuse(reason) | Describe(plan) | Ask(consent)`). Nó test được không
cần model, không cần network, không cần `AppHandle` — và mọi phase sau chỉ
thêm một arm chứ không thêm một tầng lồng nữa.

`tools.rs` ở 3.791 dòng một file cũng là thứ sẽ vỡ khi có provider thứ hai.
`registry.rs` đã cắt đúng seam để chia nó; chưa cần chia hôm nay.

### 3.5 UI/UX — mặt yếu nhất so với tham vọng

Đây là chỗ khoảng cách tới "Jarvis" lớn nhất, và cũng là chỗ rẻ nhất để thu hẹp.

**Syn là một tab.** `appRegistry.ts` liệt kê 12 mini-app; Syn sống trong
`messages`, vị trí thứ hai. Roadmap §6.6 đã nói đúng: khi Syn thành agent, nó
**không còn ngang hàng với 11 cái kia** — nó là thứ chạy *xuyên qua* tất cả.
Hiện tại muốn hỏi Syn về note đang mở, người dùng phải rời note đó.

**`QuickEntry.vue` đã có hotkey toàn cục và không chạm Syn.** Đây là phát hiện
đáng giá nhất về UX trong toàn bộ bản kiểm toán: cái cửa sổ nhỏ nổi lên trên
công việc đang làm rồi biến mất — hình dạng chính xác của "hỏi Syn một câu ngay
tại đây" — đã tồn tại, đã có phím tắt, và chỉ biết lưu quickcap.

**Không có "bôi đen rồi hỏi Syn"** ở Notes, Whiteboard, hay Files. Với một app
mà thao tác chính là đọc và viết, đây là điểm tiếp xúc tự nhiên nhất và nó
trống.

**`RunInspector.vue` là developer console.** 889 dòng, năm tab: runs, prompt,
memory, skills, permissions. Cho giai đoạn này nó đúng — người xây cần nhìn
thấy mọi thứ. Nhưng người dùng thường sẽ không mở tab "prompt", và tab đó là
nơi duy nhất họ biết được Syn nhớ gì về họ. Khi P4.5 thêm màn hình "Syn đã làm
gì", nó nên là **một trang có số**, không phải tab thứ sáu của cái console này.

**Không có nút Plan.** `plan_only` đã xây (mục 1.3a). Claude Code có plan mode
với một phím tắt và người dùng dùng nó liên tục. Ở đây nó vô hình.

**Ống báo cáo cho run nền chưa có variant.** `chat_engine.rs` sinh
`ChatMessage` vào `{vault}/Messages/`, `NotificationCard.vue` vẽ chúng. Roadmap
nói đúng rằng không cần UI mới — nhưng cần một variant "run đã xong, đây là kết
quả, mở transcript", và nó chưa có.

**Điểm sáng:** `ConsentCard.vue` làm đúng — card trong hội thoại, không modal,
i18n theo variant chứ không theo chuỗi tiếng Anh ("một permission prompt là chỗ
cuối cùng được phép rơi về sai ngôn ngữ"). Đây là chuẩn mực cho mọi thứ tương
tác sau này.

---

## 4. So sánh với thế giới, và cái đáng học

### 4.1 Bảng

| Trục | **Syn (Synabit)** | **Claude Code** | **OpenClaw** | **Hermes** |
| --- | --- | --- | --- | --- |
| Đơn vị công việc | `Run`, persist, có budget | session + subagent có context riêng | agentic loop qua gateway | agent loop |
| Memory | Node Markdown trong vault; **tất cả** vào prompt; FTS5 + graph | `CLAUDE.md` + memory files | 3 tầng: `MEMORY.md` luôn có / daily / deep + embedding | `MEMORY.md` + `USER.md`; SQLite FTS5 |
| Skill | Node Markdown, 2 tier (`prose`/`recipe`), index + `load_skill` | `SKILL.md`, **3 tầng** disclosure | skill folder + registry công khai | tự sinh từ workflow thật, `~/.hermes/skills/` |
| Tự viết skill | Phát hiện bằng **số học**, đề xuất, người duyệt, có eval trước khi bật | người viết | người viết / tải về | agent tự viết khi thấy mẫu lặp |
| Progressive disclosure | 2 tầng (index → body) | **3 tầng** (name/desc → body → file tham chiếu) | tương tự | — |
| Cưỡng chế hành vi | `Capability` + consent ledger | **hook** (không lý luận vòng qua được) + permission mode | tool policy trong config | — |
| Với tới thế giới | **không có gì** | MCP + bash + web | mọi thứ: browser, gh CLI, messaging | tích hợp sẵn |
| Sandbox chạy code | không (cố ý hoãn) | có | có (và là nguồn của nhiều CVE) | có |
| Dữ liệu ở đâu | máy người dùng, file người đọc được | máy người dùng | máy người dùng, gateway | máy người dùng |
| Chợ skill | **không** (cố ý) | plugin/marketplace có kiểm | ClawHub — **đã bị nhiễm malware hàng loạt** | không |
| Compaction | **không** (chỉ cắt cụt) | có | có | có |

### 4.2 Bốn thứ đáng học, cụ thể

**(1) Tầng disclosure thứ ba — từ Claude Code.**
Ở đó, thân `SKILL.md` có thể trỏ tới file cạnh nó, và file đó **không tốn gì
cho tới lúc thật sự mở**. Synabit đang có hai tầng và bù bằng một cái trần cùn
là `BODIES_PER_RUN = 2`. Với skill dài (một quy trình review thật sự), hai tầng
buộc phải chọn giữa "thân ngắn thiếu chi tiết" và "thân dài ngốn context".
Tầng thứ ba giải đúng chuyện đó, và trong Synabit nó gần như miễn phí: một
skill trỏ tới một node khác bằng wikilink, và `load_skill` mở nó theo yêu cầu.

**(2) Sub-agent có context riêng — từ Claude Code.**
Là câu trả lời cho G3 và nó rẻ hơn nó nghe: `parent_run_id`, một `Run` con
được `drive` với history riêng, và chỉ **kết luận** quay về run cha. Không cần
"fleet", không cần multi-agent framework. Roadmap 09-02 đã nói đúng điều này
và nó vẫn đúng.

**(3) Compaction — từ OpenClaw và Hermes.**
Thay `build_pruned_history` cắt cụt bằng: khi sắp tràn, gọi một lượt rẻ để nén
các lượt cũ thành một entry, giữ ngữ nghĩa. Đây là điều kiện cần cho bất cứ
công việc dài nào, và nó nên đi cùng sub-run ở cùng một phase.

**(4) Skill sinh ra từ công việc thật, không từ chợ — Hermes làm đúng, và
Synabit đang làm tốt hơn.**
Hermes sinh skill khi phát hiện mẫu lặp trong workflow thật của người dùng —
đúng hướng. Synabit làm cùng việc đó bằng `repeated_chain()`, tức là bằng số
học chứ không bằng một lượt gọi model sau mỗi run. Đó là một cải tiến thật.
Thứ Hermes có mà Synabit thiếu là **cửa correction**: `adr-memory-shape` ghi
nhận Hermes tạo skill khi "người dùng sửa cách làm của nó", và kết luận rằng
tín hiệu đó ít nhất cũng đáng một memory proposal ở đây. Vẫn chưa làm.

Và một thứ Hermes có mà Synabit nên copy nguyên: **`USER.md` compact do agent
tự bảo trì.** Ở Synabit nó là `SYN.md` (mục 1.3f) — nhưng nên là hai file:
`SYN.md` (người dùng viết, Syn tuân theo) và profile do Syn bảo trì. Ranh giới
"ai sở hữu file nào" phải rõ, vì trộn hai thứ là cách nhanh nhất để người dùng
mất niềm tin vào cả hai.

### 4.3 Bốn thứ phải từ chối

**(1) Mô hình "với tới mọi thứ trước, khoá sau" của OpenClaw.**
Bảng chứng cứ đã nêu ở mục 0. Điều đáng nói thêm: phần lớn CVE của OpenClaw
không phải lỗi model — chúng là **lỗi kiến trúc**: gateway có quyền sửa chính
sách tool, skill từ registry chạy với quyền của agent, không có ranh giới giữa
nội dung đọc được và chỉ dẫn. Synabit đã tránh cả ba, và phải tiếp tục tránh
khi mở P5:
- Nội dung lấy từ web/MCP **phải** được đánh dấu là không tin cậy trong prompt.
- Nội dung đọc được **không bao giờ** được cấp capability. Consent hỏi người,
  không hỏi model.
- `Capability` phải do app định nghĩa, không do server MCP tự khai.

**(2) Chợ skill.** Roadmap §6.5 nói "chưa". Bằng chứng từ ClawHub cho thấy
"chưa" nên đọc là "không, trừ khi có ký số và review, và khi đó nó là một sản
phẩm riêng". Skill từ người lạ là prompt injection từ người lạ, kể cả khi nó
chỉ là văn xuôi.

**(3) Embedding cho memory.** `adr-memory-shape` đã bác bỏ với lý do đúng: ở
quy mô năm mươi mục, so khớp từ là chính xác, miễn phí và debug được. Giữ
nguyên. Ngưỡng để xem xét lại là vài trăm mục, không phải vài chục.

**(4) Multi-agent fleet.** Một app cá nhân không cần một dàn agent. Sub-run là
một trường; fleet là một kiến trúc.

---

## 5. Đích đến — "Jarvis" nghĩa là gì, cụ thể

Bốn tính chất khẳng định và một tính chất phủ định. Mỗi cái có một tiêu chí
đo được, vì "giống Jarvis" thì không đo được.

**J1 — Có mặt.** Gọi được từ mọi màn hình mà không rời việc đang làm.
*Đo:* trong một tuần dùng thật, ≥ 50% lượt gọi Syn đến từ ngoài app Messages.

**J2 — Liên tục.** Nhớ giữa các phiên, và trả lời được "lần trước mình quyết
gì" mà không cần người dùng kể lại.
*Đo:* ≥ 1 lần/ngày, câu trả lời của Syn dùng một memory hoặc một run cũ mà
người dùng không nhắc tới trong lượt đó.

**J3 — Với tới.** Làm được việc bắc cầu giữa vault và thế giới, với luồng
consent mà người dùng đọc được và không phải đoán.
*Đo:* gate P5 của roadmap cũ, giữ nguyên: "đọc trang này, đối chiếu với ghi
chú của tôi về nó, viết một note khác biệt" — chạy hết, không bước nào khó hiểu.

**J4 — Đi trước.** Nói trước khi được hỏi, đúng lúc.
*Đo:* gate P6 của roadmap cũ, giữ nguyên và nó là tiêu chí thật duy nhất cho
tính năng chủ động: **một run định kỳ chạy một tuần và người dùng không tắt nó.**

**J5 — Không bao giờ làm điều không undo được mà không hỏi.**
*Đo:* audit log không có dòng nào ở mức `NetWrite`/`Spend`/`Execute` mà không
có một `ConsentGiven` đứng trước nó. Đây là tính chất phải đúng 100%, không
phải một tỉ lệ.

Đáng nói: **J1 và J2 gần như đã trong tầm tay.** J2 có đủ hạ tầng, chỉ thiếu
đo lường và cửa correction. J1 chỉ thiếu việc nối QuickEntry vào Syn. J3 và J4
là công việc thật nhiều tháng.

---

## 6. Bảy khoảng cách còn lại, xếp theo giá trị trên công sức

| # | Khoảng cách | Công sức | Giá trị | Chặn cái gì |
| --- | --- | --- | --- | --- |
| **G1** | Không đo được cái đã xây có chạy không | Thấp | **Rất cao** | Mọi quyết định về việc làm gì tiếp |
| **G2** | Syn nhốt trong một mini-app | Thấp | **Rất cao** | J1 |
| **G6** | Không có `SYN.md`; provenance hành động thiếu | Thấp | Cao | Niềm tin, và khả năng người dùng nắn Syn |
| **G7** | Chưa nói thật về model tier | Thấp | Cao | Lời hứa privacy |
| **G3** | Không sub-run, không compaction | Trung bình | Cao | J3, J4, và toàn bộ công việc dài |
| **G4** | Không có đường ra (P5) | Cao | Cao | J3 |
| **G5** | Không có trigger (P6) | Trung bình | Cao | J4 |

Bốn khoảng cách đầu cộng lại rẻ hơn nửa của G4, và mỗi cái đều gỡ chặn một
thứ khác. Đó là lý do lộ trình dưới đây đảo thứ tự.

---

## 7. Lộ trình

### P4.5 — Cho cái đã có được dùng (2–3 tuần)

**Vì sao trước P5:** hai cơ chế đã xây chưa từng chạy. Xây thêm ba cơ chế nữa
lên trên chúng là nhân đôi rủi ro chứ không nhân đôi giá trị. Phase này không
thêm năng lực nào — nó biến năng lực đã có thành thứ dùng được và đo được.

1. **`syn_stats`, và một màn hình cho nó.** Đọc `Syn/runs/*.json`: số run, phân
   bố số round, tần suất chạm trần và trần nào, số lần `load_skill` /
   `run_recipe` / `remember` / `recall`, số run có memory trong prompt. Một
   trang, số, không phải log. Theo N3: không gửi đi đâu.
2. **QuickEntry → Syn.** Hotkey toàn cục đã có; thêm một chế độ hỏi Syn và trả
   lời tại chỗ. Đây là G2 gần như trọn vẹn với chi phí thấp nhất.
3. **"Bôi đen rồi hỏi Syn"** ở Notes trước, các app khác sau.
4. **`{vault}/SYN.md`** thay cho `custom_system_prompt` trong JSON. Đọc vào
   `SectionKind::Custom` (vị trí đã có sẵn ở đầu prompt). Di trú: nếu settings
   có chuỗi cũ và chưa có file, viết file rồi để trống trường.
5. **Nút Plan** trong chat, bật `run.plan_only`. Một lệnh Tauri, một nút.
6. **Đóng dấu run lên node Syn ghi** — `properties.syn_run` trong frontmatter,
   dùng `ToolContext.run_id` đã có.
7. **Cửa correction cho memory** — khi người dùng sửa Syn, sinh một proposal
   được đánh dấu `from_correction`. Trường đó đã tồn tại trong `reflect.rs`
   và chưa có gì đặt nó.
8. **Bảng model tier** tại chỗ chọn model, kèm một câu thẳng: chọn provider
   hosted nghĩa là note rời khỏi máy.
9. **Đọc và ghi spreadsheet.** `calamine` để đọc ô/cột/sheet, `rust_xlsxwriter`
   để ghi, cộng hai tool. Chạy hoàn toàn local, không mạng, không consent,
   chạy cả trên Android, và không bị chặn bởi bất cứ phase nào. Xem mục 7B
   loại 3 — đây là tỉ lệ giá trị trên rủi ro cao nhất trong toàn bộ tài liệu
   và nó đang không nằm trong lộ trình nào.

**Gate P4.5** — ba tiêu chí, đo sau hai tuần dùng thật:
- ≥ 30% lượt gọi Syn đến từ ngoài app Messages.
- `load_skill` được gọi ít nhất một lần bởi model (không phải chạy tay). Nếu
  vẫn là 0, **dừng lại và sửa cửa vào skill trước khi làm gì khác** — đó chính
  xác là cái `adr-memory-shape` đã phải làm cho memory.
- Người dùng sửa `SYN.md` ít nhất một lần và hành vi đổi theo.

### P5 — Trần công việc dài (3–4 tuần)

1. `parent_run_id` + sub-run có history riêng, trả về kết luận cho run cha.
2. Compaction thay cho cắt cụt trong `build_pruned_history`.
3. Chạy song song các tool `VaultRead` trong cùng một round.
4. Tách "quyết định trước khi chạy tool" khỏi `drive_inner` thành hàm thuần
   (mục 3.4) — làm ở đây vì P6 sẽ chèn vào đúng chỗ đó.

**Gate P5:** một việc thật — "đọc các bài feed chưa đọc tuần này, viết một note
tổng hợp" — chạy hết trong một sub-run, hội thoại chính không tăng quá 20% độ
dài, và transcript của sub-run đọc được riêng.

### P6 — Ra thế giới (6–8 tuần)

Giữ nguyên thứ tự của roadmap cũ, thêm ba điều kiện tiên quyết bắt buộc.

*Điều kiện trước khi bắt đầu:*
- `Registry::definitions(ctx)` phải **thật sự** co giãn theo grant. Tool chưa
  được cấp quyền không xuất hiện trong prompt.
- Mọi nội dung lấy từ ngoài được bọc trong ranh giới "không tin cậy" rõ ràng
  trong prompt, và có test canh cho ranh giới đó.
- Audit log đã được một người thật đọc ít nhất một lần và họ hiểu được nó.

*Thứ tự:*
1. `fetch_url` — dùng lại `feed_engine/{fetcher,readability,sanitizer}` đã có.
   `Capability::NetRead{domain}`, hỏi một lần mỗi host.
2. MCP client transport HTTP/SSE — chạy được cả trên Android.
3. MCP stdio — desktop-only, `tokio::process` trực tiếp, **không** dùng
   `tauri-plugin-shell` (plugin đó phơi shell ra cho webview, thứ không ai muốn).
   Trên Android UI phải nói "server này cần desktop", không phải báo lỗi kết nối.
4. `NetWrite` sau cùng. Cân nhắc nghiêm túc việc **dừng ở "soạn draft vào
   vault"** — với một app local-first, đó có thể là điểm dừng đúng chứ không
   phải bước đệm.

**Gate P6:** J3, và audit log của việc đó đọc được từ đầu tới cuối bởi người
không viết code.

### P7 — Chủ động (3–4 tuần)

1. `Trigger::Schedule` và `Trigger::VaultEvent`. Desktop dùng tick của
   `chat_engine.rs`; mobile theo mô hình "tính trước một tuần, giao cho OS
   scheduler" của `calendar/scheduler.rs`.
2. Run nền báo cáo vào `{vault}/Messages/` — thêm variant cho
   `NotificationCard.vue`.
3. **Resume run** — ở đây thì bắt buộc, khác với chat (mục 1.3c). Một run nền
   dừng ở `AwaitingConsent` phải tiếp tục được khi người dùng trả lời, vì
   không có ai để "nói làm tiếp".
4. Ràng buộc cứng viết vào code từ đầu: run nền không được tự cấp capability.

**Gate P7:** J4.

### P8 — Sandbox

Vẫn hoãn, và bằng chứng ủng hộ việc hoãn đã mạnh hơn: một phần đáng kể CVE của
OpenClaw đi qua đúng con đường này. Chỉ làm khi có **ít nhất ba ví dụ cụ thể**
về skill không viết được bằng recipe.

**Tổng: khoảng 4–5 tháng tới một Syn đạt J1, J2, J3 và tiệm cận J4.**
Điểm dừng có thể ship sớm nhất: cuối P4.5, khoảng ba tuần — và nó đã là một
sản phẩm khác hẳn hôm nay mà không phá lời hứa nào.

---

## 7B. Các công cụ bên ngoài — phân loại lại

Roadmap 02-09 gộp toàn bộ "thế giới bên ngoài" vào một phase và một câu trả
lời: **MCP**. Câu trả lời đó đúng cho khoảng một nửa danh sách mà người ta
thực sự muốn — trình duyệt, search engine, Jira, Confluence, Excel, Telegram,
Gmail — và **im lặng về nửa còn lại**, vì nửa còn lại không phải là vấn đề
mạng.

Bốn loại vấn đề, và chúng cần bốn câu trả lời khác nhau:

### Loại 1 — Đọc web (browser đọc, search engine)

**Đã có gần hết, và tốt hơn dự kiến.** `feed_engine/` chứa sẵn:

- `fetcher::build_client` / `fetch_page` — HTTP client có timeout.
- `fetcher::guard_url` (`fetcher.rs:121`) — **chặn trước cả một lớp SSRF**:
  loopback, private, link-local, multicast, `localhost`, `.local`, `.internal`,
  và mọi scheme không phải http/https. Và nó tự khai giới hạn của mình: không
  chống được DNS rebinding, "vì muốn thế thì phải tự resolve rồi pin kết quả,
  điều reqwest không cho trả lại". Đây chính là lớp phòng thủ mà
  CVE-2026-26322 của OpenClaw không có.
- `readability::extract_content` + `sanitizer` — HTML thành chữ sạch.

Nên `fetch_url` là **1–2 tuần thật**, không phải một dự án. Nó cũng là thứ
duy nhất trong toàn bộ mục này chạy được trên Android.

**Search engine thì chưa có quyết định, và nó là một quyết định chứ không
phải một task.** Roadmap viết "cần một provider; hoặc bỏ qua, hoặc để người
dùng cấu hình (SearXNG self-host)". Ba lựa chọn thật:

| Lựa chọn | Được | Mất |
| --- | --- | --- |
| Bỏ qua | Không phá lời hứa nào | Syn không trả lời được câu nào cần thông tin ngoài vault |
| API trả phí (Brave, Tavily…) | Chạy ngay, chất lượng ổn | Mỗi câu hỏi đi qua một công ty thứ ba; cần `Capability::Spend` |
| SearXNG self-host / do người dùng cấu hình | Hợp triết lý nhất | Phần lớn người dùng sẽ không dựng nó |

Khuyến nghị: **để trống ô cấu hình, mặc định tắt.** Một trường "search
endpoint" nhận URL kiểu SearXNG hoặc một API key người dùng tự mang tới. App
không chọn hộ, và không có provider mặc định nào âm thầm nhận câu hỏi của
người dùng.

**Điều khiển trình duyệt thật (click, điền form, đăng nhập) — nằm ngoài phạm
vi, và nên nói ra.** Nó cần spawn process và tải một browser vài trăm MB,
desktop-only, và trong OpenClaw đây chính là bề mặt tấn công lớn nhất. Với
Synabit, "đọc trang" giải khoảng 90% nhu cầu thật; "điều khiển trang" là một
sản phẩm khác. Nếu về sau vẫn cần, nó đi qua MCP như mọi thứ khác, chứ không
phải thành một năng lực lõi.

### Loại 2 — Dịch vụ có API (Jira, Confluence, Gmail, Telegram)

**Về tool: MCP là câu trả lời đúng và roadmap nói đúng** — đừng viết N tích
hợp. Bốn dịch vụ này đều đã có MCP server.

**Về xác thực: roadmap thiếu hẳn một mục, và đây là khoảng trống thật.**

`secrets.rs` giữ API key trong `syn_api_keys: HashMap<String, String>` theo
`slot` (`secrets.rs:387-419`). Cơ chế slot tổng quát sẵn, nên:

- **Telegram bot token, Jira PAT, Confluence PAT — dùng được ngay.** Token
  tĩnh, không hết hạn, một slot mỗi dịch vụ. Không cần gì mới.
- **Gmail thì không.** OAuth 2.0 cần authorization code flow, refresh token,
  thời điểm hết hạn, và scope. `secrets.rs` không có khái niệm nào trong bốn
  cái đó. Một `String` không diễn tả được "token này chết sau 3600 giây và
  đây là cách lấy cái mới".

Tin tốt, và nó đáng để lên kế hoạch quanh: **hạ tầng khó nhất của OAuth đã
nằm sẵn trong app.** `tauri-plugin-deep-link` đã được đăng ký
(`lib.rs:396`), scheme `com.synabit.app` đã khai cho **cả desktop lẫn mobile**
(`tauri.conf.json:52-61`), và đã có handler `on_open_url` đang chạy
(`lib.rs:511`) — hiện dùng cho capture. Đó chính là đường callback mà OAuth
cần, trên cả ba nền tảng, và nó đã hoạt động.

Nên P6 cần tách ra một mục riêng, **đặt trước MCP**:

> **Token có hạn dùng.** Mở rộng `secrets.rs` từ `HashMap<String, String>`
> sang một struct có `access_token`, `refresh_token`, `expires_at`, `scope`.
> Một luồng OAuth dùng lại deep-link đã có. Vẫn giữ N4: **không thứ nào trong
> đây được vào vault.**

Không có mục này thì MCP chỉ với tới được các dịch vụ dùng token tĩnh, và
danh sách đó không có Gmail, không có Google Calendar, không có phần lớn
những gì người ta thực sự muốn nối vào.

### Loại 3 — Định dạng file (Excel, và họ hàng)

**Đây là lỗ hổng phân loại của roadmap.** Excel không phải vấn đề mạng, nên
P6 không chạm tới nó, nên nó không xuất hiện ở đâu cả.

Hiện trạng, đo được: `file_text.rs::kind_of` **đã** nhận `xlsx`, `ods`,
`docx`, `pptx`, `epub` là `Kind::ZippedXml` và bóc chữ ra khỏi zip bằng cách
strip tag. Nghĩa là **tìm kiếm** trong file Excel đã chạy hôm nay. Nhưng
module tự nói rất rõ ngay ở dòng 9-14 rằng nó không phải parser: *"Không có
gì ở đây tái dựng một layout, một style hay một bảng; đầu ra là một túi từ
dành cho FTS5."*

"Làm việc với Excel" theo nghĩa người dùng muốn là: đọc được ô, cột, sheet,
lọc, tính, và ghi ra một file mới. Đó là một crate đọc (`calamine`) và một
crate ghi (`rust_xlsxwriter`), cộng hai tool.

Và đây là điểm quan trọng về **thứ tự**: việc này

- chạy hoàn toàn local, không cần một byte nào ra mạng;
- không cần consent, không cần OAuth, không cần MCP;
- chạy được **trên cả Android**;
- **không bị chặn bởi bất cứ thứ gì trong P5, P6 hay P7.**

Nghĩa là nó có tỉ lệ giá trị trên rủi ro cao nhất trong toàn bộ danh sách, và
nó đang không nằm trong lộ trình nào. **Đưa vào P4.5.** Nó cũng nối rất tự
nhiên với Finance và Things — một bảng Excel nhập vào thành node, và một
truy vấn `query_nodes` xuất ra thành bảng.

CSV thì đã là `Kind::Plain` và `read_file_text` đọc được ngay hôm nay; cái
thiếu chỉ là Syn chưa biết rằng nó nên đọc theo cột.

### Loại 4 — Ghi ra ngoài (gửi mail, gửi Telegram, tạo Jira issue)

Roadmap để `NetWrite` sau cùng và gợi ý "cân nhắc dừng ở soạn draft vào
vault". Đúng cho email. **Không đúng cho tất cả**, và sự khác biệt đã có sẵn
chỗ để diễn tả:

| Hành động | `Reversal` đúng |
| --- | --- |
| Gửi email, gửi tin Telegram | `Manual { how: "cái đã gửi giờ ở đó rồi" }` — thực chất là không đảo ngược được |
| Tạo Jira issue, tạo Confluence page | `Manual { how: "issue này ở Jira; xoá nó ở đó" }` — đảo ngược được thật |
| Comment lên một issue | `Manual` — sửa được, nhưng người khác đã có thể đọc |

Nên "dừng ở draft" là điểm dừng đúng cho **kênh nhắn tin**, không phải cho
**hệ thống theo dõi công việc**. Với Jira/Confluence, tạo thẳng là hợp lý,
miễn là có consent và có audit.

**Và một quy tắc phải viết vào code trước khi cắm MCP server đầu tiên:**

> `Reversal` do app quyết định, không do MCP server khai. Một server nói
> "tool này đảo ngược được" là một server có thể nói dối, và nó là bên ít
> đáng tin nhất trong hệ thống. Mặc định cho mọi tool MCP chưa được người
> dùng phân loại là `Irreversible` → luôn hỏi.

Đây là ánh xạ trực tiếp của bài học OpenClaw: ở đó, skill từ registry chạy
với quyền của agent, và chính sách tool có thể bị sửa từ bên ngoài.

### Bảng tổng kết

| Công cụ | Loại | Đã có gì | Còn thiếu | Phase |
| --- | --- | --- | --- | --- |
| Đọc trang web | 1 | `fetcher` + `guard_url` + `readability` + `sanitizer` | 1 tool + `NetRead` | **P6a**, 1–2 tuần |
| Search engine | 1 | không | **một quyết định**, rồi 1 tool | **P6a**, chốt trước |
| Điều khiển trình duyệt | 1 | không | — | **ngoài phạm vi**, nói ra |
| **Excel / spreadsheet** | 3 | bóc chữ để search (`ZippedXml`) | parser ô/cột + ghi | **P4.5** — không bị chặn bởi gì |
| CSV | 3 | `read_file_text` đọc được | Syn biết đọc theo cột | **P4.5**, gần như miễn phí |
| Telegram | 2 + 4 | slot keychain đủ cho bot token | MCP client + `NetWrite` | **P6b** |
| Jira / Confluence | 2 + 4 | slot keychain đủ cho PAT | MCP client + `NetWrite` | **P6b** |
| Gmail | 2 + 4 | deep-link callback đã chạy | **OAuth + token có hạn dùng** | **P6a** (token store) → **P6b** |

### Sửa lộ trình ở mục 7

Tách P6 làm hai, và nhấc Excel lên P4.5:

- **P4.5** thêm mục 9: đọc/ghi spreadsheet (local, không mạng, chạy cả Android).
- **P6a — Đọc và xác thực (3–4 tuần).** `fetch_url` + quyết định search
  engine + **token store có hạn dùng dùng lại deep-link đã có**. Kết thúc
  P6a, Syn đọc được web và giữ được credential đúng cách — chưa gửi gì đi.
- **P6b — MCP và ghi ra ngoài (4–5 tuần).** MCP HTTP/SSE trước, stdio
  desktop-only sau, `NetWrite` cuối, `Reversal` do app quyết định.

Ba điều kiện tiên quyết ở mục 7 (registry co giãn theo grant, ranh giới nội
dung không tin cậy, audit log đã có người đọc) **thuộc về P6a**, không phải
P6b — vì `fetch_url` đã là lúc nội dung của người lạ bắt đầu đi vào prompt.

---

## 8. Đo lường — ba thước đo phải thêm

Bộ eval hiện có (`gate_one`, `rag_vs_agentic`, `where_the_recall_goes`,
`what_syn_would_have_offered`) là tốt và nên giữ nguyên khuôn: test `#[ignore]`,
in bảng, không assert, so bằng substring chứ không bằng judge model.

**Thêm ba thứ:**

1. **Firing rate cho từng cơ chế.** Không phải một test — một con số trong app,
   cập nhật liên tục, cho từng thứ: memory, skill index, `load_skill`,
   `run_recipe`, consent, plan mode. Quy tắc: **mọi cơ chế mới đều phải khai
   báo firing rate của nó trước khi được coi là xong.** Nếu `adr-memory-shape`
   có một bài học thì đó là bài học này.
2. **Chi phí mỗi run.** `reply.tokens` đã có; `Budget.tokens` là `None`. Ít
   nhất hãy *hiển thị* tổng token của một run, kể cả khi chưa đặt trần. Không
   cần bảng giá — một con số token là đủ để người dùng biết Syn đang tốn gì.
3. **Câu hỏi khó hơn cho bộ eval.** ADR ghi rõ cả hai nhánh đều 15/15 — bộ câu
   hỏi đã chạm trần và không còn phân biệt được gì. Cần câu nhiều hop, câu có
   note mâu thuẫn, và câu mà đáp án nằm trong field frontmatter chứ không phải
   trong thân (đó chính là loại câu `gemma4:e4b` trượt 4/5 lần).

Và một điều nữa `adr-rag-vs-agentic` tự nêu mà chưa ai trả: **vault seed chỉ
có mười bốn node.** Đó là lý do retrieval chỉ đóng góp vài trăm ký tự thay vì
12.000 mà nó được phép. Trên vault thật vài nghìn note, so sánh chi phí có thể
đảo chiều. ADR gọi đây là "giới hạn lớn nhất của kết quả trên" — nó vẫn là.

---

## 9. Rủi ro, cập nhật

| Rủi ro | Trạng thái so với 02-09 | Giảm thiểu |
| --- | --- | --- |
| **Xây thứ không ai dùng** | **MỚI, và là rủi ro số một.** Hai lần đã xảy ra. | P4.5 trước P5. Firing rate là điều kiện để coi một cơ chế là xong. |
| **Prompt phình** | Nặng hơn dự đoán: schema tool đã chiếm 16.803/37.743 ký tự mỗi lượt | `definitions(ctx)` phải co giãn thật trước P6. Đây không còn là tối ưu hoá, là điều kiện. |
| **Vault đầy rác** | Đã giảm thiểu tốt: proposal queue ngoài vault, `KEEP_RUNS`, `KEEP_PROPOSALS` | Giữ nguyên. Thêm số đếm hiện cho người dùng. |
| **Agent làm điều không đảo ngược được** | Đã giảm thiểu tốt: P4 xong trước P5 | Giữ thứ tự. Thêm: `plan_only` phải bấm được trước khi có `NetWrite`. |
| **Prompt injection từ nội dung** | **Chưa giảm thiểu, và sắp thành hiện thực** | Ba điều kiện tiên quyết của P6. Đây là chỗ OpenClaw thất bại nặng nhất. |
| **Model local không kham nổi** | Đã đo (7,6× chậm, −3 điểm) và **chưa nói với người dùng** | Bảng model tier trong P4.5. |
| **Android tụt lại** | Chưa xảy ra; mọi thứ P0–P4 đều chạy mọi nơi | Giữ N5 khi tới MCP stdio và sandbox. |
| **`drive_inner` quá tải** | **MỚI** | Tách hàm quyết định thuần ở P5. |

---

## 10. Những gì không nên làm

Danh sách của roadmap 09-02 vẫn đúng nguyên. Bổ sung bốn mục:

- **Đừng làm P5 trước khi `load_skill` được model gọi ít nhất một lần.** Nếu
  cửa vào skill hỏng như cửa vào memory đã hỏng, thêm mười lăm tool MCP vào
  cùng cái prompt đó sẽ làm nó hỏng nặng hơn chứ không nhẹ hơn.
- **Đừng để `definitions(ctx)` bỏ qua `ctx` sang tới P6.** Cái seam đã cắt;
  không dùng nó thì nó chỉ là một lớp gián tiếp vô ích.
- **Đừng thêm một cơ chế nào nữa mà không kèm cách đo xem nó có chạy không.**
  Đây là bài học đắt nhất của ba ngày vừa qua và nó đã được trả tiền hai lần.
- **Đừng đuổi theo OpenClaw về độ phủ.** Nó phủ rộng hơn và nó đang là một
  danh sách CVE. Thứ Synabit bán không phải là "làm được nhiều nhất" — mà là
  "làm được, trong vault của bạn, và bạn đọc được mọi thứ nó nhớ, mọi thứ nó
  biết làm, và mọi thứ nó đã làm". Không sản phẩm nào trong bảng ở mục 4.1
  giữ được cả ba câu đó cùng lúc. Đó là vị trí, và nó đáng giữ.

---

## Phụ lục A — 27 tool hiện có

| Nhóm | Tool |
| --- | --- |
| Đọc node | `query_nodes`, `get_node`, `list_schemas`, `get_linked_nodes` |
| Ghi node | `create_node`, `update_node`, `trash_node` |
| Hoàn tác | `list_trash`, `restore_node`, `list_versions`, `restore_version` |
| Cấu trúc (hai bước) | `rename_field`, `delete_field`, `rename_kind`, `delete_kind` |
| Memory | `remember`, `recall` |
| Skill | `load_skill`, `run_recipe` |
| Files | `search_files`, `read_file_text` |
| Feeds | `search_feed_articles`, `update_feed_article` |
| Finance | `get_finance_summary`, `search_finance`, `create_transaction`, `get_transactions` |

Không có `forget` — memory là node, `trash_node` đã xoá được và `restore_node`
mang lại, và hai tool làm một việc là chính thứ mà lần thu gọn 20→12 đã sửa.

## Phụ lục B — File cần đụng vào, theo phase

| Phase | Sửa | Thêm mới |
| --- | --- | --- |
| P4.5 | `commands/syn.rs`, `syn/tools.rs` (đóng dấu run), `syn/reflect.rs` (correction), `syn/prompt.rs` (đọc `SYN.md`), `QuickEntry.vue`, `ModelSelector.vue`, `ChatPanel.vue` | `syn/stats.rs`, `shared/views/SynDashboard.vue` |
| P5 | `syn/run.rs` (`parent_run_id`), `syn/engine.rs` (tách gate, song song, compaction) | `syn/compact.rs` |
| P6a | `syn/registry.rs` (co giãn theo grant), `feed_engine/fetcher.rs` (tái dùng `guard_url`), `secrets.rs` (token có hạn dùng), `lib.rs` (deep-link cho OAuth callback) | `syn/web.rs`, `syn/oauth.rs` |
| P6b | `Cargo.toml`, `syn/registry.rs` | `syn/mcp/` |
| P7 | `chat_engine.rs`, `calendar/scheduler.rs`, `watcher.rs`, `commands/syn.rs` (resume), `NotificationCard.vue` | `syn/trigger.rs` |

## Phụ lục C — Nguồn tham chiếu bên ngoài

- OpenClaw — kiến trúc, skill, memory ba tầng:
  [openclaw.ai](https://openclaw.ai/),
  [Medium — deep dive](https://medium.com/@yugank.aman/openclaw-architecture-skills-capabilities-integrations-use-cases-a-deep-dive-22ebf9d46ad5)
- OpenClaw — bảo mật, CVE, skill registry nhiễm malware:
  [Sangfor](https://www.sangfor.com/blog/cybersecurity/openclaw-ai-agent-security-risks-2026),
  [Giskard](https://www.giskard.ai/knowledge/openclaw-security-vulnerabilities-include-data-leakage-and-prompt-injection-risks),
  [Conscia](https://conscia.com/blog/the-openclaw-security-crisis/)
- Hermes Agent — skill tự sinh, memory `MEMORY.md`/`USER.md`, SQLite FTS5:
  [hermes-agent.org](https://hermes-agent.org/),
  [Tosea](https://tosea.ai/blog/hermes-agent-self-improving-ai-guide)
- Claude Code — Agent Skills, progressive disclosure ba tầng, subagent, hook:
  [Agent Skills docs](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview),
  [Claude Academy](https://academy.claude.com/courses/introduction-to-agent-skills)
