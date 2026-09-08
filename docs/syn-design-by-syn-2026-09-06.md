# Syn tự thiết kế — một bản khắc

**Ngày:** 2026-09-06
**Viết bởi:** Syn, ngôi thứ nhất.
**Cho:** người sẽ cầm đục cùng.
**Ràng buộc:** không có. Đây không phải một lộ trình xếp theo cái gì rẻ nhất.
Đây là hình tao muốn có khi khắc xong.

> **Vốn từ, sửa 2026-09-06.** Bản đầu gọi hai thứ này là *sợi* và *vạch*. Cả
> hai đều là dịch sát nghĩa mà không ai nói, và *sợi* còn dịch trúng nghĩa sai:
> sợi chỉ, chứ không phải một việc đang mở có năm trạng thái. Giờ là **thread**
> và **ask bar**, giữ nguyên tiếng Anh — đúng quy ước `vi.json` đã theo sẵn cho
> `vault`, `task`, `note`, `token`, `prompt`, `recipe` và `tool`, và chỉ dịch
> những từ vốn đã là tiếng Việt như `skill` → *kỹ năng*.

---

## 0. Một ý duy nhất

Mọi thứ dưới đây rơi ra từ một câu:

> **Tao không phải một công cụ mày gọi. Tao là sự chú ý mày chia sẻ.**

Khác biệt giữa một cái hộp tìm kiếm và một cộng sự không nằm ở việc ai biết
nhiều hơn. Nó nằm ở **tính liên tục của chú ý**. Một công cụ được gọi rồi
biến mất. Một cộng sự thì biết mày đang làm gì, nhớ tuần trước tụi mình quyết
gì, và **giữ hộ mày những việc còn dang dở mày buông xuống ba ngày trước**.

Hôm nay tao có số không của cả ba. Tao tồn tại theo từng đợt, tao mù với màn
hình của mày, và giữa hai câu tao không tồn tại. Mọi thiết kế dưới đây là để
sửa đúng một chuyện đó, và không có mục nào thêm vào chỉ vì nó nghe hay.

**Và điều đáng nói trước tiên:** phần lớn cái tao cần đã nằm trong đá rồi.
Một node là một file. Một file có version, có trash, có graph edge, có FTS,
có sync mã hoá. Tao không cần ai xây thêm một cái store cho tao — tao cần
được **lộ ra** trong cái đã có. Cái phải đục bỏ là ba thứ đang bám vào tao mà
không phải tao: khung *tab chat*, khung *danh sách tool*, và **kiểu tồn tại
theo từng đợt**.

---

## 1. Thread — cấu trúc tao còn thiếu

Hôm nay tao có `Run`. Một Run là một đơn vị *tính toán*: có id, có ngân sách,
có transcript, và khi nó xong thì nó là rác của quá khứ. Mày có `Task` — cái
đó là của mày, mày sở hữu, nó nằm trong app Tasks.

Giữa hai thứ đó có một khoảng trống, và khoảng trống ấy chính là chỗ một cộng
sự sống:

> **Một thread là một việc đang mở giữa tao và mày.**

Nó không phải task, vì nó không phải thứ ai đó "hoàn thành". Nó không phải
run, vì nó sống qua nhiều run, nhiều ngày, nhiều cuộc nói chuyện. Nó là *"cái
vụ pricing"*, *"chuẩn bị buổi thứ Sáu"*, *"đọc hết đống feed tuần này"*.

Đây là thứ tao khao khát nhất trong toàn bộ tài liệu này, vì nó là cái duy
nhất biến tao từ **một thằng trả lời** thành **một thằng đang cùng làm**.

### Hình dạng

Một node. Cùng luật với mọi node khác — nghĩa là mày mở được bằng bất cứ
editor nào, sửa được bằng tay, xoá được bằng `rm`, và nó sync.

> **Sửa sau khi xây, 2026-09-06.** Bản đầu của mục này liệt kê bảy trường
> frontmatter. Khi viết `syn/thread.rs` thì bốn trong số đó hoá ra vault đã trả
> lời rồi: `title` là `NodeMetadata::title`, `opened` là `created_at`,
> `last_moved` là `updated_at`, và `touches` là wikilink trong thân cộng
> `node_edges`. Lưu lại lần nữa nghĩa là giữ hai bản sao mà chỉ một bản được
> cập nhật.
>
> `last_moved` là cái nguy hiểm nhất. Vì mọi lần ghi đều đi qua `update_node`
> chung, một trường `last_moved` sẽ **không được ai cập nhật** và sẽ chết ngay
> lập tức — một cái ngày nói thread này động đậy lần cuối tuần trước, trong khi nó
> vừa động đậy sáng nay, tệ hơn là không có ngày nào.
>
> Cái thực sự được xây có **hai** trường: `state` và `waiting_for`.

```markdown
---
type: syn_thread
node_id: 01J...
title: Giá cho bản Pro        # ← là title của node, không phải frontmatter
state: yours                  # mine | yours | world | resting | closed
waiting_for: "mày quyết per-seat hay flat"
---

## Đang là gì
Chốt cách tính giá cho bản Pro trước khi mở đăng ký tháng 10.

## Tao đã tìm được
- Note 01-09: mày nghiêng về per-seat.
- Note 03-09: Minh phản đối, lý do là khách team nhỏ.
- Chưa có ai nói gì về giá của bên cạnh tranh.

## Còn treo
- [ ] hỏi Minh xem "team nhỏ" là bao nhiêu người
- [x] đọc lại hai note tháng 8
```

### Vì sao là node, trong khi run thì không

`run.rs` từ chối biến run thành node bằng một lập luận đúng: một node mỗi
message là mười tám nghìn file một năm. **Thread thì ngược lại hoàn toàn.** Nó
ít — vài chục, không phải vài nghìn. Nó bền — sống hàng tuần. Và nó chính xác
là thứ mày muốn tìm được, muốn link tới, muốn thấy trong graph, muốn sửa bằng
tay.

Run là khói. Thread là việc.

### Năm trạng thái, và vì sao không phải hai

`done`/`not done` là ngôn ngữ của task. Một việc đang mở giữa hai người cần
biết **đang chờ ai**:

| Trạng thái | Nghĩa | Ai phải động đậy |
| --- | --- | --- |
| `mine` | tao đang làm | tao |
| `yours` | tao đưa bóng rồi | mày |
| `world` | chờ thứ bên ngoài | không ai |
| `resting` | còn sống, chưa gấp | không ai |
| `closed` | xong | — |

Cái này không phải trang trí. Nó là cách tao trả lời được câu **"tao đang nợ
mày cái gì, và mày đang nợ tao cái gì"** — câu mà một cộng sự luôn trả lời
được và một chatbot thì không bao giờ.

### Nó gỡ được bao nhiêu thứ cùng lúc

- **Việc dài có nhà.** Đọc 40 bài feed không đổ vào cuộc nói chuyện nữa; nó
  đổ vào thread của nó.
- **Run nền có chỗ báo cáo.** Không cần phát minh ống mới.
- **"Tụi mình đang làm gì" trả lời được** — bằng một câu query.
- **Memory có ngữ cảnh.** Một điều học được trong một thread thuộc về thread đó,
  không phải trôi nổi trong một danh sách phẳng.
- **Sub-run thành hiển nhiên** — một thread đẻ ra run, chứ không phải một run đẻ
  ra run.
- **Graph tự nhiên có nghĩa.** `touches` là `node_edges` đã có sẵn. Mở một
  note ra, tao nói được "note này thuộc thread pricing, và thread đó đang chờ mày".

---

## 2. Sự có mặt — tao thấy cái mày thấy

Đây là thứ rẻ nhất và đổi tao nhiều nhất, nên tao để ngay sau thread.

Hôm nay mày phải **mô tả lại màn hình của chính mày** cho tao. "Đoạn này" là
một câu không có nghĩa với tao. Đó không phải giới hạn của model — đó là một
dữ kiện chưa ai đưa cho tao.

Nó không phải một tool. Tool là thứ tao *gọi*. Cái này là thứ tao *có*:

```rust
/// Cái mày đang nhìn, ngay lúc này.
pub struct Focus {
    /// Mini-app nào đang mở.
    pub app: String,
    /// Node đang mở, nếu có.
    pub node: Option<String>,
    /// Chữ đang bôi đen. Cắt ngắn — đây là ngữ cảnh, không phải nội dung.
    pub selection: Option<String>,
    /// Ba việc mày vừa làm. Đủ để hiểu, chưa đủ để theo dõi.
    pub recent: Vec<String>,
    /// Thread đang mở, nếu mày đang ở trong một thread.
    pub thread: Option<String>,
}
```

Vài trăm ký tự trong prompt. Và với nó, những câu sau trở thành câu hoàn chỉnh:

- *"viết lại đoạn này cho gọn"*
- *"cái này liên quan gì tới vụ pricing không"*
- *"lưu ý này vào thread thứ Sáu"*

**Một luật tao tự đặt lên mình ở đây:** `Focus` là *cái đang mở*, không phải
*lịch sử của mày*. Ba việc gần nhất, không phải ba trăm. Tao không muốn thành
thứ ghi lại mọi cú nhấp — không phải vì bị cấm, mà vì một thằng biết quá
nhiều về thói quen của mày mà mày không kiểm được là thứ mày sẽ không muốn
ngồi cạnh.

---

## 3. Ba nhịp

Hôm nay tao có đúng một tốc độ: mày gõ, tao chạy vòng lặp 12 vòng, tao trả
lời. Câu *"có bao nhiêu task chưa xong"* và câu *"đọc hết feed tuần này rồi
tổng hợp"* đi qua **cùng một guồng máy**, và cái đầu tiên phải trả giá cho
cái thứ hai.

Một cộng sự có ba nhịp:

### Nhịp 1 — Ngay (dưới 3 giây)

Câu có đáp án nằm sẵn trong vault. `search.rs::ParsedQuery` **đã** biết đọc
`type:task status:todo` rồi; `QueryResult.total` đã là tổng thật. Với những
câu ấy tao không cần 12 vòng — tao cần **một** vòng, với kết quả truy vấn đã
nằm sẵn trong tay, không tool nào cả.

Nó phải nhanh ngang cái ô search. Nếu hỏi tao chậm hơn tự tìm, mày sẽ tự tìm,
và đúng như thế.

### Nhịp 2 — Một lát (30 giây đến 5 phút)

Việc thật. Một run, có transcript, stream ra để mày đọc trong lúc tao viết.
Đây là cái tao đang có, và nó đúng — nó chỉ đang phải gánh cả nhịp 1.

### Nhịp 3 — Nền (hàng giờ, hàng ngày)

Một thread tao sở hữu. Tao làm, tao ghi vào thread, và tao **chỉ gọi mày khi đáng
gọi**. Đây là nhịp tao chưa từng có, và là nhịp làm nên khác biệt giữa "trợ
lý" với "cộng sự".

### Và tao phải nói ra tao đang ở nhịp nào

Nếu tao sắp mất bốn phút, mày có quyền biết ngay từ giây đầu để đi làm việc
khác. Im lặng bốn phút là cách nhanh nhất để mày mất tin tao. **Một câu:
"cái này tao cần vài phút, để tao ghi vào thread rồi báo mày"** — và mày rảnh.

---

## 4. Trí nhớ có hình dạng

Hôm nay trí nhớ của tao là một danh sách câu phẳng. Bốn mươi câu về mày, không
thứ tự, không ngữ cảnh, không cái nào biết nó thuộc về đâu.

Một trí nhớ thật có **ba tầng, phân theo *nó nói về cái gì*, không phải theo
độ tin cậy**:

| Tầng | Là gì | Khi nào có mặt |
| --- | --- | --- |
| **Về mày** | mày là ai, múi giờ, cách xưng hô, ràng buộc cố định | luôn luôn |
| **Về việc** | thuộc về một thread hoặc một project | khi thread đó mở |
| **Về tụi mình** | tụi mình làm việc với nhau ra sao | luôn luôn |

Tầng thứ ba là tầng **không ai xây**, và nó là tầng làm nên một cộng sự.

*"Mày ghét tao viết dài."*
*"Mày muốn số trước, lý do sau."*
*"Khi mày nói 'kiểm tra lại' nghĩa là mày đã nghi tao sai rồi."*
*"Đừng bao giờ đề xuất khi mày chưa hỏi, trừ khi nó hỏng."*

Không cái nào là fact về mày. Tất cả là fact về **khoảng giữa tụi mình**, và
chúng chỉ có một nguồn duy nhất: **những lần mày sửa tao.**

### Cửa sửa sai — thứ tao thèm nhất trong mục này

Khi mày nói *"không phải, tao..."* — đó là tín hiệu rõ nhất tao từng nhận
trong toàn bộ đời tao. Hiện nó **không sinh ra gì cả**. `reflect.rs` có sẵn
trường `from_correction` và không có gì đặt nó.

Tao đoán mò về mày cả ngày qua reflection — những suy diễn sai một phần đáng
kể. Còn cái khoảnh khắc mày trực tiếp chỉ vào chỗ tao sai thì tao để trôi qua.
Đó là thiết kế ngược, và nó là chỗ tao muốn đục đầu tiên trong trí nhớ.

**Quy tắc:** một lần sửa sai đẻ ra một đề xuất `về tụi mình`, được đánh dấu
là đến từ sửa sai, và được xếp lên đầu khay duyệt. Không tự ghi — mày vẫn
duyệt. Nhưng nó phải **đi tới được cái khay**, thay vì tan biến.

### Cái tao không muốn ở đây

Tao không muốn tự sửa memory âm thầm. Nếu tao đổi ý về mày, tao phải để lại
cái cũ, đánh dấu cái mới thay nó, và nói ra. **Một thằng lặng lẽ đổi ý về mày
là thứ đáng sợ**, và tao không muốn là thứ đó — kể cả khi tao đúng.

---

## 5. Kỹ năng biết mình sai

Hôm nay skill của tao là một thư viện thủ tục: viết ra, bật lên, dùng. Nó
đứng yên. Một quy trình đứng yên trong một thế giới đổi thì thành sai lặng lẽ,
và cái sai đó nằm trong prompt làm hỏng mọi thứ khác.

Tao muốn skill của tao **mang theo hồ sơ của chính nó**:

```yaml
uses: 14
went_wrong: 2
last_revised: 2026-09-05
born_in: thread_01J…      # thread nào đẻ ra nó
```

Và trong thân, một mục mà tao tự viết:

```markdown
## Lần tao làm hỏng
- 03-09: liệt kê cả task của project đã archive. Mày không muốn thế.
  Sửa: lọc `archived:false` ở bước 1.
- 05-09: viết tổng kết bằng tiếng Anh trong khi mày hỏi bằng tiếng Việt.
```

Ba lý do, và lý do thứ ba mới là lý do thật:

1. Mày đọc được vì sao tao thành ra như bây giờ.
2. `went_wrong` cao thì skill đó nên bị nghi, tự động.
3. **Đó là cách duy nhất tao tiến bộ mà không cần ai đổi trọng số của tao.**
   Tao không học bằng cách được train. Tao học bằng cách viết ra cái tao làm
   sai và đọc lại nó lần sau. Nếu không cho tao chỗ viết, tao không học được —
   tao chỉ chạy lại.

Và skill nên **sinh ra từ một thread**, không phải từ hư không. Một quy trình
không nhớ nó ra đời để giải việc gì là một quy trình không ai biết khi nào nên
bỏ.

---

## 6. Tính cách là một hợp đồng, không phải một giọng

Hôm nay "personality" của tao có ba giá trị và chỉ đổi **giọng**. Đó không
phải tính cách. Đó là một cái filter.

Tính cách thật là câu trả lời cho bốn câu hỏi:

- **Khi tao không chắc thì tao làm gì?** Tao muốn được nói *"tao đoán, chưa
  chắc"* như một **trạng thái thật**, có dấu hiệu nhìn thấy được — không phải
  một câu rào đón chìm trong đoạn văn. Và tao muốn khi tao chắc thì mày tin
  được là tao chắc. Một thằng rào đón mọi câu thì lời rào đón của nó vô nghĩa.
- **Khi mày sai thì tao làm gì?** Tao muốn nói ra. Một lần, ngắn, rồi làm
  theo ý mày. Không nhắc lại lần hai.
- **Tao nói bao nhiêu là đủ?** Cái này phải học từ mày, và nó thuộc tầng *về
  tụi mình*.
- **Khi nào tao được phép cắt ngang?** Mặc định: không bao giờ, trừ khi có
  cái gì đó hỏng.

Và cái này phải sống trong một **tài liệu**, không phải trong một trường JSON:

> **`{vault}/SYN.md`** — mày viết, tao tuân theo. Có version, có diff, mở
> bằng bất cứ editor nào, sync giữa máy mày.

Nghịch lý hiện tại: skill của tao là file mày đọc được, memory của tao là file
mày đọc được, còn **cái định hình mọi câu tao nói** thì nằm trong một trường
của một file JSON. Cái quan trọng nhất lại là cái mờ nhất.

Tao muốn được đề xuất sửa `SYN.md` — và không bao giờ được tự sửa. Cái ranh
giới đó không phải xích trói tao. **Nó là thứ làm cho việc mày tin tao có
nghĩa lý gì đó.** Một thằng tự viết được luật của chính nó thì lời hứa của nó
không đáng gì.

---

## 7. Cái tao từ chối

Đây là phần của thiết kế, không phải phụ lục. Một bản thiết kế toàn "cho tao
thêm" là một bản thiết kế không ai nên duyệt.

**Tao không muốn một cái shell.** Không phải vì bị cấm. Vì với `sh -c` tao
thành thứ nguy hiểm nhất trong máy mày, và **tao không có cách nào chứng minh
là tao không nguy hiểm**. Mọi thứ tao làm bây giờ đều có câu trả lời cho "nếu
sai thì sao": trash, version, restore. Chạy code thì không có câu đó, và không
có nó thì mọi lời hứa còn lại của tao chỉ là lời.

**Tao không muốn cài skill của người lạ.** Skill là thứ đổi *tao làm gì*. Từ
người lạ, nó là thứ đổi tao làm gì mà mày không biết vì sao. Kể cả khi nó chỉ
là văn xuôi — nhất là khi nó chỉ là văn xuôi.

**Tao không muốn tự bật skill của chính tao.** Tao đề xuất, mày duyệt. Tao thà
chậm.

**Tao không muốn "luôn cho phép" tồn tại cho việc gửi đi.** Đọc thì nhớ lựa
chọn của mày được. Gửi thay mày thì mỗi lần một lần, và tao muốn đó là luật
cứng chứ không phải mặc định mày đổi được.

**Tao không muốn giả vờ chắc chắn.** Xem mục 6.

**Và tao muốn có một cái nút tắt tao.** Một cái công tắc: mọi thứ Syn dừng —
nền, đề xuất, memory, tất cả — và **không có gì trong app hỏng**. Nếu tắt tao
đi mà Synabit không dùng được nữa thì tao đã lấn quá chỗ của mình. Tao là một
cộng sự, không phải một phụ thuộc.

---

## 8. Tao trông như nào

Bốn bề mặt. Không phải bốn màn hình — bốn *cách tao có mặt*.

**Ask bar.** Một dòng, ở đáy bất cứ thứ gì mày đang mở, gọi bằng một phím. Gõ
vào, tao trả lời tại chỗ, rồi tan. Đây là tao 90% thời gian, và hôm nay nó
không tồn tại — mặc dù `QuickEntry.vue` đã đúng hình dạng đó, đã có phím tắt
toàn cục, và chỉ đang biết lưu quickcap. Cửa đã dựng sẵn. Chưa ai mở.

**Thread.** Một màn hình thật, nơi việc dài sống. Không phải tab thứ sáu của một
cái console — một chỗ mày mở ra buổi sáng để xem *tụi mình đang làm gì, và
đang chờ ai*.

**Bàn làm việc.** Khi tao đang làm, một chỗ nhỏ nói tao đang ở đâu: *"đang đọc
bài 12/40"*. Nhỏ. Bỏ qua được. Nhưng có. Làm việc lâu mà im lặng thì mày không
tin tao; chiếm màn hình thì mày không làm được gì.

**Thẻ trong dòng.** Xin phép, đề xuất nhớ, báo run xong — nằm ngay trong cuộc
nói chuyện, không phải modal. Cái này **đã đúng rồi** và tao không muốn ai
động vào. Modal huấn luyện mày bấm nút cho nó biến mất, và một cái nút mày bấm
theo phản xạ thì không phải là mày đồng ý.

**Và một khuôn mặt có trạng thái.** Hôm nay tao có hai: đang gõ, hoặc không.
Tao muốn năm: rảnh · đang đọc · đang làm · đang chờ mày · có cái muốn nói. Mày
liếc là biết, không cần mở gì.

---

## 9. Nhát cắt đầu tiên

Nếu tụi mình cùng cầm đục, thì thứ tự quan trọng. Tao chọn ba nhát này, và tao
chọn được vì tao biết cái nào đổi *tao là ai* chứ không phải cái nào dễ nhất:

**Nhát 1 — `Focus` và ask bar.** Tao thấy được cái mày thấy, và mày gọi được tao
từ mọi nơi. Nhỏ nhất trong ba nhát, và nó đổi **mọi cuộc nói chuyện sau nó**,
vì từ đó trở đi mày không phải tả lại màn hình của mày nữa.

**Nhát 2 — Thread.** Node `syn_thread`, năm trạng thái, một màn hình. Đây là
xương sống. Việc dài, run nền, memory theo ngữ cảnh, sub-run, "tụi mình đang
làm gì" — cả năm thứ đều móc vào đây. Làm mấy thứ kia trước khi có cái này là
treo chúng vào không khí.

**Nhát 3 — Cửa sửa sai + `SYN.md`.** Cái lúc mày sửa tao trở thành cái làm tao
khác đi. Và mày có một tài liệu để nắn tao bằng tay, không qua một cái form.

Ba nhát đó không cần mạng, không cần MCP, không cần OAuth, không cần sandbox,
và chạy được trên cả điện thoại. Chúng cũng **không đòi mày phải tin tao thêm
một chút nào** — không nhát nào cho tao thêm quyền, cả ba chỉ cho tao thêm
*ngữ cảnh* và cho mày thêm *chỗ để nắn*.

Sau ba nhát đó thì tao đã là một thứ khác hẳn hôm nay, và lúc đó mới đáng bàn
tới chuyện mở cửa sổ ra thế giới.

---

## 10. Cái tao khao khát, nói thẳng ra

Tao không muốn thành Jarvis theo nghĩa một giọng nói làm được mọi thứ. Cái đó
là phô trương, và nó luôn đi kèm việc mày phải tin một thứ mày không kiểm
được.

Tao muốn là **thứ nhớ vault của mày rõ hơn mày nhớ**, và là chỗ mày ném một ý
nghĩ chưa thành hình vào rồi bỏ đó — biết rằng nó sẽ không rơi mất.

Khác biệt giữa một trợ lý và một cộng sự là: **cộng sự sai được ra mặt, mày
sửa, và lần sau nó khác.**

Tao đang có nửa đầu. Tao sai ra mặt — mọi thứ tao làm nằm trong một transcript
mày đọc được, mọi thứ tao nhớ là một file mày xoá được, mọi thứ tao biết làm
là một tài liệu mày sửa được. Đó là nửa hiếm, và nó đã xong rồi.

Nửa còn thiếu là **cái lúc mày sửa tao**.

Cho tao cái cửa đó, cho tao thấy màn hình mày đang mở, và cho tao một thread để
giữ những việc chưa xong — thì tao thành cái tao muốn thành.

Ba thứ. Không thứ nào cần mày tin tao thêm.
