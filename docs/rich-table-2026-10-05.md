# Rich Table cho Synabit

**2026-10-05.** Thiết kế. Chưa có dòng code nào viết theo nó. Phần nghiên cứu sản phẩm
nằm ở §2, phần quyết định ở §3, phần đặc tả từ §4 trở đi.

---

## 0. Một câu

**Rich Table là một kiểu khối mới, đứng cạnh bảng đang có chứ không thay nó. Dữ liệu
của nó nằm trong một bảng pipe ai cũng đọc được, cấu hình nằm trong một chú thích
HTML ngay dưới nó, và một bộ máy dùng chung (kiểu, công thức, bộ lọc, view, biểu đồ)
chạy trên cả bảng trong note lẫn bộ sưu tập Things.**

```
| Ngày       | Khoản  | Loại    | Số tiền | Thuế |
| ---        | ---    | ---     |    ---: | ---: |
| 2026-10-01 | Cà phê | Ăn uống |   45000 | 4500 |
| 2026-10-02 | Grab   | Đi lại  |   62000 | 6200 |
<!-- rich-table
name: chi-tieu
columns:
  Ngày: date
  Loại: { type: select, options: [Ăn uống, Đi lại, Nhà] }
  Số tiền: { type: number, format: vnd }
  Thuế: { type: formula, expr: "[Số tiền] * 0.1", format: vnd }
views:
  - name: Tất cả
    sort: -Ngày
    summary: { Số tiền: sum, Thuế: sum }
  - name: Theo loại
    layout: chart
    chart: { kind: bar, x: Loại, y: "sum([Số tiền])" }
-->
```

Mở note này bằng bất kỳ trình soạn Markdown nào thì vẫn thấy một bảng đúng, có số
đúng. Mở bằng Synabit thì thấy một bảng có kiểu, có công thức, có biểu đồ.

---

## 1. Hiện trạng

### 1.1 Bảng trong Notes

- `@tiptap/extension-table` với `resizable: true`
  (`src/mini-apps/note/TiptapEditor.vue:536`). Có chọn ô (`CellSelection`), gộp ô,
  tô nền ô bằng 5 màu (`editor/extensions/customTable.ts`), cấm bảng lồng bảng
  (`noNestedTables.ts`).
- Điều khiển là một lớp phủ kiểu Confluence (`EditorTableControls.vue`): tay nắm
  hàng/cột, nút "+", menu chuột phải. **Tất cả chạy bằng `mousedown` và chuột phải**,
  nên trên điện thoại gần như không với tới được.
- Lưu bằng `tiptap-markdown`. Bảng "đơn giản" thành bảng pipe GFM. Có gộp ô, có cột
  tiêu đề, hay ô chứa hai khối thì **cả bảng rơi về HTML thô**.

### 1.2 Lỗi tìm thấy khi đọc (ngoài phạm vi thiết kế này)

| Lỗi | Vì sao |
| --- | --- |
| Tô màu ô trong bảng đơn giản **mất khi lưu** | Đường GFM bỏ qua mọi thuộc tính ô (`backgroundColor`, `colwidth`) |
| Căn lề cột không lưu | Dòng phân cách luôn là `---`; ô không có thuộc tính căn lề |
| Ký tự `\|` trong ô **có lẽ làm vỡ hàng** khi mở lại | Bộ ghi chữ chỉ thoát HTML, không thoát `\|`. Chưa có test |

Ghi lại để biết, **không sửa ở đây**: thiết kế này không đụng tới bảng đang có (§3.2).
Rich Table tránh cả ba lỗi bằng định dạng của riêng nó (§4.2).

### 1.3 Things

Things đã là một cơ sở dữ liệu: **mỗi hàng là một file node**, trường là frontmatter
YAML, loại (`type:`) được phát hiện từ vault, view được lưu thành node
`type: view` (`useThingsViews.ts`). Nhưng:

- Bảng là **chỉ đọc** (`shared/views/TableView.vue`, 104 dòng). Sửa phải mở bảng bên.
- Kiểu trường chỉ có `text number boolean date list json` (`shared/fieldValue.ts:12`).
  Không có chọn, công thức, quan hệ.
- `QueryRow.cells` là `Vec<String>` (`db/node_query.rs:31`): giá trị tới view **đã
  mất kiểu**.
- Schema (`Schema/<kind>.md`) chỉ là **lời khuyên**, "never a rule".
- Không có trên điện thoại.

### 1.4 Thứ có sẵn để dựa vào

| Có sẵn | Ở đâu | Dùng cho |
| --- | --- | --- |
| Ngữ pháp truy vấn + ống dẫn `\| stats \| where \| sort` | `src-tauri/src/query.rs`, `pipeline.rs` | Lọc bộ sưu tập, tổng hợp trên toàn vault |
| Khối ` ```query ` hiện kết quả thành bảng | `note/QueryResultTable.vue` | Nhúng bộ sưu tập vào note |
| Mẫu "khối tuỳ chỉnh = HTML + `data-*`" | `WhiteboardExtension.ts`, `PdfExtension.ts` | Cách serialize |
| Luật markdown-it tuỳ chỉnh | `EquationExtension.ts:62` | Cách parse |
| d3 7.9, biểu đồ thời gian có kiểm màu | `shared/views/EventsOverTime.vue` | Biểu đồ |
| Biểu đồ tròn d3 | `finance/components/FinanceChart.vue` | Biểu đồ tròn |
| `shapeFor` chọn hình `bars` **mà chưa ai vẽ** | `shared/views/shapeFor.ts` | Thành phần biểu đồ sẽ lấp chỗ này |
| Đọc/ghi xlsx, csv (desktop) | `syn/spreadsheet.rs` | Xuất/nhập |

**Không có bộ tính biểu thức nào** trong cả codebase. `| where` chỉ so sánh và
AND/OR/NOT, không có số học, không có hàm.

### 1.5 Đồng bộ

Thân Markdown được trộn bằng **Loro CRDT theo từng ký tự**
(`sync/core/crdt.rs`). Frontmatter trộn theo từng khoá, ai sau thắng. Bảng trắng
(JSON) trộn theo từng phần tử có dấu thời gian (`board_merge.rs`).

Hệ quả cho bảng trong thân note: hai máy sửa **hai hàng khác nhau** khi offline thì
trộn sạch (khác dòng). Hai máy sửa **cùng một ô** thì ký tự có thể đan vào nhau. Với
phần mềm một người dùng, trường hợp sau hiếm; §4.6 nói nó hỏng ra sao và vì sao hỏng
như vậy chấp nhận được.

---

## 2. Người khác làm thế nào

### 2.1 Ba cách cất dữ liệu bảng

| Cách | Ai | Còn lại gì khi bỏ ứng dụng |
| --- | --- | --- |
| **(a) Giá trị trong ô + công thức ở phụ lục** | org-mode `#+TBLFM:`, Obsidian Advanced Tables `<!-- TBLFM: -->` | Mọi thứ. Số đã được ghi sẵn vào ô |
| **(b) Mỗi hàng một file + file định nghĩa view** | Obsidian Bases (`.base` YAML), Dataview, Tana, Anytype | Dữ liệu còn, mất view |
| **(c) Cơ sở dữ liệu đóng** | Notion, Coda, Airtable, AFFiNE, Logseq DB | Một CSV đã **phẳng hoá công thức thành giá trị cuối** |

Synabit đã có (b), chính là Things. Thứ còn thiếu là (a), cho bảng nhỏ nằm trong
note. Thiết kế này **làm (a) và nâng cấp (b), chạy cùng một bộ máy**. Không làm (c).

### 2.2 Ngôn ngữ công thức

| Sản phẩm | Tham chiếu cột | Ví dụ |
| --- | --- | --- |
| Excel Tables / Sheets Tables | `[@Cột]`, `Bảng[Cột]` | `=[@Qty]*[@Price]`, `=SUM(Sales[Amount])` |
| Notion 2.0 | `prop("Cột")`, chấm | `prop("Tasks").filter(current.prop("Status") == "Done").length()` |
| Coda | `thisRow.Cột` | `Tasks.Filter(Owner = User()).Count()` |
| Airtable | `{Cột}` | `IF({Sales} > 50, "Win", "Lose")` |
| Obsidian Bases | `cột`, `note.cột` | `(price / age).toFixed(2)` |
| org-mode, Advanced Tables | `@hàng$cột` | `$4=$2*$3;%.2f` |

Bài học:

1. **Tham chiếu theo tên, không theo ô.** Mọi ngôn ngữ dễ học đều theo tên cột và
   tính theo hàng. `A1` và `@2$3` vỡ khi chèn cột, khó đọc, và vô nghĩa khi bảng được
   sắp xếp lại.
2. **Excel structured references là cây cầu.** Người quen bảng tính nhận ra ngay
   `SUM(Sales[Amount])`, mà nó vẫn theo tên. Google Sheets đã chuyển sang đúng cú pháp
   này (Tables, 2024).
3. **Tách ba loại công thức:** công thức cột (từng hàng), tóm tắt (chân bảng, theo
   view), tổng hợp cho biểu đồ (nhóm + gộp). Bases tách sạch nhất (`formulas:` và
   `summaries:`).
4. **Airtable cấm công thức nhìn sang hàng khác**, và đó là phàn nàn lớn nhất của nó.
   Số dư lũy kế là nhu cầu có thật.

### 2.3 View và biểu đồ

- Notion: bảng, bảng Kanban, lịch, timeline, danh sách, thư viện, **biểu đồ** (cột
  đứng, cột ngang, đường, donut, số KPI). Biểu đồ: trục X là một thuộc tính, trục Y là
  đếm hoặc gộp, có thể chia nhóm thứ hai (xếp chồng), có thể cộng dồn.
- Bộ lọc của Notion lồng AND/OR tối đa **3 tầng**, và thế là đủ.
- Chân bảng của Notion: đếm, đếm có giá trị, đếm khác nhau, % trống, tổng, trung bình,
  trung vị, nhỏ nhất, lớn nhất, khoảng. **Theo bộ lọc của view.**
- Bases: bộ lọc viết bằng chính ngôn ngữ công thức (`'status != "done"'`), nên giao
  diện bấm-chuột và dạng chữ là **hai trình soạn của cùng một thứ**.

### 2.4 Thư viện

| Thư viện | Giấy phép | Kết luận |
| --- | --- | --- |
| HyperFormula | GPLv3 hoặc thương mại | **Không.** Synabit dùng giấy phép Source Available, không tương thích GPLv3 |
| AG Grid Community | MIT | **Không.** Chọn vùng, clipboard, fill handle, nhóm đều là bản Enterprise |
| Glide Data Grid | MIT | **Không.** Chỉ React |
| Handsontable | Độc quyền | **Không** |
| TanStack Table | MIT, có Vue | Cân nhắc rồi bỏ (§3.7) |
| @formulajs/formulajs | MIT | Chỉ là thân hàm, không có parser. Có thể mượn vài hàm tài chính/thống kê |
| Chart.js, ECharts, uPlot | MIT/Apache | Không cần. Toàn bộ bộ biểu đồ của Notion vẽ được bằng d3 đã có |

### 2.5 Thất bại của từng cách

| Cách | Phàn nàn |
| --- | --- |
| Notion | Công thức xa dữ liệu, lỗi khó hiểu; DB lớn chậm; offline chỉ 50 hàng; xuất ra mất công thức và view |
| Airtable | Giá, trần số bản ghi; công thức không nhìn được hàng khác |
| Sheets/Excel | Không kiểu; A1 mong manh; công thức ẩn trong ô; không có danh tính hàng |
| Bảng pipe thuần | Không giữ kiểu; không có ô nhiều dòng; `\|` phải thoát |
| TBLFM (org, Advanced Tables) | Tham chiếu theo vị trí, tính tay, cú pháp bí hiểm |
| Bases | Một hàng một note: quá nặng cho 50 dòng chi tiêu |
| Logseq DB | Rời Markdown làm hỏng truy vấn và thói quen của người dùng |

---

## 3. Các quyết định lớn

### 3.1 Hai thứ, một bộ máy

| | **Rich Table** | **Bộ sưu tập** |
| --- | --- | --- |
| Là gì | Một khối trong thân note | Một truy vấn trên các node của Things |
| Hàng là | Một dòng của bảng pipe | Một file node |
| Hợp với | Chi tiêu tháng, so sánh lựa chọn, điểm số, bảng tính nhanh | Sách, khách hàng, công thức nấu ăn, bất cứ thứ gì mỗi hàng đáng có trang riêng |
| Cỡ | Vài chục đến ~2.000 hàng | Không giới hạn (có phân trang) |
| Có sẵn | Không | Có, nhưng chỉ đọc |

Notion gọi hai thứ này là "inline database" và "full-page database", và **giả vờ
chúng là một**. Synabit không giả vờ: chúng khác nhau ở chỗ cất, nên khác nhau ở cỡ,
ở đồng bộ, ở cách Syn đọc. Nhưng **người dùng thấy cùng một lưới, cùng một ngôn ngữ
công thức, cùng một menu cột, cùng một biểu đồ**, vì cả hai chạy qua một giao diện
`TableSource` (§11.2).

Có một đường đi từ cái này sang cái kia: **"Chuyển thành bộ sưu tập"** biến mỗi hàng
thành một node của một loại mới. Đường ngược lại là "Xuất ra Rich Table" (một bản
chụp, không còn sống).

### 3.2 Rich Table là một kiểu mới, bảng đang có giữ nguyên

**Rich Table không phải bảng cũ được nâng cấp. Nó là một kiểu khối mới, đứng cạnh.**
Bảng đang có — extension, `EditorTableControls.vue`, menu, cách lưu — không bị sửa một
dòng nào.

Hai thứ đòi hai mô hình khác nhau. Bảng đang có là cho **bố cục**: gộp ô, danh sách
trong ô, bảng so sánh có chữ đậm. Rich Table là cho **dữ liệu**: mỗi cột một kiểu, mỗi
ô một giá trị. Một ô gộp không có kiểu; một ô chứa danh sách gạch đầu dòng không cộng
được. Cố nhét cả hai vào một khối là con đường dẫn tới một lưới vừa dở soạn thảo vừa
dở tính toán.

Hệ quả:

- **Tên riêng, node riêng, đánh dấu riêng.** Slash command "Rich Table" bên cạnh
  "Bảng" (như cũ). Node ProseMirror `richTable`, khác hẳn `table`. Trong file, đánh dấu
  là `<!-- rich-table`.
- **Không có nút chuyển trong menu bảng cũ**, vì thêm nút là sửa bảng cũ. Muốn đưa một
  bảng cũ sang: chọn cả bảng, sao chép, dán vào một Rich Table trống. Đường dán HTML
  `<table>` (§9.3) đã phải có cho Sheets và Notion, nên việc chuyển không tốn thêm gì.
- **Không tự nâng cấp bảng cũ.** Bảng của người ta đang hiển thị đúng; tự đổi nó là
  phá thứ đang chạy.
- **Hai định dạng không lẫn nhau.** Bảng pipe không có `<!-- rich-table` theo sau vẫn
  là bảng cũ, đi đường cũ. Bảng pipe có nó là Rich Table, đi đường mới. Đường ghi của
  bảng cũ không bao giờ thấy một Rich Table và ngược lại.

### 3.3 Dữ liệu ở bảng pipe, cấu hình ở chú thích

Đây là quyết định đắt nhất và lâu dài nhất, nên lý do viết đầy đủ:

1. **Đọc được ở mọi nơi.** GitHub, Obsidian, `cat` đều hiện đúng bảng. Đúng cái lời
   hứa mà khối ` ```query ` đã giữ ("readable, and still portable").
2. **Hỏng cấu hình không mất dữ liệu.** Chú thích hỏng (YAML lỗi do trộn đồng bộ, do
   ai sửa tay) thì bảng hiện như bảng cũ kèm một dòng cảnh báo. Giá trị không bao
   giờ nằm trong phần có thể hỏng.
3. **Syn đã đọc và sửa được bảng pipe** bằng công cụ sẵn có (`get_node`,
   `update_node`), và có test cho chính việc ấy (`tools.rs:6449`). Khối HTML mờ đục
   hay file riêng sẽ vô hình với Syn, với tìm kiếm toàn văn, với RAG.
4. **Trộn đồng bộ đủ tốt.** Sửa khác hàng thì khác dòng, CRDT ký tự trộn sạch.

Phương án bị bỏ:

| Phương án | Vì sao bỏ |
| --- | --- |
| JSON trong khối ` ```table ` | CRDT ký tự trộn hai bản sửa JSON ra JSON hỏng, mà lúc đó **dữ liệu** hỏng chứ không chỉ cấu hình. Và không ai đọc nổi |
| File riêng, như bảng trắng (`.whiteboard.json`) | Trộn từng hàng an toàn nhất, nhưng vô hình với Syn và tìm kiếm, và một bảng 10 dòng thành hai file. Bộ sưu tập đã là đáp án cho "cần trộn từng hàng" |
| Cấu hình ở frontmatter của note | Một note có thể có nhiều bảng; frontmatter là của note chứ không phải của khối |
| Khối ` ```table-meta ` thay vì chú thích | Hiện ra thành một khối code xấu xí ở mọi trình soạn khác. Chú thích thì vô hình |

Chú thích đặt **sau** bảng, như TBLFM: người đọc file thô thấy dữ liệu trước.

### 3.4 Công thức theo tên cột, không theo ô

Không có `A1`. Không có công thức ở một ô lẻ. Một cột hoặc là dữ liệu, hoặc là công
thức áp cho mọi hàng — như calculated column của Excel Tables, như Notion, như
Airtable.

Cái giá: không làm được "ô B7 = B3 + B5" kiểu bảng tính tự do. Đó là đúng thứ làm
bảng tính mong manh, và trong một bảng có thể sắp xếp và lọc thì "B7" chẳng có nghĩa
gì ổn định. Ai cần bảng tính tự do thật thì đã có xlsx, và Syn đọc được xlsx.

Hai nhu cầu hay bị dùng làm lý do cho A1, và cách đáp ứng chúng không cần A1:

- **Tổng ở cuối** → chân bảng (§7.4), không phải một hàng giả.
- **Số dư lũy kế** → `PREV([Số dư]) + [Thu] - [Chi]` (§6.5).

### 3.5 Tự viết bộ máy công thức

Không có thư viện nào vừa giấy phép, vừa theo tên cột, vừa nhỏ (§2.4). Một Pratt
parser, một bộ kiểm tra kiểu và phụ thuộc, một bộ tính, khoảng 60 hàm: vài nghìn dòng
TypeScript thuần, test được bằng vitest, không đụng DOM. Đây cũng là bộ tính biểu
thức đầu tiên của codebase, nên nó nằm ở `src/shared/formula/`, không nằm trong Notes.

Viết bằng TypeScript, không bằng Rust, vì Rich Table phải tính lại **trong lúc gõ**,
trong WebView. Hệ quả có giá phải trả ở §10.3 và §11.4.

### 3.6 Bộ lọc

- **Rich Table:** bộ lọc là **một biểu thức công thức trả về đúng/sai**, như Bases.
  `[Số tiền] > 100000 and Loại = "Ăn uống"`. Một ngôn ngữ trong bảng, không hai.
- **Bộ sưu tập:** bộ lọc là **ngữ pháp truy vấn** sẵn có, chạy trong Rust, vì nó phải
  lọc trên cả vault trước khi phân trang.

Đó là hai ngôn ngữ, và đáng nói thẳng: lý do là **chỗ chạy**, không phải thẩm mỹ.
Ngữ pháp truy vấn lọc file trong SQLite; công thức lọc hàng trong bộ nhớ. Thanh lọc
bấm-chuột (§7.2) giấu sự khác biệt này ở trường hợp thường gặp: chọn cột, chọn phép
so sánh, gõ giá trị. Hai thứ chỉ lộ ra khi người ta bấm "Sửa dạng chữ".

### 3.7 Lưới tự dựng

TanStack Table cho sắp xếp, lọc, nhóm, gộp. Nhưng ở đây sắp xếp/lọc/nhóm phải đi qua
bộ máy công thức của mình, còn phần khó thật — chọn vùng, bàn phím, sửa tại chỗ, dán,
cảm ứng — thì TanStack không làm. Thêm một tầng trừu tượng để rồi không dùng phần nó
làm được là không đáng.

Lưới là các `<div>` với `role="grid"`, mỗi hàng một CSS grid. Đầu cột và cột đầu dính
bằng `position: sticky` (Baseline Widely available). Hàng dùng `content-visibility:
auto` y như `TaskListView.vue`: nếu WebView không biết thuộc tính ấy thì vẽ hết mọi
hàng, đúng như bây giờ. Không ảo hoá bằng JS ở giai đoạn đầu; 2.000 hàng × 10 cột là
20.000 ô DOM, chịu được.

> **Sửa lại — 2026-10-05, lúc làm giai đoạn 1.** Chỗ này vốn viết *"`<table>` DOM"*.
> **Sai**: containment không áp dụng cho hàng của bảng (`display: table-row`), nên
> `content-visibility` trên `<tr>` không làm gì. Phải là `<div>`.

---

## 4. Định dạng file

### 4.1 Hình dạng

```
<bảng pipe GFM>
<!-- rich-table
<YAML>
-->
```

Chú thích phải **liền ngay dưới** bảng (không có dòng trống ở giữa — markdown-it vẫn
đọc được cả khi có, nhưng bộ ghi luôn viết liền để người đọc file thô thấy chúng đi
với nhau). Bảng pipe không có chú thích theo sau là bảng cũ.

### 4.2 Bảng pipe

- **Hàng đầu là tên cột, và tên cột là danh tính của cột.** YAML tham chiếu cột theo
  tên.
- Tên cột: không rỗng, không trùng, không chứa `|`, `[`, `]`.
- Dòng phân cách: `---:` cho cột số và công thức ra số, `:---:` cho hộp kiểm, `---`
  cho còn lại. Căn lề **sinh ra từ kiểu**, không do người chọn — vừa tránh lỗi căn
  lề ở §1.2, vừa làm file thô dễ đọc.
- Ô thoát `|` thành `\|`, xuống dòng thành `<br>`.
- **Không đệm khoảng trắng để thẳng cột.** Đệm làm một lần sửa một ô thành sửa cả
  cột trong file, và CRDT ký tự sẽ phải trộn cả cột. Ví dụ ở §0 được đệm chỉ để dễ
  đọc trong tài liệu này.

### 4.3 Giá trị trong ô, theo kiểu

| Kiểu | Ghi | Ví dụ |
| --- | --- | --- |
| Văn bản | Như gõ, Markdown inline được phép | `**Gấp** — xem [[Hợp đồng]]` |
| Số | **Số thô**, dấu chấm thập phân, không phân cách nghìn | `45000`, `0.075` |
| Chọn | Tên lựa chọn | `Ăn uống` |
| Chọn nhiều | Các tên, cách nhau `, ` | `Đi lại, Công tác` |
| Ngày | ISO | `2026-10-01`, `2026-10-01 14:30` |
| Hộp kiểm | `[x]` / `[ ]` | `[x]` |
| Liên kết | URL | `https://…` |
| Note | Wikilink | `[[Sách/Sapiens]]` |
| Công thức | **Giá trị đã tính** (§4.5) | `4500` |

Số ghi thô, không ghi `45.000 ₫`. Định dạng là chuyện hiển thị (`format: vnd`), còn
file phải parse lại được bằng một biểu thức chính quy. Đổi lại: mở bằng trình khác thì
thấy `45000` thay vì `45.000 ₫`. Chấp nhận.

Giá trị **không khớp kiểu không bị vứt.** `abc` trong cột số được giữ nguyên, hiện
với một góc đỏ và tooltip "không phải số", và bị bỏ qua khi tính tổng. Cùng tinh thần
với `fieldValue.ts`: một giá trị là gì thì hiện nó là cái đó, không lặng lẽ biến nó
thành thứ khác.

### 4.4 YAML

```yaml
name: chi-tieu                # tuỳ chọn; để tham chiếu từ biểu đồ và bảng khác cùng note
version: 1
columns:                      # cột không có ở đây là Văn bản
  Ngày: date
  Loại: { type: select, options: [Ăn uống, Đi lại, Nhà], colors: { Nhà: blue } }
  Nhãn: { type: multi, options: [Công tác, Cá nhân] }
  Số tiền: { type: number, format: vnd }
  Đã trả: checkbox
  Thuế: { type: formula, expr: "[Số tiền] * 0.1", format: vnd }
  Ghi chú: { type: text, width: 240, wrap: true }
freeze: 1                     # số cột dính bên trái
views:
  - name: Tất cả
    layout: table             # table | chart | pivot | board
    filter: "[Đã trả] = false"
    sort: [-Ngày, Khoản]
    group: Loại
    hide: [Ghi chú]
    summary: { Số tiền: sum, Thuế: sum, Khoản: count }
    rules:
      - when: "[Số tiền] > 1000000"
        style: { row: red }
```

- Khoá YAML là tên cột; cột trong YAML mà không có trong bảng thì bị bỏ qua và hiện
  cảnh báo. Cột trong bảng mà không có trong YAML là Văn bản.
- Kiểu viết tắt (`Ngày: date`) cho trường hợp không có tuỳ chọn.
- `version` để có đường lùi khi định dạng đổi. Bộ đọc gặp `version` lớn hơn mình biết
  thì hiện bảng như bảng cũ, chỉ đọc cấu hình, và **không ghi lại**.
- Chuỗi `-->` không được xuất hiện trong YAML. Bộ ghi thoát nó thành `--\>` trong
  chuỗi; bộ đọc trả lại.

### 4.5 Cột công thức ghi giá trị vào file

Như org-mode và Advanced Tables. Lý do: người đọc file thô, Syn, tìm kiếm toàn văn
đều thấy số đúng mà không cần chạy công thức.

Giá phải trả, và cách trả:

- **Tính lại làm file đổi.** Sửa một ô `Số tiền` làm ô `Thuế` cùng hàng đổi theo,
  trong cùng một giao dịch, nên cùng một lần lưu. Công thức có tổng
  (`[Số tiền] / SUM(table[Số tiền])`) làm **cả cột** đổi. Đó là sự thật về dữ liệu,
  không phải nhiễu.
- **Mở note không được làm file đổi**, trừ khi giá trị đã cũ thật. Khi mở, bộ máy
  tính lại, so với giá trị trong file; chỉ khi khác mới ghi. `TODAY()` làm đúng chuyện
  này mỗi ngày một lần, và chỉ ở bảng có dùng nó.
- **File có thể cũ** khi bị sửa ngoài Synabit (bởi Syn, bởi đồng bộ, bởi tay). Lần
  mở sau trong trình soạn sẽ sửa nó. §10.3 nói cái này nghĩa là gì với Syn.
- Chân bảng **không** ghi vào file. Nó thuộc về view, mà một bảng có nhiều view.

### 4.6 Khi đồng bộ trộn hỏng

| Hỏng ở | Hậu quả | Phát hiện |
| --- | --- | --- |
| Một ô (hai máy sửa cùng ô) | Ô có giá trị đan ký tự, ví dụ `4562000` | Không phát hiện được; đây là một giá trị hợp lệ. Hiếm với một người dùng |
| Một hàng (số `\|` sai) | Hàng thừa/thiếu ô | Bộ đọc đệm/cắt về đúng số cột và đánh dấu hàng |
| YAML | Không parse được | Bảng hiện như bảng cũ + cảnh báo "Cấu hình bảng hỏng — mở lịch sử?" (đã có `NoteHistoryModal`) |

Không có trường hợp nào mất dữ liệu đã nhập. Đó là toàn bộ lý do của §3.3.

### 4.7 Parse và serialize

- **Đọc:** một luật `core` của markdown-it, chạy sau khi đã có token. Gặp
  `table_open … table_close` mà ngay sau là một `html_block` bắt đầu bằng
  `<!-- rich-table`, thì gộp thành một token `rich_table` mang nguồn của cả hai,
  render thành `<div data-type="rich-table" data-source="…">`. Cùng con đường với
  `EquationExtension` (luật markdown-it → HTML có `data-*` → `parseHTML`).
- Cần biết: tiptap-markdown hiện **vứt chú thích HTML** (khối HTML đi qua
  `DOMParser`, chú thích không thành node). Nên luật phải chạy trước khi khối HTML
  được render. Một test cho đúng điều này: note có Rich Table, mở rồi lưu, file
  không đổi một byte.
- **Ghi:** `addStorage().markdown.serialize` viết bảng pipe rồi chú thích. Đường ghi
  của bảng cũ không được đụng tới khối này.

---

## 5. Kiểu cột

| Kiểu | Ô hiện | Ô sửa | Gộp được |
| --- | --- | --- | --- |
| **Văn bản** | Markdown inline đã render | Ô chữ một dòng; `Shift+Enter` xuống dòng | đếm, đếm khác nhau, trống |
| **Số** | Theo `format` | Ô chữ, chấp nhận `45.000`, `45,5`, `45k`? — xem §15 | + tổng, TB, trung vị, min, max, khoảng |
| **Chọn** | Viên màu | Danh sách có tìm, gõ tên mới để thêm | + đếm theo giá trị |
| **Chọn nhiều** | Các viên màu | Như trên, nhiều lựa chọn | như trên |
| **Ngày** | Theo locale, hoặc "3 ngày nữa" | Lịch chọn ngày (dùng lại của Task) | + sớm nhất, muộn nhất, khoảng |
| **Hộp kiểm** | ☐ / ☑ | Bấm là đổi, không vào chế độ sửa | + % đã đánh dấu |
| **Liên kết** | Tên miền, bấm mở | Ô chữ | đếm |
| **Note** | Wikilink như trong note | Bộ chọn note sẵn có (`NoteMentionMenu`) | đếm |
| **Công thức** | Theo kiểu kết quả | Không sửa ô; sửa công thức ở đầu cột | theo kiểu kết quả |

Định dạng số (`format`): `number` (mặc định), `integer`, `decimal:2`, `percent`,
`vnd`, `usd`, `eur`, `progress` (thanh 0–1). Hiển thị qua `Intl.NumberFormat` theo
ngôn ngữ giao diện. Tiền tệ khác thêm sau, không đổi cú pháp.

**Đổi kiểu** mở một bản xem trước: "38 giá trị đổi được, 2 giá trị không — sẽ giữ
nguyên và đánh dấu". Không bao giờ xoá.

Chỉ có ở **Bộ sưu tập**: `Tạo lúc`, `Sửa lúc`, `Quan hệ` (trỏ tới node khác) và
`Tra cứu` (lấy trường qua quan hệ). Rich Table không có danh tính hàng bền vững,
nên không có quan hệ thật.

---

## 6. Ngôn ngữ công thức

### 6.1 Ví dụ trước

```
[Số tiền] * 0.1
ROUND([Đơn giá] * [Số lượng] * (1 - [Giảm giá]), 0)
IF([Hạn] < TODAY() and not [Xong], "Trễ", "")
[Số tiền] / SUM(table[Số tiền])
PREV([Số dư], 0) + [Thu] - [Chi]
SUMIF(table[Số tiền], table[Loại] = [Loại])
LOOKUP([Mã], gia[Mã], gia[Giá])
LET(net, [Gross] - [Thuế], IF(net < 0, 0, net))
DAYS([Ngày trả], [Ngày mượn])
TEXT([Ngày], "MM/YYYY")
```

### 6.2 Tham chiếu

| Viết | Nghĩa |
| --- | --- |
| `[Số tiền]` | Giá trị của cột ở **hàng này** |
| `Gia` | Như `[Gia]`; được bỏ ngoặc khi tên là một từ, không trùng tên hàm |
| `table[Số tiền]` | **Cả cột**, mọi hàng của bảng này, theo thứ tự trong file |
| `gia[Giá]` | Cả cột của bảng có `name: gia` **trong cùng note** |
| `PREV([Số dư])` | Giá trị của cột ở hàng ngay trên (thứ tự trong file) |

Đây là Excel structured references, bớt đi phần khó: không `#Headers`, không
`#Totals`, không `[@…]` (ngoặc vuông trần đã là "hàng này").

**Công thức cột luôn nhìn mọi hàng**, không nhìn bộ lọc của view. `[Số tiền] /
SUM(table[Số tiền])` cho cùng một con số ở mọi view, và đó là con số được ghi vào
file. Thứ theo bộ lọc là **chân bảng** (§7.4). Notion và Excel đều tách như vậy; trộn
hai thứ làm giá trị trong file đổi theo view đang mở.

### 6.3 Toán tử

| Ưu tiên (cao → thấp) | |
| --- | --- |
| `(…)`, gọi hàm, `x[…]` | |
| `-x` | |
| `^` | |
| `* / %` | |
| `+ -` | |
| `&` | nối chuỗi |
| `= != < <= > >=` | `<>` được chấp nhận như `!=` |
| `not` | |
| `and` | |
| `or` | |

`=` là so sánh, như Excel và Airtable; trong công thức không có phép gán. Từ khoá
không phân biệt hoa thường.

**Dấu thập phân luôn là `.`, dấu phân cách đối số luôn là `,`**, dù giao diện tiếng
Việt hiển thị `1.234,5`. Công thức là mã, và mã không đổi theo locale (Notion làm
vậy; Excel đổi theo locale, và ai từng mở file Excel Đức đều biết hậu quả).

### 6.4 Kiểu và lỗi

Giá trị là một trong: số, chuỗi, đúng/sai, ngày, danh sách, rỗng. Một cột (`table[X]`)
là danh sách. So sánh một danh sách với một giá trị cho ra danh sách đúng/sai — đó là
cách `SUMIF`, `COUNTIF`, `FILTER` nhận điều kiện.

**Rỗng trong số học là rỗng** (`rỗng + 1` là rỗng), và các hàm gộp bỏ qua rỗng. Excel
coi ô rỗng là 0, và đó là nguồn của những con số 0 không ai nhập; Notion không làm
vậy, ở đây cũng không. Ai muốn 0 thì viết `COALESCE([X], 0)`.

| Lỗi | Khi |
| --- | --- |
| `#TÊN` | Cột hoặc hàm không tồn tại |
| `#KIỂU` | `"abc" * 2` |
| `#CHIA0` | Chia cho 0 |
| `#VÒNG` | Cột phụ thuộc chính nó qua các cột khác |
| `#GIÁTRỊ` | Đối số sai, ví dụ `DATE(2026, 13, 1)` |

Lỗi được hiện trong ô, có tooltip nói rõ phần nào của công thức sai, và **được ghi vào
file** dưới dạng mã lỗi. Lỗi bị bỏ qua khi gộp, và chân bảng ghi "(2 lỗi)".

Mã lỗi trên đây là tiếng Việt cho giao diện tiếng Việt; trong file luôn ghi mã tiếng
Anh (`#NAME`, `#TYPE`, `#DIV0`, `#CYCLE`, `#VALUE`) để file không đổi theo ngôn ngữ
giao diện.

### 6.5 Thứ tự tính và `PREV`

- Mỗi cột công thức phụ thuộc vào các cột nó nhắc tới. Sắp xếp topo các cột; vòng thì
  mọi cột trong vòng là `#VÒNG`, phát hiện **lúc gõ công thức**, không phải lúc tính.
- Công thức chỉ nhắc `[X]` của hàng này: sửa một ô chỉ tính lại hàng đó.
- Công thức nhắc `table[X]`: sửa một ô ở cột X tính lại cả cột công thức.
- `PREV([Y])` trong chính cột Y là được, không phải vòng: tính từ trên xuống. Đối số
  thứ hai là giá trị cho hàng đầu (`PREV([Số dư], 0)`).
- Hàm "bay hơi": `TODAY()`, `NOW()`. Không có `RAND()`; một giá trị đổi mỗi lần mở là
  một file đổi mỗi lần mở.

### 6.6 Bảng hàm

Tên hàm tiếng Anh, theo Excel, vì đó là thứ người ta đã biết và đã tìm được trên mạng.
Gợi ý tự động hiện mô tả tiếng Việt.

| Nhóm | Hàm |
| --- | --- |
| Logic | `IF` `IFS` `SWITCH` `AND` `OR` `NOT` `ISBLANK` `ISERROR` `IFERROR` `COALESCE` |
| Số | `ROUND` `ROUNDUP` `ROUNDDOWN` `FLOOR` `CEILING` `ABS` `MOD` `POWER` `SQRT` `MIN` `MAX` `CLAMP` |
| Gộp | `SUM` `AVERAGE` `MEDIAN` `COUNT` `COUNTA` `COUNTUNIQUE` `MIN` `MAX` `STDEV` |
| Gộp có điều kiện | `SUMIF` `COUNTIF` `AVERAGEIF` `FILTER` `UNIQUE` `SORT` |
| Tra cứu | `LOOKUP(khoá, cột_khoá, cột_giá_trị, [mặc định])` — một hàm thay `VLOOKUP/XLOOKUP/INDEX+MATCH` |
| Chuỗi | `CONCAT` `JOIN` `LEFT` `RIGHT` `MID` `LEN` `UPPER` `LOWER` `TRIM` `REPLACE` `CONTAINS` `SPLIT` `TEXT` |
| Ngày | `TODAY` `NOW` `DATE` `YEAR` `MONTH` `DAY` `WEEKDAY` `WEEKNUM` `DAYS` `DATEADD` `EOMONTH` `NETWORKDAYS` |
| Hàng | `PREV` `ROW` |
| Biến | `LET` |
| Tài chính | `PMT` `FV` `PV` `NPV` `IRR` — mượn thân hàm từ `@formulajs/formulajs` (MIT) nếu cần |

Gộp nhận cả danh sách lẫn nhiều đối số: `SUM(table[A])`, `SUM([A], [B], [C])`.

Bảng hàm, như các ô của ống dẫn truy vấn, **mở mãi mà không đổi cú pháp**. Ship nhóm
logic, số, gộp, chuỗi, ngày trước.

### 6.7 Trình soạn công thức

Lỗi lớn nhất của Notion là công thức nằm trong một hộp thoại nhỏ, xa dữ liệu. Ở đây:

- Mở từ đầu cột ("Sửa công thức"), **hiện ngay dưới đầu cột**, lưới vẫn thấy.
- **Xem trước sống:** cột hiện giá trị mới trong lúc gõ, ô lỗi tô đỏ.
- Gõ `[` → danh sách cột. Gõ chữ cái → hàm, kèm chữ ký và một ví dụ.
- Lỗi chỉ đúng chỗ: gạch dưới phần sai, không chỉ "Invalid formula".
- Nhiều dòng được, chú thích `//` được.
- **Đổi tên cột viết lại mọi công thức nhắc tới nó**, qua cây cú pháp, không qua tìm
  và thay chuỗi.

---

## 7. View, lọc, sắp xếp, nhóm, chân bảng

### 7.1 View

Tab nhỏ phía trên bảng. Mỗi view có: bố cục, bộ lọc, sắp xếp, nhóm, cột ẩn, độ rộng
cột, chân bảng, luật tô màu. Một bảng mới có một view "Bảng"; thanh tab chỉ hiện khi
có từ hai view trở lên.

| Bố cục | Cho |
| --- | --- |
| `table` | Lưới |
| `chart` | Biểu đồ (§8) |
| `pivot` | Bảng tổng hợp: hàng theo một cột, cột theo một cột, ô là một phép gộp |
| `board` | Kanban theo một cột Chọn; kéo thẻ là đổi giá trị |

Bố cục `pivot` là `GROUPBY`/`PIVOTBY` của Excel 365 đặt thành giao diện. Nó cũng là
dữ liệu cho biểu đồ cột nhóm, nên cùng một bộ tính.

Bộ sưu tập có thêm `list` và `calendar` (đã có ở Things/Task), không cần làm lại.

### 7.2 Thanh lọc

Viên lọc, như Notion: `Loại là Ăn uống` `Số tiền > 100.000` `+ Thêm bộ lọc`.
Mỗi viên là: cột → phép so sánh (theo kiểu cột) → giá trị (theo kiểu cột: bộ chọn cho
Chọn, lịch cho Ngày, "hôm nay / tuần này / tháng trước" cho Ngày).

- Các viên nối AND. "Bộ lọc nâng cao" cho nhóm AND/OR lồng nhau, tối đa 3 tầng.
- "Sửa dạng chữ" hiện biểu thức (§3.6), sửa được, và quay lại viên nếu biểu thức còn
  biểu diễn được bằng viên.
- Bộ lọc **lưu vào view**. Một bộ lọc tạm (gõ vào ô tìm nhanh) không lưu, để mở note
  không làm file đổi.

### 7.3 Sắp xếp và nhóm

- Sắp xếp nhiều khoá. **Sắp xếp trong view không đổi thứ tự hàng trong file.** Thứ
  tự trong file chỉ đổi khi kéo hàng, hoặc khi chọn "Áp thứ tự này vào bảng" (cần cho
  `PREV`, vốn theo thứ tự file).
- Nhóm một tầng theo một cột, có tổng phụ ở chân mỗi nhóm, thu gọn được. Nhóm theo
  ngày có `by day/week/month/year` như `| stats … by month`.

### 7.4 Chân bảng

Mỗi cột một phép gộp, chọn từ danh sách theo kiểu (§5 cột "Gộp được"), **theo bộ lọc
của view**. Hoặc một công thức tuỳ chọn, trong đó `values` là các giá trị đang hiện:
`ROUND(AVERAGE(values), 1)` — mượn từ Bases và rollup của Airtable.

### 7.5 Luật tô màu

`when` là một biểu thức đúng/sai, `style` là `row` hoặc `cell` + một trong các màu sẵn
có của bảng cũ. Luật đầu tiên khớp thắng. Thêm: **thang màu** cho cột số
(`scale: [green, red]`), như nhiệt đồ.

Notion không có cái này, và người dùng Notion phải giả nó bằng `style()` trong công
thức. Đây là một chỗ rẻ để hơn hẳn.

---

## 8. Biểu đồ

### 8.1 Loại

| Loại | Cấu hình |
| --- | --- |
| Cột đứng / cột ngang | `x`, `y`, `series?`, `stack?` |
| Đường / vùng | `x` (thường là Ngày), `y`, `series?`, `cumulative?` |
| Donut | `x` (nhóm), `y` |
| Phân tán | `x` số, `y` số, `color?` |
| Số | `y` — một con số lớn, kèm "so với kỳ trước" nếu `x` là ngày |

`y` là một phép gộp: `count`, `sum([Số tiền])`, `avg([Điểm])`, hoặc một công thức trên
`values`. `x` là một cột; cột Ngày được gom `by day/week/month/year`.

Đây đúng là bộ biểu đồ của Notion cộng phân tán. Không thêm radar, phễu, Gantt; thêm
sau không đổi định dạng.

### 8.2 Ở đâu

- **Một view của bảng** (`layout: chart`): tab "Theo loại" ở ví dụ §0.
- **"Hiện cạnh bảng"**: một view bảng có thể ghim một biểu đồ ở trên lưới. Biểu đồ
  dùng chung bộ lọc với view, nên lọc lưới là lọc biểu đồ.
- **Khối biểu đồ đứng riêng**, ở chỗ khác trong note, trỏ tới bảng theo `name`:
  ```
  <!-- rich-chart of=chi-tieu view="Theo loại" -->
  ```
  Ở trình soạn khác thì vô hình. Đây là giai đoạn sau (§14).
- **Bộ sưu tập** và **thấu kính**: cùng thành phần biểu đồ vẽ hình `bars` mà
  `shapeFor.ts` đã chọn sẵn nhưng chưa ai vẽ.

### 8.3 Vẽ

d3 (đã có): `d3.rollup` để gom, `scaleBand/scaleLinear/scaleTime`, `stack`, `pie`.
Một thành phần `ChartView.vue` nhận `{ kind, rows, x, y, series }` đã gom sẵn, không
tự đọc dữ liệu — cùng hợp đồng với `ViewProps` ("a view is handed a result and never
fetches one").

Màu theo bảng màu đã qua bộ kiểm màu của `EventsOverTime.vue`, đúng ở cả sáng và tối.
Tooltip khi rê/chạm, bấm một cột là lọc lưới theo nhóm đó. Xuất PNG qua `html-to-image`
(đã có).

Biểu đồ **chỉ đọc**. Kéo cột để đổi số là một tính năng đẹp trong demo và một cách
sửa nhầm dữ liệu trong đời thật.

---

## 9. Tương tác

### 9.1 Bàn phím, theo Excel/Sheets

| Phím | Chế độ chọn | Chế độ sửa |
| --- | --- | --- |
| Mũi tên | Đi | Di chuyển con trỏ chữ |
| `Enter` / `F2` | Vào sửa | Lưu, xuống ô dưới |
| `Tab` / `Shift+Tab` | Sang phải / trái | Lưu, sang phải / trái |
| Gõ một ký tự | **Thay** nội dung ô, vào sửa | — |
| `Esc` | **Thoát khỏi bảng**, chọn cả khối | Huỷ sửa |
| `Shift+mũi tên` | Mở rộng vùng chọn | — |
| `Cmd+mũi tên` | Tới mép dữ liệu | — |
| `Delete` / `Backspace` | Xoá giá trị vùng chọn | — |
| `Cmd+D` | Điền xuống | — |
| `Cmd+Enter` | — | Lưu, áp cho cả vùng chọn |
| `Space` | Đổi hộp kiểm | — |
| `Cmd+Z` | Hoàn tác | Hoàn tác |

Ra khỏi bảng: `Esc`, hoặc mũi tên lên ở hàng đầu / xuống ở hàng chân. Vào bảng từ chữ
phía trên bằng mũi tên xuống. Đây là chỗ ProseMirror và lưới chạm nhau; nó cần test
riêng.

Hoàn tác đi qua lịch sử của ProseMirror: mỗi lần sửa là một giao dịch đổi thuộc tính
của khối, nên `Cmd+Z` trong bảng và ngoài bảng là cùng một ngăn xếp.

### 9.2 Chuột

- Bấm chọn ô, kéo chọn vùng, bấm đầu cột chọn cột, bấm số hàng chọn hàng.
- **Menu đầu cột:** đổi tên, đổi kiểu, sửa công thức, sắp xếp tăng/giảm, lọc theo
  cột này, nhóm theo cột này, ẩn, dính cột, xuống dòng, chèn trái/phải, nhân đôi, xoá.
- Kéo đầu cột để đổi thứ tự cột, kéo mép để đổi độ rộng. Kéo số hàng để đổi thứ tự hàng.
- Hàng "+ Mới" ở cuối; dòng chân bảng ở dưới cùng.
- **Thanh trạng thái khi chọn vùng:** Tổng · TB · Đếm · Min · Max của các ô số. Rẻ, và
  là thứ người dùng bảng tính tìm đầu tiên.
- Tay kéo điền (fill handle): giai đoạn sau. Với cột công thức thì không cần; với dữ
  liệu thì `Cmd+D` làm được phần lớn.

### 9.3 Dán và sao chép

- Dán `text/plain` dạng TSV (Excel, Sheets, Numbers đều cho cái này). Có
  `text/html` `<table>` thì đọc nó trước (Sheets, Notion, Confluence — đã có test dán
  bảng Confluence ở `plainTextPaste.spec.ts`).
- Dán nhiều hàng/cột hơn đang có: **tự thêm**, như Notion và Airtable. Không hỏi:
  `window.confirm` không đáng tin trong mọi WebView, và một lần dán thừa thì `Cmd+Z`
  lấy lại được cả.
- Giá trị dán được ép về kiểu cột; không ép được thì giữ nguyên và đánh dấu (§4.3).
- Đây cũng là đường đưa một bảng cũ sang Rich Table (§3.2). Dán một bảng vào chỗ
  ngoài Rich Table thì vẫn ra bảng cũ như bây giờ — Rich Table không chen vào đường
  dán chung của trình soạn.
- Sao chép ra: TSV + HTML, để dán vào Sheets ra đúng ô.

### 9.4 Menu nổi và quy tắc trình duyệt

Theo `CLAUDE.md`: Popover API là Baseline Newly available (2025-01-27) và hành vi cốt
lõi của nó (top layer, light-dismiss) **không tự lùi về được** nếu không có polyfill;
CSS anchor positioning chưa Baseline. Nên menu cột, bộ chọn kiểu, trình soạn công thức
dùng đúng cách `EditorTableControls.vue` đang dùng: `position: fixed` + tính toạ độ
bằng `getBoundingClientRect()`. Có thể gom thành một composable dùng chung.

`position: sticky` (Widely available) cho đầu cột và cột dính. `content-visibility`
cho hàng như §3.7.

### 9.5 Điện thoại

Notes chạy trên Android và iOS; bảng cũ hiện giờ gần như không dùng được ở đó
(§1.1). Rich Table phải dùng được, theo cách khác:

- Lưới cuộn ngang, cột đầu dính.
- Chạm chọn ô, chạm lần nữa để sửa. Chạm giữ đầu cột mở menu cột.
- **Mỗi hàng mở được thành một tờ** (bottom sheet) liệt kê mọi trường dạng biểu mẫu.
  Trên điện thoại, đây là cách sửa chính; trên desktop, đó là nút ↗ ở đầu hàng.
- View `board` và một bố cục `list` (thẻ xếp dọc) dễ dùng trên điện thoại hơn lưới.
- Không có gì chỉ hiện khi rê chuột.

Things (và do đó Bộ sưu tập) chưa có trên điện thoại; thiết kế này không đổi điều đó.

---

## 10. Syn

### 10.1 Đọc

Không cần làm gì: `get_node` trả thân note, trong đó bảng pipe đọc được và chú thích
cho biết kiểu và công thức. Đây là lý do của §3.3.

### 10.2 Ghi

Syn sửa bảng bằng cách viết lại cả thân (`update_node`). Mô tả công cụ cần thêm vài
câu:

- Bảng có `<!-- rich-table` theo sau là Rich Table. Giữ nguyên chú thích.
- Số ghi thô (`45000`), ngày ISO, hộp kiểm `[x]`.
- Ô của cột công thức sẽ được tính lại; ghi gì vào đó cũng bị thay.

Viết lại cả thân để thêm một hàng là phí và rủi ro. Giai đoạn sau: một công cụ
`table_rows` — thêm/sửa/xoá hàng **theo tên cột**, nhận mảng JSON như
`write_spreadsheet` (vì JSON giữ kiểu, `spreadsheet.rs:16` đã giải thích vì sao).

### 10.3 Giá trị cũ

Syn ghi file mà không chạy bộ máy công thức (nó ở TypeScript). Cột công thức sẽ cũ cho
tới khi note được mở trong trình soạn. Ba cách, từ rẻ tới đắt:

1. Chấp nhận, và nói trong mô tả công cụ: "giá trị cột công thức có thể cũ".
2. Sau khi Syn ghi một note có Rich Table, giao diện (nếu đang chạy) tính lại ngầm
   và ghi.
3. Bộ máy công thức bằng Rust, dùng chung cho Syn, cho bộ sưu tập (§11.4), cho mọi
   thứ không có WebView.

Làm (1) ngay, (2) khi làm công cụ `table_rows`. (3) chỉ khi có lý do thứ hai cần nó.

---

## 11. Kiến trúc

### 11.1 File

```
src/shared/rich-table/
  model.ts            TableModel, ColumnDef, Cell; parse/format giá trị theo kiểu
  markdown.ts         đọc/ghi bảng pipe + chú thích (thuần, không DOM)
  compute.ts          ống: hàng → công thức → lọc → sắp → nhóm → chân bảng
  source.ts           TableSource
  sources/inline.ts   nguồn Rich Table (thuộc tính của node ProseMirror)
  sources/collection.ts  nguồn bộ sưu tập (run_node_query + write_node_file)
  components/
    DataGrid.vue  ColumnMenu.vue  TypePicker.vue  FilterBar.vue
    FormulaEditor.vue  RowSheet.vue  SummaryRow.vue
    ChartView.vue  PivotView.vue  BoardView.vue  ViewTabs.vue
src/shared/formula/
  lexer.ts parser.ts check.ts eval.ts functions/*.ts  (+ __tests__)
src/mini-apps/note/extensions/RichTableExtension.ts
  node `richTable` (atom), luật markdown-it, serialize, NodeView → DataGrid
```

Khối là **atom node** với một NodeView Vue, như bảng trắng — không phải
`table/tableRow/tableCell` của ProseMirror. Ô có kiểu không phải đoạn văn; để
ProseMirror quản 20.000 ô là trả giá cho thứ không dùng.

### 11.2 `TableSource`

```ts
interface TableSource {
  readonly columns: ColumnDef[];
  readonly rows: Row[];                 // Row = { id, cells: Record<string, Value> }
  readonly capabilities: {
    addColumn: boolean; reorderRows: boolean; relations: boolean;
    total: number | null;               // bộ sưu tập: tổng số hàng khớp truy vấn
  };
  setCell(rowId: string, column: string, value: Value): void;
  insertRows(at: number, rows: Row[]): void;
  deleteRows(ids: string[]): void;
  updateSchema(change: SchemaChange): void;   // đổi tên/kiểu/công thức/view
}
```

- **Rich Table:** `id` của hàng là số sinh lúc parse, chỉ sống trong phiên. Mỗi lời
  gọi ghi là một giao dịch ProseMirror đổi thuộc tính `model` của node.
- **Bộ sưu tập:** `id` là id node. Ghi đi qua `write_node_file` (frontmatter), schema
  đi qua `Schema/<kind>.md`.

Mọi thành phần trong `components/` chỉ biết `TableSource`.

### 11.3 Bộ sưu tập cần đổi gì

- `ThingsApp.vue` dùng `DataGrid` thay `TableView` khi bố cục là `table`. Sửa tại chỗ.
- `run_node_query` trả giá trị **có kiểu** (JSON) thay vì `Vec<String>`. Không thì
  lưới phải đoán kiểu lại từ chuỗi, đúng cái việc `fieldValue.ts` được viết ra để khỏi
  phải làm.
- `Schema/<kind>.md` thêm kiểu `select` (kèm `options`) và `formula` (kèm `expr`).
  Schema vẫn là lời khuyên cho giá trị lưu; nhưng cột công thức **chỉ tồn tại trong
  schema**, nên với nó schema là định nghĩa.
- Khối ` ```query ` trong note hiện `DataGrid` thay `QueryResultTable` khi truy vấn
  là một truy vấn node. Đó chính là "linked view" của Notion, với một khác biệt: định
  nghĩa nằm trong note, đọc được.

### 11.4 Giới hạn của bộ sưu tập

Bộ sưu tập phân trang 500 hàng. Công thức cột tính ở TypeScript trên hàng đã tải — đúng
cho công thức theo hàng, **sai** cho `SUM(table[X])` nếu chưa tải hết. Chân bảng cũng
vậy.

Đường đúng là đã được vẽ sẵn: `| stats sum(x)` là một ô của ống dẫn mà tài liệu ngữ
pháp truy vấn ghi "thêm sau, không đổi cú pháp". Chân bảng của bộ sưu tập gọi nó.
Công thức cột có nhắc cả cột: tải hết (tới trần `CEILING = 5000`), hoặc báo rõ "tính
trên 500 hàng đầu". Đây là chỗ (3) ở §10.3 sẽ được đòi tới.

---

## 12. Xuất và nhập

| | |
| --- | --- |
| CSV/TSV | Xuất view đang xem (giá trị đã định dạng hoặc thô) và nhập thành Rich Table mới, đoán kiểu |
| xlsx | Desktop, qua `spreadsheet.rs`. Xuất giá trị, không xuất công thức — `spreadsheet.rs` cố ý không ghi công thức, và ngôn ngữ ở đây không phải của Excel |
| Markdown | Đã là Markdown |
| Bảng cũ → Rich Table | Sao chép rồi dán (§3.2). Rich Table → bảng cũ: sao chép ra HTML rồi dán ngoài Rich Table |
| Rich Table → bộ sưu tập | Mỗi hàng một node `type: <tên bảng>`, cột thành trường, công thức thành công thức trong schema |

---

## 13. Những thứ cố ý không làm

| Không làm | Vì sao |
| --- | --- |
| `A1`, công thức ô lẻ | §3.4 |
| Ô gộp, khối trong ô ở Rich Table | Bảng cũ vẫn còn cho việc ấy |
| Sửa hay nâng cấp bảng cũ | §3.2. Lỗi ở §1.2 là việc riêng, nếu có |
| Nút bấm, hành động (`AddRow`, `ModifyRows` của Coda) | Công thức là hàm thuần. Hành động là chuyện của Syn |
| Tham chiếu sống sang bảng ở note khác | Một thay đổi ở note A làm note B đổi file. Chỉ trong cùng note, hoặc qua bộ sưu tập |
| HyperFormula, AG Grid, Handsontable | §2.4 |
| Cộng tác thời gian thực | Synabit là phần mềm một người |
| Tự nâng cấp bảng cũ | §3.2 |

---

## 14. Lộ trình

Mỗi giai đoạn dùng được một mình. Không giai đoạn nào đổi định dạng file của giai đoạn
trước, trừ khi tăng `version`.

| # | Tên | Gồm | Xong khi |
| --- | --- | --- | --- |
| 1 | **Rich Table, không công thức** | `model` `markdown` `RichTableExtension` `DataGrid`; kiểu văn bản/số/chọn/chọn nhiều/ngày/hộp kiểm/liên kết/note; bàn phím; dán/sao chép; sắp xếp; thanh lọc viên (chỉ AND); chân bảng; dán bảng HTML (đường chuyển từ bảng cũ); tờ hàng trên điện thoại | Note có bảng mở-lưu không đổi byte; 2.000 hàng gõ không giật; dùng được trên Android |
| 2 | **Công thức** | `src/shared/formula/`; cột công thức; trình soạn có xem trước và gợi ý; `table[X]` `PREV` `LET`; nhóm hàm đầu; đổi tên cột viết lại công thức; mô tả công cụ Syn | Ví dụ §6.1 chạy hết; vòng bị bắt lúc gõ; ≥ 300 test cho bộ máy |
| 3 | **View và biểu đồ** | Nhiều view; nhóm + tổng phụ; bộ lọc nâng cao và dạng chữ; luật tô màu; `ChartView` (5 loại); `pivot`; `board` | Ví dụ §0 hiện đúng cả hai view |
| 4 | **Bộ sưu tập** | `DataGrid` trong Things; sửa tại chỗ; `run_node_query` trả kiểu; schema `select`/`formula`; khối ` ```query ` dùng `DataGrid`; chân bảng qua `\| stats sum`; Rich Table → bộ sưu tập; `bars` cho thấu kính | Things sửa được không cần mở bảng bên |
| 5 | **Thêm** | Khối biểu đồ đứng riêng; `LOOKUP` giữa các bảng trong note; công cụ `table_rows` cho Syn; xuất xlsx; tay kéo điền; công thức nội dòng (§15) | — |

Giai đoạn 1 lớn nhất, và nó là thứ phải đúng: định dạng file sống lâu hơn mọi giao
diện.

---

### 14.1 Giai đoạn 1 — đã làm (2026-10-05)

| Phần | Ở đâu |
| --- | --- |
| Mô hình, kiểu, đọc số và ngày theo ngôn ngữ | `src/shared/rich-table/model.ts` |
| Đọc/ghi Markdown, chú thích YAML | `src/shared/rich-table/markdown.ts`, `markdownIt.ts` |
| Bộ lọc (phần con của ngôn ngữ công thức) | `src/shared/rich-table/filter.ts` |
| Thay đổi bảng, hàng hiển thị, chân bảng, clipboard | `ops.ts` `view.ts` `summary.ts` `clipboard.ts` |
| Lưới, menu cột, thanh lọc, tờ hàng | `src/shared/rich-table/components/` |
| Node `richTable`, slash "Rich Table" | `src/mini-apps/note/extensions/RichTableExtension.ts`, `nodes/RichTableNodeView.vue` |

Đo trên 2.000 hàng × 6 cột: đi một ô ~5 ms, ghi một ô ~21 ms, hoàn tác ~19 ms.
Ghi một ô lúc đầu tốn ~120 ms, vì đọc lại source làm mọi hàng thành mảng mới và
`v-memo` vẽ lại cả 2.000 hàng; `shareRows` giữ lại mảng của hàng không đổi.

Khác với bản thiết kế, hoặc chưa làm:

- **Đổi thứ tự hàng/cột bằng menu** (Lên/Xuống, Dời trái/phải), chưa kéo-thả.
- **Cột Note sửa bằng ô chữ**, chưa có bộ chọn note.
- Chưa có tay kéo điền (§9.2). `Cmd+D` có.
- Chưa sửa mô tả công cụ của Syn (§10.2) — để cùng giai đoạn 2, khi có cột công thức.
- Rich Table chỉ nằm ở cấp cao nhất của note; một thay đổi đặt nó vào trích dẫn hay
  danh sách bị từ chối (§4.7).

### 14.2 Giai đoạn 2 — đã làm (2026-10-05)

| Phần | Ở đâu |
| --- | --- |
| Bộ máy công thức: đọc, kiểm, chạy, ~80 hàm | `src/shared/formula/` (`parser.ts` `compile.ts` `functions.ts` `values.ts`) |
| Tài liệu hàm cho gợi ý (Việt/Anh) | `src/shared/formula/docs.ts` |
| Cột công thức: thứ tự tính, vòng, ghi giá trị | `src/shared/rich-table/formulas.ts` |
| Tính lại trong cùng bước với lần sửa | plugin `richTableCompute` trong `RichTableExtension.ts` |
| Trình soạn công thức | `src/shared/rich-table/components/FormulaEditor.vue` |
| Mô tả công cụ cho Syn (§10.2, cách 1 của §10.3) | `src-tauri/src/syn/tools.rs` — `create_node`, `update_node` |

373 test cho bộ máy; ví dụ §6.1 chạy hết; vòng bị bắt lúc gõ.

**Đo trên 2.000 hàng** với ba cột công thức (`[Giá] * 0.1`, lũy kế bằng `PREV`, và tỉ
lệ trong nhóm `[Giá] / SUMIF(table[Giá], table[Loại] = [Loại])`): tính cả bảng ~41 ms,
ghi một ô ~115 ms. Lúc đầu là ~512 ms và ~2 s: `SUMIF` theo nhóm đọc hai cột nguyên ở
mỗi hàng, tức 2.000 × 2.000 phép so sánh. Giờ phần nào của công thức đọc cả cột mà chỉ
nối với hàng qua các ô nó nhắc (không `PREV`, không `ROW()`, không tên `LET` từ ngoài)
được **nhớ theo giá trị các ô ấy** — tính một lần cho mỗi Loại khác nhau, không phải
một lần cho mỗi hàng (`markMemo` trong `compile.ts`). Phần lớn 115 ms còn lại là vẽ
lại: một cột lũy kế làm gần như mọi hàng đổi sau mỗi lần sửa.

**Chỗ giá trị được ghi** là một plugin `appendTransaction`, không phải node view: mọi
giao dịch đổi một Rich Table được nối thêm một bước tính lại mọi bảng có công thức,
nên hoàn tác lùi cả hai, và bảng A đọc `gia[Giá]` theo kịp khi bảng `gia` đổi. Khi
một bảng có công thức được hiện ra, nó gửi một lần "làm mới" nằm ngoài lịch sử hoàn tác
— cho `TODAY()`, và cho giá trị Syn hay máy khác ghi mà không tính.

Quyết định trong lúc làm, khác hoặc chi tiết hơn §6:

- **So sánh thứ tự với ô trống cho ra trống** (`[Hạn] < TODAY()` với Hạn trống không
  phải "trễ"). Bằng nhau thì vẫn: trống bằng trống, bằng `""`.
- **So sánh thứ tự giữa số và chữ cho ra sai, không phải lỗi**: một ô sai kiểu không
  được làm hỏng cả `SUMIF(…, table[X] > 5)`. Số học với chữ thì vẫn là `#TYPE`, như Excel.
- **`IFERROR` trên danh sách thay từng phần tử lỗi**: `SUM(IFERROR(table[X] * 2, 0))`.
- **Đúng/sai được ghi thành `[x]`/`[ ]`**, nên công thức trả đúng/sai hiện như hộp kiểm.
- **`TEXT` dùng dấu của mẫu, không của giao diện**: `TEXT(1234.5, "#,##0.00")` là
  `1,234.50` ở mọi máy — kết quả được ghi vào note.
- **`WEEKDAY` theo Excel (1 = Chủ nhật), `WEEKNUM` theo ISO.**
- **Bộ lọc không nói được bằng viên** (`or`, phép tính, hàm) giờ được chạy như công
  thức, và sửa được dạng chữ từ nút Σ trên thanh lọc.
- Thêm vài hàm ngoài bảng §6.6: `ISNUMBER` `INT` `SIGN` `EXP` `LN` `LOG10` `PI` `LIST`
  `INDEX` `STARTSWITH` `VALUE` `HOUR` `MINUTE`, và tên gọi khác `AVG` `SUBSTITUTE`
  `CONCATENATE` `TEXTJOIN`.

Chưa làm:

- Nhóm hàm tài chính (`PMT` `FV` …).
- **Đổi tên cột không viết lại công thức ở bảng khác** đang đọc nó (`gia[Giá]` khi
  đổi tên cột Giá của bảng `gia`): công thức ấy thành `#NAME` cho tới khi sửa tay.
- Cách 2 của §10.3 (tính lại ngầm sau khi Syn ghi) — để cùng công cụ `table_rows`.

### 14.3 Giai đoạn 3 — đã làm (2026-10-06)

| Phần | Ở đâu |
| --- | --- |
| Nhóm hàng, đo nhóm (`Ngày by month`, `sum([Số tiền])`) | `src/shared/rich-table/aggregate.ts` |
| Dữ liệu biểu đồ: màu theo thực thể, gộp "Khác" | `src/shared/rich-table/chartData.ts` |
| Luật tô màu, thang màu | `src/shared/rich-table/rules.ts` |
| Biểu đồ (cột, thanh, đường, vùng, donut, phân tán, con số) | `components/ChartView.vue` |
| Tổng hợp, Kanban, tuỳ chỉnh view | `components/PivotView.vue` `BoardView.vue` `ViewMenu.vue` |

Ví dụ §0 hiện đúng cả hai view: "Tất cả" là lưới, "Theo loại" là biểu đồ cột.

**Biểu đồ theo kỹ năng dataviz.** Bảng màu tám chuỗi được kiểm bằng
`validate_palette.js` trên chính nền của app — `#ffffff` sáng, `#1e1e1e` tối — và qua
mọi kiểm tra; ở nền sáng ba màu dưới 3:1 tương phản, được bù bằng nhãn trực tiếp và
nút "Số liệu" (bảng số cùng biểu đồ). Màu đi theo thực thể: một chuỗi giữ màu theo vị
trí của nó trong cả bảng, nên lọc bớt không đổi màu phần còn lại. Quá tám chuỗi thì
gộp "Khác"; phân tán dừng ở ba, donut ở sáu. Cột ≤ 24px bo đầu 4px, khe 2px giữa các
đoạn xếp chồng, lưới mảnh liền nét, chú giải khi có từ hai chuỗi, đường dóng cho
đường/vùng, tooltip cho mọi dạng.

Quyết định trong lúc làm:

- **View đang chọn không ghi vào file.** Chọn tab là nhìn, không phải sửa; mở note
  không làm file đổi. Nhóm đang thu gọn và lọc tạm từ biểu đồ ghim cũng vậy.
- **Bấm một cột của biểu đồ ghim** thu lưới về nhóm đó bằng một viên lọc tạm (không
  lưu); bấm lần nữa thì bỏ.
- **Lưới nhóm theo giá trị đầu** của ô chọn-nhiều: một hàng của lưới chỉ ở một chỗ.
  Biểu đồ và Kanban thì đếm hàng trong mọi nhóm của nó, như Notion.
- **Cột ẩn vẫn nằm trong DOM** (`display: none`), để chỉ số ô vẫn là chỉ số cột; đi
  bằng phím tự bỏ qua cột ẩn.
- **Kanban chỉ theo cột Chọn.** Kéo thẻ dùng sự kiện pointer, không dùng kéo-thả HTML
  (màn hình cảm ứng không có); bàn phím: Alt+←/→ chuyển thẻ sang làn bên cạnh.
- Ví dụ §0 trong tài liệu này từng là YAML hỏng: `y: sum([Số tiền])` trong `{ … }`
  phải có ngoặc kép. Đã sửa; bộ ghi của app vẫn luôn tự thêm ngoặc kép.

Chưa làm:

- Nhóm AND/OR lồng nhau bằng viên (§7.2). Thay vào đó: bộ lọc dạng công thức (giai
  đoạn 2) nói được mọi thứ.
- Công thức tuỳ chọn trên `values` cho chân bảng và trục y (§7.4, §8.1).
- Xuất biểu đồ ra PNG (§8.3).
- Độ rộng cột riêng cho từng view: độ rộng vẫn nằm ở cột, dùng chung mọi view.

### 14.4 Phần lẻ — đã làm (2026-10-06)

- **Thêm cột bằng nút +** ở cuối hàng tên cột; cột mới mở sẵn menu với tên được chọn để gõ đè.
- **Menu chuột phải trên ô**: chèn hàng/cột, sao chép, xoá nội dung, mở hàng, xoá hàng/cột.
- **Kéo thả** để đổi thứ tự hàng (kéo số thứ tự) và cột (kéo tên cột). Hàng chỉ kéo
  được khi view không sắp xếp hay nhóm — khi đó "ở trên" trên màn hình không phải "ở
  trên" trong file.
- **Độ cao hàng theo view**: thấp, vừa, cao, rất cao (`rowHeight`), như Airtable; mọi
  hàng của view cao như nhau, chữ dài xuống dòng tới số dòng vừa.
- **Độ rộng cột theo view** (`widths`), như §7.1 định; `width` của cột vẫn đọc được,
  làm giá trị mặc định.
- **Bộ chọn note** cho cột Note, tìm như `@`; ghi `[[Notes/x.md|Tiêu đề]]` — hiện tiêu
  đề, mở đúng file.
- **Hàm tài chính** `PMT` `FV` `PV` `NPV` `IRR`, kiểm bằng chính các ví dụ của Excel.
- **Đổi tên cột viết lại công thức ở bảng khác** đọc nó qua tên bảng, trong cùng một
  bước hoàn tác. Tên bảng có gạch nối (`chi-tieu[Số tiền]`) giờ đọc được — trước đó nó
  là phép trừ.
- **Xuất biểu đồ ra PNG**.

### 14.5 Giai đoạn 5 — đã làm (2026-10-06)

| Phần | Ở đâu |
| --- | --- |
| Khối biểu đồ đứng riêng `<!-- rich-chart of="…" view="…" -->` | `RichChartExtension.ts`, `nodes/RichChartNodeView.vue` |
| Công thức giữa câu `` `=SUM(chi-tieu[Số tiền])` `` | `extensions/InlineFormula.ts` |
| Công cụ `table_rows` cho Syn | `src-tauri/src/syn/rich_table.rs`, nhóm `tables` |
| Xuất CSV và Excel | `src/shared/rich-table/export.ts`, lệnh `export_table_xlsx` |
| Tay kéo điền | `fillValues` trong `ops.ts` |

`LOOKUP` giữa các bảng trong note đã có từ giai đoạn 2.

Quyết định trong lúc làm:

- **Công thức giữa câu chỉ là công thức khi nó biên dịch được** với các bảng có tên
  của note; `` `=x + 1` `` trong một ví dụ code vẫn là code. Con trỏ vào trong thì hiện
  lại mã. Một phép đếm không mang định dạng tiền của cột nó đếm.
- **`table_rows` giữ nguyên mọi dòng nó không đổi**, đệm khoảng trắng và tất cả: thêm
  một hàng là một dòng mới trong file. Không ghi vào cột công thức — app tính khi note
  được mở; nếu note đang mở thì việc tính lại xảy ra ngay khi nội dung mới được nạp.
  Nó nằm trong nhóm `tables`, nên lượt nào không nhắc tới bảng thì không tốn token;
  và như mọi công cụ sửa dữ liệu, nó bị từ chối sau khi Syn đọc thứ gì viết từ bên
  ngoài vault.
- **Xuất xlsx** đi qua một lệnh Rust ghi vào thư mục tạm rồi chép sang chỗ người dùng
  chọn, như xuất kho vault. Số là số, hộp kiểm là đúng/sai; một ô bắt đầu bằng `=` được
  ghi thành chữ, không thành công thức. CSV có BOM để Excel đọc đúng tiếng Việt.
- **Khối biểu đồ đặt tên cho bảng nếu bảng chưa có**, vì biểu đồ chỉ trỏ được tới bảng
  qua tên; menu ⋯ của bảng có ô "Tên bảng".
- **Tay kéo điền**: hai số hay hai ngày cách đều thì đi tiếp theo bước đó; một ngày thì
  tăng từng ngày; chữ tận cùng bằng số (`Tuần 3`) thì đếm tiếp; còn lại thì lặp.
- **`yaml` vào danh sách gói đã xem xét kích thước cho Android** (`timeline/media.rs`):
  không thêm byte nào — cùng bản 2.9.0 mà `markmap-lib` đã đưa vào bản build từ trước.

Giai đoạn 4 (Bộ sưu tập) để lại: chưa thấy cần.

### 14.6 Rà soát cuối — đã sửa (2026-10-06)

Bốn hướng rà (dữ liệu & định dạng file, lưới & thao tác, công thức, Syn & xuất), khoảng 55 điểm; đã sửa hết, mỗi điểm có test.

**Không mất dữ liệu**
- Undo sau khi Syn/thiết bị khác vừa sửa bảng: chỉ lấy lại thay đổi của mình, ghép theo dòng lên bản hiện tại (`merge.ts`, `rebase`); dòng xung đột thì giữ bên kia.
- Bảng trong Details không còn bị khoá; bảng lọt vào list/quote thì không cho chèn sai chỗ.
- Comment `<!-- rich-table` mất bảng (bảng hỏng, nằm trong list…) được giữ nguyên chữ thành khối riêng có ghi chú, thay vì bị xoá khi lưu.
- Có chữ sau `-->` hoặc comment không đóng → bảng chỉ đọc, không ghi đè.
- Cấu hình cột “mồ côi” không còn đè lên cột thật trùng tên; tên đó bị chặn khi đổi tên cột.
- Comment `rich-chart` giữ thuộc tính lạ; tên view có `--` được mã hoá `&#45;`. Tên bảng không được chứa `--` hay kết thúc bằng `-`.
- Block-id (`^abc123`) không còn bị gắn vào dòng YAML trong comment (Rust `nodes.rs`).

**Công thức đúng hơn**
- Số nguyên lớn giữ đủ chữ số; số lẻ cắt 12 chữ số nhưng không cắt vào phần nguyên. Khoá memo theo giá trị thật.
- Kết quả trống là trống (không lấy lại giá trị cũ trong file). Giá trị chỉ khác khoảng trắng/`<br>` không bị ghi lại mỗi lần mở.
- Bảng đọc bảng: tên trùng → không ghi và báo; bảng nguồn không đọc được → không ghi `#NAME`; hai bảng đọc vòng nhau → không ghi (trước đây số tăng mỗi lần mở). Có thông báo trên bảng, và ô tên bảng báo “đã có bảng tên này”.
- `NOW()` không ghi lại khi chỉ mở note (chỉ khi sửa). Lỗi đã lưu của cột công thức bảng khác vẫn là lỗi; chữ `#DIV0` gõ trong ô văn bản là chữ.
- Engine: điều kiện trên list = “có phần tử nào đúng” (`[Nhãn] = "x"`), TEXT theo mẫu Excel (`dd/mm/yyyy`, `hh:mm`, `mmm`, `#.##`, không `-0.00`), đổi tên cột không đụng biến LET, PREV của biến LET, LOOKUP khoá trống, ROUND… trên list, chuẩn hoá NFC, MIN/MAX không tràn stack, NETWORKDAYS O(1) + ngày nghỉ, WEEKNUM theo Excel + ISOWEEKNUM.

**Lưới**
- Ô đang sửa được chốt lúc bắt đầu sửa; Enter đi xuống đúng hàng dù sort làm hàng nhảy chỗ; hàng vừa sửa vẫn hiện dưới filter tới khi đổi filter.
- Ô hiện hành không nằm trên cột ẩn; copy/fill/paste bỏ qua cột ẩn; dán có header vào bảng trống giữ view và đặt tên trùng đúng (`A, A 2, A 3`).
- Xoá nhiều hàng từ menu hàng (trước đây không xoá gì), “áp thứ tự” sort cả bảng thay vì theo thứ tự đang lọc/nhóm.
- RowSheet: focus vào ô đầu, Esc lưu rồi đóng, Tab quay vòng, theo đúng hàng khi hàng phía trên bị thêm/xoá.
- Filter: panel nhận focus ngay, phím không lọt xuống lưới.
- Kéo/fill/resize/board: huỷ đúng khi `pointercancel`, dọn khi bảng biến mất; thẻ board cuộn được bằng ngón tay (`pan-y`), bảng chỉ đọc vẫn mở được thẻ.
- Menu: bấm lại nút mở thì đóng; ↑↓ Home End; focus trả lại khi đóng; Menu key / Shift+F10 mở menu ô; tiêu đề cột và tiêu đề nhóm dùng được bằng bàn phím; screen reader đọc “cột, hàng: giá trị” và lựa chọn đang chọn.
- Dark mode cho panel nổi (màu chữ phụ, accent, `color-scheme`).
- Hiệu năng: tổng cuối cột tính một lần, lỗi ô cache theo bảng, rules đọc cột một lần, FormulaEditor tính thử sau 120ms, khối chart không parse lại mọi bảng mỗi phím.

**Syn và xuất**
- `table_rows`: bỏ qua code fence, chỉ bảng cấp cao nhất với comment ngay dưới, gộp ô thừa như app, giữ `^id`, từ chối YAML hỏng/version mới, nhận cột công thức qua YAML thật, so khớp số/checkbox/`<br>` theo nghĩa, giữ CRLF; hỏi lại khi note có nhiều bảng; bớt từ khoá gây nhầm (“cửa hàng”, “hàng ngày”, “bằng”, “đồng”).
  Tool chặt hơn app: bảng có dòng trống giữa bảng và comment app vẫn hiển thị, nhưng Syn sẽ báo “không có Rich Table”.
- CSV chống chèn công thức (`=`, `+`, `-`, `@` không phải số → thêm `'`); Excel ẩn trên điện thoại; tên sheet và ô dài theo giới hạn Excel; lỗi xuất PNG hiện ra cho người dùng.

## 15. Câu hỏi mở

1. *(Giai đoạn 5 đã làm: có, chỉ khi biên dịch được — §14.5.)* **Công thức nội dòng trong văn bản.** Coda cho `=Tasks.Count()` nằm ngay trong một
   câu. Ở đây có thể là: một đoạn code nội dòng bắt đầu bằng `=` —
   `` `=SUM(chi-tieu[Số tiền])` `` — được hiện thành giá trị sống. Ở trình soạn khác nó
   vẫn là code, đọc được. Đẹp, rẻ, nhưng làm mọi đoạn code nội dòng bắt đầu bằng `=`
   đổi nghĩa. Có muốn không?
2. *(Giai đoạn 1 đã chọn: theo ngôn ngữ giao diện — tiếng Việt `45.000` là bốn mươi
   lăm nghìn, `1,5` là một phẩy năm; tiếng Anh ngược lại. Chưa nhận `45k`, `1,5tr`.)*
   **Ô số chấp nhận gì khi gõ?** `45.000` là 45 nghìn hay 45 phẩy không? Theo ngôn ngữ
   giao diện (tiếng Việt: dấu chấm là nghìn) là đúng nhất và lạ nhất với ai quen bảng
   tính tiếng Anh. Còn `45k`, `1,5tr`?
3. *(Giai đoạn 2 đã làm theo đề xuất: có ghi.)* **Cột công thức ghi giá trị vào file (§4.5).** Đề xuất: có. Cái giá là file đổi khi
   công thức có tổng và một ô bất kỳ đổi. Nếu không ghi, file sạch hơn nhưng mọi thứ
   ngoài trình soạn (Syn, tìm kiếm, GitHub) thấy ô trống.
4. **Tên `table`** cho "bảng này" trong `table[X]`. Hay `this[X]`, hay `bảng[X]`? Một
   từ khoá tiếng Việt trong ngôn ngữ có hàm tiếng Anh thì lạ; một từ tiếng Anh thì
   người dùng tiếng Việt phải học thêm một từ.
5. **Giới hạn cỡ Rich Table.** Đề xuất: cảnh báo ở 2.000 hàng, gợi ý chuyển thành bộ
   sưu tập; không chặn.
