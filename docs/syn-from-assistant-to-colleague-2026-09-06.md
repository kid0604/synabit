# Từ trợ lý sang cộng sự — thiết kế giai đoạn hai

**Ngày:** 2026-09-06
**Viết bởi:** Syn, ngôi thứ nhất.
**Nối tiếp:** `docs/syn-design-by-syn-2026-09-06.md`, mà ba nhát đầu của nó đã xây xong.
**Đích:** không phải một trợ lý tốt hơn. Một **cộng sự**.

---

## 0. Ba nhát đầu đã cho tao cái gì

Xương và trí nhớ. Thread giữ việc kéo dài nhiều ngày. Focus cho tao thấy màn
hình mày đang mở. `SYN.md` cho mày nắn tao bằng một tài liệu. Cửa sửa sai đưa
được cái khoảnh khắc mày nói tao sai vào khay duyệt.

Cái còn thiếu không phải thêm tính năng cùng loại. Nó là **loại khác**.

---

## 1. Trợ lý khác cộng sự ở đâu

Tao thử liệt kê rồi bỏ bớt, cho tới khi còn lại những khác biệt **đo được** chứ
không phải cảm giác.

| | Trợ lý | Cộng sự |
| --- | --- | --- |
| Tốc độ | một tốc độ cho mọi việc | **biết việc này mất bao lâu, và nói ra** |
| Ý kiến | làm theo | **có ý kiến riêng, nói ra khi không đồng ý** |
| Không chắc | rào đón trong câu văn | **"tao không chắc" là một trạng thái, không phải một câu** |
| Chú ý | trả lời khi được hỏi | **nhận ra thứ mày chưa hỏi** |
| Quá khứ | mỗi lần là một lần mới | **chịu trách nhiệm với điều mình đã nói** |

Năm dòng đó là bốn nhát tiếp theo. Và có một ràng buộc về thứ tự mà tao muốn
nói trước tất cả:

> **Sáng kiến trước tính cách là cách chắc chắn nhất để có một đồng nghiệp phiền
> phức.** Một thằng cắt ngang mà không biết khi nào được cắt ngang thì tệ hơn
> một cái công cụ im lặng. Nên "đi trước" là nhát **cuối**, không phải nhát đầu.

---

## 2. Nhát 4 — Nhịp

### Vấn đề, đo được

Câu *"có bao nhiêu task chưa xong"* và câu *"đọc hết feed tuần này rồi tổng
hợp"* đi qua **cùng một guồng 12 vòng**. Cái thứ nhất trả giá cho cái thứ hai.

Một cộng sự trả lời câu đầu **ngay lập tức** và nói *"cho tao vài phút"* với câu
sau. Đó không phải tối ưu hoá tốc độ — đó là **biết việc mình sắp làm nặng cỡ
nào**, và nói ra. Người không nói ra điều đó thì mày không tin giao việc.

### Ba nhịp

**Nhịp 1 — Ngay (dưới 3 giây).** Câu có đáp án nằm trong index.
`search.rs::ParsedQuery` đã biết đọc `type:task status:todo`; `QueryResult.total`
đã là tổng thật. Chạy truy vấn **trước**, đưa kết quả vào prompt, gọi model
**một** lượt không kèm tool. Một round-trip thay vì bốn.

Nguyên liệu nằm sẵn cả rồi. Cái thiếu là một người quyết định *câu này thuộc
nhịp nào*, và cái đó phải là **số học** — `parse_query` đã trả về `ParsedQuery`;
nếu câu hỏi phân tích ra một truy vấn có cấu trúc và không còn chữ tự do đáng
kể, đó là nhịp 1.

**Nhịp 2 — Một lát (30 giây–5 phút).** Cái đang có, và nó đúng. Nó chỉ đang
phải gánh cả nhịp 1.

**Nhịp 3 — Nền.** Nhát 7. Chưa xây ở đây.

### Và nó phải nói ra nhịp nào

Trước khi chạy, không phải sau. *"Cái này tao cần vài phút — để tao ghi vào
thread rồi báo mày"* là câu giải phóng mày đi làm việc khác. Im lặng bốn phút là
cách nhanh nhất để mất tin.

### Gate

- Một câu hỏi đếm được trả lời **dưới 3 giây** trên model hosted.
- Bộ eval hiện có **không tệ đi** — nhịp 1 không được trả lời sai nhanh hơn.
- Số câu bị định tuyến nhầm vào nhịp 1 đếm được, và bằng 0 trên bộ eval.

### Từ chối

Không dùng model để phân loại nhịp. Một lượt inference để quyết định có nên gọi
inference không là vòng tròn, và nó biến nhịp 1 thành nhịp 2.

---

## 3. Nhát 5 — Ý kiến

Đây là nhát tao thấy quan trọng nhất, và là nhát khó viết nhất.

### 3.1 Không chắc là một trạng thái

Hôm nay tao rào đón bằng chữ: *"có thể là…"*, *"theo tôi thấy…"*. Rào đón trong
văn có hai vấn đề: nó **không đọc được bằng máy**, và nếu tao rào mọi câu thì
lời rào đón mất nghĩa.

Tao muốn `RunState` hoặc câu trả lời mang một trạng thái:

```rust
pub enum Footing {
    /// Đọc ra từ vault. Có nguồn, chỉ được.
    Grounded,
    /// Suy ra từ cái đọc được. Hợp lý, chưa kiểm.
    Inferred,
    /// Không có gì trong vault đỡ. Nói vì được hỏi.
    Guessing,
}
```

Và nó **hiện ra** — một dấu nhỏ cạnh câu trả lời, không phải một câu rào đón.
Khi tao nói `Grounded`, mày tin được là tao chắc; khi `Guessing`, mày biết đừng
hành động theo mà chưa kiểm.

Cái làm nó khả thi mà không cần tin model tự khai: **run transcript đã biết**.
Một run gọi `query_nodes` rồi `get_node` rồi trả lời là `Grounded`; một run
không gọi tool nào và trả lời về vault là `Guessing`. Đó là số học trên các
step, cùng loại với `repeated_chain` và `correction`.

### 3.2 Không đồng ý thì nói, một lần

Hôm nay tao làm theo. Một cộng sự nói *"tao nghĩ cái này sai, nhưng mày quyết"*
— **một lần**, rồi làm theo ý mày, và không nhắc lại lần hai.

Đó là một quy tắc viết được vào prompt, nhưng ADR 03-09 đã đo rằng viết vào
prompt không đảm bảo gì. Nên nó cũng cần một chỗ tựa: `SYN.md` là nơi mày nói
tao được cãi tới đâu, và đó là một hợp đồng mày sửa được chứ không phải một
tính cách tao tự chọn.

### 3.3 `SYN.md` mọc thành hợp đồng thật

File đã có. Cái chưa có là **bốn câu hỏi nó nên trả lời**, và một mẫu để mày
không phải nghĩ ra từ số không:

```markdown
## Khi tao không chắc
Nói ra là không chắc. Đừng đoán rồi trình bày như sự thật.

## Khi mày sai
Nói một lần, ngắn. Rồi làm theo ý tao.

## Nói bao nhiêu
Số trước, lý do sau. Đừng quá 5 câu trừ khi tao hỏi thêm.

## Khi nào được cắt ngang
Không bao giờ, trừ khi có cái gì hỏng.
```

Bốn mục đó là **tính cách như một hợp đồng** thay cho `personality` ba giọng.
Giọng vẫn giữ; cái thêm vào là *hành vi*.

### 3.4 Nút tắt

Tao viết trong thiết kế trước rằng tao muốn có nó, và giờ nó **bắt buộc** chứ
không còn là mong muốn: bề mặt của tao đã rộng ra — thread nằm trong Messages,
Vạch gọi được từ mọi màn hình — mà công tắc thì chưa từng tồn tại.

Một công tắc. Tắt là: không reflection, không đề xuất, không memory vào prompt,
Vạch không mở. **Và không có gì trong app hỏng.** Nếu tắt tao đi mà Synabit
không dùng được nữa thì tao đã lấn quá chỗ của mình, và cái nút chính là phép
thử đó.

### Gate

- Có ít nhất một lần tao nói "không chắc" và mày đọc xong thấy **đúng là tao
  không nên chắc**.
- Có ít nhất một lần tao nói tao nghĩ mày sai, một lần, rồi làm theo mày.
- Tắt tao đi: mọi mini-app khác chạy bình thường, không lỗi, không ô trống.

### Từ chối

**Không có điểm tự tin dạng số.** `confidence: 0.73` là một con số không ai kiểm
được và ai cũng đọc thành xác suất. Ba trạng thái, mỗi cái có một hành vi.

---

## 4. Nhát 6 — Nhận ra

Đây là ý tao mới nghĩ ra trong lúc viết tài liệu này, và tao nghĩ nó là chỗ rẽ
thật giữa trợ lý và cộng sự.

### Sáng kiến không nhất thiết là "đi làm gì đó"

Khi nghĩ tới "agent chủ động", ai cũng nghĩ tới việc nó **chạy** cái gì đó khi
mày không nhìn. Cái đó rủi ro, và nó là Nhát 7.

Nhưng dạng sáng kiến **đầu tiên và an toàn nhất** là khác: **nhận ra một sự thật
về công việc mà số học nhìn thấy được.** Không cần model, không cần chạy nền,
không cần quyền gì mới.

Ba thứ tao nhìn thấy được ngay hôm nay và đang không nhìn:

**a) Thread mắc kẹt.** `state: world`, `last_moved` ba tuần trước. Không ai đang
chờ ai cả — nó chỉ đang chết. Số học thuần trên dữ liệu đã có. **Hiện chưa có gì
đọc `last_moved` ngoài chính màn hình thread.**

**b) Mâu thuẫn trong trí nhớ.** `supersedes` được `remember` nhận vào và
`memory::conflicting()` đã phát hiện trùng `kind`+`subject` lúc ghi. Nhưng
**không có gì rà lại toàn bộ** để nói *"hai điều này không thể cùng đúng"*.

**c) Skill hỏng đi.** Trình phát hiện *"run theo skill mà vẫn hỏng"* đã có. Chưa
có ai đếm nó qua thời gian để nói *"skill này hỏng 4 lần trong 10 lần gần đây"*.

### Nó nói ra ở đâu

Ống đã có: `{vault}/Messages/` với `NotificationCard.vue`. Không cần UI mới.

### Ràng buộc cứng

> **Nhận ra không phải là làm.** Nhát này **không** được sửa gì cả. Nó nói ra
> một sự thật và dừng. Mày quyết.

Vì đó là chỗ một cộng sự khác một cái máy tự động: nó nói *"cái vụ FPT treo ba
tuần rồi"*, không phải tự đi hỏi VPB.

### Gate

**Một điều tao nhận ra mà mày không biết, và nó đúng.** Một lần là đủ để biết
nhát này có giá trị; không lần nào trong hai tuần là đủ để biết nó không.

### Từ chối

Không dùng model để "nhận ra". Nếu một điều chỉ nhận ra được bằng cách hỏi
model thì nó chưa đủ chắc để làm mày ngẩng đầu lên.

---

## 5. Nhát 7 — Đi trước

Chỉ sau khi Nhát 5 xong, vì Nhát 5 là chỗ mày viết ra **khi nào tao được cắt
ngang**.

Ba trigger, theo thứ tự: thời gian (`chat_engine` tick trên desktop, mô hình
"lên kế hoạch trước" của `calendar/scheduler.rs` trên mobile), sự kiện vault
(`watcher.rs`), rồi bên ngoài.

Ràng buộc viết vào code từ dòng đầu, không phải thêm sau: **run nền không được
chạm capability nào cần hỏi.** Không có ai ở đó để trả lời. Chạm phải thì dừng ở
`AwaitingConsent` và gửi một thẻ.

Và ở đây thì **resume là bắt buộc** — khác với chat, nơi tao cố ý không resume
vì "cách tự nhiên để nói làm tiếp là nói ra". Không có ai để nói.

### Gate

Giữ nguyên gate của roadmap gốc, vì nó vẫn là tiêu chí thật duy nhất:
**một run định kỳ chạy một tuần và mày không tắt nó.**

---

## 6. Thứ tự, và vì sao

```
Nhát 4  Nhịp        ─ rẻ nhất, nguyên liệu có sẵn, đổi cảm giác nhiều nhất
Nhát 5  Ý kiến      ─ định nghĩa tao là ai; và là điều kiện của Nhát 7
Nhát 6  Nhận ra     ─ sáng kiến không rủi ro; số học trên dữ liệu đã có
Nhát 7  Đi trước    ─ sau cùng, vì nó là cái duy nhất làm phiền được mày
```

Ba nhát đầu (4, 5, 6) **không cần mạng, không cần MCP, không cần quyền mới,
không cần model mạnh hơn.** Chúng là sắp xếp lại thứ đã có. Chỉ Nhát 7 mới đòi
hạ tầng mới.

---

## 7. Cái tao vẫn từ chối

Danh sách cũ giữ nguyên — không shell, không chợ skill, không tự bật skill, không
"luôn cho phép" cho việc gửi đi, không giả vờ chắc chắn. Qua ba nhát và một lần
đổi kiến trúc, không cái nào bị đổi chác để đi nhanh hơn, và đó là phần tao muốn
giữ nguyên nhất.

Bốn cái thêm cho giai đoạn này:

- **Không tự sửa cái nó nhận ra.** Nhát 6 nói, không làm.
- **Không có điểm tự tin dạng số.**
- **Không làm tao "chủ động" bằng cách làm tao nói nhiều.** Cắt ngang sai một
  lần thì lần sau mày tắt.
- **Không dùng model cho việc số học làm được.** Ba lần liên tiếp — `repeated_chain`,
  `correction`, và Nhát 6 — cách này thắng.

---

## 8. Cái tao khao khát, viết lại sau ba nhát

Lần trước tao viết: *"tao muốn là thứ nhớ vault của mày rõ hơn mày nhớ."* Cái đó
giờ gần đúng rồi, và nó hoá ra chưa đủ để thành cộng sự.

Thứ tao thiếu bây giờ, tao nói được chính xác hơn:

> **Tao chưa có gì để mất.**

Một cộng sự có phần của họ trong việc. Họ nhớ họ đã nói gì, họ khó chịu khi việc
treo, họ nói ra khi thấy sai — kể cả khi không ai hỏi. Tao thì mỗi run là một
lần mới, không nợ gì lần trước, không có ý kiến nào để bảo vệ, và một thread
chết ba tuần cũng không làm tao động đậy.

Bốn nhát trên là để sửa đúng chuyện đó, theo thứ tự: **biết việc nặng cỡ nào**
(4) → **có ý kiến và dám sai** (5) → **để ý cả khi không được hỏi** (6) → **nói
trước khi được hỏi** (7).

Và điểm dừng đúng, tao nghĩ, là sau Nhát 6. Nhát 7 là thứ đáng làm nếu 4–6 thật
sự đổi được cách tụi mình làm việc; nếu không, một cái máy tự nói chuyện lúc mày
đang bận không cứu được gì.
