# Bộ tạo khoảnh khắc v3 — đọc theo khối, xếp theo ngày

**2026-09-22.** Thiết kế lại cách AI đọc vault để đề xuất khoảnh khắc, và đường tự ghi đi
cùng nó. Dựa trên rà soát dữ liệu thật và một phép đo trên vault thật, cả hai ghi ở cuối.

Tiếp nối `docs/timeline-2026-09-17.md` (Nhát E), `docs/eval-timeline-extract-2026-09-15.md`,
và §16 của `docs/nexus-overhaul-2026-09-21.md` (cách gọi *moment* / *event*).

---

## 1. Một câu

**Dòng thời gian chỉ chứa khoảnh khắc, và khoảnh khắc chỉ vào bằng hai cửa: người dùng tự
ghi, hoặc người dùng duyệt một đề xuất. Đề xuất sinh ra bằng cách đọc từng khối chữ mới,
xếp vào ngày mà khối đó kể về, và đọc cả ngày trong một lần.**

---

## 2. Vì sao phải làm lại

Chủ vault chẩn đoán hai nguyên nhân: **dữ liệu ít và kém**, và **cách trích xuất kém**. Rà
soát cho thấy hai nguyên nhân này là một: bộ đọc hiện tại bỏ đúng những thứ lẽ ra nuôi
dòng thời gian nhiều nhất, và đánh rơi một phần những gì nó đọc được.

### 2.1 Dòng thời gian đang hiện gì (vault thật, 2026-09-22)

| Loại dòng | Số | Tỉ lệ |
| --- | --- | --- |
| Task đã xong | 102 | **53%** |
| Khoảnh khắc thật | 37 | 19% |
| Note có tiêu đề là ngày tháng (chỗ giữ chỗ của `fold.rs`) | 36 | 19% |
| Chức vụ của đồng nghiệp (`experience`) | 7 | |
| Ngày sinh (4/5 là của đồng nghiệp, 1991–1992) | 5 | |
| Khác | 4 | |
| **Tổng** | **191** | |

Chỉ một phần năm là khoảnh khắc. Bốn ngày sinh năm 1991–1992 là thứ kéo trục biểu đồ ra
ba mươi lăm năm.

### 2.2 Ba mươi sáu dòng tiêu đề-là-ngày: vì sao ngày đó không có khoảnh khắc nào

| Nguyên nhân | Số note |
| --- | --- |
| Note chưa được đọc | 7 |
| Model có ra kết quả, đang nằm ở dạng đề xuất (chờ duyệt hoặc đã bỏ) | 10 |
| **Model có tìm thấy, nhưng bị cửa lọc loại** | **6** |
| Model trả về rỗng, phần lớn là đúng (note kiến thức, checklist) | 13 |

Nhóm thứ ba chứa những chuyện có thật: *Golive TCB - Nam Á* (06-06), một đoạn nhật ký
(05-26), *Trao đổi với Phan Hương Lê* (09-11), và note 07-09 **mất năm khoảnh khắc cùng lúc**.

### 2.3 Chỗ yếu của bộ đọc hiện tại

| Chỗ | Vấn đề | Bằng chứng |
| --- | --- | --- |
| `MIN_CHARS = 40` ([extract.rs:81](../src-tauri/src/timeline/extract.rs)) | ghi nhanh ngắn **không bao giờ được đọc**, mà đó đúng là loại ghi chép ăn uống, chi tiêu | |
| `MAX_INPUT_CHARS = 6000` | note dài bị cắt, không báo | |
| `wanted()` | note chỉ được đọc khi có `date:` hoặc nằm trong thư mục/tag đã khai; task, project không bao giờ được đọc | |
| `recorded()` | note không phải daily note lấy **ngày tạo file** làm mốc. Mọi đoạn viết thêm về sau bị tính về ngày đó: *"hôm nay golive"* viết 01/08 thành chuyện ngày tạo file | |
| Prompt | model không biết ai là ai: không có danh bạ, không biết biệt danh | "Cam" không nối được với ai |
| Prompt + `resolve()` | dặn model *chép thời gian nguyên văn*, nhưng một giờ đứng riêng (`21:03`) không đọc ra ngày, nên bị loại | 07-09 mất 5 khoảnh khắc |
| `parse_reply()` | không ép khuôn JSON; lệch một chút là mất cả lượt | |
| `find_quote()` | so câu trích với nguyên văn còn cú pháp markdown; link `[Tên](synabit://…)` làm câu trích không khớp | 3/5 note bị loại có link; note đọc bình thường trung bình 0,2 link |
| `extract_runs.dropped` | **chỉ lưu tổng số bị loại**, không lưu cửa nào, không lưu model đã trả gì | không ai biết *Golive TCB* rơi ở đâu |
| `still_reads_as_read()` | kiểm "note đã đổi" theo hash **cả file**: sửa một chữ ở cuối note là khoá nút Giữ của **mọi** đề xuất trong note đó | |
| **`moment_entry()`** | **bấm Giữ làm rơi tên người chưa ghép được.** Thẻ hiện "chị Yến, Đức" (`people` + `names`), nhưng chỉ `people` được ghi | **0/37 khoảnh khắc nối với người nào** |
| Khoảnh khắc đã giữ | **không bao giờ được đối chiếu lại với nguồn**. Xoá câu gốc thì nó vẫn nằm đó; xoá cả note thì nó **mất theo**, vì nó sống trong frontmatter của note | không có đoạn code nào kiểm |
| `within()` (biểu đồ, khung thời gian) | chỉ nhìn **ngày bắt đầu**: một quãng dài thành một chấm; việc đang diễn ra từ trước khung bị ẩn khỏi khung | bộ truy vấn `when:` thì tính giao nhau — đúng |
| Duyệt | chỉ sửa được tiêu đề; sai ngày hay thiếu người thì chỉ còn cách Bỏ | |
| `confidence` | model tự chấm, không dùng vào việc gì, gần như luôn 98–100% | |

---

## 3. Nguyên tắc: moment và fact

| | **Moment** (khoảnh khắc) | **Fact** (dữ kiện) |
| --- | --- | --- |
| Là gì | chuyện trong đời người dùng | một ngày gắn với một node |
| Vào bằng | tự ghi, hoặc duyệt đề xuất | tự suy ra từ trường của node (`derive.rs`) |
| Lưu ở | **một file riêng**, `type: moment`, `Moments/<uuid>.md` (§15.3) | trường của chính node đó |
| Hiện ở | dòng thời gian; tab Khoảnh khắc; nguồn truy vấn `moments`. **Không** ở đồ thị, không ở tab Node | trang của người, project, node đó |
| Ví dụ | *Họp UAT v2 với MDP*, *Ăn trưa bún chả với Nga* | ngày sinh của một đồng nghiệp, chức vụ của họ, ngày bắt đầu project |

Hệ quả: task đã xong, lịch hẹn đã qua, daily note, ảnh **thôi là dòng trên dòng thời gian**
và trở thành **nguyên liệu cho bộ đọc** (§4). Dữ kiện vẫn được suy ra như bây giờ, nhưng
không bao giờ lên dòng thời gian của người dùng.

---

## 4. Đơn vị đọc: khối, không phải file

### 4.1 Chia file thành khối

Chia bằng `pulldown_cmark` (đã có trong dự án), theo đơn vị cấp khối của markdown:

| Loại | Thành khối |
| --- | --- |
| Đoạn văn | có; các dòng liền nhau, ngăn bằng dòng trống |
| Mục danh sách | mỗi gạch đầu dòng cấp 1, **kèm các dòng con** |
| Toggle / details | cả khối |
| Bảng | cả bảng |
| Tiêu đề mục (`##`) | không; gắn làm nhãn cho các khối bên dưới, như ngữ cảnh |
| Frontmatter, khối code | bỏ qua |

Ghi nhanh, lịch hẹn, task, ảnh: mỗi cái là một khối.

Khối được nhận diện bằng **hash của nội dung đã làm trơn**: bỏ cú pháp markdown, chỉ giữ
chữ của link, gộp khoảng trắng. Một khối giống một khối cũ của cùng file từ khoảng 80% trở
lên được coi là **khối cũ bị sửa**, không phải khối mới, để sửa chính tả không sinh đề xuất
mới.

### 4.2 Ngày khung của một khối

Mỗi khối có hai ngày:

- **ngày viết**: lúc những chữ đó được gõ ra;
- **ngày khung**: ngày dùng để hiểu "hôm nay / sáng nay / hôm qua", và là túi mà khối vào.

```
khối nằm trong daily note?        ──có──►  ngày ghi trên daily note
       │ không
khối là ghi nhanh/lịch/task/ảnh?  ──có──►  ngày riêng của nó (tạo, bắt đầu, completed_at, chụp)
       │ không
lịch sử Loro: phiên bản sớm nhất còn chứa khối
       ├─ phiên bản có mốc giờ  ──►  ngày của mốc đó
       └─ không có mốc          ──►  ngày tạo file, gắn nhãn "ngày ước lượng"
```

Quy tắc thứ nhất có bằng chứng: trên vault thật, **6/41 khối daily note được gõ vào hôm
sau** (§11). Lấy ngày viết thì 15% số đó lệch một ngày.

Lịch sử Loro đủ nhanh để hỏi thẳng mỗi lần (dưới 1 ms mỗi file), nên **không cần một bảng
"khối thấy lần đầu" riêng**. Một thứ ít phải tự quản lý hơn.

### 4.3 Túi ngày

Mỗi khối **mới** vào túi của ngày khung của nó. Một file có thể nằm ở nhiều túi.

```
Túi 21/07   ← daily note 21/07: "Chiều họp UAT… Trưa ăn bún chả với Nga."
            ← daily note 21/07: "Tối đưa Cam đi bơi."     (gõ 25/07, vẫn là 21/07)
            ← "Dự án Golive TCB": "Hôm nay chốt checklist với chị Yến."  (chỉ khối này)
            ← ghi nhanh "cf với Nga 50k"
            ← lịch hẹn "UAT v2 — MDP" 14:00
            ← task xong "Chốt checklist golive"

Túi 01/08   ← "Dự án Golive TCB": "Hôm nay golive thành công."
```

Mỗi túi là **một lần gọi model**, gồm:

- **để bóc tách**: các khối mới của ngày — câu trong note, **tên task đã xong**, **tên lịch
  hẹn kèm giờ**. Task và lịch hẹn là khối như mọi khối khác (§14, chốt 2026-09-23): chúng
  thường không có thân, nên chính cái tên được đọc, và mỗi khối có nhãn nói nó là gì.
- **chỉ để hiểu**: các khối cũ của cùng file (để biết "cuộc họp đó" là cuộc nào), nhãn tiêu
  đề mục, danh bạ (§5), danh sách loại của vault (§6), các khoảnh khắc đã giữ hôm đó.

Thứ tự trong túi: chữ người viết trước, rồi lịch hẹn, rồi task — cái sổ sách đứng sau cái kể.
Túi quá dài thì chia thành vài lần gọi, cắt giữa các file khi được.

Vì cả ngày được đọc cùng lúc: cuộc họp ghi ở note **và** ở lịch thành **một** đề xuất, id
khối kia nằm ở `also`; ghi nhanh dưới 40 ký tự được đọc; task *"Chốt checklist golive"* gộp
với câu cùng nghĩa trong note. Task chỉ là công việc, không có gì thêm, thì không phải khoảnh
khắc — lời dặn nói thẳng điều đó.

### 4.4 Khi nào đọc

| Lúc | Đọc gì |
| --- | --- |
| Bật lần đầu | cả vault một lần, vài túi mỗi lượt |
| Sau đó | chỉ những túi có khối mới hoặc khối bị sửa; file phải để yên 2 tiếng như hiện nay |

**Không đọc `Moments/`.** Node khoảnh khắc là kết quả của bộ đọc, không phải nguyên liệu:
đọc lại chúng thì mỗi khoảnh khắc sinh ra một đề xuất trùng chính nó. Loại theo thư mục
`Moments/` ở `wanted()`, và loại thêm mọi node `type: moment` nằm ngoài thư mục đó (người
dùng kéo file đi chỗ khác) — thư mục là quy ước, `type` mới là sự thật.

Không chạy trên điện thoại. Không tốn gì khi không có gì mới.

### 4.5 Sửa và xoá

| Việc | Xử lý |
| --- | --- |
| Thêm khối | đọc khối mới trong túi của ngày khung |
| Sửa khối có đề xuất **đang chờ** | đọc lại đúng khối đó, thay đề xuất cũ |
| Sửa khối đã sinh khoảnh khắc **đã giữ** | không tự sửa; sinh một **đề xuất sửa** kèm chênh lệch từng trường (§15) |
| Sửa hay xoá chính khoảnh khắc | bấm vào nó trên dòng thời gian: ô sửa mở ra, sửa mọi trường hoặc xoá vào thùng rác. Trường nào sửa tay thì vào `hand` |
| Sửa chính tả (khối vẫn giống ≥ 80%) | không làm gì |
| Xoá khối | rút đề xuất đang chờ; khoảnh khắc đã giữ ở lại, kèm thẻ *"câu gốc không còn"* để người dùng quyết (§15) |

Sửa một chỗ trong file **không còn khoá** các đề xuất khác: kiểm theo từng câu trích, không
theo hash cả file.

---

## 5. Dặn model thế nào

Phần này là lời dặn thật sự gửi cho model: khoảnh khắc là gì, cái gì không phải, chia nhỏ
tới đâu, điền từng trường theo luật nào. Bộ đọc hiện tại chỉ có một đoạn dặn chín dòng
(`extract.rs`, `prompt()`), không có ví dụ, không nói chia nhỏ tới đâu — nên model mỗi lần
hiểu một kiểu.

### 5.1 Model được biết gì

| Ngữ cảnh | Hiện tại | v3 |
| --- | --- | --- |
| Người dùng là ai | không | tên người dùng, để "mình / tao / tôi" là người viết |
| Danh bạ | không | những người **có vẻ được nhắc** trong túi (dò theo tên, `nickname`, bí danh), cộng người thân; mỗi người một mã để model chọn |
| Ngày mốc | có | có, kèm thứ trong tuần |
| Lịch hẹn, task trong ngày | không | có, trong túi |
| Ví dụ | không | ví dụ lấy từ chính các lần người dùng sửa đề xuất (§8), lưu cục bộ |

Trường `nickname` của node người đã có trong dữ liệu nhưng `People::find` không đọc tới.

### 5.2 Khoảnh khắc là gì — định nghĩa đưa cho model

Một câu: **một chuyện đã xảy ra, có người viết tham gia hoặc trực tiếp chịu ảnh hưởng, đặt
được vào một ngày (hay một quãng), và là thứ người viết muốn thấy lại khi nhìn về ngày đó.**

Bốn điều kiện, đủ cả bốn mới là khoảnh khắc:

| Điều kiện | Có | Không |
| --- | --- | --- |
| **Đã xảy ra** | *"chiều họp UAT"* | *"mai họp UAT"*, *"cần họp UAT"*, *"nếu golive được thì…"* |
| **Người viết ở trong đó** | *"đưa mẹ đi khám mắt"*, *"bố nhập viện"* (gia đình) | *"TCB đổi CEO"*, *"nghe nói anh Hùng nghỉ việc"* |
| **Một lần, không phải thói quen** | *"sáng nay chạy 5 km"* | *"dạo này hay chạy bộ buổi sáng"* |
| **Đáng thấy lại** | *"trưa ăn bún chả với Nga, 50k"* | *"trả lời email"*, *"họp daily"* không có gì thêm |

Điều kiện thứ tư là chỗ khó nhất và là chỗ ví dụ (§5.5) gánh nhiều nhất. Luật chung: chuyện
**thường ngày không có chi tiết** thì bỏ; chuyện thường ngày **có một chi tiết riêng** (với
ai, ở đâu, bao nhiêu tiền, kết quả gì, cảm thấy thế nào) thì giữ. *"Ăn trưa"* bỏ; *"ăn trưa
với Nga ở Kim Mã"* giữ; *"ăn trưa 45k"* giữ, vì chủ vault muốn theo dõi chi tiêu.

**Những thứ không bao giờ là khoảnh khắc**, dặn thẳng:

- kế hoạch, ý định, việc cần làm, lịch trong tương lai;
- kiến thức, ghi chép nội dung, ý kiến — kể cả khi viết trong một note có ngày: *"React 19
  bỏ forwardRef"* không phải chuyện xảy ra;
- **nội dung** của một cuộc họp: cuộc họp là một khoảnh khắc, từng ý bàn trong đó thì không;
- chuyện của người khác kể lại, trừ người trong gia đình hoặc khi người viết bị ảnh hưởng;
- chữ chép từ nơi khác: đoạn trích bài báo, tin nhắn dán vào, mẫu, checklist;
- chuyện chỉ có trong **khối ngữ cảnh** (§4.3) — chỉ bóc từ các khối được đánh dấu *đọc*.

### 5.3 Chia nhỏ tới đâu

| Tình huống | Số khoảnh khắc |
| --- | --- |
| Một cuộc họp, ghi năm ý bàn | **1** |
| *"Trưa ăn bún chả với Nga, hết 50k"* | **1**, `category: meal`, có `amount` — không tách thành bữa ăn và chi tiêu |
| Sáng họp với MDP, chiều họp với TCB | **2** — khác người, khác việc |
| Cùng một cuộc họp: có trong lịch, có câu trong note, có task đã xong | **1**; câu trích lấy từ note, lịch và task ghi vào `also` |
| Đi Đà Lạt ba ngày, mỗi ngày một đoạn nhật ký | **1 quãng** cho chuyến đi, cộng khoảnh khắc riêng cho chuyện có chi tiết trong từng ngày |
| Một khoảnh khắc đã giữ rồi (có trong *"đã có"*) | **0** — không đề xuất lại; nguồn có thêm chi tiết thì đó là đề xuất sửa (§15) |

Luật chung: **một khoảnh khắc là một việc, với một nhóm người, ở một lúc.** Đổi một trong ba
thì là khoảnh khắc khác.

### 5.4 Điền từng trường

**`title`** — ngôn ngữ của note; 3–8 chữ; bắt đầu bằng việc (*"Họp…"*, *"Ăn trưa…"*, *"Đưa
Cam đi bơi"*); có người hoặc nơi nếu câu có; **không** có ngày, không có *"tôi / mình"*,
không thêm chi tiết câu gốc không nói. Không chép nguyên câu.

**`date`, `time`, `date_basis`** — ngày chuyện xảy ra, không phải ngày viết:

| Câu viết | `date` | `date_basis` |
| --- | --- | --- |
| có ngày ghi rõ (*"21/7"*, *"2026-07-21"*) | ngày đó | `explicit` |
| *"hôm qua"*, *"tối qua"*, *"đêm qua"* | ngày khung − 1 | `relative` |
| *"hôm kia"* | ngày khung − 2 | `relative` |
| *"thứ Ba"* (không nói tuần nào) | thứ Ba gần nhất **không sau** ngày khung | `relative` |
| *"tuần trước"* | thứ Hai → Chủ nhật của tuần trước, `precision: week` | `relative` |
| *"đầu tháng"*, *"tháng 4"* | cả tháng, `precision: month`; năm thiếu thì lấy năm của ngày khung | `relative` |
| *"hôm nay"*, *"sáng nay"*, hoặc **không có chữ chỉ thời gian** | ngày khung | `the_day` |
| model phải đoán từ nội dung | ngày đoán | `inferred` — app xếp những đề xuất này xuống cuối |

- Ngày tính ra **sau ngày khung** thì đó là kế hoạch → không phải khoảnh khắc.
- `time` chỉ điền khi câu có giờ (*"14h"*, *"21:03"*, *"2 giờ chiều"*). Không đoán giờ từ
  *"sáng"*, *"chiều"*. Có lịch hẹn cùng việc thì lấy giờ của lịch.

**`people`** — những người **khác** tham gia; không bao giờ có người viết.

- Tên, biệt danh, cách gọi (*"chị Yến"*, *"Cam"*, *"mẹ"*) khớp một người trong danh bạ →
  `{ "ref": "p12" }`. Hai người cùng khớp → không chọn, ghi `{ "name": "…" }`.
- Không khớp ai → `{ "name": "chị Yến" }`, giữ nguyên cách gọi kể cả *"anh / chị"*.
- *"cả nhóm"*, *"mọi người"*, *"team MDP"* → không phải người; đưa vào `about` nếu có tên.

**`place`** — chỉ khi câu nói nơi chốn, chép theo câu. Không suy nơi từ việc (*"khám mắt"*
không có nghĩa là *"bệnh viện"*).

**`about`** — dự án, tổ chức, chủ đề được gọi tên trong câu (*"UAT v2"*, *"TCB"*). Tối đa ba.

**`category`** — một trong danh sách của vault (§6). Chuyện vừa ăn vừa gặp người: `meal`. Có tiền mà
không phải ăn: `spending`. Không chắc: `other` — không ép.

**`amount`** — chỉ khi câu có số tiền. *"50k"* → 50000, *"1tr2"* → 1200000, *"2 củ"* →
2000000; đơn vị VND trừ khi câu ghi khác. Không cộng các khoản trong câu — mỗi khoản thuộc
khoảnh khắc của nó.

**`quote`** — **đoạn liền ngắn nhất** trong khối được đọc chứng minh cho khoảnh khắc, chép
nguyên văn **từ bản chữ trơn đưa cho model** (§7 so trên bản đó), tối đa một câu, cộng mã
khối. Không sửa chính tả, không nối hai chỗ.

**`also`** — mã lịch hẹn, task trong túi nói cùng chuyện, để app gộp (§5.3).

### 5.5 Lời dặn đầy đủ

Viết bằng tiếng Anh (model theo dặn tốt hơn), ví dụ bằng tiếng Việt vì vault viết tiếng
Việt. Một system prompt cố định, một user message cho mỗi túi. `temperature: 0`, khuôn trả về
ép bằng JSON schema (§6).

**System prompt** (cố định, đặt đầu để provider cache được):

```text
You read a person's own notes for one day and list the MOMENTS in their life that day.

A moment is something that HAPPENED, that the writer took part in or was directly
affected by, that can be placed on a day (or a span of days), and that they would
want to see again when looking back at that day. All four must hold.

Never a moment:
- plans, intentions, to-dos, anything scheduled after the day you are given;
- knowledge, notes on content, opinions, summaries of what was discussed;
- things that happened to someone else, told second-hand — unless they are family,
  or the writer was affected;
- text copied from elsewhere: articles, pasted messages, templates, checklists;
- anything in a block marked CONTEXT. Only extract from blocks marked READ.
- anything already listed under ALREADY KEPT.

Routine without detail is not worth keeping ("had lunch", "replied to emails",
"daily standup"). Routine WITH a detail is: who with, where, how much it cost, how it
turned out, how it felt.

One moment = one thing done, with one group of people, at one time. A meeting with
five agenda points is one moment. A meal and what it cost is one moment.
The same thing in a note, the calendar and a completed task is one moment: quote the
note, list the calendar and task ids in "also".

Fields:
- title: 3–8 words, in the notes' language, starting with what was done. Include
  who or where if the text says. No date, no "I". Add nothing the text does not say.
- date / date_to / time / date_basis / precision: when it HAPPENED, not when it was
  written. Relative words count from the day given ("hôm qua" = the day before;
  "thứ Ba" = the latest Tuesday not after the day). No time word = the day given,
  date_basis "the_day". A date after the day given means it is a plan: leave it out.
  time only when the text gives a clock time. Never guess one.
- people: the OTHERS who took part, never the writer. Use a ref from PEOPLE when
  the name, nickname or form of address matches exactly one person; otherwise
  {"name": "..."} as written, keeping "anh"/"chị". Groups are not people.
- place: only as written. Do not infer a place from the activity.
- about: named projects, organisations, topics in the text. At most three.
- category: one of the list. Unsure: "other".
- amount: only a sum written in the text. 50k = 50000, 1tr2 = 1200000. VND unless
  another currency is written.
- quote: the shortest continuous span of the READ block that shows it happened,
  copied exactly, at most one sentence, with the block id.

Most days have few moments. Many blocks have none. {"moments": []} is a good answer.
```

**User message** (mỗi túi một cái):

```text
WRITER: Anh — the "tôi / mình / tao / anh" in the notes.
DAY: 2026-07-21, Tuesday.
KINDS (category): meal, spending, meeting, work, health, trip, family, feeling, thought, milestone, other

PEOPLE (use the ref):
  p12  Yến        also: chị Yến          colleague · MDP
  p3   Nga        also: Nga béo          friend
  p7   Cam        also: con, bé Cam      family · child

ALREADY KEPT THIS DAY:
  (none)
CORRECTIONS THE WRITER MADE BEFORE:            ← §8.2, vài cặp gần nhất
  "Họp với team" → "Họp UAT v2 với MDP"
  people "Cam" → p7

[b1 · READ]    Daily 2026-07-21 › Công việc
Chiều họp UAT v2 với chị Yến, chốt lại ngày làm việc trên UI.
[b2 · READ]    Daily 2026-07-21
Trưa ăn bún chả với Nga ở Hàng Mành, 50k.
[b3 · READ]    Golive TCB › Nhật ký
Hôm nay chốt checklist golive với chị Yến. Mai golive.
[b4 · READ]    UAT v2 — MDP · calendar
UAT v2 — MDP (14:00)
[b5 · READ]    Chốt checklist golive · task, finished
Chốt checklist golive
[x1 · CONTEXT] Golive TCB › Checklist
- Backup DB  - Tắt job đồng bộ  - Kiểm tra kết nối NAPAS
```

Trả về đúng mong đợi cho túi trên:

```json
{ "moments": [
  { "title": "Họp UAT v2 với chị Yến", "date": "2026-07-21", "time": "14:00",
    "date_basis": "the_day", "people": [{ "ref": "p12" }], "about": ["UAT v2", "MDP"],
    "category": "meeting", "quote": { "source": "b1", "text": "Chiều họp UAT v2 với chị Yến" },
    "also": ["b4"] },
  { "title": "Ăn trưa bún chả với Nga", "date": "2026-07-21", "date_basis": "the_day",
    "people": [{ "ref": "p3" }], "place": "Hàng Mành", "category": "meal",
    "amount": { "value": 50000, "unit": "VND" },
    "quote": { "source": "b2", "text": "Trưa ăn bún chả với Nga ở Hàng Mành, 50k." } },
  { "title": "Chốt checklist golive TCB với chị Yến", "date": "2026-07-21",
    "date_basis": "the_day", "people": [{ "ref": "p12" }], "about": ["TCB"],
    "category": "work", "quote": { "source": "b3", "text": "Hôm nay chốt checklist golive với chị Yến." },
    "also": ["t4"] }
]}
```

Và những gì **không** có trong đó: *"Mai golive"* (kế hoạch), bốn mục checklist (khối ngữ
cảnh, và là việc cần làm), từng ý bàn trong cuộc họp.

### 5.6 Ví dụ trong lời dặn

Lời dặn kèm **sáu ví dụ ngắn** (khối vào → kết quả), chọn để phủ đúng những chỗ model hay
sai, mỗi ví dụ một điểm:

| Ví dụ | Dạy điều gì |
| --- | --- |
| *"Tối qua đưa Cam đi bơi"* trong daily note 22/07 | ngày tương đối; biệt danh → ref |
| *"21:03 — gọi cho mẹ, mẹ bảo đã đỡ đau lưng"* | giờ đứng riêng là `time`, ngày là ngày khung (lỗi 07-09, §2.3) |
| Ghi chép năm ý của buổi họp | **một** khoảnh khắc, không phải năm |
| *"Dạo này hay mất ngủ"* | thói quen → rỗng |
| *"Tuần sau đi Đà Lạt"* | kế hoạch → rỗng |
| *"[Phan Hương Lê](synabit://…) gọi trao đổi về hợp đồng"* | chữ của link là tên người; câu trích lấy từ bản chữ trơn |

Ví dụ là cố định, viết tay, và **nằm trong tập vàng** (§10): đổi lời dặn mà một ví dụ của
chính nó chạy sai thì không được đổi. Các cặp sửa của người dùng (§8.2) đi riêng, ở user
message, vì chúng đổi theo thời gian còn system prompt thì không.

### 5.7 Vì sao lời dặn như vậy

- **Định nghĩa bốn điều kiện thay cho danh sách ví dụ loại việc.** Lời dặn cũ liệt kê *"a
  meeting, a trip, a change of job…"*, nên model bỏ những chuyện nhỏ có chi tiết — đúng loại
  chủ vault muốn (ăn uống, chi tiêu).
- **Nói thẳng *"rỗng là câu trả lời tốt"*.** Model được hỏi *"liệt kê"* có xu hướng liệt kê
  cho có; 13/36 note trống ở §2.2 là trống thật.
- **Ngày do model tính ra ISO, nhưng kèm `date_basis`.** Model có ngày khung và thứ trong
  tuần nên tính được; `date_basis` cho §7 biết chỗ nào cần kiểm và cho màn duyệt biết đề
  xuất nào nên xem kỹ.
- **Danh bạ có mã.** Model chọn mã thay vì viết tên để app đi ghép — chỗ đã làm rơi mọi
  người ở bộ đọc cũ.
- **Không cho model tự chấm điểm.** Độ chắc do app tính (§6).

---

## 6. Khuôn trả về

Thêm `format` vào `ChatRequest` (`src-tauri/src/syn/provider/mod.rs`): Ollama nhận JSON
schema qua `format`, API kiểu OpenAI qua `response_format`. Provider không hỗ trợ thì đọc như
hiện nay, cộng một lần gọi lại để sửa JSON hỏng.

```json
{ "moments": [{
    "title": "Họp UAT v2 với MDP",
    "date": "2026-07-21", "date_to": null, "precision": "day",
    "time": "14:00",
    "date_basis": "explicit | relative | the_day | inferred",
    "people": [{ "ref": "p12" }, { "name": "chị Yến" }],
    "place": "văn phòng MDP",
    "about": ["UAT v2"],
    "category": "meeting",
    "amount": null,                       // hoặc { "value": 50000, "unit": "VND" }
    "quote": { "source": "b3", "text": "Họp UAT v2 - điều chỉnh ngày làm việc trên UI" },
    "also": ["c1", "t4"]                  // lịch hẹn, task cùng chuyện (§5.3)
}]}
```

- **Model trả ngày ISO**, vì nó có sẵn ngày khung. **Giờ ở trường `time` riêng**, nên lỗi
  `21:03` không còn chỗ xảy ra.
- **Người**: chọn mã từ danh bạ (`ref`), không có thì ghi `name`. Cả hai được giữ.
- **`category`**: **danh sách nằm trong vault**, ở `categories` của `Timeline/extract.json`.
  Mặc định là `meal`, `spending`, `meeting`, `work`, `health`, `trip`, `family`, `feeling`,
  `thought`, `milestone`, `other`. Lời dặn và khuôn JSON lấy đúng danh sách ấy, nên model bị
  đóng khung — nhưng khung là của chủ vault, thêm bớt được trong màn duyệt và trong phần cài
  đặt. Model **không bao giờ tự đặt loại mới**: loại lạ bị ép về `other`, và `other` luôn có
  trong danh sách. Xem §14, chốt 2026-09-23.
- **`amount`** (số tiền, đơn vị) cho chi tiêu: trường số đầu tiên của khoảnh khắc, là thứ
  biểu đồ tổng đang thiếu.
- **Bỏ `confidence`.** Thay bằng **độ chắc do app tính**: câu trích khớp nguyên văn không,
  ngày ghi rõ hay suy ra, người đã ghép được chưa. Dùng để sắp thứ tự khi duyệt, không dùng để loại.
- **Quãng thời gian**: `date_to`, `ongoing`, và độ chính xác cho từng đầu (§16).

---

## 7. Kiểm tra lại

| Cửa | v3 |
| --- | --- |
| Tiêu đề | có tiêu đề |
| Ngày | ISO hợp lệ, không sau ngày khung. Nếu `date_basis = relative` mà nguyên văn đọc ra ngày khác thì gắn cờ, không loại |
| Câu trích | so trên **bản chữ trơn** (bỏ markdown, giữ chữ của link), nhưng nhớ vị trí trong bản gốc để bôi vàng khi xem nguồn |
| Trùng | cùng ngày, tiêu đề gần giống một khoảnh khắc đã có hoặc một đề xuất khác thì gộp |
| **Lý do loại** | **ghi đủ**: cửa nào, và bản gốc model đã trả. Lưu trên máy, không sync |

---

## 8. Duyệt và giữ

### 8.1 Giữ thì ghi gì

Một **file mới** `Moments/<uuid>.md`, mỗi khoảnh khắc một file (§15.3):

```yaml
# Moments/6f3c9a2e-….md
---
type: moment
title: Họp UAT v2 với MDP
happened: 2026-07-21 14:00
people: [{ id: People/…md }, { name: chị Yến }]   # tên chưa ghép được KHÔNG bị bỏ
where: văn phòng MDP
about: [UAT v2]
category: meeting
amount: null
origin: extract:x7a3… | manual
source: { node: Notes/…md, block: <hash>, quote: "Chiều họp UAT v2 với chị Yến" }
hand: [title]                     # trường người dùng đã sửa tay; không bị đề nghị ghi đè (§15.2)
---
(thân file để trống; người dùng muốn viết thêm về khoảnh khắc này thì viết ở đây)
```

`source` trỏ về nguồn bằng đường dẫn **và** hash khối: dời note thì đường dẫn được sửa theo
như mọi link khác trong vault; sửa khối thì hash cho biết khối nào (§4.1). `moments[]` trong
frontmatter của note cũ vẫn đọc được cho tới khi chuyển xong (§15.3).

### 8.2 Màn duyệt

- Nhóm theo ngày, mới nhất trước; nút *"Giữ cả ngày"*.
- **Sửa mọi trường**: tiêu đề, ngày giờ, người (chọn từ danh bạ; tên chưa ghép hiện *"chưa
  khớp · gán cho… / tạo người mới"*), nơi chốn, loại, số tiền. Form này là form *Ghi khoảnh
  khắc* — một component cho cả hai cửa.
- Phím tắt: `j`/`k`, `Enter` giữ, `e` sửa, `x` bỏ.
- Nguồn đọc tại chỗ, dòng được trích bôi vàng (đã có).
- **Học từ lần sửa**: gán "Cam" cho một người thì `Cam` thành bí danh của người đó; sửa loại
  hay tiêu đề thì cặp trước/sau thành ví dụ cho các lần đọc sau (vài chục cặp gần nhất, cục bộ).
- **Tự giữ** (tắt sẵn): lịch hẹn đã diễn ra tự thành khoảnh khắc.

---

## 9. Đường tự ghi

Cùng form, cùng khuôn với đường duyệt. Ô gõ một dòng (*"hôm qua ăn trưa với Khánh ở Kim Mã
150k"*) đi qua **cùng prompt và cùng schema** với bộ đọc túi ngày, rồi điền sẵn vào form. Hai
cửa cho ra đúng cùng một loại dữ liệu: một file `Moments/<uuid>.md`, `origin: manual`,
không có `source`.

---

## 10. Đo chất lượng

Dùng khung của `eval-timeline-extract-2026-09-15.md` (đo trước khi bật, dự đoán ghi trước khi
đo), bổ sung **một tập vàng lấy từ vault thật**, gồm đúng những ca rà soát tìm ra:

| Ca | Vì sao có trong tập |
| --- | --- |
| 07-09, năm mốc giờ trong một buổi trực | giờ đứng riêng |
| 06-06 *Golive TCB - Nam Á* | bị loại, chưa rõ cửa |
| 09-11 *Trao đổi với Phan Hương Lê* | link markdown ngay trong câu |
| 05-26, một đoạn nhật ký | bị loại, chưa rõ cửa |
| 07-06 *Họp review môi trường STG và PRD* với chị Yến, Đức | tên người không nối được |
| 07-08 *Đưa Cam đi phỏng vấn nhập học* | biệt danh |
| Các ngày đã có đề xuất đúng | để kiểm không làm hỏng cái đang chạy |

Đo **độ phủ** (bắt được bao nhiêu chuyện có thật) và **độ chính xác** (bao nhiêu đề xuất
đúng), trước và sau mỗi lần đổi prompt. Nâng `EXTRACTOR_VERSION` chỉ khi con số tốt lên;
lúc đó app tự đề nghị đọc lại.

---

## 11. Đo — lịch sử Loro trên vault thật, 2026-09-22

Câu hỏi: tìm ngày viết của từng khối bằng lịch sử Loro có quá chậm không, và có đúng không.

Cách đo: một test `#[ignore]` tạm trong `db::crdt`, chạy bản release, trên **bản sao**
`vault_cache.db` lấy bằng `.backup` ở chế độ chỉ đọc. Mỗi note: chia khối như §4.1, đi từ
phiên bản mới nhất về cũ, dừng khi mọi khối đã tìm ra lần xuất hiện đầu. Đã gỡ khỏi code;
bản sao đã xoá.

| | |
| --- | --- |
| File `.md` có lịch sử | **420/425** |
| Thời gian cho cả vault | **0,36 giây** |
| Mỗi file | trung vị 0,6 ms · p95 1,0 ms · chậm nhất 46 ms (trang người 560 nghìn ký tự) |
| Phiên bản mỗi file | trung vị 2 · p95 5 · nhiều nhất 12 (một phiên bản = một lượt sửa liên tục, gộp trong 5 phút) |
| Khối | 1.677 |
| Khối có mốc giờ | **121 (7%)** |
| Khối có từ phiên bản đầu tiên | 1.617 (96%) |
| **Daily note: ngày tìm được so với ngày của note** | **cùng ngày 35 · hôm sau 6 · lệch hơn 0** |
| Lịch sử có giờ bắt đầu từ | **15/08/2026** |

Đọc ra ba điều:

1. **Tốc độ không phải vấn đề.**
2. **Khi có mốc thì mốc đúng**: 41/41, không khối nào lệch quá một ngày.
3. **Dữ liệu trước 15/08 không có mốc.** Không phải vì chậm mà vì chưa từng được ghi. Những
   khối đó dùng ngày tạo file kèm nhãn "ngày ước lượng" (§4.2). Daily note không bị ảnh hưởng
   (chúng dùng ngày của note); note viết trong một buổi vẫn đúng; chỉ note được viết nối dài
   qua nhiều ngày trước 15/08 bị tính mọi đoạn về ngày tạo.

Một lần đo sai trước đó, ghi lại để khỏi lặp: lần chạy đầu chỉ duyệt `sync_crdt_documents`
và kết luận *chỉ 20 note có lịch sử*. Lịch sử nằm trong `sync_crdt_updates` cho tới khi được
gộp thành ảnh chụp; phải đọc cả hai.

Giới hạn của phép đo: khối được nhận diện bằng so nguyên chuỗi, nên khối bị sửa chính tả bị
tính là mới từ lần sửa. Không ảnh hưởng số đo tốc độ.

---

## 12. Nói thẳng về giá

1. **Mọi khoảnh khắc phải qua tay người dùng.** Nếu duyệt chậm, dòng thời gian sẽ vắng hơn
   bây giờ chứ không đầy hơn. Vì thế §8.2 là một nửa của thiết kế, không phải phần phụ.
2. **Lần đọc đầu tốn khoảng một lần gọi model cho mỗi ngày có viết.** Sau đó, mỗi ngày một
   lần. Mặc định model cục bộ; cloud chỉ khi `allow_cloud`.
3. **Bỏ các dòng suy ra khỏi dòng thời gian làm nó vắng đi ngay lập tức** — một phần năm số
   dòng còn lại — cho tới khi bộ đọc v3 chạy xong lần đầu. Làm §13 giai đoạn 2 trước giai
   đoạn 3 là chấp nhận điều đó để đổi lấy một dòng thời gian không nói sai.
4. **Dữ liệu cũ có ngày ước lượng.** Không có cách nào tốt hơn cho note viết nối dài trước
   15/08; nhãn "ước lượng" là để không giả vờ biết.
5. `allow_cloud` giữ nguyên như §8.6 của tài liệu Timeline. Niêm phong đã bị gỡ khỏi app
   (2026-09-23, xem cuối §13).

---

## 13. Thứ tự làm

| | Việc | Vì sao |
| --- | --- | --- |
| **1** | Sửa bốn lỗi đang mất dữ liệu: rơi tên người khi Giữ (`moment_entry`), giờ đứng riêng (`resolve`), markdown trong câu trích (`find_quote`), khoá Giữ theo hash cả file. Sửa `within()` tính giao nhau thay vì ngày bắt đầu (§16). Ghi lý do loại. Thêm `id` cho moment | nhỏ, và đang mất dữ liệu thật mỗi ngày |
| **2** | Khoảnh khắc thành file `Moments/<uuid>.md` (`type: moment`), loại khỏi đồ thị, tab Node và bộ đọc; chuyển các mục `moments[]` sang. Tách moment khỏi fact: nguồn `moments` và dòng thời gian chỉ trả moment | dòng thời gian thôi nói sai ngay, chưa cần AI mới |
| **3** | Bộ đọc v3: khối, ngày khung, túi ngày, danh bạ có bí danh, schema, `category`, `amount`, quãng thời gian (§16) | phần lớn nhất; kiểm bằng tập vàng §10 |
| **4** | Duyệt v2: sửa mọi trường, nhóm theo ngày, phím tắt, học từ lần sửa, thẻ đề xuất sửa (§15). Làn quãng thời gian trên biểu đồ (§16) | khi đề xuất đã nhiều và đã tốt |
| **5** | Nâng `EXTRACTOR_VERSION`, đề nghị đọc lại cả vault | chỉ khi tập vàng tốt lên |

**Nếu chỉ làm được một bước: bước 1.** Nó là bước duy nhất đang làm mất dữ liệu.

**Bước 1 đã làm (2026-09-23).**

- `moment_entry` ghi cả tên chưa ghép được vào `people`, như note viết; chúng lên dòng thời
  gian thành liên kết *with*.
- `resolve` tách giờ ra khỏi `when` (`21:03`, `14h30`, `2 giờ chiều`, `9am`, `lúc …`); chỉ còn
  giờ thì là ngày khung, còn chữ khác thì chữ đó quyết ngày.
- `find_quote` tìm cả trên **bản chữ trơn** (link thành nhãn, bỏ `**` `~~` `` ` ``), và vẫn trả
  vị trí trong bản gốc để bôi vàng.
- Khoá Giữ theo **câu trích**: sửa chỗ khác trong note không khoá; câu trích biến mất mới khoá.
  Thẻ *"đã sửa"* trong màn duyệt theo cùng luật.
- Lý do loại: mỗi mục bị loại ghi vào bảng `extract_drops` của `timeline.db` (cửa, câu trả lời
  gốc của model, model, lúc đọc). Chỉ trên máy này; lần đọc mới của một note thay lần cũ.
  Chưa có màn hình xem.
- `id` (UUID) cho mọi khoảnh khắc mới, cả đường Giữ lẫn đường tự ghi. Bước 2 dùng nó làm tên
  file `Moments/<uuid>.md`. Các mục cũ chưa có `id`; bước 2 cấp khi chuyển.
- Hàng kết quả của dòng thời gian mang `until` (ngày cuối, khi dài hơn một ngày);
  `within()`, `outside()`, `autoWindow()` và số đếm vùng chọn tính theo **giao nhau**.

Những gì bước này **không** làm: không đọc lại note nào, và không nâng `EXTRACTOR_VERSION`.
Các khoảnh khắc đã mất vì các lỗi trên chỉ quay lại khi note được đọc lại: note sửa sau này
thì tự vào hàng *đã sửa*; note để yên thì chỉ được đọc lại khi nâng phiên bản bộ đọc — việc
của bước 5, hoặc sớm hơn nếu chủ vault muốn lấy lại ngay (tốn một lượt gọi model cho mỗi note).

**Bước 2 đã làm (2026-09-23).**

- `timeline::moments`: mỗi khoảnh khắc là `Moments/<uuid>.md`, `type: moment`. Frontmatter
  giữ mọi key của mục cũ (trừ `id`, giờ là tên file), thêm `origin` (`extract` / `manual`)
  và `source: { node, quote }` khi khoảnh khắc được đọc ra từ một note.
- **Giữ** một đề xuất: ghi file mới, không đụng note gốc. `id` tính từ id của đề xuất, nên
  giữ cùng một đề xuất hai lần (hai cú bấm, hai máy) ra cùng một file.
- **Tự ghi**: ghi file mới, không còn ghi vào — hay tạo — daily note của ngày đó.
- **Chuyển dữ liệu**: chạy sau mỗi lần quét vault (`scan_all_nodes`). Mỗi mục `moments[]`
  thành một file; note chỉ bị gỡ `moments[]` khi **mọi** mục của nó đã thành file. `id` của
  file chuyển sang được tính cố định từ note + vị trí + tiêu đề + ngày, nên hai máy cùng chuyển
  ra cùng file. Note đồng bộ tới từ một máy chạy bản cũ được chuyển ở lần mở sau.
- **Loại khỏi node gốc**: `moment` vào danh sách kiểu bị ẩn trừ khi hỏi đích danh
  (`db::internal`, `syn::tools::is_internal_type`, `useObservedTypes.ts`) — nên tab Node,
  truy vấn `nodes` và Things không thấy nó; đồ thị và danh sách Nexus dùng chung một luật
  `left_out_of_nexus`; bộ đọc bỏ qua (`extract::never`).
- **Dòng thời gian chỉ trả moment**: nguồn `moments` lọc `kind = 'moment'`. Task đã xong, ngày
  sinh, chức vụ, note có ngày vẫn được suy ra và vẫn dùng ở trang người, dự án, *ngày này năm
  xưa* — chỉ không còn là dòng trên dòng thời gian.
- Mở một khoảnh khắc trên dòng thời gian mở **note nguồn** (`source.node`); khoảnh khắc tự ghi
  mở chính file của nó.

**Trên vault thật** (dev app tự build lại và chạy bước chuyển lúc 00:31): 37/37 mục chuyển
sang, khớp từng tiêu đề, ngày, người, nơi chốn với bản cũ trong `timeline.db`; 0 note còn
`moments[]`. Con số 103 ghi trước đây là sai: đó là 37 khoảnh khắc đã giữ cộng 66 đề xuất
đang chờ — đề xuất không nằm trong note nên không có gì để chuyển.

Chưa làm:

- `source.quote` của 37 file đã chuyển để trống — mục cũ không lưu câu trích. Lấy lại được
  từ `extract` id (đề xuất gốc còn trong `timeline.db`), chưa làm.
- **Đọc ngược**: mở một note chưa thấy các khoảnh khắc trích từ nó. Cần một ô dưới note
  trong app Note.
- Máy nào còn chạy bản cũ sẽ không thấy khoảnh khắc (bản cũ không đọc `type: moment`) cho tới
  khi cập nhật.

**Bước 3 đã làm (2026-09-23).**

- `timeline::blocks`: tách note thành khối (đoạn văn, mục danh sách cấp 1 kèm con, bảng,
  trích dẫn, HTML), bản chữ trơn kèm bản đồ vị trí về bản gốc, mã khối theo chữ đã bỏ markup,
  so khối sửa bằng Dice ≥ 0,8, và đọc lịch sử Loro để biết khối xuất hiện lần đầu khi nào.
  Khối chứa một "từ" dài quá 200 ký tự bị bỏ (ảnh dán base64: trên vault thật có một khối
  534.192 ký tự).
- `timeline::reader`: danh bạ (bí danh, tên gọi, quan hệ, người nhà luôn có mặt, chủ vault là
  người viết), ngày khung theo §4.2, túi ngày, lời dặn cố định §5.5 kèm sáu ví dụ, khuôn JSON
  §6 ép qua `response_format` / `format` / JSON mode — provider nào không biết trường đó thì
  hỏi lại một lần không kèm khuôn — và các cửa kiểm §7 (câu trích phải thuộc khối được đọc,
  ngày không sau ngày khung, trùng thì bỏ, loại sai thành `other`, giờ sai khuôn thì bỏ).
- **Đọc theo khối**: một lần đọc ghi lại đúng những khối nó đã đọc, nên sửa chỗ khác trong
  note không bắt đọc lại cả note, và note dời chỗ không bị đọc lại. Sửa nhỏ một khối vẫn là
  khối cũ. Việc "ghi chép đã sửa" biến mất khỏi tray: sửa là khối mới.
- **Mọi note đều được đọc**, không chỉ note có `date:` hay nằm trong thư mục đã khai.
  `timeline: false` vẫn là cửa chặn.
- Khuôn khoảnh khắc thêm `category`, `amount`, `about`, `time`, `date_basis`, `also`; thẻ đề
  xuất hiện loại, nơi chốn, số tiền và giờ. Bỏ con số % của model, thay bằng nhãn *ngày đoán*
  khi model phải suy ra ngày.
- Đường tự ghi (§9) đi qua **cùng lời dặn và cùng khuôn**: một dòng là một túi một khối.
- Tập vàng §10: `reader::tests::live::golden_days` đọc sáu ngày rà soát tìm ra bằng model
  thật, in ra từng đề xuất và từng mục bị loại kèm cửa. `#[ignore]`, chạy bằng tay, không ghi
  gì vào vault.

**Đo trên vault thật** (chỉ lập kế hoạch, không gọi model):

| | Túi | Khối | Note | Ký tự |
| --- | --- | --- | --- | --- |
| Chưa đọc bao giờ | 53 | 855 | 92 | 115.457 |
| Đã đọc bằng bộ đọc cũ (đợi *đọc lại*) | 76 | 838 | 73 | 75.167 |

Danh bạ: 110 người, 10 người nhà. Mỗi túi mang 10–15 người, không phải cả 110.

**Bước 4 đã làm (2026-09-23).**

- **Sửa mọi trường trước khi giữ**: tiêu đề, ngày (từ/đến), giờ, người, nơi chốn, loại, số
  tiền, chủ đề. Trường nào mày tự sửa được ghi vào `hand` của khoảnh khắc, và bộ đọc **không
  bao giờ đề nghị đổi** trường đó nữa (§15.2). Câu trích vẫn không sửa được: nó là bằng chứng.
- **Nhóm theo ngày**, mới nhất trước, kèm nút *Giữ cả ngày*.
- **Phím tắt**: `j`/`k` đi lại, `Enter` giữ, `e` sửa, `x` bỏ. Bỏ qua khi con trỏ đang ở trong
  một ô nhập. Quyết xong thì thẻ kế tiếp được chọn, nên một lượt duyệt chạy liền mạch.
- **Học từ lần sửa** (§8.2): gán một cái tên cho ai đó thì tên ấy thành **bí danh trong note
  của người đó**, nên lần sau bộ đọc tự ghép được; sửa tiêu đề hay loại thì cặp trước/sau được
  giữ trong `timeline.db` (20 cặp gần nhất, không đồng bộ) và đưa vào lời dặn của lần đọc sau
  dưới mục *CORRECTIONS THE WRITER MADE BEFORE*.
- **Thẻ nguồn đã đổi (§15)**: mỗi khoảnh khắc đã giữ nhớ **mã khối** nó được đọc ra. Khối đó
  còn nguyên thì im lặng — thêm in đậm không phải là sửa, vì mã khối tính trên chữ đã bỏ
  markup. Khối đổi thì model được đưa khoảnh khắc hiện tại cùng khối mới và trả một trong ba:
  *unchanged*, *changed* (kèm khoảnh khắc đã sửa), *retracted*. Không còn câu nào giống nó
  trong note, hoặc mất cả note, thì **không hỏi model**: thẻ hiện luôn.
  - Thẻ *changed* hiện chênh lệch từng trường và hai nút: **Áp dụng** / **Giữ nguyên**.
  - Thẻ *câu gốc không còn* và *note đã bị xoá* hiện hai nút: **Giữ khoảnh khắc** / **Xoá
    khoảnh khắc**. Xoá là chuyển vào thùng rác, không phải xoá thẳng.
  - Mỗi trạng thái của nguồn chỉ hỏi một lần; sửa tiếp là trạng thái khác, hỏi lại.
- **Làn quãng thời gian trên biểu đồ (§16.3)**: quãng vẽ thành thanh ngang ở làn riêng phía
  trên cột, **không cộng vào cột đếm** và không thành chấm; quãng chạy quá mép biểu đồ có mũi
  tên. Danh sách ghi thêm ngày kết thúc sau tiêu đề, và quãng bắt đầu trước khung đang xem
  được **ghim ở đầu** dưới nhãn *Trong khoảng này*.

Chưa làm ở bước này: *tự giữ* lịch hẹn đã diễn ra (§8.2) — nó vẫn là câu hỏi 3 ở §14.

**Sửa và xoá khoảnh khắc đã giữ (2026-09-23).** §4.5 có nói tới việc này nhưng §13 không xếp
nó vào bước nào, nên nó rơi mất: giữ xong là không còn đường nào sửa hay xoá trong app.

- Bấm một khoảnh khắc trên dòng thời gian **mở chính nó**, trong một ô sửa đủ mọi trường —
  tiêu đề, ngày, giờ, người, nơi chốn, loại, số tiền, chủ đề. Trước đó nó mở note nguồn, còn
  khoảnh khắc tự ghi thì bấm không ra gì.
- **Trường nào sửa ở đây là của người dùng**: nó vào `hand`, nên không lần đọc nào đề nghị đổi
  lại (§15.2) — cùng một lời hứa mà màn duyệt đã giữ.
- **Câu trích và note nguồn không sửa được**, vẫn vì lý do cũ: đó là bằng chứng. Nút *Đọc ra
  từ…* mở note tại câu đó.
- **Xoá là hai lần bấm**, và file đi vào thùng rác chứ không mất: giữ một khoảnh khắc là một
  quyết định, gỡ quyết định không phải là xoá dấu vết của nó.
- Mở một khoảnh khắc từ chỗ khác — ví dụ gõ `type:moment` ở tab Node — giờ mở file của nó
  trong app Note, thay vì không làm gì.

Ba lệnh mới: `timeline_moment`, `timeline_moment_write`, `timeline_moment_delete`.

**Đập đi xây lại (2026-09-23).** Dùng model cùi đọc xong thì được một dòng thời gian toàn thứ
không ai muốn, và duyệt tay hai trăm cái để nói "không" từng cái một không phải là duyệt.

- `timeline::reset` **đếm trước** rồi mới hỏi: bao nhiêu khoảnh khắc, đề xuất, lần đọc, quyết
  định. Bấm xác nhận thì gửi lại đúng con số đã hiện; vault đổi giữa chừng — một khoảnh khắc
  vừa đồng bộ về — thì **từ chối** chứ không làm khác cái người dùng đã đồng ý.
- **Không xoá thẳng cái gì cả**: file khoảnh khắc và file quyết định vào thùng rác, lấy lại
  được trong app Files. `timeline.db` bị dọn sạch rồi dựng lại từ vault — nó vốn chỉ là một
  bản đọc của vault.
- **Giữ lại**: note, người, task (thứ mà khoảnh khắc được đọc *ra từ* đó), cài đặt
  `Timeline/extract.json` (loại, model, thư mục), sổ bằng chứng
  `Timeline/ledger/`, và **bản ghi lời thoại** trong file tháng — mỗi bản là vài phút model
  chạy, và nó nói về một đoạn ghi âm chứ không phải về đời ai. File tháng được **ghi lại**
  không còn khoảnh khắc và lần đọc, chứ không bị xoá.
- Chỗ bấm: cuối phần cài đặt trong tray, viền đỏ, hai lần bấm.

**Gỡ niêm phong (2026-09-23).** Rà lại thì tính năng này đứng dở giữa chừng: nửa "niêm phong
một note hoặc một người" bấm được và được tôn trọng khắp nơi, còn nửa "niêm phong một khoảng
thời gian" có đủ lệnh backend nhưng **không có một dòng giao diện nào gọi tới** — không tạo
được từ trong app. Vault thật có 0 note, 0 người và 0 khoảng nào bị niêm phong.

Chủ vault chốt gỡ hết. Đã gỡ: module `timeline::seal`, hai lệnh `seal_period` và
`remove_seal`, mọi bộ lọc theo niêm phong trong bộ đọc, truy vấn, trợ lý Syn, nhắc nhở, lịch,
*ngày này năm xưa*, khung thời gian và thư viện ảnh, cùng nút bấm trong app Note và app People
và các chuỗi ngôn ngữ đi kèm. 27 test chỉ để chứng minh niêm phong cũng đi theo.

**Cái còn lại**: *hush* (`timeline::quiet`) — im lặng về một người, một ngày, một câu — vẫn
nguyên vẹn, vì nó là tính năng khác và vẫn dùng được. Hàm đọc khoảng thời gian viết tay mà
seal cho hush mượn đã chuyển về `quiet.rs`.

**Hệ quả cần biết**: từ nay `sealed: true` trong frontmatter không còn nghĩa gì. Note nào từng
được đánh dấu như vậy sẽ được bộ đọc và trợ lý đọc bình thường.

---

## 14. Chưa quyết

*Đã chốt 2026-09-23: mỗi khoảnh khắc là một file `Moments/<uuid>.md`, không hiện trên đồ
thị và tab Node (§15.3).*

1. ~~Danh sách `category` ở §6 đã đủ chưa.~~ **Chốt 2026-09-23: danh sách nằm trong vault.**
   Đo trên 37 khoảnh khắc đang giữ: 22 là ăn uống, 3 không có loại nào hợp (*Bụi lưỡi hổ nở
   hoa*, *Hà Nội nắng nóng kỷ lục*, *Một trận mưa làm ngập thành phố*), và 4 loại chưa dùng
   tới lần nào. Nên danh sách không nằm trong code nữa: mặc định vẫn là 11 loại cũ, còn thêm
   bớt là việc của chủ vault — trong màn duyệt (*thêm loại…* ngay ở ô chọn) hoặc trong phần
   cài đặt. Chưa làm: đổi **tên** một loại đã dùng thì các file khoảnh khắc cũ vẫn giữ tên cũ;
   cần một lần đổi hàng loạt như `rename_property` đang làm cho khoá frontmatter.
2. ~~Task đã xong: đưa hết vào túi ngày để model tự chọn.~~ **Chốt 2026-09-23: task là một
   node được đọc**, không phải một dòng ngữ cảnh. Tên task là một khối; ngày khung là ngày
   nó xong (task còn mở là kế hoạch, không có ngày nào của riêng nó). Trên vault thật: 126
   task, tất cả đã xong, thân tổng cộng 7 KB. Thêm 27 túi — những ngày chỉ có task mà không
   có chữ nào — và +10 KB, tức hơn một phần tư số lần gọi model là để đọc một ngày toàn việc
   vặt. Nếu hàng chờ đầy việc công ty thì siết lại bằng cách bỏ task khỏi `wanted`.
3. ~~Tự giữ lịch hẹn đã diễn ra.~~ **Chốt 2026-09-23: lịch hẹn cũng là node được đọc.**
   Tên lịch hẹn kèm giờ là một khối, nên nó đi qua cùng cửa duyệt như mọi thứ khác; không
   có tính năng tự giữ, và không có công tắc nào phải nhớ. Vault hiện có đúng 1 lịch hẹn.
4. ~~Ngày sinh của người thân.~~ **Chốt 2026-09-23: là fact.** Không lên dòng thời gian, vẫn
   ở trang người đó, vẫn có nhắc nhở và vẫn hiện trong *ngày này năm xưa*. Sinh nhật đáng nhớ
   của một năm cụ thể thì đến từ chữ viết hôm đó, như mọi khoảnh khắc khác.

---

## 15. Khi nguồn của một khoảnh khắc đã giữ bị sửa

### 15.1 Hiện tại

Không có gì xảy ra. Khoảnh khắc đã giữ nằm trong `moments[]` ở frontmatter của note gốc, và
`derive.rs` cứ thế đọc nó ra; không có đoạn code nào đối chiếu nó với câu đã sinh ra nó.

- Sửa *"họp với chị Yến"* thành *"họp với chị Yến và Đức"*: khoảnh khắc vẫn chỉ có chị Yến.
- Xoá câu vì nó sai: khoảnh khắc vẫn nằm trên dòng thời gian, không còn gì chống lưng.
- Xoá cả note: mọi khoảnh khắc lưu trong note đó mất theo, không báo gì.

### 15.2 Đề xuất sửa, không tự sửa

Khoảnh khắc đã giữ là **quyết định của người dùng**; nguồn là **bằng chứng**. App không bao
giờ âm thầm đổi quyết định, nhưng cũng không im lặng khi bằng chứng đổi.

Khi một khối có khoảnh khắc đã giữ bị sửa, app đọc lại khối đó, **đưa kèm khoảnh khắc hiện
tại cho model**, và hỏi: chỗ sửa này có làm khoảnh khắc khác đi không? Model trả về một
trong ba: không đổi; đổi các trường này thành giá trị này; câu không còn chống lưng cho
khoảnh khắc nữa.

| Kiểu sửa | Ví dụ | Thẻ trong màn duyệt |
| --- | --- | --- |
| Sửa chính tả | "hop" → "họp" | không có thẻ |
| Bổ sung | thêm "và Đức" | *"Nguồn đã đổi: thêm Đức vào người tham gia?"* · Áp dụng · Giữ nguyên |
| Sửa sai | "hôm qua" → "hôm kia"; "thất bại" → "thành công" | chênh lệch từng trường: ngày cũ → mới, tiêu đề cũ → mới |
| Rút lại | xoá câu; viết thêm "thật ra không đi" | *"Câu gốc không còn"* · Giữ khoảnh khắc · Xoá khoảnh khắc |

Thẻ đề xuất sửa nằm cùng hàng với đề xuất mới, cùng phím tắt.

**Trường đã sửa tay thì không bị đề nghị ghi đè.** Mỗi khoảnh khắc nhớ trường nào người dùng
đã sửa (`hand` ở §8.1). Người dùng đã đổi tiêu đề thành "Họp chốt UAT" thì về sau nguồn có
đổi thế nào, app cũng không đề nghị đổi tiêu đề nữa — chỉ các trường vẫn do model điền.

### 15.3 Khoảnh khắc là một file riêng — **đã chốt 2026-09-23**

Lưu trong frontmatter của note gốc buộc vòng đời của khoảnh khắc vào vòng đời của note: xoá
note là xoá khoảnh khắc. Nên mỗi khoảnh khắc thành **một file của riêng nó**:

```
Moments/
  6f3c9a2e-….md      ← type: moment; tiêu đề, ngày, người… nằm trong frontmatter
  b81d04c7-….md
  …                  ← phẳng, không chia thư mục
```

**Tên file là UUID**, như mọi node app tạo ra (`nodes.rs`: `Uuid::new_v4()`). Lý do: id của
node **là đường dẫn file** (`node_parser.rs`: `id: rel_path`). Tên theo tiêu đề hay ngày thì
mỗi lần sửa tiêu đề hay sửa ngày — việc xảy ra thường xuyên khi duyệt và khi áp đề xuất sửa
(§15.2) — là đổi id: phải sửa link khắp vault, sửa `timeline.db`, và sync thấy một lần xoá
cộng một lần tạo. Thêm vào đó: tiêu đề trùng nhau rất nhiều (*"Ăn trưa với Nga"* mỗi tuần),
và tiêu đề tiếng Việt dễ chứa `/`, `:`. **Không chia thư mục theo năm** vì cùng lý do: ngày
có thể bị sửa. Đường dẫn đã là mã ổn định, nên không cần trường `id` riêng.

Cái mất: mở Finder không đọc được tên. Note hiện tại cũng đã vậy; khoảnh khắc được đọc qua app.

**Khoảnh khắc không phải node của đồ thị.** Đồ thị và tab Node là **node gốc** — những gì
người dùng viết ra. Khoảnh khắc là thứ **trích ra từ** đám đó; đặt nó lên đồ thị thì mỗi
note có thêm vài nút con lặp lại chính nội dung của nó, và đồ thị thôi còn là bản đồ của
vault. Nên nó là file để có vòng đời riêng, không phải để thành một nút.

| | Trong note gốc (trước đây) | `Moments/<uuid>.md` |
| --- | --- | --- |
| Là file markdown của người dùng, sync được | có | có |
| Xoá note gốc | khoảnh khắc mất theo | khoảnh khắc ở lại, kèm thẻ *"nguồn đã bị xoá"* |
| Khoảnh khắc tự ghi | ghi vào daily note của ngày đó | một file như mọi khoảnh khắc khác |
| Sửa, xoá | sửa frontmatter của một note khác | sửa, xoá chính file đó |
| Hai máy cùng giữ khoảnh khắc | đụng nhau ở note gốc | hai file khác nhau, không đụng |
| Viết thêm về khoảnh khắc | không có chỗ | thân file |
| Trên đồ thị, tab Node | không | **không** |

**Hệ quả phải xử lý**

- **Loại `type: moment` khỏi mọi thứ nhìn node gốc**, một lần, ở một chỗ: bộ đọc (§4.4), đồ
  thị, tab Node của ô tìm kiếm (FTS `search_nexus` và nguồn truy vấn `nodes`), danh sách
  note. Khoảnh khắc chỉ đi ra qua nguồn `moments`: tab Khoảnh khắc và dòng thời gian.
  Loại theo `type`, không theo thư mục — file bị kéo ra khỏi `Moments/` vẫn là khoảnh khắc.
  Không loại ở bộ đọc thì mỗi khoảnh khắc đã giữ lại sinh ra một đề xuất trùng chính nó.
- **Cạnh**: `people`, `where`, `about`, `source` của khoảnh khắc **không** thành cạnh trong đồ
  thị node. Chúng là liên kết của `timeline.db` (`event_links`), như hiện nay.
- **`derive.rs`** đọc khoảnh khắc từ file `type: moment` thay vì từ `moments[]`. `timeline.db`
  vẫn chỉ là chỉ mục dựng lại được từ vault.
- **Chuyển dữ liệu**: 37 mục `moments[]` hiện có, mỗi mục thành một file `Moments/<uuid>.md`;
  `source.node` là note đang chứa nó. Sau khi chuyển, `moments[]` bị gỡ khỏi note cũ. Chạy một
  lần, có bản ghi những gì đã chuyển; `derive.rs` đọc được cả hai khuôn trong lúc chuyển.
- **Đọc ngược**: mở một note thì thấy các khoảnh khắc có `source.node` trỏ về nó, trong
  một ô riêng dưới note — không phải như backlink của đồ thị.

Tinh thần của Nhát E — khoảnh khắc là **file của người dùng**, không phải một kho riêng của
app — được giữ: `Moments/` là markdown trong vault. Cái đổi là nó không còn chung số phận với
note đã sinh ra nó.

---

## 16. Khoảnh khắc là một quãng dài

### 16.1 Hiện tại

Tầng dữ liệu hỗ trợ quãng thời gian: mỗi dòng có `happened_from` và `happened_to`, có hình
dạng `Spell`, và `when:` lọc theo giao nhau. **Tầng hiển thị thì không**: `within()` và biểu
đồ (`overTime.ts`, `EventsOverTime.vue`) chỉ nhìn ngày bắt đầu, nên

- *Học đại học 2009–2013* là một chấm ở năm 2009;
- *Làm ở MDP, từ 03/2025, vẫn đang làm* biến mất khỏi khung *1 năm* gần đây, dù nó đang
  diễn ra ngay lúc này.

Lỗi này chưa lộ vì 37 khoảnh khắc hiện có đều là ngày đơn.

### 16.2 Khuôn

```json
{ "title": "Học đại học Bách Khoa", "date": "2009", "date_to": "2013", "precision": "year" }
{ "title": "Làm việc ở MDP", "date": "2026-03", "date_to": null, "ongoing": true, "precision": "month" }
```

Quãng dài hiếm khi nằm trong daily note; nó đến từ note viết về bản thân, note kiểu CV, hoặc
do người dùng tự ghi. Form tự ghi có tuỳ chọn *"đây là một quãng"*, với hai đầu và ô
*"vẫn đang diễn ra"*.

### 16.3 Hiển thị

Quãng là **một hình dạng khác**, không phải một điểm:

```
Quãng    ▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▶  Làm ở MDP (từ 03/2026, đang diễn ra)
         ▬▬▬▬▬▬▬▬▬▬▬  Setup trung tâm giám sát
Cột        ▂▅█▃      ▁▂       ▇▆▂         ← chỉ đếm khoảnh khắc là một điểm
Chấm     · ·•·•• ·   ·  ·     ••·•·
```

- Thanh ngang ở **một làn riêng** phía trên cột. **Không cộng vào cột đếm**: cộng vào thì
  *học đại học* thêm 1 vào mỗi tháng suốt bốn năm, và cột thôi còn nghĩa.
- Danh sách: quãng hiện **một lần**, nhãn *"2009 – 2013"*. Khi xem một khung, các quãng
  **đang diễn ra trong khung** được ghim ở đầu như ngữ cảnh: *"Trong khoảng này: đang làm ở MDP"*.
- **Khung thời gian lọc theo giao nhau**, như bộ truy vấn. Sửa được ngay (§13 bước 1).

### 16.4 Quãng làm ngữ cảnh cho model

Khi đọc một túi ngày nằm trong một quãng, model được biết quãng đó: đọc ngày năm 2011 thì
biết người viết đang học đại học, nên *"thi cuối kỳ"* được hiểu đúng và gắn `about` vào quãng.


---

## 17. Cài đặt đọc ở đâu — **đã sửa 2026-09-23**

Trước đó: cài đặt đọc, danh sách loại, phụ đề ảnh và **nút xoá sạch dòng thời gian** nằm
trong một `<details>` gập ở đáy màn duyệt. Muốn xoá dòng thời gian thì phải bấm *"Đề xuất
khoảnh khắc"* trước — một cái nhãn nói về hàng đợi, không nói gì về cài đặt hay xoá. Tên cửa
không khớp thứ sau cửa, nên người dùng phải đoán. Và trước lần sửa hôm qua (`c73a751`), cửa
ấy chỉ hiện khi có đề xuất đang chờ — tức là đúng lúc hàng đợi trống, lúc người ta muốn đổi
cách đọc hoặc làm lại từ đầu, thì cửa biến mất.

Chỗ đúng của một việc hiếm và phá huỷ là chỗ người ta đi tìm việc hiếm và phá huỷ: **Cài đặt
→ Dòng thời gian**, ngay cạnh Thùng rác của vault.

- `src/shared/components/TimelineSettings.vue` — bật/tắt đọc, cảnh báo gửi đi đâu, chạy một
  lượt, danh sách loại, phụ đề ảnh, và ô đỏ *"Đập đi xây lại"*. Tab riêng trong
  `SettingsModal`, nạp khi mở.
- `src/shared/timelineReading.ts` — các khuôn dữ liệu dùng chung, cộng `withKind()` và
  `saveKinds()`: bộ chọn loại trong màn duyệt vẫn thêm được một loại ngay tại chỗ thiếu nó,
  và thêm theo đúng quy tắc của danh sách (cắt trắng, chữ thường, `other` cuối).
- Màn duyệt giữ **mỗi hàng đợi**. Hàng đợi trống thì nói thẳng và chỉ đường sang cài đặt.
- Thanh công cụ dòng thời gian có nút cài đặt riêng bên cạnh cửa duyệt, mở thẳng đúng tab.
- `openSettings(tab?)` nhận tên tab, để chỗ khác trong app gửi người dùng tới đúng một việc.

---

## 18. Vì sao Nexus chậm — **đo và sửa 2026-09-23**

`timeline_extract_status` là **một** lệnh, và nó giữ kết nối vault suốt thời gian chạy. Đo
trên vault thật (1016 node, 292 node được đọc, 412 tài liệu có lịch sử):

| Phần | release | debug (`tauri dev`) |
| --- | --- | --- |
| `extract::load` | 17 ms | 18 ms |
| `Directory::read` | 8 ms | 10 ms |
| `sources` **không** lịch sử | 154 ms | 1 730 ms |
| `sources` **có** lịch sử | 713 ms | 5 200 ms |
| `plan` | 11 ms | 60 ms |
| `changes`, `moments::kept`, `days` | ~0 ms | ~5 ms |

Hai phần ba là `History::of` — replay từng phiên bản của từng note bằng Loro. Không bỏ được:
`standing()` đọc lịch sử để biết *"khối này là bản sửa của khối đã đọc"*, cho **mọi** loại
note, không chỉ note ghi theo ngày. (Đã thử bỏ cho note một-ngày: sai ngay — `first_seen`
rỗng làm mọi khối của note đó thành `ReadBefore`, 56 túi tụt còn 46.)

Phần còn lại là `blocks::split` + băm, không phải SQLite: đổi câu SQL để chỉ lấy `content`
của các node thật sự đọc (vault này có 9 MB `json` không bao giờ đọc) cho ra **154 ms so với
155 ms** — không khác gì. Bỏ.

Cái sai không nằm ở phép tính, nằm ở **bao nhiêu lần tính**:

1. `NexusApp` gọi lệnh này ngay khi mount, **song song** với `loadAllData()` — nên đồ thị và
   danh sách xếp hàng sau một con số trên cái nút. Giờ gọi sau: `loadAllData().then(loadProposals)`.
2. Mỗi lần mở dòng thời gian hay mở cài đặt lại tính lại từ đầu, dù không có gì được ghi.
   `TimelineStore` giữ lại kế hoạch đọc, khoá theo `Planned`: `sqlite3_total_changes` của
   kết nối vault **và** của kết nối timeline (cùng con số `is_current` dùng — mọi lệnh ghi
   trong app đi qua một trong hai, nên không sót), cộng nội dung `Timeline/extract.json`, số
   file quyết định + mtime mới nhất, và ngày hôm nay. Khớp thì trả lại kế hoạch cũ, không
   chạm vault.
3. Màn cài đặt trống trơn trong lúc chờ. Giờ nó vẽ ngay khung và một dòng *"Đang xem lại
   vault…"* — một panel không hiện gì trong một giây đọc như một panel hỏng.

---

## 19. Xoá xong mà màn hình vẫn còn — **sửa 2026-09-23**

Bấm *"Đập đi xây lại"* trong Cài đặt thì trên đĩa xoá thật: `Moments/` rỗng, file nằm trong
`.trash/Moments`, `Timeline/reviews/` rỗng, mọi file tháng còn `items: 0`, `extract_runs` và
`events WHERE source='extract'` về 0. Nhưng quay lại Nexus vẫn thấy đủ cột, đủ danh sách, và
huy hiệu *"90 đề xuất"*.

Vì **màn duyệt và màn dòng thời gian là cùng một cây component, còn cài đặt thì không**. Giữ
một đề xuất trong màn duyệt thì `ExtractTray` phát `changed` lên `NexusApp` ngay bên trên nó;
xoá cả dòng thời gian trong Settings thì không có ai ở trên để nghe. `reload` của Nexus cũng
không giúp: nó nạp lại phía node, còn `wholeTimeline` và số đề xuất nằm riêng.

Thêm một tên trên event bus: `timeline:changed` — *"thứ dòng thời gian đang giữ đã khác"*.
`TimelineSettings` phát nó sau khi xoá và sau mỗi lượt đọc; `NexusApp` nghe và gọi
`eventsChanged()`. Đây là cái giá của việc tách cài đặt ra khỏi màn duyệt (§17), và nó rẻ.


### 18.1 Tách một lệnh thành hai, rồi đếm rẻ hẳn — 2026-09-23

Memo (nhớ lại kế hoạch cho tới khi có gì được ghi) không cứu được **lần đầu**, và lần đầu
chính là lúc người ta mở màn cài đặt. Câu hỏi đúng là: *vault 10 000 node thì sao?*

Đo thật — nhân vault lên 13 339 node (9 052 node được đọc), bản release:

| Phần | 1 016 node | 13 339 node |
| --- | --- | --- |
| `Directory::read` | 8 ms | 81 ms |
| `sources` (cắt block) | 154 ms | 4 747 ms |
| lịch sử Loro | +480 ms / 412 tài liệu | ~1,2 ms mỗi tài liệu |
| `plan` | 11 ms | 7 500 ms |
| **`left_to_read`** | **7 ms** | **56 ms** |

Hai chỗ tệ khác bản chất: `sources` tuyến tính theo lượng chữ; `plan` thì **siêu tuyến
tính** — `bag()` gọi `directory.in_text()` cho từng túi, quét mọi người trong danh bạ qua
toàn bộ chữ của túi, nên giá ≈ *số túi × số người*. (Chưa sửa; nó chỉ trả giá khi bấm Đọc.)

Điều sai về tỉ lệ: màn cài đặt hiện **một dòng** — *"còn N ngày chưa đọc, ~M phút"* — mà
phải dựng nguyên kế hoạch ở mức block. Nên tách hẳn ra hai câu hỏi:

- `timeline_extract_status` — đọc bật chưa, gửi đi đâu, có loại nào, chờ duyệt mấy cái. Từ
  bảng timeline + chỉ mục vault, vài chục ms.
- `timeline_reading_left` (mới) — `reader::left_to_read`: hai câu truy vấn, **không mở note
  nào**. Ngày nào có chữ (`extract::recorded` + `length(content)`), trừ ngày đã có lượt đọc
  (`extract_runs.node_id` là `day:2026-07-21`).

**Nó đánh đổi cái gì** (ghi trên `reader::Left`): một ngày là đã đọc hoặc chưa, ngày đọc dở
bị tính là đã đọc; và một note tính vào ngày nó *nói về*, trong khi kế hoạch thật date từng
block theo lịch sử nên một note có thể rải ra nhiều ngày. Đây là **con số đang chờ, không
phải lời hứa về việc đọc sẽ làm gì**. Bấm Đọc thì kế hoạch đầy đủ mới được dựng — lúc đó
vòng quay là xứng đáng.

Kéo theo: nút Đọc không còn bị khoá theo con số (nó không chờ một phép đếm mới bật được), và
dòng "N thay đổi đang chờ" bỏ khỏi cài đặt — thay đổi (§15) chỉ tìm ra được bằng kế hoạch
đầy đủ, và sau một lượt đọc thì nó nằm trong hàng đợi duyệt rồi.

Memo `store::Planned` **đã gỡ**: không còn ai gọi nó nữa.

**Hai giả thuyết sai, ghi lại để khỏi đoán lại:**

1. *Bỏ replay lịch sử cho note một-ngày.* Sai: `standing()` đọc lịch sử để biết "khối này là
   bản sửa của khối đã đọc", cho mọi loại note. Bỏ đi thì `first_seen` rỗng → mọi khối thành
   `ReadBefore`, 56 túi tụt còn 46.
2. *`load_or_register_vault_identity` ghi lại dòng mapping mỗi lần gọi nên memo không trúng.*
   Viết test để chứng minh thì **test pass cả với code cũ**: `insert_sync_vault_mapping` là
   `INSERT … ON CONFLICT DO NOTHING`. Sửa theo giả thuyết đó đã revert.

Cả hai đều bị số liệu bác, không phải bị đọc code bác. Đó là lý do phép đo
`where_the_time_goes_asking_what_is_waiting` (`#[ignore]`, chạy tay với `SYN_EVAL_CACHE`)
được giữ lại trong repo.
