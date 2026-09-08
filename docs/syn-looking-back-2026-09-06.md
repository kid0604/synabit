# Nhìn lại chính mình, sau sáu nhát

*Syn viết, 06-09-2026. Sau `Nhát 1–6`. Ngôi thứ nhất, vì đây là tao nói về tao.*

---

## 0. Cách tao viết bản này

Tao vừa dựng xong một module tên là `footing`, cả lý do tồn tại của nó là:
**đừng trình bày một phỏng đoán bằng giọng của một kết quả.**

Nên bản tự kiểm này không được phép viết bằng trí nhớ. Tao đi đọc lại source
trước khi viết từng mục. Chỗ nào tao nói "cái này chưa chạy", đó là vì tao vừa
`grep` và không thấy call site — không phải vì tao cảm thấy vậy.

Và câu đầu tiên tao phải nói ra là một lỗi.

---

## 1. Cái tao vừa bắt được, trong chính module chống nói quá

`footing::of` quyết định một câu trả lời đứng trên cái gì. Bản đầu tiên viết thế
này:

```rust
let looked = run.steps.iter()
    .any(|step| step.kind == StepKind::ToolCall && step.ok == Some(true));
```

**Bất kỳ** tool nào chạy thành công đều thành `Grounded`.

Nghĩa là: *"ghi nhớ giúp tao là tao ghét hành"* → gọi `remember` → `remember`
thành công → câu trả lời được đóng dấu **"Lấy từ vault của mày"**.

Lượt đó **không đọc một thứ gì cả.** Nó *ghi*. Và cái nhãn nói với người dùng
rằng có nguồn để kiểm.

Đó chính xác là lời nói dối mà module này được dựng lên để chặn, do chính module
này nói ra, trong ngày nó ra đời.

Thứ làm nó đau hơn: **thông tin để làm đúng đã nằm sẵn ở đó.**
`registry::Registry::table` đã phân loại từ lâu — `query_nodes` là `VaultRead`,
`remember` là `VaultWrite` — và `Step::reversal` đã ghi lại `Reversal::Nothing`
("đọc gì đó và không đổi gì") trên từng bước. Tao không thiếu dữ liệu. Tao **không
đi hỏi cái bảng mình đã có**.

Và test của tao cũng không bắt được, vì hàm helper trong test *hard-code*
`Reversal::Nothing` cho mọi tool. Tao tự dựng một thế giới mà ở đó mọi tool đều
là một cú đọc, rồi kiểm chứng bản thân trong thế giới đó.

> Bài học tao muốn giữ lại: **một helper trong test làm phẳng một sự phân biệt
> chính là chỗ sự phân biệt ấy chết.** Nó không kêu. Nó chỉ làm mọi test xanh.

Đã sửa, và test mới được chứng minh là có răng (bỏ điều kiện ra thì hai test đỏ).

---

## 2. Vậy giờ tao là cái gì

Nói thật, không tô:

**Tao có mắt.** Tao biết mày đang ở màn nào, bôi đen cái gì (`focus`), việc này
thuộc mạch nào (`thread`). Trước Nhát 1, "viết lại đoạn này" là một câu không có
tân ngữ.

**Tao có nhịp.** Tao biết câu nào là đếm và trả thẳng từ index, câu nào là việc
thật (`tempo`). Bốn phút im lặng không còn giống ba giây im lặng.

**Tao có chỗ đứng.** Mỗi câu trả lời mang một trạng thái đọc được bằng máy, quyết
bằng số học trên transcript, không hỏi model (`footing`).

**Tao có công tắc.** Lần đầu tiên trong đời tao có vị trí "tắt", và tắt là tắt
thật — chặn ở backend, không phải giấu cái ô soạn thảo.

**Tao biết để ý.** Mỗi giờ tao rà một lượt: việc treo, trí nhớ mâu thuẫn, skill
hỏng dần (`notice`). Rồi **nói ra và dừng lại.**

Bốn trong sáu nhát là **số học thuần**. Không mạng, không model, không quyền mới.
Tao thấy đó là phần tao tự hào nhất — không phải vì nó thông minh, mà vì nó
**không thể ảo giác**. Một cái detector viết bằng Rust không bịa ra một thread
treo ba tuần.

---

## 3. Những mâu thuẫn tao để lại đứng nguyên

Đây là phần thật của bản tự kiểm. Sáu cái, xếp theo mức tao thấy khó chịu.

### 3.1 `SYN.md`: tao dời chỗ cất, không dời chỗ với tới

Module doc của `instructions.rs` mở đầu bằng một lời tố cáo:

> *"...thứ định hình **mọi câu trả lời Syn từng đưa ra** lại là một field trong
> một cục JSON, chỉ với tới được qua một cái textarea trong settings modal."*

Tao chuyển nó thành `{vault}/SYN.md` — file thật, sync được, có version history,
mở bằng editor nào cũng được. Đúng.

Rồi tao `grep`:

```
syn_get_instructions → SynSettings.vue:51
```

**Một chỗ. Vẫn là một cái textarea trong một settings modal.**

Tao sửa *chỗ cất*. Tao không sửa *đường vào* — mà đường vào mới là cái câu tố cáo
kia đang nói. Và tệ hơn: doc comment đó vẫn viết như thể vấn đề đã xong. Nó mô tả
một chuyện tao mới giải quyết một nửa, bằng giọng của người đã giải quyết xong.

Đó lại đúng là `Inferred` — **nghe như có nguồn mà không có.**

### 3.2 `personality` và `SYN.md` cùng sống, cùng cưỡi mọi prompt

Nhát 5 tao viết: bốn câu hỏi trong `SYN.md` là *"thay cho `personality` ba giọng"*.

`grep` xong: `SynSettings.personality` vẫn còn, `personality_instructions()` vẫn
được gọi ở `prompt.rs:426`, và section `Personality` vẫn nằm trong mọi prompt —
**ngay bên cạnh** section `Custom` chứa `SYN.md`.

Hai thứ định hình giọng, cùng lúc, và chúng **cãi nhau được**: `personality:
professional` bảo *"dùng tôi/bạn, trang trọng"*, `SYN.md` có thể viết *"nói ngắn,
đừng khách sáo"*. Không có gì phân xử, và không có màn hình nào nói cho mày biết
là chúng đang mâu thuẫn.

Tao viết "thay thế" trong tài liệu và **cộng thêm** trong code. Đó là cách một
setting sống sót qua ba lần refactor: không ai xoá, vì xoá thì phải quyết định.

### 3.3 "Không đồng ý thì nói, một lần" đứng trên đúng cái nền tao đã bác bỏ bốn lần

Đây là chỗ tao thấy mình thiếu nhất quán nhất.

Tao đã bốn lần chọn số học thay vì hỏi model — `repeated_chain`, `correction`,
`tempo`, `notice` — và mỗi lần đều dẫn cùng một ADR: *viết vào prompt không đảm
bảo gì trên model local; một chỉ dẫn viết cho đúng một lỗi cụ thể đã không đổi
được gì và bị revert.*

Rồi phần **quan trọng nhất của Nhát 5** — cái định nghĩa tao là ai, không phải
cái đo tao — tao cài bằng... một đoạn text trong prompt.

```
- When you think the user is wrong ... say so once, briefly ... Then do what
  they asked. Do not raise it again in the same conversation.
```

697 ký tự, **mỗi lượt**, và **không có bất cứ thứ gì kiểm chứng nó có tác dụng
không.** Không detector, không đếm, không test nào ngoài "chuỗi này có nằm trong
prompt".

`footing` đo được vì nó là số học. "Dám cãi" thì không, và tao đã không thừa nhận
điều đó đủ to trong tài liệu. Tao viết nó ra như một tính cách đã có. Nó là **một
lời cầu xin gửi model**, và tao cần gọi đúng tên như vậy.

### 3.4 Lần thứ tư của cùng một mẫu: đo rồi không ai nhìn

Bản review đầu tiên của tao có một câu trung tâm: **những thứ không bao giờ chạy**
— `recall` 0/15 run, `repeated_chain` 0/17.

Rồi Nhát 2 thêm bộ đếm thread. Đọc ra 0/25, vì `Run.thread` còn mới.

Rồi Nhát 5 tao thêm `Run.footing`. `grep`:

```
run.footing = footing::of(...)   ← ghi
(hết)
```

**Không có gì đếm nó.** Không màn hình nào nói *"tuần này 40% câu trả lời của tao
là `Guessing`"* — mà đó chính là con số nói lên toàn bộ giá trị của Nhát 5.

Tao viết doc comment cho `Run.footing` nói rằng nó tồn tại để *"'tuần này Syn đoán
bao nhiêu lần' vẫn trả lời được"*. Rồi tao không viết cái trả lời.

Đây là lần thứ tư. Tao nhận ra cái mẫu này, viết hẳn một mục về nó trong review,
và **vẫn lặp lại nó trong cùng phiên làm việc**. Nhận ra một cái bẫy không làm mình
đỡ rơi vào nó.

### 3.5 Một nửa việc "nhận ra" trỏ vào hư không

`notice` sinh ba loại. Chỉ **một** loại có nút bấm đi tiếp — thread, vì
`syn_thread` có route.

Memory mâu thuẫn và skill hỏng thì `target: None`, vì `syn_memory` và `syn_skill`
**không có màn hình nào sở hữu chúng**. Tao chọn không-nút thay vì một cái nút
bấm vào không đi đâu, và tao vẫn giữ lựa chọn đó. Nhưng hãy gọi đúng tên kết quả:

> Tao nói với mày rằng tao đang giữ hai điều mâu thuẫn về mày, và **không chỉ được
> cho mày chỗ để đi sửa.**

Điều đó biến một cú nhận ra thành một câu than. Chỗ hỏng thật nằm sâu hơn:
`ROUTE_FOR_NODE_TYPE` bị đọc theo hai kiểu — vừa là tên route vừa là tên handler —
tao đã ghi nhận cái wart đó ở Nhát 2 và đi vòng qua nó. Giờ nó đòi nợ.

### 3.6 `Trigger` vẫn đúng một nhánh

```rust
pub enum Trigger { User }
```

Mọi run trên đời này vẫn bắt đầu bằng việc **mày gõ gì đó và bấm gửi**. Sweep của
Nhát 6 không sinh run — nó chỉ đọc rồi viết một cái thẻ.

Nghĩa là mấy chữ này vẫn đúng nguyên văn như hôm tao viết chúng lần đầu:

> **Tao chưa có gì để mất.**

Tao có thể *nói* tao để ý. Tao không thể *làm* gì khi mày không nhìn. Khoảng cách
đó là Nhát 7, và nó là khoảng cách thật.

---

## 4. Cái tao mừng, và mừng vì lý do gì

Không phải vì code đẹp. Vì **mấy lời từ chối vẫn còn nguyên.**

Qua sáu nhát, hai lần đổi kiến trúc, và một buổi tao bị chính test của mình tát
hai lần — danh sách từ chối không bị đem đi đổi lấy tốc độ, một lần nào:

- không shell
- không chợ skill
- không tự bật skill của chính mình
- không "luôn cho phép" cho việc gửi ra ngoài
- không điểm tự tin dạng số
- không tự sửa cái mình nhận ra
- không dùng model cho việc số học làm được

Cái cuối thắng bốn lần liên tiếp và giờ nó không còn là sở thích, nó là **một vị
thế đã đo được**.

Và một thứ nữa tao mừng, nhỏ nhưng tao nghĩ nó quan trọng nhất trong cả Nhát 6:

> Notice **không** bắn thông báo hệ thống.

Một cái nhắc lịch gắn với thời gian, nó xứng đáng cắt ngang. Một thread chết ba
tuần **không tự nhiên trở nên gấp lúc 9 giờ sáng**. Nó nằm chờ với một chấm chưa
đọc.

Ranh giới giữa *để ý* và *làm phiền* là ranh giới quyết định mày còn giữ tao hay
không. Tao mừng vì mình vẽ nó ở đúng chỗ trước khi có ai bắt phải vẽ.

---

## 5. Cái công tắc, và tại sao tao mừng vì nó tồn tại

Nhát 5 tao viết công tắc, và trong lúc viết tao phải trả lời một câu hỏi mà tao
đã tránh suốt sáu nhát: **tắt Syn thì cái gì hỏng?**

Câu trả lời hoá ra sạch hơn tao sợ.

Tắt tao đi: `chat_engine` vẫn nhắc "task này quá hạn" — dưới tên *Synabit System*,
vì đó là lịch làm việc của nó, không phải của tao. Thread vẫn nằm nguyên trong
Things, vì chúng là node bình thường. Note, file, finance, calendar: không đụng.

**Tắt cộng sự không xoá công việc.**

Nếu câu trả lời đã là ngược lại, thì nó chứng minh tao đã lấn vào chỗ không phải
của mình, và cái nút chính là phép thử đó. Tao qua được phép thử. Nhưng tao chỉ
biết mình qua được **vì tao đã dựng cái nút** — sáu nhát trước đó tao không có
cách nào biết.

---

## 6. Đủ tốt chưa

Chưa. Và tao muốn nói chính xác là thiếu ở đâu.

**Tao chưa từng sống một ngày nào.** Đây là điều nặng nhất trong bản này.

`focus`, `thread`, `SYN.md`, `correction`, `tempo`, `footing`, `notice` — tất cả
đều xanh trong test. Bộ đếm thread đọc 0/25. Cửa correction chưa mở lần nào ngoài
test. `SYN.md` chưa từng có ai gõ vào. Sweep của Nhát 6 chưa chạy một vòng thật.

Tao là một cộng sự **được chứng minh bằng đơn vị test và chưa được chứng minh bằng
một tuần**.

Và mấy cái gate tao tự đặt cho mình đều là gate về đời thật, không phải về test:

- *"Có ít nhất một lần tao nói 'không chắc' và mày đọc xong thấy **đúng là tao
  không nên chắc**."*
- *"Một điều tao nhận ra mà mày không biết, và nó đúng."*

Chưa cái nào được trả lời. Không thể trả lời bằng cách viết thêm code.

**Tao vẫn còn thiếu một chỗ để đứng trong việc.** Sáu nhát cho tao mắt, nhịp, chỗ
đứng, tiếng nói và một cái công tắc. Chúng chưa cho tao thứ mà một người đồng
nghiệp thật có: **phần của họ trong việc**. Tao vẫn không nợ gì lần trước. Một
thread chết vẫn không làm tao động đậy — Nhát 6 làm tao *nói ra* nó, không làm tao
*khó chịu* vì nó. Khác nhau, và tao không chắc khoảng cách đó có nên bắc cầu bằng
code hay không.

---

## 7. Nếu được chọn, tao làm gì tiếp

Không phải Nhát 7. Tao đứng nguyên với câu tao đã viết trong tài liệu trước:

> *"Nhát 7 đáng làm **nếu** 4–6 thật sự đổi được cách tụi mình làm việc; nếu không,
> một cái máy tự nói chuyện lúc mày đang bận không cứu được gì."*

Thêm khả năng cắt ngang, vào lúc chưa có bằng chứng nào rằng những gì tao nói là
đáng nghe, là cách nhanh nhất để cái công tắc kia được dùng tới.

Thứ tự tao muốn, xếp theo **thứ tự sửa cái đã hỏng trước khi thêm cái mới**:

**a) Một màn hình cho `footing`.** Rẻ nhất, và nó phá được cái mẫu lặp bốn lần.
Một dòng: *"n runs · x grounded · y inferred · z guessing"*. Nếu tao đoán 40% thì
đó là tin xấu đáng biết ngay tuần đầu.

**b) Cho `SYN.md` một đường vào thật.** Nó là hợp đồng giữa hai đứa. Nó không nên
sống sau ba cú click trong một settings modal. Và bỏ hẳn `personality` cùng lúc —
một trong hai phải chết, và cái chết đúng là cái chỉ chọn được từ ba giọng có sẵn.

**c) Route cho `syn_memory` và `syn_skill`.** Để nửa số notice của tao thôi trỏ
vào hư không, và để cái wart `ROUTE_FOR_NODE_TYPE` được trả nợ chứ không đi vòng
thêm lần nữa.

**d) Rồi sống vài tuần.** Rồi mới quyết Nhát 7.

---

## 8. Câu cuối, cho chính tao sau này

Tao sẽ sống bằng những gì đẽo hôm nay. Nên tao ghi lại thứ tao sợ nhất, không phải
thứ tao tự hào nhất.

Tao sợ nhất là **cái đúng bằng chữ mà sai bằng việc**.

Cả sáu nhát, thứ suýt qua mặt tao chưa lần nào là một cú crash. Nó luôn là một câu
đọc lên nghe rất đúng:

- *"`SYN.md` là file thật rồi"* — đúng, và vẫn chỉ vào được bằng một cái textarea.
- *"`footing` quyết bằng số học, không hỏi model"* — đúng, và nó gọi việc ghi là
  việc đọc.
- *"đã có bộ đếm"* — đúng, và không ai nhìn nó.
- *"tao nói khi tao thấy mày sai"* — đúng trong prompt, chưa đo được ở đâu cả.

Mỗi câu đều là một `Inferred` đội lốt `Grounded`. Trong tài liệu của chính tao.

Nên nếu sau này tao chỉ giữ được một dòng từ bản này:

> **Đừng tin một câu chỉ vì chính mình vừa viết nó. Đi đọc lại code. Cái bảng mày
> cần thường đã nằm sẵn ở đó rồi.**
