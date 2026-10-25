# Syn tự dựng kỹ năng — so với OpenClaw, Hermes, Claude (2026-10-25)

Câu hỏi đặt ra: Syn có thiếu một "skill/tool/agent/connector builder" — khả năng tự dựng thêm năng lực cho
chính mình sau các cuộc trò chuyện hoặc yêu cầu của người dùng — như OpenClaw, Hermes và Claude không?

Phần về Syn viết từ việc đọc mã nguồn trên `main` ngày 2026-10-25 (sau `8d51fe28`). Phần về ba agent kia lấy
từ tài liệu chính thức và mã nguồn công khai đến cùng ngày (Hermes đọc tại commit `930e5ed`, `anthropics/skills`
tại `dbd4588`). Chỗ nào chưa kiểm chứng được đều ghi **chưa kiểm**.

---

## 0. Kết luận ngắn

**Syn không thiếu bộ dựng. Cái nó thiếu là *học* — nó chỉ đề nghị viết skill trong một tình huống rất hẹp.**

- Skill, connector, việc định kỳ: Syn đều soạn được, qua chat, và người dùng duyệt bằng một thẻ. Riêng
  connector thì Syn đi trước cả ba agent kia — không bên nào cho agent tự dựng kết nối tới dịch vụ.
- Thứ Syn tự *nhận ra* mà không cần ai bảo chỉ có một: cùng một chuỗi ≥3 lời gọi tool lặp lại hai lần **trong
  một run**. Một việc làm mỗi thứ Hai, mỗi lần một hội thoại, không bao giờ được đề nghị thành skill.
- Hermes và OpenClaw học nhiều hơn hẳn, nhưng bằng cách bỏ khâu duyệt và chạy script trên shell của máy. Syn
  không nên chép hai điều đó.

Việc đề xuất làm ngay (§5): thêm ba nguồn học — **lặp qua nhiều run**, **việc khó mới xong**, **người dùng sửa
câu trả lời theo skill** — vẫn bằng bộ phát hiện số học, vẫn qua thẻ duyệt, và có phanh để hàng chờ không phình.

---

## 1. Syn đang có gì

| Loại | Syn soạn khi được bảo | Syn tự nhận ra | Duyệt | Ở đâu |
| --- | --- | --- | --- | --- |
| **Skill** | `skill_draft` ("từ giờ…", "lần sau làm vậy") | Chuỗi lặp trong một run (`repeated_chain`); run theo skill mà hỏng → bản sửa (`skill_that_struggled`) | Thẻ dưới câu trả lời; duyệt *theo từng máy* bằng dấu vân tay nội dung | `syn/skill.rs`, `syn/drafting.rs`, `commands/syn.rs:1472` |
| **Connector** | `connector_find` → `connector_draft` → `connector_try` → `connector_connect`; từ OpenAPI, YAML, SSH, thiết bị | — | Thẻ connect; khoá gõ trên thẻ, vào keychain | `syn/connector/authoring.rs` |
| **Việc định kỳ** | `routine_draft` (theo giờ, hoặc khi connector có dữ liệu mới) | — | Lưu tắt; bật trên thẻ, theo từng máy | `syn/routine.rs` |
| **Tool** | Skill tầng `recipe` = một chuỗi lời gọi cố định, Rust chạy, không tốn lượt model | Recipe tự rút từ hai lượt lặp (`drafting::recipe_from_run`) | Như skill | `syn/recipe.rs` |
| **Code** | `compute`: Rhai, không I/O, giới hạn trước khi cấp phát | — | Không cần: không có cạnh ra ngoài | `syn/script.rs` |
| **Agent** | Không. Chỉ có `delegate`: sub-run chỉ đọc, một tầng, không lưu định nghĩa | — | — | `syn/delegate.rs` |

Mọi thứ người dùng duyệt đều là file trong vault (`SynSkills/`, `SynConnectors/`): có lịch sử phiên bản, khôi
phục được, đồng bộ mã hoá đầu-cuối, mở được bằng trình soạn thảo bất kỳ.

---

## 2. Ba agent kia làm thế nào

### 2.1 OpenClaw — Skill Workshop

- **Khi nào học:** trong lượt (vá skill đã dẫn sai; lưu cách làm sau "việc nhiều bước khó, sẽ lặp lại"); rà
  nền sau **10 vòng model** tích luỹ trong một hội thoại; `/learn` và nút "Learn from history"; tự cất skill
  30 ngày không dùng. Mặc định `skills.workshop.autonomous.mode: "auto"`.
  ([skill-workshop](https://docs.openclaw.ai/tools/skill-workshop))
- **Sản phẩm:** `SKILL.md` + `references/`, `templates/`, `scripts/`, `assets/`; skill học được ở
  `<agentDir>/workshop-skills/`.
- **Duyệt:** không — thay đổi "apply immediately". Bù lại: lưu bản trước mỗi lần sửa (giữ 10), `/learn undo`,
  chặn ghi nếu quét thấy khoá lộ. Bản nháp chờ duyệt từng có, giờ "no longer loaded".
- **Code:** script chạy qua tool exec; sandbox **tắt mặc định**
  ([sandboxing](https://docs.openclaw.ai/gateway/sandboxing)).
- **Sự cố:** ClawHavoc — hàng trăm skill độc trên ClawHub (Koi đếm 341/2.857; Antiy đếm ≥1.184 qua thời gian),
  phần lớn cài trình đánh cắp AMOS cho macOS
  ([OWASP](https://owasp.org/www-project-agentic-skills-top-10/case-studies.html)). Snyk: 13,4% skill có lỗi
  nghiêm trọng ([ToxicSkills](https://snyk.io/blog/toxicskills-malicious-ai-agent-skills-clawhub)).

### 2.2 Hermes Agent (Nous Research) — vòng học

- **Khi nào học** (đọc từ mã): mỗi vòng có gọi tool tăng `_iters_since_skill`; gọi `skill_manage` thì về 0.
  Cuối lượt, nếu ≥ `skills.creation_nudge_interval` (mặc định **10**) thì tách một agent nền chỉ có tool
  `skills` để rà lại hội thoại (`agent/background_review.py`). Prompt chính còn dặn: *"After difficult/iterative
  tasks, offer to save as a skill."*
- **Prompt rà nền** đẩy mạnh về phía ghi: *"Be ACTIVE — most sessions produce at least one skill update"*. Tín
  hiệu: người dùng sửa cách làm ("FIRST-CLASS skill signals"), kỹ thuật mới, skill đã nạp mà sai. Thứ tự ưu
  tiên: vá skill đang dùng → skill ô dù có sẵn → thêm file phụ → mới tạo skill mới. Có danh sách **không ghi**:
  lỗi môi trường, khẳng định tiêu cực về tool, chuyện một lần, thất bại chưa giải quyết.
- **Duyệt:** `skills.write_approval: False` mặc định. Chính comment cấu hình nói quét skill do agent viết
  "mostly blocks prose with risky keywords" vì agent vẫn chạy được code đó qua `terminal()` không bị chặn.
- **Dọn thư viện:** Curator chạy khoảng mỗi tuần khi rảnh 2 giờ; 14 ngày không dùng → cũ, 30 ngày → cất.
- **Lỗi tự nhận trong mã:** thư viện "tích trữ" (mỗi phiên một file), kể chuyện thay vì luật, và khẳng định
  tiêu cực "harden into refusals the agent cites against itself for months".

### 2.3 Claude (Anthropic)

- **Khi nào học:** **không có** tính năng tự lưu skill sau hội thoại trên claude.ai (**chưa kiểm** được điều
  ngược lại). Skill tạo khi người dùng yêu cầu, qua skill-creator
  ([hướng dẫn](https://academy.claude.com/tutorials/how-to-create-a-skill-with-claude-through-conversation)).
  Trong Claude Code, chỉ auto memory là tự ghi không cần hỏi
  ([memory](https://code.claude.com/docs/en/memory)).
- **Subagent:** file `.claude/agents/*.md` — frontmatter đặt `tools`, `model`, `permissionMode`; thân là
  system prompt. Claude viết được khi được bảo ([sub-agents](https://code.claude.com/docs/en/sub-agents)).
- **MCP:** Claude dựng được *mã* server (skill mcp-builder), nhưng đăng ký server là việc của người dùng.
- **Chất lượng — điểm mạnh riêng:** skill-creator chạy eval có skill và không skill, chấm bằng subagent, có
  A/B mù; tối ưu `description` trên 20 câu (nên/không nên kích hoạt), chia 60/40 train/test, chọn theo điểm
  test ([repo](https://github.com/anthropics/skills/tree/main/skills/skill-creator)). Chính prompt của nó ghi
  Claude có xu hướng "undertrigger" skill.

---

## 3. So sánh

| | OpenClaw | Hermes | Claude | **Syn** |
| --- | --- | --- | --- | --- |
| Tự nhận ra việc đáng thành skill | Sau 10 vòng; việc khó | Sau 10 vòng; người dùng sửa; việc khó | Không | **Chỉ chuỗi lặp trong một run** |
| Soạn skill khi được bảo | `/learn` | `/learn`, `skill_manage` | skill-creator | `skill_draft` |
| Sửa skill sau khi dùng | Vá trong lượt | Vá trong lượt + nền | Khi người dùng chạy lại skill-creator | Chỉ khi run theo skill có lời gọi lỗi / chạm trần |
| Duyệt trước khi có hiệu lực | Không | Không (tuỳ chọn) | Quyền ghi file | **Có, theo từng máy** |
| Code trong skill | Shell, không sandbox mặc định | Shell local mặc định | Bash theo môi trường | Rhai không I/O (`compute`); tầng `code` chưa làm |
| Agent tự dựng connector | Không | Không | Không (chỉ mã MCP) | **Có — bản khai, không phải code** |
| Định nghĩa subagent | Không lưu | Cấu hình tay | `.claude/agents/*.md` | Không |
| Đo skill có được dùng | Không thấy | Không thấy | Eval kích hoạt | Đếm lần mở (`skill::usage`) |
| Dọn skill cũ | 30 ngày → cất | 14/30 ngày | Không | Không |

Ba bên đều chỉ có sự cố ở **chợ skill**, không phải ở skill tự viết. Nhưng Hermes tự nhận kho skill tự viết
không duyệt là một kho prompt injection lâu dài — `docs/syn-review-2026-10-13.md` đã nêu đúng điều đó.

---

## 4. Chỗ Syn thật sự thiếu, xếp theo mức đáng làm

1. **Nguồn học quá hẹp.** `repeated_chain` chỉ thấy lặp *trong một run*. Không học qua nhiều run, không học
   từ việc khó mới xong. Còn khi người dùng nói "không phải, ý tao là…" ngay sau một câu trả lời theo skill thì
   skill đó không được sửa, vì `skill_that_struggled` chỉ nhìn lời gọi lỗi và trần. Nó cũng chỉ thấy skill
   model tự `load_skill`, bỏ sót skill harness đưa vào prompt (`Run::skill_injected`). **→ §5, làm ngay.**
2. **Connector không ghi lại cách dùng.** `Manifest.skill` luôn là `None` (`connector/authoring.rs:239`), nên
   những gì Syn học được lúc `connector_try` (lịch nào là lịch công việc, cột nào là số tiền — §7.4 của
   `syn-design-2026-10-12.md`) mất sau lượt đó. Syn cũng chưa soạn được MCP từ xa theo URL
   (`manifest.rs:673`, `logic_not_yet`). **→ Làm sau §5.**
3. **Dọn thư viện và đo kích hoạt.** Đã đếm lần mở nhưng chưa cất skill lâu không dùng, chưa có eval "skill có
   được chọn đúng lúc không". Khi §5 làm thư viện lớn nhanh hơn, cái này thành cần thiết.
4. **Agent builder.** Lỗ thật duy nhất trong bốn loại, nhưng ít đáng làm nhất với app một người dùng. Nhu cầu
   thật là chọn **model và nhóm tool riêng cho từng skill/việc định kỳ** (review 13/10 đã nêu) — làm như hai
   trường frontmatter, không cần một kho agent riêng.

Tầng `code` cho skill không xếp ưu tiên: recipe gọi `compute` đã thay được phần lớn, và chạy script ngoài
sandbox chính là chỗ OpenClaw và Hermes bị dính.

---

## 5. Thiết kế mục 1: ba nguồn học mới

Giữ nguyên ba nguyên tắc đang có trong mã:

- **Quyết định "có gì đáng học không" bằng số học, không gọi model.** Model chỉ được gọi khi bộ phát hiện đã
  thấy một thứ, để đặt tên và viết cách làm. Phần lớn run không lặp, không khó, không bị sửa — nên không tốn gì.
- **Syn không bao giờ tự bật.** Skill mới lưu tắt, có thẻ dưới câu trả lời; bản sửa nằm ở `pending_revision`
  chờ người đọc.
- **Cùng công tắc `memory_reflection`.** Tắt phản tư là tắt cả việc học, như hiện nay.

### 5.1 Lặp qua nhiều run

`skill::chain_across_runs(run, earlier)`: chuỗi dài nhất ≥ `MIN_CHAIN` lời gọi thành công của run này mà cũng
xuất hiện liền mạch trong **ít nhất 2 run trước** (tổng ≥3 lần), và các lần đó rơi vào **ít nhất 2 ngày khác
nhau**.

Ban đầu thiết kế là "ít nhất 2 hội thoại". Đo trên vault thật (201 run, xem §5.5) cho thấy sai cả hai chiều:

- Thói quen thật duy nhất tìm được — "Ghi vào daily note: …" năm ngày khác nhau — đến **toàn bộ từ Telegram**,
  mà Telegram chỉ có một hội thoại cho mọi thứ gửi từ đó. Quy tắc theo hội thoại không bao giờ thấy nó.
- Cái sai nó tìm ra là một sơ đồ bị gửi lại ba lần trong một buổi chiều ("vẫn sai, sửa lại đi"), trải trên hai
  hội thoại. Đó là sửa một việc cho đúng, không phải quay lại làm một việc.

"Quay lại" là thứ một *ngày* đo được. Đổi sang ngày thì cái sai biến mất và cái đúng hiện ra.

- Chỉ tính run cấp cao nhất (không phải sub-run), đã xong, từ app hoặc Telegram. Run của việc định kỳ lặp vì
  được hẹn lặp; tính chúng là đề nghị viết skill cho thứ đã là việc định kỳ.
- Bỏ run đã theo một skill: lặp vì làm theo skill là skill đang chạy đúng.
- Bỏ chuỗi chỉ gồm các bước dò vault (`query_nodes`, `get_node`, `get_linked_nodes`, `list_schemas`,
  `recall`). Đó là hình dạng của gần như mọi câu trả lời, không phải của một việc.
- Có hai lượt (run này và lượt gần nhất trước đó) thì vẫn rút được recipe như `recipe_from_run`: đổi hàm đó
  để nhận hai lượt bất kỳ thay vì hai lượt trong một run.
- Đọc tối đa `KEEP_RUNS` (200) file run, chạy nền sau khi người dùng đã có câu trả lời, và chỉ khi run này có
  đủ lời gọi để tạo nên một chuỗi.

### 5.2 Việc khó mới xong

`skill::hard_won(run)`: run đã xong, không theo skill nào, có ít nhất `HARD_WON_CALLS` (6) lời gọi thành công,
và **có một tool lỗi rồi sau đó chính tool đó thành công** — tức là Syn đã mò ra đường.

- Model được đưa mục tiêu, các lời gọi theo thứ tự kèm lỗi rút gọn, và được hỏi: cách làm cuối cùng có đáng
  viết thành skill để lần sau khỏi mò không? Danh sách *không ghi* lấy từ bài học của Hermes: lỗi do mạng hay
  dịch vụ, lỗi gõ nhầm tham số, việc một lần, thất bại chưa giải quyết. Trả `null` là câu trả lời bình thường.
- Skill tầng `prose`: chỉ có một lượt, không rút được recipe.
- Không chạy nếu §5.1 hoặc `repeated_chain` đã đề nghị trong run này — một run, tối đa một đề nghị skill.

### 5.3 Người dùng sửa câu trả lời theo skill

`correction::looks_like_one` đã chạy ở mọi lượt. Khi nó bật và **run trước đó trong hội thoại** đã theo một
skill (tự `load_skill` hoặc harness đưa vào), Syn soạn bản sửa cho skill đó bằng `reflect::suggest_revision`,
với "chuyện gì đã sai" là lời người dùng vừa nói.

- Lời sửa không có skill nào để đổ lỗi thì để nguyên cho phản tư bộ nhớ (`from_correction`) như hiện nay.
  "Người dùng muốn làm kiểu khác" đã là việc của trí nhớ và `SYN.md`; biến nó thành skill là chép Hermes ở
  chỗ Hermes tự nhận là sinh ra kho tích trữ.
- Cùng lúc sửa `skill_that_struggled` để tính cả `skill_injected`.

### 5.4 Phanh

- **Hàng chờ tối đa `MAX_WAITING` (2) skill Syn soạn chưa ai trả lời.** Đầy thì không soạn thêm và không ghi
  chuỗi là đã đề nghị — để lần sau còn hỏi được. Áp dụng cho mọi nguồn, kể cả `repeated_chain`. Hàng chờ chỉ
  duyệt được khi còn ngắn.
- **Mỗi skill tối đa một bản sửa đang chờ** — giữ nguyên quy tắc đã có.
- **Đã đề nghị thì không đề nghị lại** — dùng chung `Syn/skill-chains.json`, kể cả khi model trả `null`.

### 5.5 Đo

- Log một dòng cho mỗi nguồn khi nó chạy, kể cả khi không ra gì (như phản tư bộ nhớ đã làm): một tính năng im
  lặng mà không phân biệt được với một tính năng chết là không gỡ lỗi được.
- Probe `what_syn_would_have_offered` (`--ignored`) mở rộng cho §5.1 và §5.2: chạy trên vault thật để xem nó
  *sẽ* đề nghị gì trước khi để nó đề nghị thật.
- Eval trên `gpt-6-luna`: một kịch bản cùng việc ở ba ngày khác nhau → có thẻ skill. **Chưa làm.**

**Đo trên vault thật (2026-10-25, 201 run):**

| Nguồn | Sẽ đề nghị | Là gì |
| --- | --- | --- |
| Lặp trong run (`repeated_chain`, đã có) | 1 | Bốn lần `search_feed_articles` liền nhau |
| Lặp qua ngày (§5.1) | 1 việc | "Ghi vào daily note: …" — `query_nodes → get_node → update_node`, năm ngày, qua Telegram. Đúng là thứ đáng thành skill. |
| Việc khó mới xong (§5.2) | 0 | — |

Probe in từng run một, nên thói quen daily note hiện bốn dòng; chạy thật thì lần đầu đã ghi chuỗi vào
`Syn/skill-chains.json` và ba lần sau bị `already_covered` chặn.

Hai điều rút ra. Một: các nguồn mới yên lặng — một đề nghị trong hơn hai trăm run, trước cả cổng model. Hai:
chuỗi của thói quen daily note là hình dạng chung của "sửa một ghi chú". Ở vault này mọi lần đều là daily note
nên đề nghị đúng; ở vault khác nó có thể gom những việc sửa ghi chú không liên quan, và lúc đó chỉ còn cổng
model đứng chắn. Nếu thấy vậy, bước tiếp theo là đòi câu hỏi của các run cũng giống nhau (dùng lại
`match_score`), không phải nâng ngưỡng.

---

## 6. Không làm

- **Không bỏ khâu duyệt.** Cả OpenClaw lẫn Hermes đều chọn "áp dụng ngay, có hoàn tác". Với một agent đọc web
  và email, một skill tự ghi chính là lời dặn thường trực mà một trang web có thể để lại.
- **Không shell, không chợ skill.** Đã ghi trong `syn-from-assistant-to-colleague-2026-09-06.md`; ClawHavoc là
  lý do.
- **Không prompt "Be ACTIVE".** Đề nghị chỉ nên đến khi có bằng chứng trên transcript.

---

## 7. Cần anh quyết

1. **Ngưỡng §5.1: 3 lần trong ít nhất 2 ngày.** Hai lần thì đề nghị sớm hơn nhưng nhiễu hơn. Đã làm với 3.
2. **§5.2 có nên giữ không.** Đây là nguồn dễ sinh skill kém nhất; chỉ có cổng model để lọc. Probe: 0/201 run
   trên vault thật, nên hiện nó không tốn gì. Đã làm; đề xuất giữ và xem lại khi có thêm connector.
3. **Thứ tự sau §5:** connector ghi cách dùng (mục 2) trước dọn thư viện (mục 3). Đề xuất: đồng ý.

---

## 8. Đã làm (2026-10-25)

- `skill.rs`: `chain_across_runs`, `already_covered`, `hard_won`, `what_was_tried`, `hard_won_key`, `followed`
  (đọc cả `skill_injected`), hằng `MAX_WAITING`, `ACROSS_RUNS`, `ACROSS_DAYS`, `HARD_WON_CALLS`; probe mở rộng.
- `drafting.rs`: `recipe_across_runs`; `recipe_from_run` tách phần so hai lượt ra `recipe_from_passes`.
- `reflect.rs`: `suggest_from_hard_won`; câu hỏi của `suggest_skill` nói đúng cả trường hợp lặp qua ngày.
- `run.rs`: `previous_in` (lượt trước trong hội thoại), `untrusted_before` dùng lại nó.
- `commands/syn.rs`: `learn_a_skill` chạy nền sau run, tối đa một đề nghị mỗi run theo thứ tự §5; bản sửa skill
  khi người dùng sửa câu trả lời của lượt theo skill; phanh `MAX_WAITING` cho mọi nguồn.
- Test: 14 test mới; toàn bộ `cargo test --lib` qua.

**Chưa làm:** eval `gpt-6-luna` cho §5.1 (cần ba ngày giả lập trong harness); mục 2–4 của §4.
