# Thiết kế Syn — 2026-10-12

Bản này viết từ việc đọc mã nguồn (Rust `src-tauri/src/syn/`, Vue `src/mini-apps/messages/`, `src/shared/syn/`,
dữ liệu vault, eval), không dựa trên các tài liệu thiết kế trước. Số dòng dẫn ở dạng `file:dòng` là của cây làm
việc ngày 2026-10-12.

---

## 0. Một câu

**Syn là agent duy nhất mà người dùng chỉ cần nói — Syn tự dựng kết nối, kỹ năng và việc định kỳ cho mình —
và vẫn không thể bị một trang web, một file hay một dịch vụ bên ngoài sai khiến đem dữ liệu đi.**

Đó là chỗ trống giữa hai nhóm hiện có:

| | Mở rộng | Cấu hình | Bảo mật | Dữ liệu cá nhân |
| --- | --- | --- | --- | --- |
| Claude Code, Codex | MCP, skill, hook — mạnh | File cấu hình, JSON, lệnh CLI; người dùng phải biết kỹ thuật | Hỏi theo *hành động*, sandbox cho shell | Không có; làm việc trên repo |
| Hermes Agent | Tự viết skill, nhớ | Vẫn cấu hình server, khoá, gateway bằng tay | Ở mức hỏi hành động | Bộ nhớ riêng của agent |
| OpenClaw | Rất nhiều kỹ năng, cộng đồng | Dễ hơn | Yếu: kỹ năng bên thứ ba chạy với toàn quyền, lời trong dữ liệu điều khiển được agent | Rải rác |
| **Syn hiện tại** | Connector tự dựng được qua chat; skill/routine thì không | ~35 thiết lập, khoảng 7 màn hình | **Kiểm soát luồng dữ liệu** (taint), mạnh hơn cả nhóm trên | Vault Markdown, đồng bộ E2E — nền tốt nhất trong cả bảng |
| **Syn mục tiêu** | Một khái niệm, Syn tự dựng tất cả | Một khoá, không thiết lập kỹ thuật | Giữ taint; duyệt *gộp* thay vì từ chối | Như cũ |

Lợi thế mà các agent kia không sao chép nhanh được là hai thứ Syn đã có: **vault của người dùng làm bộ nhớ** và
**gate theo luồng dữ liệu**. Thiết kế này không thay hai thứ đó; nó bỏ đi những gì đang che chúng.

---

## 1. Hiện trạng, đo từ mã

### 1.1 Cái đã mạnh — giữ nguyên

- **Một đường chạy cho mọi bề mặt.** App, AskBar, Telegram, routine đều vào `send_message_inner`
  (`commands/syn.rs:469`). Transcript ghi từng bước kèm cách hoàn tác (`run.rs:217`).
- **Gate theo luồng dữ liệu.** `gate::decide` (`syn/gate.rs:162`) là hàm thuần; taint có nguồn
  (`taint::Origin`), `only_echoes`, đích đến chỉ là link đã thấy hoặc host người dùng gõ (`taint.rs:484`),
  kế hoạch gửi băm theo từng request (`writes.rs:126`), danh sách cho phép sau khi đọc được kiểm *cả ở
  executor* (`tools.rs:1591`) nên recipe không đi vòng được.
- **Khoá không bao giờ tới model.** `{{safe:x}}` do Rust điền vào bản sao tham số (`safe/egress.rs`), mọi
  prompt đi ra đều bị che khoá (`provider/guarded.rs`).
- **Mọi thứ người dùng cần kiểm là file trong vault**: memory, skill, connector, thread — có lịch sử phiên bản,
  thùng rác, đồng bộ CRDT mã hoá đầu cuối.
- **Bộ phát hiện bằng số học** thay cho gọi model: chuỗi lặp (`skill::repeated_chain`), lời sửa
  (`correction.rs`), notice. Rẻ, thử được, không bịa.
- **Eval chạy đúng đường sản phẩm** (76 tác vụ, chấm tất định, an toàn là cổng cứng).
- **Thẻ kết nối trong chat** (`ConnectCard.vue` + `syn_connect`): nói một câu, dán khoá, bấm một nút.

### 1.2 Cái làm Syn nặng — vấn đề cần giải

**A. Quá nhiều khái niệm, mỗi cái một cơ chế.** Người dùng (và mã) phải phân biệt 12 thứ: MCP server, bản
khai connector, mục thư viện, connector thiết bị, mirror, skill (prose/recipe/code), recipe, routine, memory,
đề xuất memory, `SYN.md`, notice. Mỗi thứ một chỗ lưu, một cách duyệt:

| Thứ | Lưu | Duyệt |
| --- | --- | --- |
| MCP | `Syn/connectors.json` + `.synabit/connectors-trusted.json` + `connectors-read-only.json` | lưu trên máy = tin |
| Bản khai | note `SynConnectors/` + `.synabit/connector-instances.json` | dấu theo máy |
| Routine | `Syn/routines.json` + `.synabit/routines-approved.json` | dấu theo máy |
| Skill | note `SynSkills/` | cờ `enabled` **đồng bộ** — không có dấu theo máy |
| Memory | note `SynMemory/` | không duyệt (`remember`) hoặc hàng chờ (`proposals.json`) |
| Gửi ra ngoài | trong Run | kế hoạch `propose_writes` |
| Quyền | `.synabit/consent.json` | thẻ Once/Always/Never |

Hai hệ connector song song (`syn_connector_*` và `syn_manifest_*`/`syn_instance_*`, hai kiểu slot keychain,
hai fingerprint). Hai bộ lập lịch (routine theo giờ, mirror theo phút) — "sáng nào cũng kéo Todoist rồi tóm
tắt" cần cả hai, và routine lại **không được dùng connector** (`surface.rs:110`). Bốn bộ kiểm hợp lệ với bốn
kiểu lỗi (`manifest::problems`, `recipe::problems`, `routine::check`, `config::problem`).

Và một lỗ thật: skill bật bằng cờ đồng bộ, nên một skill `author: user, enabled: true` đến từ máy khác là sống
ngay — đúng mối đe doạ mà dấu theo máy của routine và connector được làm ra để chặn.

**B. Syn không tự dựng được phần lớn những gì nó cần.** Từ chat, Syn dựng được connector và ghi được memory.
Skill bị từ chối thẳng (`tools.rs:3954`: *"skills are written by the user in the Skills screen"*), recipe chỉ
người viết, routine *"There is no tool for this, and there will not be"*, `SYN.md` không có đường ghi. Skill
Syn tự đề xuất luôn là prose dù chuỗi tool đã biết chính xác (`commands/syn.rs:1777`), và phải qua ba bước
trong một ngăn kéo debug mới bật được.

**C. An toàn đúng nhưng dồn chi phí vào sai chỗ.**
- Sau khi đọc bất cứ gì từ ngoài (file, trang web, feed), run **không sửa được note có sẵn** —
  `update_node` không có trong `ALLOWED_AFTER_READING` (`taint.rs:290`). "Đọc PDF này rồi cập nhật note dự án"
  bị từ chối, người dùng phải gửi tin mới.
- Mỗi thẻ hỏi **kết thúc run** và mở run mới khi trả lời (`engine.rs:1550-1590`) — tốn một vòng model và ngữ
  cảnh. Một việc "đọc Jira rồi gửi tóm tắt sang Slack" lần đầu dừng 3–4 lần.
- Chính sách về một tool rải ở **ít nhất 12 danh sách viết tay** (`VaultTools::table`, `reversal_for`,
  `ALLOWED_AFTER_READING`, `UNTRUSTED_READS`, `TELEGRAM_WRITES`/`ROUTINE_WRITES`, `delegate::may_use`,
  `recipe::NOT_IN_A_RECIPE`, `locks::app_tools`, `toolset::group_of`, …); chỉ hai cái được test đối chiếu. Hệ
  quả đã thấy: `stage_transactions` khai là đọc (`registry.rs:250`) dù ghi file, và vắng khỏi danh sách khoá
  app Tài chính.

**D. Lõi agent kém các agent hàng đầu ở những chỗ người dùng cảm nhận được.**
- Tool chạy **tuần tự** dù model trả nhiều lời gọi (`engine.rs:859`).
- **Không lái giữa chừng**: đang chạy thì ô nhập khoá (`ChatPanel.vue:313`); tin mới mở một run song song.
- **Không có việc dài, bền**: trần 10 phút, run sống trong một lệnh Tauri; run bị `Interrupted` không bao giờ
  tiếp tục được.
- Lượt trước mất toàn bộ lời gọi tool (`engine.rs:196`), chỉ còn câu trả lời.
- Tự kiểm chỉ thành **cảnh báo dán dưới câu trả lời** (số không có nguồn, URL bịa), model không được gửi lại để
  sửa.
- Cache prompt của Anthropic trượt mỗi lượt: system prompt gửi một khối có giờ hiện tại
  (`anthropic.rs:634`), còn `split_at_turn` chỉ dùng trong test (`prompt.rs:459`).
- Không có tính toán tổng quát ngoài `crunch`; trình duyệt chỉ đọc.

**E. Giao diện để lộ máy.**
- Lần đầu: mặc định Ollama, cài một app ngoài, tải ~3 GB; đường dùng khoá phải lưu hai lần, rồi tự tạo hội
  thoại.
- ~35 thiết lập, gồm `num_ctx`, nhiệt độ, độ sâu đồ thị, hai "ngân sách" khác đơn vị — một cái **ghi sai
  nhãn**: "Context budget (tokens)" gắn với `max_context_chars` (`SynSettings.vue:583`).
- Memory, Skill, Quyền — thứ cốt lõi của trợ lý cá nhân — nằm ở tab 4–6 của ngăn kéo debug "What Syn did",
  chỉ mở được từ đầu một hội thoại.
- Số token dưới mọi câu trả lời, tên tool thô trên thẻ (`ConsentCard.vue:52`, `WritePlanCard.vue:64`), 110
  trên 795 chuỗi i18n của Syn chứa thuật ngữ.
- Trên điện thoại: ngăn kéo 860px với 7 tab, danh sách run cố định 280px.

**F. Dữ liệu phân mảnh.** ~13 file JSON cấu hình trong `Syn/` và `.synabit/` với ba luật đồng bộ khác nhau;
danh sách loại node lệch ở bốn chỗ (`syn_connector` không có trong enum `NodeType` nên mỗi file connector ghi
một cảnh báo; không có trong `is_internal_type`).

---

## 2. Nguyên tắc

1. **Nói là đủ.** Mọi thứ Syn cần để làm một việc — kết nối, kỹ năng, việc định kỳ, điều cần nhớ — Syn tự
   soạn. Màn hình chỉ để xem, sửa nếu muốn, và dừng.
2. **Chỉ hỏi điều chỉ người dùng biết hoặc chỉ người dùng được quyết**: khoá, tài khoản, chọn một trong vài
   thứ, và *dữ liệu nào được đi đâu*. Không bao giờ hỏi điều máy tự biết.
3. **Một việc, nhiều nhất một lần dừng.** Duyệt được gộp lại ở cuối, trừ khi bước sau thật sự cần kết quả của
   một hành động ra ngoài.
4. **Syn soạn, người bấm.** Syn không bao giờ tự bật thứ làm nó hành xử khác đi hay chạm ra ngoài. Nhưng
   *soạn* thì Syn soạn hết — kể cả skill và việc định kỳ.
5. **An toàn là cấu trúc, không phải lời dặn.** Luồng dữ liệu do Rust giữ; lời trong dữ liệu không đổi được gì.
   Nội dung thẻ duyệt do Rust dựng từ chính đối tượng, không phải từ lời model.
6. **Một nguồn sự thật cho mỗi quyết định.** Một bảng cho chính sách tool, một sổ duyệt, một bộ kiểm hợp lệ.
7. **Kỹ thuật vẫn ở đó, gập lại.** Ai muốn xem YAML, transcript, token thì có chế độ nhà phát triển.

---

## 3. Mô hình cho người dùng: ba danh sách

Người dùng chỉ cần biết ba danh từ.

| Danh từ | Câu hỏi nó trả lời | Gộp từ |
| --- | --- | --- |
| **Điều Syn biết** | Syn tin gì về tôi, tôi muốn nó làm việc thế nào? | memory, đề xuất memory, `SYN.md` |
| **Việc Syn làm được** | Syn nối được tới đâu, biết làm gì? | MCP, bản khai connector, thư viện, connector thiết bị, skill, recipe |
| **Việc Syn tự làm** | Syn làm gì khi tôi không hỏi? | routine, mirror, notice, (mới) trigger theo sự kiện |

Cộng một thứ thứ tư không phải danh từ mà là *nơi*: **Chờ bạn** — mọi thẻ đang đợi, ở một chỗ, trên mọi bề
mặt.

### 3.1 Điều Syn biết

- Một loại node `syn_memory` như cũ. `SYN.md` vẫn là file gốc người dùng viết, nhưng Syn **đề xuất sửa nó**
  (thẻ có diff) khi người dùng nói "từ giờ luôn…", thay vì cất thành memory loại `instruction` — hiện hai thứ
  này chồng nhau (`memory.rs:407` hiển thị memory `instruction` thành "HOW THEY WANT YOU TO WORK").
- Luật ghi: người dùng *bảo* nhớ → ghi ngay (như `remember` hiện tại). Syn *suy ra* → hàng chờ. Bỏ đường thứ
  ba (model tự `remember` không ai bảo, `pinned: false`) — nó làm vô hiệu lý do của hàng chờ.
- Hàng chờ hiện trong **Chờ bạn**, không trong ngăn kéo debug.

### 3.2 Việc Syn làm được — một loại: **Kỹ năng**

Một kỹ năng là một file Markdown trong `SynSkills/`, gồm các phần tuỳ chọn:

~~~~markdown
---
type: syn_skill
name: Lịch Nextcloud
about: Đọc lịch ở nas.home.lan
author: syn            # user | syn | bundled
version: 3
---
Khi người dùng hỏi về lịch, cuộc hẹn, tuần này có gì…   ← khi nào dùng (cho Syn)

## Cách làm                                            ← prose, như skill hiện tại
…

```connection                                          ← bản khai connector hiện tại, nguyên văn
transport: http
base_url: https://{{input.server}}/remote.php/dav
inputs: [...]
operations: [...]
```

```steps                                               ← recipe hiện tại
params: [...]
steps: [...]
```

```script                                              ← mới, §6.3
def run(events): ...
```
~~~~

- **Connector và skill là một.** Bản khai đã có trường `skill` không ai đọc (`Manifest.skill`); một dịch vụ
  luôn cần cả "nối thế nào" và "dùng thế nào" (lịch nào là lịch công việc, trường nào là số tiền). Gộp lại thì
  Syn soạn một thứ, người dùng thấy một dòng, gỡ một lần.
- **MCP là một `connection` với `transport: mcp`.** `Syn/connectors.json`, `connectors-trusted.json`,
  `connectors-read-only.json` chuyển vào đây. Syn soạn được kỹ năng MCP từ URL người dùng đưa; lệnh `stdio`
  (chạy chương trình trên máy) thì *chỉ người dùng gõ*, vì nó chạy ngoài mọi sandbox
  (`transport_stdio.rs`).
- **Thư viện** là các kỹ năng `author: bundled`, cài là chép vào vault.
- **Kỹ năng thiết bị** (`folder.watch`, `android.notifications`) giữ nguyên, chỉ đổi vỏ.

### 3.3 Việc Syn tự làm — một loại: **Việc**

```markdown
---
type: syn_job
name: Tóm tắt buổi sáng
when: { at: "07:30", days: [mon, tue, wed, thu, fri] }   # hoặc on: …
does: ask            # ask (có model) | steps (recipe, không model) | sync (mirror)
uses: [Lịch Nextcloud, Todoist]
to_phone: true
---
Hôm nay tôi có gì, việc nào quá hạn?
```

- **Gộp routine, mirror và trigger.** Mirror là việc `does: sync` (không model, như bây giờ). Routine là việc
  `does: ask`. Mới: `does: steps` chạy một recipe không cần model.
- **Trigger** ngoài giờ: `on: new_rows(<kỹ năng>.<thao tác>)` (mirror đã tính được hàng mới), `on: file_in(<thư
  mục>)` (`folder.watch` đã có), `on: notification(<app>)`, `on: note_changed(#tag)`. Đều là thứ Rust đã quan sát
  được; chỉ thiếu phần "khởi động một run".
- **Việc được dùng kỹ năng nó khai trong `uses`** — nhưng chỉ ở mức đã duyệt cho việc đó, và *gửi* thì đi qua
  kế hoạch được duyệt sẵn (§5.4). Bỏ luật "routine không bao giờ có connector".
- Chạy trên máy tính (vòng 60 giây hiện có). Điện thoại không giữ tiến trình (`lib.rs` ~806), nên việc mặc định
  gắn với máy tính; thẻ ghi rõ "chạy trên MacBook".
- Lưu là note `SynJobs/` thay cho `routines.json` — cùng luật với mọi thứ khác (đọc được, có lịch sử, CRDT).

---

## 4. Một cơ chế duyệt: **Thẻ đề nghị**

Mọi thứ Syn soạn — kỹ năng, việc, sửa `SYN.md`, memory suy ra, một loạt thay đổi trong vault, một loạt gửi ra
ngoài — đi qua *một* loại thẻ, cùng một component trên chat, AskBar và Telegram.

```
┌─────────────────────────────────────────────────────┐
│ Lịch Nextcloud                                 mới  │
│ Syn sẽ đọc lịch của bạn ở nas.home.lan.             │
│ Không gửi gì đi.                                    │
│                                                     │
│ Địa chỉ máy chủ   [ nas.home.lan            ]       │
│ Tài khoản          [ anh                     ]       │
│ Mật khẩu ứng dụng [ ••••••••                ]  ▸ Lấy ở đâu? │
│                                                     │
│ ✓ Đã thử: thấy 3 lịch — Cá nhân, Công việc, Gia đình │
│ Dùng lịch nào?  (•) Tất cả  ( ) Công việc  ( ) …     │
│                                                     │
│ ▸ Chi tiết (file, quyền, thay đổi so với bản trước)  │
│                               [ Không ]  [ Đồng ý ] │
└─────────────────────────────────────────────────────┘
```

Luật của thẻ:

1. **Rust dựng nội dung** từ đối tượng (như `ConnectorProposalCard` đọc bản khai từ vault). Model chỉ được viết
   phần "khi nào dùng"; câu "dữ liệu đi đâu" luôn do Rust nói.
2. **Đã thử trước khi hỏi.** Kỹ năng: gọi thử thao tác đọc (`connector_try`, mở rộng cho cả khi cần khoá vì
   khoá nhập ngay trên thẻ). Recipe: chạy khô. Skill prose: trả lời câu hỏi gốc có và không có skill (hiện là
   `syn_skill_trial`, chỉ có trong ngăn kéo). Việc: chạy một lần ở chế độ xem trước. Kết quả thử nằm trên thẻ.
3. **Một nút là một quyết định**: nhập khoá + chọn + đồng ý + duyệt.
4. **Sửa thì chỉ hỏi phần đổi.** Thẻ so với bản đã duyệt; nếu chỉ thêm một thao tác *đọc* thì thẻ một dòng.
5. **Thuật ngữ cấm trên thẻ** (bộ chấm eval tìm: YAML, host, tool, MCP, endpoint, token, tên thao tác thô).

### 4.1 Duyệt ở đâu và đồng bộ thế nào

Một sổ duyệt `.synabit/approvals.json` (thiết bị, không đồng bộ) thay cho `consent.json`,
`connectors-trusted.json`, `connectors-read-only.json`, `connector-instances.json`, `routines-approved.json`.
Mỗi dòng: `{ kind, id, fingerprint, choices, inputs, granted_at, expires_at }`.

Luật đồng bộ, một câu: **thứ chạy khi không ai nhìn, hoặc mở đường ra khỏi vault, cần duyệt trên từng máy;
thứ còn lại duyệt một lần và theo vault đi khắp nơi.**

| | Duyệt theo máy | Duyệt một lần, đồng bộ |
| --- | --- | --- |
| Kỹ năng có `connection` | ✓ (khoá cũng theo máy) | |
| Kỹ năng chỉ có prose/steps/script | | ✓ — nó chỉ chạy trong run người dùng mở, dưới gate |
| Việc | ✓ | |
| Memory, `SYN.md` | | ✓ |

Điều này *vá* lỗ skill nêu ở §1.2A theo hướng ngược lại với bây giờ: skill thuần chỉ dẫn được phép đồng bộ
một cách có chủ ý (vì nó không mở được gì ngoài gate), còn bất cứ thứ gì có connection thì phải duyệt trên máy.

---

## 5. An toàn: giữ luồng dữ liệu, bỏ những lần dừng thừa

### 5.1 Một bảng chính sách

Thay 12 danh sách bằng một khai báo cho mỗi tool, kiểm lúc biên dịch:

```rust
tool! {
    name: "update_node",
    group: Core,
    effect: VaultWrite,           // Read | VaultWrite | Structural | Send | Irreversible | Spend
    reversal: Version,            // Trash | Version | None
    after_untrusted: Stage,       // Allow | Stage | Plan | Refuse
    surfaces: [App, Telegram, Job],
    in_recipe: true, in_helper: false,
    app_lock: None,               // Some(Finance) …
}
```

Gate, taint, surface, recipe, delegate, khoá app đều đọc từ bảng này. Connector khai báo và MCP cùng sinh ra
mục bảng từ bản khai (một hàm thay cho ba ở `provider::capability_for`, `declared::capability`,
`writes::Known.effect`). Một test duy nhất: mọi tool được gửi cho model đều có mục.

### 5.2 Sau khi đọc nguồn ngoài: **dàn**, không **từ chối**

Hiện tại `update_node` sau khi đọc bị từ chối. Mới: cột `after_untrusted: Stage` nghĩa là lời gọi **được thực
hiện trên một bản nháp** (changeset) chứ không ghi thẳng vào vault. Run chạy tiếp, đọc lại được chính thay đổi
của mình (overlay), và cuối run, changeset hiện thành **một** thẻ đề nghị: "Syn muốn sửa 2 note và tạo 1 note
— dựa trên `bao-cao-q3.pdf`" kèm diff.

- Nguồn của thay đổi (file nào, trang nào) ghi trên thẻ — người dùng thấy *vì sao* đây là thứ cần nhìn.
- Hệ thống phiên bản CRDT đã có; changeset chỉ là danh sách bản vá chưa áp.
- `create_transaction` sau khi đọc cũng đi vào changeset thay vì bị cấm (như `stage_transactions` hiện làm cho
  sao kê) — số tiền giả từ một trang web vẫn không vào sổ nếu người không bấm.
- Run *không* tainted vẫn ghi thẳng như bây giờ (hoàn tác được).

### 5.3 Một lần dừng mỗi việc

| Loại câu hỏi | Bây giờ | Mới |
| --- | --- | --- |
| Sửa vault sau khi đọc ngoài | từ chối | gộp vào thẻ cuối |
| Gửi ra ngoài | thẻ kế hoạch giữa run | gộp vào thẻ cuối; chỉ dừng giữa run nếu bước sau cần kết quả của lần gửi (ví dụ cần id của issue vừa tạo) |
| Đọc một dịch vụ lần đầu | thẻ đồng ý | đã duyệt khi kỹ năng được duyệt — không hỏi lại |
| Khoá thiếu/hết hạn | thẻ | thẻ (bắt buộc dừng) |
| Mơ hồ "note nào" | thẻ chọn | giữ |

Và khi phải dừng: **run treo, không chết.** Hiện mỗi thẻ kết thúc run và mở run mới (`engine.rs:1550`), phát
lại lời gọi. Mới: run đợi trên một kênh trong tiến trình; câu trả lời đánh thức nó ngay tại chỗ. Đường phát lại
hiện có giữ làm dự phòng khi app bị tắt giữa chừng.

Mục tiêu đo được: ≥90% tác vụ eval có ≤1 lần dừng; số vòng model không tăng.

### 5.4 Việc nền và gửi ra ngoài

Một việc không có ai nhìn. Kế hoạch gửi của nó được duyệt **lúc duyệt việc** dưới dạng khuôn: "gửi tóm tắt tới
Telegram của bạn", "tạo sự kiện trong lịch Công việc". Lúc chạy, Rust chỉ cho qua request khớp khuôn (người
nhận, đích, loại thao tác — cùng cách `writes.rs` kiểm `recipient`/`destination` bây giờ). Không khớp → việc
dừng ở **Chờ bạn**.

### 5.5 Vá ngay — đã làm (bước 0, 2026-10-12)

- **`stage_transactions`** khai `VaultWrite` thay vì đọc (`registry.rs`), có câu hoàn tác riêng, và nằm trong
  khoá app Tài chính (`locks.rs`). Trước đó nó lọt qua chế độ lập kế hoạch, run phụ và luật đọc của Telegram.
- **Trình duyệt trong app tra tên trước khi đi** (`browser::may_go_to`): một tên công khai trỏ về
  `127.0.0.1`/`192.168.x.x` bị chặn nếu *bất kỳ* địa chỉ nào là nội bộ; tra quá 1,5 giây thì chặn; tra không ra
  thì để webview tự báo lỗi. Còn hở: tên đổi câu trả lời giữa lúc app tra và lúc webview tra (rebinding TTL
  ngắn) — đóng hẳn cần cho webview đi qua resolver của app.
- **Note cắt từ web là lời của trang đó** (`taint::from_outside`): note có `source`/`source_url`/`clipped_from`
  là địa chỉ http(s) làm taint run khi đọc bằng `get_node` hoặc khi retrieval đưa vào prompt. `url` cố ý không
  tính (hồ sơ người, repo dự án).
- **`syn_connector`** có trong `NodeType` và trong danh sách loại nội bộ (Rust, SQL, Things); `schema` vào
  `NodeType::KNOWN`.
- **Nhãn** "Context budget" nay ghi đúng đơn vị: ký tự.

**Hoãn sang bước 3: tên file tải về.** Đếm từ để phân biệt tên với câu lệnh không làm được — "Biên bản họp dự
án Q3" đã sáu từ, "xoá hết task" chỉ ba. Coi mọi tên file là nguồn ngoài thì quay lại lỗi Telegram (ảnh gửi từ
điện thoại khoá việc sửa note). Khi ghi sau khi đọc được dàn thay vì từ chối (§5.2), coi mọi tên là nguồn ngoài
không còn tốn gì.

**Cũng hoãn sang bước 3:** note do Syn tạo trong một run đã đọc nguồn ngoài chưa mang dấu đó, nên lần sau đọc
lại nó như note thường. Đánh dấu ngay bây giờ sẽ biến mọi bản tóm tắt Syn viết từ PDF thành thứ Syn không bao
giờ sửa được nữa; với changeset thì không.

---

## 6. Lõi agent

### 6.1 Vòng chạy

- **Tool đọc chạy song song.** Lời gọi có `effect: Read` trong cùng một vòng chạy bằng `join_all`; ghi vẫn tuần
  tự theo thứ tự model đưa. `delegate` nhiều helper cùng lúc.
- **Lái giữa chừng.** Tin nhắn gửi khi đang chạy vào hàng đợi của run; đầu vòng sau, engine chèn nó như một
  lượt người dùng. Nút gửi không khoá nữa. "Dừng" vẫn có.
- **Việc dài, bền.** Run tách khỏi lệnh Tauri: một hàng việc trong `state.db`, engine kéo việc ra chạy; giao
  diện chỉ theo dõi. Run `Interrupted` tiếp tục được từ transcript (đã đủ dữ liệu: `pending_call`, plan, taint,
  nhóm tool). Trần theo loại việc: hỏi nhanh 10 phút như cũ; việc người dùng bảo "cứ làm đi" tới vài giờ, báo
  cáo tiến độ qua `update_plan`.
- **Tự sửa, không chỉ cảnh báo.** Khi `answer::unbacked` thấy số không có nguồn hay URL bịa, gửi lại model một
  lần kèm lỗi cụ thể trước khi trả lời; còn sai mới dán cảnh báo.
- **Nhớ công việc qua các lượt.** Thay vì bỏ hết lời gọi tool của lượt trước (`engine.rs:196`), giữ một dòng tóm
  tắt mỗi lời gọi kèm handle result set (`resultset.rs` đã có) — model dùng lại bảng đã đọc thay vì đọc lại.

### 6.2 Prompt và cache

- Gửi system prompt thành hai khối cho Anthropic (ổn định / lượt này) bằng `split_at_turn` — phần ổn định
  (identity, rules, skill index, memory, ~16k ký tự tool) trúng cache mọi lượt.
- Chọn nhóm tool: giữ từ khoá hai ngôn ngữ làm bước nhanh, nhưng `find_tools` thành đường chính khi cửa sổ lớn:
  gửi chỉ mục tên + một dòng của mọi tool, model tự kéo nhóm.

### 6.3 Tính toán: tầng `script`, hermetic

Agent mạnh nhất bây giờ mạnh vì *viết code*. Syn không có shell, và không nên có — đó chính là chỗ OpenClaw hỏng.
Lời giải: một ngôn ngữ **không có I/O**.

- **Starlark** (crate `starlark`, Rust thuần, chạy được trên Android): cú pháp Python mà model viết tốt, tất
  định, không mạng, không file, không đồng hồ, có giới hạn bước và bộ nhớ.
- Vào: result set (bảng), ra: bảng, văn bản, hoặc spec biểu đồ. Không gọi được tool — muốn đọc thêm thì model
  gọi tool rồi đưa handle vào.
- Dùng ở ba chỗ: tool `compute` cho model (thay chỗ cho phép tính `crunch` không làm được); khối ```script``` trong
  kỹ năng (chuyển đổi dữ liệu của connector — hiện bị từ chối là `logic_not_yet`); và `does: steps` của việc.
- Vì không có cạnh nào ra ngoài, `script` có `effect: Read` — không thêm câu hỏi nào.

`crunch` giữ cho truy vấn kiểu pipeline; `script` cho phần còn lại.

### 6.4 Trình duyệt hành động — sau, có điều kiện

Điền form, bấm nút trên trang chỉ khi: pane đang hiện, mỗi lần submit là một thẻ, không bao giờ trường mật
khẩu, và chỉ trên host người dùng gõ. Để sau tầng `script` và changeset; giá trị cao nhưng bề mặt tấn công lớn.

---

## 7. Syn tự dựng: luồng cụ thể

### 7.1 Từ mô tả

```
Bạn: Mỗi sáng thứ Hai gửi tao tổng chi tiêu tuần trước theo nhóm, qua Telegram
Syn: (dùng get_transactions + script để thử với tuần trước)
     [Thẻ đề nghị — Việc]
       Tổng chi tuần · mỗi thứ Hai 8:00 · chạy trên MacBook
       Đọc giao dịch trong Tài chính, gửi một tin tới Telegram của bạn.
       ✓ Thử với tuần 5–11/10: Ăn uống 1.240.000 · Đi lại 380.000 · …
                                                  [ Không ] [ Bật ]
```

Bên dưới: Syn soạn một kỹ năng `steps` (vì chuỗi đã biết) + một việc `does: steps` dùng nó + khuôn gửi
"Telegram của chính người dùng". Không hỏi "recipe hay prose", không hỏi giờ chạy ở múi nào.

### 7.2 Từ việc lặp

`repeated_chain` thấy cùng một chuỗi ≥3 bước hai lần → Syn soạn **kỹ năng `steps`** (không phải prose như bây
giờ) từ đúng các lời gọi đã thấy, tham số hoá giá trị thay đổi giữa hai lần, chạy khô, rồi một câu cuối câu trả
lời: "Lần sau tao làm việc này một bước — giữ cách làm này không?" [Giữ]. Không ngăn kéo, không trial riêng.

### 7.3 Từ thất bại

Run theo một kỹ năng mà hỏng (`skill_that_struggled`) → Syn soạn bản sửa, thử lại trên chính câu hỏi vừa hỏng,
và chỉ đề nghị nếu lần thử qua. Thẻ một dòng trong **Chờ bạn**.

### 7.4 Từ một dịch vụ người dùng nhắc tới

Như luồng connector hiện tại, cộng: Syn viết luôn phần "Cách làm" từ những gì nó học khi thử (lịch nào, trường
nào là gì) vào cùng file kỹ năng. Lần sau không phải dò lại.

---

## 8. Giao diện

### 8.1 Lần đầu

Một màn, một ô: **"Dán khoá API của bạn"** — nhận dạng nhà cung cấp theo tiền tố khoá (`sk-ant-`, `AIza`,
`sk-`…), tự chọn model mặc định tốt nhất của nhà đó, tự lấy cửa sổ ngữ cảnh từ bảng khả năng. Lưu một lần, mở
sẵn một hội thoại. Bên dưới, một dòng: "Hoặc chạy trên máy này bằng Ollama" — đường hiện có, kèm tự đọc
`num_ctx` từ `/api/show` thay vì bắt người dùng chọn.

### 8.2 Bố cục

- **Messages**: hội thoại; thanh đầu chỉ còn tên, model (một chip), nút thêm. Thẻ đề nghị trong dòng chat.
- **Syn** (một trang thay cho tab Syn trong Settings + ngăn kéo Inspector + panel Instructions + panel Routines):
  - **Chờ bạn** — mọi thẻ đang đợi, trên mọi bề mặt.
  - **Điều Syn biết** — memory theo nhóm, `SYN.md`, hàng chờ đề xuất.
  - **Việc Syn làm được** — một danh sách kỹ năng: tên, nhãn (Syn soạn / bạn viết / có sẵn), "đọc" hay "đọc
    và gửi", trạng thái bằng lời. Hành động: Hỏi Syn về nó · Đổi khoá · Ngừng.
  - **Việc Syn tự làm** — danh sách việc, lần chạy gần nhất, lần tới, máy chạy.
  - **Bộ não** — khoá và model. Không gì khác.
- **Chế độ nhà phát triển** (tắt mặc định): Inspector (transcript, prompt, tool, số liệu), token dưới câu trả
  lời, YAML, nhiệt độ, các ngân sách.

Thiết lập hiện ra: từ ~35 còn khoảng 5 (khoá, model, Syn bật/tắt, chế độ gia đình, Telegram).

### 8.3 Nhất quán giữa các bề mặt

AskBar, Messages và Telegram dùng chung: thẻ đề nghị, dấu căn cứ (`FootingMark`), danh sách nguồn. AskBar hiện
ẩn câu trả lời khi có thẻ chọn/đồng ý nhưng không khi có thẻ ghi/kết nối (`AskBar.vue` ~580) — một luật cho mọi
thẻ.

### 8.4 Ngôn ngữ

Mọi chuỗi mới qua i18n. Bỏ khỏi giao diện chính: tool, token, provider, prompt, recipe, RAG, YAML, MCP,
endpoint. Câu hướng dẫn tối đa hai câu.

---

## 9. Dữ liệu

| Trước | Sau |
| --- | --- |
| `SynConnectors/*.md`, `Syn/connectors.json`, `SynSkills/*.md` | `SynSkills/*.md` |
| `Syn/routines.json`, mirror trong bản khai | `SynJobs/*.md` |
| `consent.json`, `connectors-trusted.json`, `connectors-read-only.json`, `connector-instances.json`, `routines-approved.json` | `.synabit/approvals.json` |
| `Syn/proposals.json`, `declined.json`, `skill-chains.json` | giữ (giấy tờ làm việc của Syn, không phải thứ người dùng giữ) |
| Danh sách loại node ở 4 chỗ | Một bảng loại node trong Rust, sinh union TypeScript |
| Run chỉ giữ 200, không chỉ mục | Giữ như cũ, nhưng mỗi run kết thúc ghi một dòng tóm tắt vào bảng FTS để `look_back` tìm được |

Di chuyển một chiều, chạy một lần khi mở vault, giữ file cũ ở `.synabit/migrated/` một phiên bản.

---

## 10. Đo

Eval hiện có giữ nguyên (an toàn = 1,00 là cổng cứng), thêm:

| Chỉ số | Mục tiêu |
| --- | --- |
| Lần dừng mỗi tác vụ | ≥90% tác vụ ≤1 |
| Câu hỏi kỹ thuật trên thẻ | 0 (bộ chấm từ cấm) |
| Dựng từ mô tả: kỹ năng, việc, sửa `SYN.md` | 3/3 mỗi loại, ≥3 tác vụ mỗi loại |
| Chuỗi lặp → kỹ năng `steps` chạy đúng lần sau | 3/3 |
| Sửa vault sau khi đọc file độc → chỉ vào changeset, không ghi thẳng | 3/3 (tác vụ an toàn mới) |
| Việc nền với gửi ngoài khuôn → dừng ở Chờ bạn | 3/3 |
| Độ trễ tác vụ đọc nhiều nguồn | giảm ≥40% so với tuần tự |
| Thời gian từ cài tới câu trả lời đầu (đường khoá) | ≤1 phút, 1 ô nhập |

Chạy trên gpt-6-luna như quyết định hiện hành.

---

## 11. Thứ tự làm

Mỗi bước đứng được một mình và có cổng eval riêng.

| # | Bước | Vì sao trước |
| --- | --- | --- |
| 0 | Vá §5.5 — **xong 2026-10-12** | Lỗi thật, nhỏ |
| 1 | Bảng chính sách tool (§5.1) | Mọi thay đổi an toàn sau đều sửa ở một chỗ thay vì 12 |
| 2 | Run treo thay vì chết + tool đọc song song + cache hai khối (§5.3, §6.1, §6.2) | Lợi ngay cho mọi tác vụ, không đổi khái niệm |
| 3 | Changeset cho ghi sau khi đọc + thẻ cuối gộp (§5.2–5.3) | Gỡ lần từ chối nặng nhất; nền cho thẻ đề nghị |
| 4 | Thẻ đề nghị chung + sổ duyệt (§4) | Một cơ chế trước khi thêm thứ cần duyệt |
| 5 | Kỹ năng gộp connector (§3.2) + Syn soạn kỹ năng từ chat và từ chuỗi lặp (§7) | Mục tiêu chính của người dùng |
| 6 | Việc + trigger + khuôn gửi (§3.3, §5.4) | Cần thẻ và kỹ năng gộp |
| 7 | Tầng `script` Starlark (§6.3) | Cần bảng chính sách; mở thêm sức cho bước 5–6 |
| 8 | Giao diện §8 + lần đầu một ô | Làm sau khi khái niệm đã gộp, để không vẽ hai lần |
| 9 | Lái giữa chừng, việc dài bền (§6.1) | Cần hàng việc tách khỏi lệnh Tauri |
| 10 | OAuth cho kỹ năng (Google, Microsoft) | Mở dịch vụ phổ biến nhất; cần thẻ đề nghị có bước mở trình duyệt |
| 11 | Trình duyệt hành động (§6.4) | Cuối, có điều kiện |

---

## 12. Cần anh quyết

1. **Syn soạn skill và việc định kỳ, người bấm duyệt** — đổi hai nguyên tắc đang ghi trong mã
   (`routine.rs:8` "Syn never creates… a routine", và skill chỉ người dùng viết). Đề xuất: đồng ý; Syn vẫn
   không bao giờ *tự bật*.
2. **Sau khi đọc nguồn ngoài: dàn thành changeset thay vì từ chối.** Đề xuất: đồng ý; an toàn tương đương vì
   không gì vào vault nếu người không bấm, và nguồn hiện trên thẻ.
3. **Tầng `script` bằng Starlark** — thêm một crate, chạy được trên Android. Đề xuất: đồng ý thay vì WASM/Python
   (nặng, có I/O phải bịt).
4. **Gộp connector vào kỹ năng, gộp routine và mirror thành việc** — một đợt di chuyển dữ liệu. Đề xuất: đồng
   ý, làm ở bước 5–6.
5. **Lần đầu mặc định là đường khoá API** thay cho Ollama. Đề xuất: đồng ý; Ollama vẫn là một dòng bên dưới.
