# Thấu kính — một cách hỏi cho cả graph lẫn dòng thời gian

**2026-09-19.** Thiết kế trải nghiệm cho Nexus, thay cho việc thêm panel.

---

## 1. Một câu

Thay vì mỗi câu hỏi là một panel viết tay, **một câu hỏi là một truy vấn, một truy
vấn lưu lại được gọi là một thấu kính, và cùng một thấu kính có ba đường để tạo ra:
bấm, hỏi bằng lời, hoặc gõ.**

---

## 2. Điều phải sửa trước: tao đã chẩn đoán sai

Ở `docs/timeline-2026-09-17.md` §16 Bước 2 tao viết rằng nhật ký ăn uống là nhiễu và
đề nghị **một cửa thứ năm** chặn thứ lặp lại hằng ngày.

**Sai.** Chủ vault nói rõ: bữa ăn, chi tiêu, cuộc họp và khoảnh khắc xúc động **đều
là sự kiện trên dòng thời gian**. Mục tiêu là một **database đầy đủ về đời một
người**. Một database cố ý bỏ sót thứ lặp lại là một database nói dối về nhịp sống —
và chính cái nhịp ấy mới trả lời được "tao ăn ngoài bao nhiêu lần năm nay", "tháng
nào tao họp nhiều nhất", "từ khi nào tao thôi nấu cơm".

Chỗ cần tinh tế **không phải lúc ghi vào, mà lúc hiện ra**. Ghi thì tham lam, hiện
thì chọn lọc. Cả tài liệu này tồn tại vì ranh giới đó.

> Cửa thứ năm **bị huỷ**. Bốn cửa của §5.4 giữ nguyên — chúng lọc thứ *không có thật*
> (không trích được câu, không có ngày), không lọc thứ *tầm thường*.

---

## 3. Cái đã có, và chỗ nó đứt

Đây là phát hiện quyết định hình dạng của thiết kế này: **một nửa những gì cần đã tồn
tại trong code, và nửa kia chưa từng gặp nó.**

| | ngôn ngữ hỏi | cách vẽ kết quả | lưu lại được | ai dùng được |
| --- | --- | --- | --- | --- |
| **`nodes`** (graph) | ✅ `ParsedQuery`: `is:`, `#tag`, `status:`, `date:`, `prop:>x`, `-prop:y`, `sort:`, `columns:`, `limit:` | ✅ `ListView` / `TableView` / `ObjectDetail`, có **hợp đồng** viết rõ: nhận kết quả, không tự gọi; không rẽ nhánh theo type | ❌ **không chỗ nào** | Things app dựng truy vấn bằng cách bấm |
| **`events`** (timeseries) | ❌ không có — chỉ hàm viết tay | ❌ **8 panel hàn cứng** | ❌ | không ai |
| **`node_edges`** (graph) | ❌ | GraphView | ❌ | — |

Tám panel tao vừa viết **chính là triệu chứng**. Dòng thời gian không có ngôn ngữ
hỏi, nên mỗi câu hỏi mới tốn một module Rust + một component Vue + hai file i18n +
một lệnh Tauri. Đó đúng là *"fix cứng và nhồi nhét"*.

Và hai kho nằm **cùng một thư mục** (`vault_cache.db`, `timeline.db`), nên `ATTACH`
là chuyện một dòng. Chỗ đứt là khái niệm, không phải hạ tầng.

---

## 4. Thấu kính

**Một thấu kính là một câu hỏi đã lưu, kèm cách nó muốn được nhìn.**

```yaml
---
type: lens
title: Những lần gặp Khánh
query: with:khanh when:2019/2026 | sort:-when
render: strip
icon: users
---
```

Bốn điều làm nó khác một "saved search" thông thường:

1. **Thấu kính là một node.** Nó nằm trong vault, sync như mọi thứ khác, gắn tag
   được, tìm kiếm được, **hiện trên đồ thị**, chia sẻ được. Có thể có một thấu kính
   đi tìm thấu kính.
2. **Cách vẽ là thuộc tính của câu hỏi**, không phải của màn hình. Cùng một truy vấn
   xem dưới dạng dải, bảng, đồ thị hay một con số — và nó nhớ mày thích dạng nào.
3. **Sửa được.** Bộ thấu kính có sẵn không phải hộp đen: mở ra, thấy câu truy vấn,
   đổi một chữ. Đây là cách người non-tech học ngôn ngữ mà không ai dạy.
4. **Không có đặc quyền.** Panel có sẵn và thấu kính mày tự viết chạy qua **cùng một
   đường**. Nếu một tính năng gốc làm được điều mà thấu kính không làm được, thiết kế
   này hỏng.

---

## 5. Ngôn ngữ: lọc, rồi biến đổi

Cú pháp lọc **giữ nguyên `ParsedQuery` đang có** — không phát minh lại thứ đã chạy và
đã có test. Thêm ba nhóm.

### 5.1 Từ khoá của dòng thời gian

```
when:2019-03            when:2019/2026        when:last-year
with:khanh              where:hanoi           about:synabit
shape:occasion          magnitude:>4          has:quote
```

`with` / `where` / `about` là **bốn vai** của §4.2 — thứ đã có trong lược đồ mà chưa
ai hỏi tới được.

### 5.2 Ống dẫn

Đây là chỗ mượn Splunk, và mượn có chọn lọc:

```
<lọc> | <biến đổi> | <biến đổi> …
```

Sáu phép biến đổi, và **không phép nào được nghĩ ra từ trí tưởng tượng** — mỗi phép
là thứ một panel hiện có đang làm bằng tay:

| Phép | Làm gì | Hôm nay là ai |
| --- | --- | --- |
| `count by <khoá>` | đếm theo tháng, theo người, theo tag | mật độ dải thời gian |
| `top <n> by <khoá>` | giữ n cái lớn nhất | zoom theo độ lớn (Bước 8) |
| `gaps` | chuỗi lần xuất hiện → các khoảng trống | khoảng lặng (Bước 5), chuyện gì đã xảy ra với (Bước 7) |
| `anniversary` | khớp ngày-tháng qua các năm | ngày này năm xưa (Bước 4) |
| `sentences` | nổ ghi chú thành từng câu trích được | một năm bằng lời mày (Bước 6) |
| `ask <n>` | **đưa model chọn**, không cho viết | một năm bằng lời mày (Bước 6) |

`ask` là chỗ model được phép bước vào ống dẫn, và nó **chỉ chọn**: nhận danh sách
đánh số, trả về số. Giao thức không có trường nào chứa văn xuôi — đúng như
`timeline::year` đang làm. Một phép biến đổi tốn tiền và tốn thời gian thì phải
**nhìn thấy được trong câu truy vấn**, không giấu trong một nút bấm.

### 5.3 Phép thử của thiết kế

Nếu tám panel hiện có không viết lại được thành thấu kính thì thiết kế này sai. Kiểm
từng cái:

| Panel | Thành truy vấn |
| --- | --- |
| Ngày này năm xưa | `anniversary shape:occasion has:quote` |
| Khoảng lặng | `with:* \| gaps \| where quiet > longest` |
| Chuyện gì đã xảy ra với | `about:* \| gaps \| where quiet > 6mo -has:ending` |
| Một năm bằng lời mày | `when:2026 \| sentences \| ask 15` |
| Zoom theo độ lớn | `when:<span> \| top 20 by magnitude` |
| Khoảnh khắc (ảnh) | `is:file when:<span>` |
| Khay duyệt | `is:proposal -decided` |
| Đã từ chối | `is:hush` + `is:pin` |

Tám trên tám. **Hai phép còn thiếu duy nhất là `gaps` và `sentences`** — phần còn
lại là lọc thuần.

---

## 6. Ba đường vào, một cỗ máy

Đây là câu trả lời cho *"người non-tech vẫn dùng dễ dàng"*, và nó không phải là "giấu
truy vấn đi".

### 6.1 Không gõ gì — thanh truy vấn là **biên lai**

Mọi thứ trên màn hình vốn đã là một bộ lọc:

- Bấm một người trên đồ thị → thêm chip `with:khánh`
- Kéo dải về 2019 → chip `when:2019`
- Bấm một tag → chip `#gia-đình`
- Bấm "chỉ chuyện lớn" → chip `magnitude:>4`

Thanh truy vấn hiện **đúng những chip ấy**, viết bằng chính ngôn ngữ trên. Người
non-tech không bao giờ phải đọc nó. Người kỹ thuật đọc, rồi bắt đầu sửa.

> Đây là chỗ then chốt: thanh truy vấn **không phải một tính năng riêng cho dân kỹ
> thuật**. Nó là biên lai của thứ vừa bấm. Người ta học `with:` giống cách người ta
> học `from:` của Gmail — bằng cách thấy nó xuất hiện sau khi mình bấm.

Ràng buộc bắt buộc: **chip và chữ phải đi được cả hai chiều.** Sửa chữ thì chip đổi
theo; bấm chip thì chữ đổi theo. Một câu truy vấn gõ tay mà không hiện lại được thành
chip là một câu làm hỏng đường số một.

### 6.2 Hỏi bằng lời — trợ lý **viết truy vấn**, không thay thế nó

Syn đã có `query_nodes` và `timeline`. Đổi một điều: trả lời xong thì **hiện cả câu
truy vấn nó đã dùng**, kèm nút *"Lưu thành thấu kính"*.

> *"cho tao xem những lần gặp Khánh năm ngoái"*
> → `with:khánh when:last-year | sort:-when` · 14 lần · **[Lưu]**

Trợ lý thành **người dạy** ngôn ngữ, không phải bức tường che nó. Và mọi câu nó viết
đều kiểm được — khác hẳn một câu trả lời bằng văn xuôi.

### 6.3 Gõ thẳng

Cho người như chủ vault. Ống dẫn đầy đủ, gợi ý khoá và giá trị theo ngữ cảnh, và
**xem trước số dòng khớp trước khi chạy** phép tốn tiền như `ask`.

---

## 7. Kết quả tự chọn hình dạng

Không panel nào được hàn cứng cách vẽ. Kết quả mang theo **hình dạng**, và cách vẽ
mặc định suy ra từ đó:

| Kết quả có | Vẽ mặc định |
| --- | --- |
| một con số | một câu: *"14 lần, lần cuối 14/3/2021"* |
| dòng có ngày | dải thời gian |
| dòng có hai cột node | đồ thị |
| dòng có nhiều cột | bảng |
| dòng có câu trích | danh sách trích dẫn, mỗi câu bấm về nguồn |
| dòng nhóm theo khoá | biểu đồ cột |

Đổi được bằng một cú bấm, và lựa chọn ấy lưu vào thấu kính. Đây là lý do **thêm câu
hỏi mới không còn tốn code**.

Hợp đồng của view primitive ở `src/shared/views/types.ts` đã viết sẵn luật đúng —
*nhận kết quả, không tự gọi; không rẽ nhánh theo type*. Thiết kế này chỉ mở rộng bộ
primitive, không sửa hợp đồng.

---

## 8. Cái kệ có sẵn

Một thanh truy vấn trống là một lời từ chối phục vụ. Vault mới đi kèm khoảng **mười
hai thấu kính**, và chúng **sửa được** — đó là giáo trình.

Tám cái đầu là tám panel hiện có. Bốn cái còn lại chính là thứ database đầy đủ mở ra
mà hôm nay chưa hỏi được:

- **Nhịp sống** — `is:note | count by month` — tháng nào viết nhiều, tháng nào im
- **Ăn ở đâu** — `#ăn | count by where` — thứ chỉ có nghĩa **nhờ** đã ghi từng bữa
- **Tiền theo tháng** — `is:transaction | count by month`
- **Ai còn gặp** — `with:* | count by person | sort:-count`

Bốn cái này là bằng chứng cho luận điểm §2: **ghi tham lam thì mới hỏi được sâu.**
Không ghi từng bữa thì không có "Ăn ở đâu".

---

## 9. Cái này thay thế cái gì

- Tám panel → tám thấu kính. `timeline_on_this_day`, `timeline_silences`,
  `timeline_ask`, `timeline_year` thôi làm lệnh riêng.
- Things, Task, Note đang mỗi nơi dựng truy vấn một kiểu → cùng một cỗ máy.
- Hàng công cụ tám nút → **một thanh truy vấn và một kệ thấu kính**.

Code ít đi, không nhiều lên.

---

## 10. Nói thẳng về giá phải trả

1. **`ParsedQuery` hôm nay không có ống dẫn, không chạm được `events`, không biết
   vai.** Đây là phần lớn công việc — nhưng là mở rộng một thứ đã có test, không phải
   viết lại.
2. **Hai database phải nối.** `ATTACH` được vì cùng thư mục, nhưng
   `events` là tầng 3 dựng lại được còn `nodes` là bản sao của vault: một truy vấn
   bắc qua cả hai phải chịu được lúc một bên đang dựng lại.
3. **Ngôn ngữ là một cam kết vĩnh viễn.** Thấu kính đã lưu trong vault của người ta
   thì cú pháp không đổi được nữa. Phải chốt ít và chốt chắc.
4. **Rủi ro lớn nhất không phải kỹ thuật.** Nếu thanh truy vấn khiến việc bấm trở nên
   khó hơn, người non-tech mất nhiều hơn người kỹ thuật được. **Đường số một phải
   dùng được trọn vẹn với thanh truy vấn cuộn lại và không ai mở ra bao giờ.**
5. **`ask` tốn tiền.** Phải hiện rõ trong truy vấn, phải xem trước được số dòng, và
   không bao giờ chạy ngầm.

---

## 11. Thứ tự nếu làm

Mỗi bước dùng được ngay và lùi lại được, như §16 của tài liệu Timeline.

| | Việc | Xong thì làm được gì |
| --- | --- | --- |
| 1 | ~~`events` vào được `ParsedQuery`~~ — **xong 2026-09-19** | hỏi được dòng thời gian bằng chữ lần đầu tiên |
| 2 | ~~Thấu kính là một node + kệ thấu kính~~ — **xong 2026-09-19** | lưu được một câu hỏi |
| 3 | ~~Thanh truy vấn hai chiều với chip~~ — **xong 2026-09-19** | bấm và gõ thành một |
| 4 | ~~Kết quả tự chọn hình dạng~~ — **xong 2026-09-19** | câu hỏi mới không tốn code |
| 5 | ~~Ống dẫn: `count by`, `top n by`~~ — **xong 2026-09-20** | thống kê, nhịp sống |
| 6 | ~~`gaps`, `anniversary`, `sentences`, `ask`~~ — **xong 2026-09-20** | bốn panel cảm xúc thành thấu kính |
| 7 | ~~Syn trả lời kèm truy vấn + nút Lưu~~ — **xong 2026-09-20** | đường vào cho người không gõ |
| 8 | ~~Bỏ panel, thay bằng kệ có sẵn~~ — **xong 2026-09-20** | code ít đi |

> **Bước 5 và 6 làm ở tài liệu khác.** Ngôn ngữ hoá ra cần sửa trước khi xây thêm lên
> nó, nên §15.3 của `query-grammar-2026-09-20.md` nuốt cả hai bước này. Cú pháp cuối
> cùng khác chỗ viết ở đây — xem §16.

**Nếu chỉ làm được một bước:** làm **bước 1**. Nó là chỗ đứt thật sự — mọi thứ còn
lại chỉ là cách bày ra thứ bước 1 mở khoá.

**Gate cho cả tài liệu này:** một câu hỏi mà hôm nay cần một panel mới, sau khi xong
phải trả lời được **không thêm một dòng Rust hay Vue nào**.

---

## 12. Bước 1 — đã làm, 2026-09-19

Sáu từ khoá vào `ParsedQuery`, một bộ chạy trên `events` ở `timeline::query`, và
**cùng một lệnh** `run_node_query` định tuyến giữa hai bên.

**Không có cửa thứ hai.** Giao diện vẫn gọi đúng lệnh cũ với đúng tham số cũ; câu hỏi
nào không mang từ khoá dòng thời gian thì chạy y như trước. Từ khoá **chính là bộ
chọn bảng**, và đó không phải mẹo: hỏi ai có mặt, hỏi chuyện đó lớn cỡ nào — chỉ
*sự kiện* mới trả lời được, nên viết một trong sáu từ ấy đã là nói rõ hỏi bảng nào.

**Cùng một `QueryResult`.** Đây là chỗ quyết định: bộ chạy mới trả về đúng struct mà
`run_node_query` vẫn trả, nên mọi view đang vẽ được truy vấn node thì vẽ được truy vấn
dòng thời gian **mà không cần biết sự kiện là gì**.

Một thứ phải thêm: `QueryRow.open`. Id của node **chính là** thứ để mở; id của sự
kiện là `path#kind#n` và mở ra không có gì — nhưng nó phải giữ nguyên làm id vì view
dùng id làm khoá, mà một ghi chú chứa nhiều sự kiện. Nên id giữ của sự kiện, `open`
mang ghi chú. Node để trống, nghĩa là "mở chính id". Người gọi viết `row.open ?? row.id`
một lần và không view nào phải biết nó nhận loại nào.

**Vault thật bắt lỗi ngay ở câu hỏi thứ tư.** `when:2026 ăn` trả về 15 kết quả, ba cái
đầu là *«công **văn**»* và *«Bùi **Văn** Phương»*. Khớp chuỗi con biến mọi từ ngắn
thành ký tự đại diện trong tiếng Việt. Nay so khớp trên một bản tiêu đề đã đệm khoảng
trắng và ép dấu câu thành khoảng trắng, nên `% ăn %` là một **từ**. Đo lại: `văn` → 5
đúng, `ăn` → 0 đúng (mấy dòng bữa ăn còn là đề xuất chờ duyệt).

Hai lỗi nhỏ hơn cũng do chạy thật mới lộ: dấu nháy đơn trong danh sách dấu câu làm vỡ
chuỗi SQL, và biểu thức ép dấu câu bị lặp lại cho **từng** từ — giờ dựng một lần thành
cột dẫn xuất.

**Còn lại của bước 1:** trợ lý vẫn chỉ hỏi được `nodes` (`query_nodes` gọi thẳng
`db.run_node_query`). Cho nó đi qua cùng bộ định tuyến là việc của bước 7.

---

## 13. Bước 2 — đã làm, 2026-09-19

**Gần như không cần Rust, và đó là thiết kế tự kiểm.** Một thấu kính là một node, nên:

- **lưu** = tạo một node (`write_node_file`, đã có từ lâu)
- **liệt kê** = `is:lens columns:title,query,render,icon` — **một truy vấn thường**
- **bỏ** = `trash_node_file`, tức là vào thùng rác chứ không mất

Kệ thấu kính được dựng bằng **chính cỗ máy mà kệ ấy phục vụ**. Nếu lưu một câu hỏi
cần một bảng mới, một lệnh mới và một đường sync mới, thì đó là thiết kế đang báo nó
sai.

Rust chỉ đụng một chỗ, và là chỗ đáng: `lens` phải nằm trong danh sách "đồ đạc của
app" chứ không phải chuyện của đời người. Ba câu hỏi khác nhau cần biết điều đó —
*không bao giờ có ngày*, *phong ấn có phủ không*, *có đáng hỏi không* — và cả ba đang
chép lại cùng một lõi. Nay lõi ấy là `derive::is_the_apps_own`, ba câu hỏi giữ phần
ngoại lệ riêng của mình. Đó đúng là cái mùi Bước 9 đi dọn, gặp lại ở tầng node.

**`TableView` vẽ được câu trả lời của dòng thời gian mà không ai dạy nó.** Component
ấy viết cho ghi chú, từ rất lâu trước khi `events` có ngôn ngữ hỏi. Có test khẳng định
đúng điều đó, vì nó là luận điểm mà cả thiết kế đứng trên.

**Chưa có gì được ưu ái.** Không có mục "thấu kính có sẵn" mà người dùng không sửa
được; thấu kính ship kèm vault và thấu kính viết sáng nay là **cùng một loại**, nằm
cùng một hàng. Khoảnh khắc một cái thành đặc biệt là khoảnh khắc quay lại làm panel.

**Nút Lưu nằm cạnh câu trả lời**, không nằm trong màn hình Cài đặt — vì lúc người ta
muốn giữ một câu hỏi là lúc vừa thấy nó trả lời đúng. Tên mặc định là **chính câu truy
vấn**, không phải một câu model đoán ra.

**Còn thiếu so với §6.1:** thanh hiện là một ô chữ thường, chưa phải **biên lai**. Bấm
người trên đồ thị chưa đổ vào nó, và chưa có chip. Đó là Bước 3, và nó là bước quyết
định người non-tech có dùng được hay không.

---

## 14. Bước 3 — đã làm, 2026-09-19

**Chip không phải một mô hình riêng — chip là chữ, cắt ra.**

Cách hiển nhiên là: phân tích chữ thành cấu trúc, vẽ chip từ cấu trúc, ghi cấu trúc
ngược ra chữ. Cách ấy đòi phân tích và ghi ra phải là **nghịch đảo chính xác của
nhau, vĩnh viễn**, kể cả cho những phần không bên nào hiểu hết. Lần đầu tiên
`sort:-when` quay về thành `sort:when` là một thấu kính đã lưu của ai đó lặng lẽ đổi
nghĩa.

Nên **chữ là trạng thái duy nhất**, và chip là một lát của nó. Bỏ chip = bỏ token;
bấm thêm = nối token; gõ = cắt lại. Hai chiều không phải một tính năng phải giữ cho
khỏi hỏng — nó là **hình dạng của thứ này**. Có test chạy vòng cho năm câu truy vấn
thật, trong đó có cả tên có dấu cách và `-status:done`.

**Hai luật chép từ Rust sang, cố ý:** tách token giữ nguyên cụm trong ngoặc kép, vì
`search.rs` làm thế; và danh sách khoá **thay thế thay vì lặp** (`when`, `shape`,
`magnitude`…) chép đúng chỗ bên kia khai `Option` hay `Vec`. Có test khẳng định
`with`/`where`/`about` **không** nằm trong danh sách ấy — vì hỏi hai người nghĩa là
**cả hai** cùng có mặt, đó là câu người ta thật sự hỏi.

**Cùng một cử chỉ bật và tắt.** Bấm lại đúng thứ vừa bấm thì nó rời thanh. Không có
luật này, bấm dải hai lần để lại một `when:` mà không ai nhìn thấy.

**Dải thời gian: ô ngày là nút, không phải cú kéo.** Dải không có sự kiện "thả tay",
mà bắn truy vấn theo từng nhịp kéo là một truy vấn mỗi pixel. Nên nửa có chủ đích của
cùng cử chỉ ấy là **bấm vào chỗ mình vừa kéo tới**. Đây là chỗ tao đi chệch §6.1 một
bước và ghi lại để không ai tưởng là quên.

**Một lỗi test bắt được mà mắt không thấy:** gõ một ký tự làm ô chữ **biến mất dưới
con trỏ**, vì có chip là chuyển sang mặt chip. Nay gõ thì giữ chữ; bấm chỗ khác mới
đổi mặt.

**Còn lại:** bấm người trên đồ thị và ô ngày đã đổ vào thanh; bấm tag cũng vậy. Kết
quả vẫn về dưới dạng bảng dù nó là gì — bước 4.

---

## 15. Bước 4 — đã làm, 2026-09-19

**Kết quả tự chọn hình dạng, và nó đọc *giá trị* chứ không chỉ đọc tên cột.**

Một cột tên `when` là ngày. Một cột tên `hạn_chót` trong schema ai đó tự đặt sáng nay
và đổ toàn ngày vào — **cũng là ngày**. Quyết theo tên thì đúng với những truy vấn
app tự viết và sai với những truy vấn người dùng viết, tức là sai đúng chiều quan
trọng. Nên **ô là bằng chứng, tên chỉ là gợi ý**, và tên chỉ được hỏi tới khi không
có dòng nào để đọc.

Có test cho cả chiều ngược: một cột tên `date` mà đổ *"hôm qua"*, *"tuần trước"* thì
**không** phải cột ngày.

**Ba hình dạng, không phải sáu.** §7 kể sáu, nhưng `bars` và `quotes` **chưa có gì để
vẽ** — chúng cần `count by` và `sentences` của bước 5–6. Dựng một renderer khi chưa
có dữ liệu là đoán xem dữ liệu ấy trông thế nào. Nên hôm nay: `dated`, `list`,
`table`, và ba cái đó đều có người sinh ra dữ liệu thật.

**`DatedView` là primitive mới duy nhất**, và nó giữ đúng hợp đồng đã viết ở
`views/types.ts`: nhận kết quả, không tự gọi; không rẽ nhánh theo type. Nó nhóm theo
ngày **mà không sắp xếp lại** — thứ tự là một phần của câu hỏi (`sort:`), sắp lại ở
đây là lặng lẽ ghi đè lên `sort:when`.

Vì sao một bảng không đủ cho câu hỏi về thời gian: **ba việc trong một ngày rồi năm
tháng im lặng là một câu trả lời**, còn trong bảng nó là bốn dòng.

**Một cú bấm đè được lên lựa chọn của kết quả, và bấm lại thì trả về cho nó.** Nút
đang dùng được đánh dấu **bất kể ai chọn** — người hay câu trả lời — vì đó không phải
thứ người ta đang tìm ở chỗ ấy.

**Và lựa chọn ấy đi vào thấu kính.** Nhưng `auto` thì **không ghi gì**: ghi `auto` ra
file là biến một mặc định thành một cam kết người ta chưa từng đưa ra. Có test cho cả
hai chiều.

**Còn lại:** `bars` và `quotes` về cùng bước 5–6. Trợ lý vẫn chưa đi qua bộ định tuyến
— bước 7.
---

## 16. Rà soát — 2026-09-20

Bốn bước đầu làm ở tài liệu này; bước 5 và 6 bị `query-grammar-2026-09-20.md` nuốt
vào, vì ngôn ngữ cần sửa trước khi xây thêm lên nó. **Bước 7 và 8 chưa làm.**

### Phép thử của §5.3, chạy thật

Tám panel, viết thành câu hỏi, chạy trên vault thật (162 sự kiện):

| Panel | Câu hỏi hôm nay | |
| --- | --- | --- |
| Ngày này năm xưa | `events when:same-day-as(today) shape:occasion` | ✓ 0 dòng |
| Khoảng lặng | `events \| seq gaps by who \| where quiet > longest` | ✓ 1 |
| Chuyện gì xảy ra với | `events columns:when,about \| seq gaps by about \| where quiet > 6mo` | ✓ 0 |
| Một năm bằng lời mày | `events when:2026 \| explode sentences \| ask 15` | ✓ (cần vault) |
| Zoom theo độ lớn | `events when:… columns:when,title,size \| top 20 by size` | ✓ 162 |
| Nhịp sống | `events \| stats count by month` | ✓ 10 |
| Ăn ở đâu | `events columns:when,place \| stats count by place` | ✓ 2 |
| Ai còn gặp | `events \| stats count by who \| sort count desc` | ✓ 1 |

**Gate của cả tài liệu — *"một câu hỏi mà hôm nay cần một panel mới, sau khi xong phải
trả lời được không thêm một dòng Rust hay Vue nào"* — đã đạt.**

### Cú pháp trong tài liệu này đã cũ

Viết trước khi có ngữ pháp, nên §5.3 và §8 dùng cách viết chưa bao giờ tồn tại. Giữ
nguyên chữ cũ ở trên để đọc lại thấy nó đã đi từ đâu tới đâu, và đây là bảng quy đổi:

| Viết ở đây | Thật ra là |
| --- | --- |
| `anniversary` | `when:same-day-as(today)` |
| `\| gaps` | `\| seq gaps by who` |
| `\| sentences` | `\| explode sentences` |
| `\| count by month` | `\| stats count by month` |
| `count by where` / `by person` | `stats count by place` / `by who` (cần `columns:` trước) |
| `with:*`, `about:*` | không cần — `seq gaps by who` tự gom |
| `has:quote`, `-has:ending` | **chưa có**, và chưa ai cần |
| `is:hush`, `is:pin` | **không chạy** — hush và pin là file trong `Timeline/`, không phải node |

Hai dòng cuối là thứ §5.3 nói quá. "Tám trên tám" đúng với tám panel; "Khay duyệt" và
"Đã từ chối" thì chỉ đúng nếu những thứ ấy là node, mà chúng không phải.

### Bước 8 chưa làm, và nó là chỗ lời hứa còn nợ

§9 hứa **"code ít đi, không nhiều lên"**. Đo hôm nay:

| | dòng |
| --- | --- |
| Tám panel Vue | 1.349 |
| Bốn module Rust dưới chúng (`onthisday`, `silence`, `asking`, `year`) | 2.157 |
| Bộ thấu kính thay cho cả hai | 1.375 |

**Code nhiều lên 3.506 dòng**, vì thứ mới đã vào mà thứ cũ chưa ra. Bốn lệnh
`timeline_on_this_day`, `timeline_silences`, `timeline_ask`, `timeline_year` vẫn đăng
ký, tám component vẫn còn.

**Nhưng không phải tất cả đều bỏ được.** Phần *câu hỏi* của bốn module ấy đã thành
truy vấn; phần **quyết định của con người** thì không:

- `quiet.rs` — lớp đồng thuận (seal/hush). Không phải truy vấn, không được thành
  truy vấn.
- `onthisday.rs` — luật "mỗi năm chỉ nhắc một lần".
- `asking.rs` — nhớ đã hỏi ai rồi.
- `year.rs` — giao thức danh-sách-đánh-số, giờ **dùng chung** với `| ask`.

Nên bước 8 là: bỏ **tám component Vue và bốn lệnh**, giữ phần tầng 2 bên dưới.

### Kệ có sẵn — chưa có, và nó là điều kiện của bước 8

§8 viết *"một thanh truy vấn trống là một lời từ chối phục vụ"*, rồi hứa mười hai
thấu kính đi kèm vault mới. **Chưa có cái nào.** `readShelf` chỉ chạy `nodes
type:lens`, và vault mới không có node nào như thế — nên gỡ tám panel bây giờ là đổi
tám nút lấy **một ô trống**.

Thứ tự đúng: **kệ trước, gỡ sau.**

### Việc còn lại, theo thứ tự

| | Việc | Vì sao theo thứ tự này |
| --- | --- | --- |
| **A** | Kệ mười hai thấu kính có sẵn | điều kiện của B; không có nó thì gỡ panel là làm app nghèo đi |
| **B** | Gỡ tám component + bốn lệnh, giữ tầng 2 | trả món nợ "code ít đi" |
| **C** | Bước 7 — Syn trả lời kèm truy vấn + nút Lưu | đường vào cho người không gõ |

**A rẻ và có ích ngay.** B chỉ là xoá, nhưng phải chờ A. C là thứ duy nhất còn cần
thiết kế: hôm nay model gọi `query_nodes` mà **câu hỏi nó viết không hiện ra cho
người dùng thấy**, nên không lưu lại được.

### Không phải việc của tài liệu này

161 trên 162 sự kiện không nêu tên ai. `seq gaps by who` đúng, có test, đo trên vault
thật — và nó tìm được **một** người. Thấu kính không chữa được chuyện đó; phần bóc
tách mới chữa được.

---

## 17. Bước 7 và 8 — đã làm, 2026-09-20

### Kệ có sẵn: mười hai câu hỏi, không ghi gì vào vault

§8 nói *"một thanh truy vấn trống là một lời từ chối phục vụ"*. Giờ một vault chưa
lưu gì vẫn có **mười hai câu hỏi trên kệ** — tám cái từng là panel, bốn cái cơ sở dữ
liệu đầy đủ mở ra.

**Không gieo file nào.** Gieo mười hai node lúc chạy lần đầu sẽ: gieo hai lần trên
hai máy, mọc lại sau khi người ta xoá, và cần một dấu mốc ở đâu đó để nhớ là đã gieo.
Không cái nào trong đó mua được gì. Một gợi ý **chỉ là gợi ý thì không cần trạng
thái nào cả**: nó hiện ra, bấm Lưu thì thành node bình thường, và từ đó `lensesOn`
thôi gợi ý nó nữa — **khớp theo câu hỏi, không theo tên**, vì tên là của người ta,
đổi tên một thấu kính đã lưu không được làm gợi ý cũ mọc lại.

Gợi ý vẽ bằng nét đứt và **không có nút xoá**: không có file nào để xoá.

### Bước 8 không thể làm như đã viết, và đó là phát hiện

§9 hứa bỏ tám panel. Đọc kỹ thì **ba trong số đó chưa bao giờ chỉ là câu hỏi**:

| Panel | Cử chỉ của nó |
| --- | --- |
| Ngày này năm xưa | `timeline_not_again` — đừng nhắc lại chuyện này |
| Khoảng lặng | `timeline_set_aside` — tạm để yên người này |
| Một năm bằng lời mình | `timeline_drop_line` — bỏ câu này ra |

Đó là **tầng 2** — quyết định của con người, cái lớp mà cả dòng thời gian tồn tại
được là nhờ nó. Xoá panel mà không mang cử chỉ theo thì không phải bớt code, mà là
**bớt đường để người ta nói không**.

Nên cử chỉ chuyển sang chính câu trả lời, và **không thêm một dòng Rust nào**: ba
lệnh kia vốn đã đúng độ mịn, mỗi lệnh là một lớp mỏng trên `quiet::write_hush` với
một `Subject` khác, và không lệnh nào từng biết panel nào gọi mình.

Cái gì chối được thì **đọc từ cột**, y như `shapeFor` đọc hình dạng:

| Câu trả lời có | Chối cái gì |
| --- | --- |
| `who` + `quiet` | người ấy |
| `day, note, text` | câu ấy |
| một cột ngày + một note | chuyện ấy, ngày ấy |
| không cái nào | **không vẽ nút** |

Thứ tự là nghĩa, không phải sở thích: một câu trả lời `seq gaps` cũng có ngày trong
đó, mà chối một dòng của nó là chối **người**, không phải ngày cuối cùng gặp họ.

### Hai thứ lens làm mất, tìm lại trước khi xoá

Đọc kỹ `year.rs` trước khi bỏ nó thì thấy bản thay thế **kém hơn ở hai chỗ**:

1. `explode sentences` **cắt** ở 2.000 dòng đầu. `year::thin` tồn tại đúng để chặn
   chuyện đó: *"lấy 300 dòng đầu là đưa cho model tháng Giêng rồi gọi đó là một
   năm."* Giờ nó **lấy cách quãng**, nên các tháng giữ đúng tỉ lệ đã viết. Có test:
   3.600 dòng trải 12 tháng, cắt còn 2.000, **cả 12 tháng đều còn hơn 100 dòng**.
2. `year::keep` sắp lại **theo thứ tự đã sống** và gộp câu trùng. `keeping_picked`
   giữ thứ tự model trả về. Giờ nó trả về **theo thứ tự được mời** — câu hỏi quyết
   định thứ tự, `sort:` là một phần của nó — và số trùng chỉ tính một lần.

Đây là lý do "xoá code cũ" phải đọc code cũ trước: nó biết những thứ bản mới chưa
biết.

### Bước 7: câu hỏi trợ lý viết, đưa lại cho người hỏi

§6.2: *trợ lý **viết** truy vấn, không **thay thế** nó.* Trước đây câu nó viết nằm
trong một lời gọi công cụ rồi biến mất. Giờ dưới mỗi câu trả lời có dùng
`query_nodes` là **chính câu truy vấn ấy**, kèm nút Lưu.

Hiện **nguyên văn câu truy vấn**, không phải một lời mô tả nó — người không biết
viết `events | seq gaps by who` học ngôn ngữ bằng cách thấy câu của **mình** được
viết ra. Đây là mặt dạy duy nhất không cần đọc tài liệu nào.

Một lượt thường hỏi cùng một câu hai lần — hẹp trước, rộng sau khi hẹp không ra gì
(`tool_query_nodes` tự nới). Gộp lại một, vì hiện hai lần đọc ra như hai phát hiện.

### Code ít đi, đúng như đã hứa

| | |
| --- | --- |
| Xoá | 3 component Vue, 3 lệnh đọc, `silence.rs` (385 dòng), 2/3 của `year.rs` |
| Thêm | `putAway.ts`, `KeepAsLens.vue`, kệ có sẵn |
| **Ròng** | **−1.178 dòng** |

### Còn giữ, và vì sao

| | |
| --- | --- |
| `AskPanel` | app **hỏi người ta** một câu — nudge có đồng thuận, không phải thấu kính |
| `RefusalsPanel` | hush và pin là **file** trong `Timeline/`, không phải node, nên không hỏi được bằng truy vấn |
| `MomentsPanel` | vẽ **ảnh thật**; chưa có hình dạng `gallery` để thay |
| `ExtractTray` | duyệt/từ chối đề xuất — tầng 2 |
| `ReflectPanel` | không nằm trong tám cái §5.3 |
| `quiet.rs`, `onthisday.rs`, `asking.rs` | tầng 2: đồng thuận, luật mỗi năm một lần, nhớ đã hỏi ai |

Ba lệnh hush **ở lại** — giờ chính thấu kính gọi chúng.

### Đo

| | |
| --- | --- |
| Test Rust | 2400 |
| Test TypeScript | 1907 |
| `vue-tsc` | sạch |
