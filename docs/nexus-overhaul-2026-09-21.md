# Nexus — lắp cỗ máy đã có vào đúng chỗ

**2026-09-21.** Rà soát hiện trạng Nexus, thiết kế đề xuất, và thứ tự làm.

Tiếp nối `docs/nexus-lenses-2026-09-19.md` và `docs/query-grammar-2026-09-20.md`.

---

## 1. Một câu

**Cỗ máy hỏi cho dòng thời gian đã tồn tại, đã đủ tốt, và đang bị chôn sau một cái
nút tên là "Tua lại".**

Hai tài liệu trước đã xây xong: ngữ pháp có nguồn / ống dẫn / `OR` / ngoặc, chip hai
chiều, kệ thấu kính, hình dạng tự chọn, lớp từ chối có chữ, 2.400 test Rust. Không
thiếu thiết kế. Thiếu **chỗ lắp**.

Nên tài liệu này chủ yếu là **di chuyển và xoá**, không phải xây mới. Đó là dấu hiệu
chẩn đoán đúng: nếu chữa một vấn đề trải nghiệm mà phải viết thêm một cỗ máy thứ hai,
thì cái chẩn đoán sai chứ không phải cỗ máy.

---

## 2. Chỗ nó chưa được lắp

`LensBar` — nơi ở của toàn bộ ngôn ngữ truy vấn — được mount **đúng một chỗ**
(`src/mini-apps/nexus/NexusApp.vue:524`): trong slot `#ask` của `TimeStrip`, mà
`TimeStrip` chỉ render khi `lookingBack && timeFrame`, và cả khối ấy lại nằm trong
`v-if="graphData && shownAs === 'graph'"`.

Ba điều kiện chồng lên nhau để một ô chữ hiện ra.

---

## 3. Ba trạng thái của màn hình

| Trạng thái | Ô hỏi | Engine | Ghi sự kiện | Duyệt đề xuất |
| --- | --- | --- | --- | --- |
| Graph (mặc định) | ô trên cùng | `search_nexus` (FTS cũ) | ✅ | ❌ |
| Graph + Tua lại | ô trên cùng **+ `LensBar`** | cả hai | ✅ | ✅ |
| **Dòng thời gian** | ô trên cùng | `search_nexus` (FTS cũ) | ❌ | ❌ |

Dòng cuối là vấn đề, và nó là một câu gọn: **dòng thời gian là view duy nhất không
hỏi được bằng chính ngôn ngữ viết ra cho nó.**

Đường để hỏi một câu về thời gian hôm nay: về hình dạng Graph → bấm *Tua lại* → chờ
`timeline_frame` dựng xong cho cả vault → ô chữ hiện dưới thanh trượt.

---

## 4. Hai ô tìm kiếm, và cái này chỉ tay sang cái kia

Ô trên cùng gọi `search_nexus` → `ParsedQuery::of()`, tức hình chiếu phẳng của cây.
Gặp ống dẫn, `OR`, hay từ khoá dòng thời gian, nó từ chối bằng đúng hai câu này
(`src-tauri/src/refusal.rs:220`):

```
{0} has to be asked in the query bar
{0} is more than this search box can ask. Ask it in the query bar.
```

App bảo người ta sang query bar. Query bar nằm sau nút *Tua lại*, và chỉ ở hình dạng
Graph. Không có gì trên màn hình nói ra điều đó.

Lớp từ chối làm đúng việc của nó. Chỗ hỏng là **cái cửa nó trỏ vào thì khoá**.

---

## 5. Toggle Graph/Timeline nói dối về dữ liệu

`NexusApp.vue:134`:

```ts
searchQuery ? asTimeline(searchResults, …)   // ← NODE (FTS), xếp lên ngày
            : wholeTimeline                   // ← EVENT (events sort:-when limit:300)
```

Cùng một toggle, **hai bảng khác nhau**. Không gõ gì thì thấy sự kiện; gõ một chữ thì
sự kiện biến mất, thay bằng note xếp theo ngày. Không có gì báo.

Đây là §4 của `query-grammar` bị vi phạm ở tầng giao diện: *nguồn phải nói ra mặt*.
Ở đây nguồn không những ngầm, mà còn đổi theo việc ô chữ có rỗng hay không.

---

## 6. Bốn lỗi

### 6.1 Bấm người/tag trên graph ở trạng thái mặc định là click chết

`NexusApp.vue:396`:

```ts
if (node.item_type === 'person') { await bar.value?.press('with', node.title); return; }
```

`bar` là `null` trừ khi đang Tua lại. Optional chaining nuốt im lặng, rồi `return`
chặn luôn đường mở node phía dưới. **Hai loại node được bấm nhiều nhất không làm gì
cả** — không lọc, không mở.

### 6.2 Dòng thời gian không bao giờ hết hạn

`NexusApp.vue:141` — `if (wholeTimeline.value) return;` — và `reload()` (`:339`)
không xoá nó. Ghi xong một sự kiện, dòng thời gian vẫn là danh sách cũ cho tới khi
khởi động lại app.

### 6.3 Phân trang cụt

`DatedView` in *"còn N dòng nữa"* (`src/shared/views/DatedView.vue:127`) nhưng không
đâu tăng `offset`. Dòng thời gian chặn cứng ở 300 kèm một lời trêu.

### 6.4 Ghim được gỡ nhưng không ghim được

`timeline_pin` đăng ký trong `lib.rs` mà không UI nào gọi, trong khi `timeline_pins`
và `timeline_unpin` thì có.

---

## 7. Đề xuất của AI bị chôn sâu nhất

`timeline_extract_run` chạy nền mỗi mười phút (`src/App.vue:1000`). Nơi duy nhất thấy
kết quả là `ExtractTray`, nằm trong hàng `#actions` của `TimeStrip`. Đường đi:

> Graph → Tua lại → chờ `timeline_frame` → hàng nút dưới đáy → popover.

Không có badge ở bất cứ đâu trong vỏ app.

Và trong chính popover ấy, danh sách duyệt nằm **dưới** khoảng sáu mươi dòng cấu
hình: bật/tắt, cảnh báo cloud, ước lượng phút, ba nút chạy lại, danh sách file không
đọc được. Hàng chờ bị cấu hình đè lên.

Đây là thứ vòng đời *AI đọc node → đề xuất → người duyệt* đứng hoặc ngã. Nó đang ở
chỗ khó tới nhất của màn hình.

---

## 8. "Rườm rà", đo cụ thể

Năm panel còn lại, tất cả nhét vào một hàng ở đáy `TimeStrip`:

| Panel | Dòng | Vào được từ đâu |
| --- | --- | --- |
| `ReflectPanel` | 396 | hai chỗ khác nhau, khác `align` |
| `ExtractTray` | 336 | chỉ trong Tua lại |
| `RefusalsPanel` | 212 | chỉ trong Tua lại |
| `MomentsPanel` | 172 | chỉ trong Tua lại |
| `AskPanel` | 153 | chỉ trong Tua lại |
| **Tổng** | **1.269** | |

`nexus-lenses` §2 gọi "một hàng chip kín" là triệu chứng, rồi §17 quyết định giữ năm
cái này vì mỗi cái mang một **cử chỉ tầng 2** — duyệt, từ chối, đồng thuận — chưa
diễn đạt được bằng truy vấn.

**Lý do giữ thì đúng. Chỗ đặt thì sai.** Chúng bị dồn vào cái ngăn kéo tên "Tua lại"
chỉ vì đó là nơi còn chỗ trống, không vì chúng có liên quan gì tới việc nhìn lại quá
khứ.

Còn `TimeStrip` (423 dòng: scrub, play, seal, reveal) là một **tính năng trình diễn**
đang chiếm đúng cái slot mà ô hỏi cần.

---

## 9. Rác

- **15 khoá i18n chết × 2 ngôn ngữ**: `onthisday_*` (5), `silence_*` (6), `year_*`
  (4). Panel đã xoá ở bước 8 của `nexus-lenses`; chuỗi còn nguyên.
- **Lệnh đăng ký mà không UI nào gọi**: `timeline_query`, `timeline_rebuild`,
  `timeline_pin`, `timeline_hush`, `ledger_verify`.

---

## 10. Cái đang tốt — không đụng

Graph. Ngữ pháp truy vấn. `QueryResult` một hình dạng cho cả hai bảng. `shapeFor` đọc
*giá trị* chứ không đọc tên cột. Lớp đồng thuận chạy **trước** khi dựng câu trả lời
(`VaultWords`). Chip là chữ cắt ra, không phải mô hình thứ hai.

Kế hoạch dưới đây không viết lại gì trong số đó.

---

## 11. Thiết kế: một câu hỏi, một câu trả lời được vẽ

> Không có "màn hình Graph" và "màn hình Timeline". Có **một câu hỏi** và **một câu
> trả lời được vẽ**. Nguồn nằm trong câu hỏi, không nằm trong cái toggle.

### Đ1 — Một ô hỏi duy nhất, trên cùng, ở mọi hình dạng

`LensBar` ra khỏi `TimeStrip`, lên đúng vị trí ô tìm kiếm hiện tại. `search_nexus`
biến khỏi Nexus.

**Giá phải trả, nói thẳng.** `search_fts` xếp hạng bằng bm25 (title×10, tags×5,
content×1, props×3) và trả `snippet(…, '<mark>', …)` (`src-tauri/src/db/search.rs:415`).
`run_node_query` **khớp cùng một bảng FTS** (`src-tauri/src/db/node_query.rs:341` —
`search_index MATCH`) nhưng **không có rank, không có snippet**.

Nên đây không phải "một engine thứ hai" — là **hai thứ thiếu, trên cùng một bảng**.
Việc thật: cho đường node-query gắn `bm25()` và `snippet()` khi câu hỏi mang một từ
trần, rồi viết `HitsView.vue` làm primitive thứ năm. Cột kết quả bên trái **chính
là** `HitsView`.

> **Cổng chặn.** Chạy hai mươi câu tìm kiếm thật qua cả hai đường, so thứ tự. Nếu tệ
> hơn thì dừng, giữ `search_nexus` làm bộ chạy cho nguồn `nodes` có từ trần, và ghi
> ra đó là nợ. Tìm kiếm đang tốt; không được để nó xấu đi.

### Đ2 — Toggle thành hình dạng, `graph` là hình dạng thứ năm

Hôm nay toggle chọn *nguồn ngầm*. Sửa: nguồn là **chip đầu tiên** trong câu hỏi (đã
có), toggle trên màn hình chỉ chọn cách vẽ — và nó chính là hàng `SHAPES` mà
`LensBar` đã dựng.

```
events when:this-year            → dated
nodes #gia-đình                  → graph
events | stats count by month    → bars
```

Bấm nút đè lên được, và lựa chọn đi vào thấu kính. Luật *`auto` không ghi ra file*
giữ nguyên.

**Cắt phạm vi cố ý.** `GraphView` (968 dòng) chưa theo hợp đồng view primitive —
nó nhận `graphData` + `matchIds`, không nhận `QueryResult`. **Không** ép nó vào hợp
đồng ở bước này. Chỉ cần `graphMatchIds` đọc từ `result.rows.map(r => r.open ?? r.id)`
thay vì gọi `search_nexus_ids`. Một dòng, xoá một lệnh.

### Đ3 — Ba thứ "rườm rà": đặt lại chỗ, không bỏ

Chẩn đoán: **chúng không rườm rà tự thân — chúng chiếm chỗ của thứ dùng hằng ngày.**

| | Hôm nay | Đề xuất |
| --- | --- | --- |
| **Tua lại** | nút hạng nhất, mở cả thanh trượt + năm panel | **thành một chip.** Câu hỏi có `when:` thì thanh trượt hiện dưới graph. Tua lại là *một câu hỏi về thời gian*, không phải một chế độ. Xoá `lookingBack`. |
| **Ghi lại** | nút cạnh Tua lại, chỉ ở graph | **nút thường trực cạnh ô hỏi**, có ở mọi hình dạng. Ghi một sự kiện là việc hằng ngày; nó phải ở chỗ dễ nhất. |
| **Đề xuất** | popover trong popover, sau ba cú bấm | **hàng một: badge số đang chờ cạnh ô hỏi** → mở **màn hình duyệt toàn khung**. Cấu hình (cloud/local/ước lượng) sang Cài đặt — đó là cấu hình, không phải hàng chờ. |
| **Chiêm nghiệm** | 396 dòng, mount hai chỗ | **ra khỏi Nexus.** Quyết định là một *loại node có vòng đời*, không phải một câu hỏi. Nexus chỉ cần *nhắc* khi có cái đến hạn. |
| **Khoảnh khắc** | panel riêng | **hình dạng `gallery`** — §17 của `nexus-lenses` đã ghi đúng lý do nó chưa đi được. |
| **Đã từ chối** | panel riêng | Cài đặt, hoặc thành thấu kính khi hush/pin trở thành node. |
| **Về chuyện này** | nút riêng | một *nudge* trong chính dòng kết quả, không phải một nút. |

### Đ4 — Dòng thời gian phải ghi được và cuộn được

`wholeTimeline` hết hạn theo bus event. `data-dated-more` thành nút tăng `offset`.

### Đ5 — Hàng rào không được phá

1. **Đường số một phải dùng trọn vẹn với ô hỏi cuộn lại và không ai mở ra bao giờ**
   (`nexus-lenses` §10.4). Mỗi bước phải kiểm lại điều này.
2. **Cú pháp không đổi.** Thấu kính đã lưu trong vault người ta là cam kết vĩnh viễn.
3. **Lớp đồng thuận đi trước.** `VaultWords` lọc *trước khi dựng*, không phải lọc
   khỏi câu trả lời. Không bước nào được lách qua.

---

## 12. Thứ tự làm

Mỗi bước dùng được ngay và lùi lại được, theo quy ước §16 của tài liệu Timeline.

| | Việc | Xong thì được gì | Rủi ro |
| --- | --- | --- | --- |
| **0** | Chụp ảnh hành vi ba trạng thái: cái gì hiện, ai gọi lệnh nào | bước 1–4 là *di chuyển*, và di chuyển không được làm mất gì | không |
| **1** | `LensBar` lên hàng đầu, luôn hiện. `bar` không bao giờ `null`. `wholeTimeline` hết hạn | **sửa ba lỗi + mở khoá ngôn ngữ cho dòng thời gian.** Không đụng một dòng Rust | thấp |
| **2** | `graphMatchIds` đọc từ kết quả lens. `bm25` + `snippet` cho node-query. `HitsView` | **một engine.** Xoá `search_nexus`, `search_nexus_ids` khỏi Nexus | ⚠ xếp hạng — có cổng chặn ở Đ1 |
| **3** | Toggle thành hình dạng, `graph` vào `SHAPES`. Xoá `shownAs` / `asTimeline` / `timelineAnswer` | hết nhị nguyên hai database trên giao diện | trung bình |
| **4** | Tua lại thành chip `when:`. Xoá `lookingBack` | `TimeStrip` thôi làm ngăn kéo | thấp |
| **5** | Đề xuất lên hàng một: badge + màn hình duyệt. Cấu hình sang Cài đặt | **thứ cốt lõi thôi vô hình** | thấp |
| **6** | Ghi lại cạnh ô hỏi. Phân trang `DatedView` | dòng thời gian ghi được, cuộn được | thấp |
| **7** | Chiêm nghiệm ra khỏi Nexus. Khoảnh khắc → `gallery`. Đã từ chối → Cài đặt | −1.269 dòng panel | trung bình |
| **8** | Dọn: 30 chuỗi i18n chết, 5 lệnh không ai gọi, quyết `timeline_pin` | trả nợ | không |

**Nếu chỉ làm được một bước: làm bước 1.** Rẻ nhất, sửa ba lỗi, và mở khoá toàn bộ
ngôn ngữ truy vấn cho dòng thời gian mà không đụng Rust.

---

## 13. Cổng cho cả tài liệu

`nexus-lenses` §9 hứa *"code ít đi, không nhiều lên"*, và tới §17 mới trả được
−1.178 dòng. **Kế hoạch này phải làm con số đó âm thêm, không phải dương lại.** Đo
lại ở bước 8 và ghi ra.

Cổng thứ hai, chép từ tài liệu ấy vì nó vẫn là phép thử đúng: **một câu hỏi mà hôm
nay cần một panel mới, sau khi xong phải trả lời được không thêm một dòng Rust hay
Vue nào.**

---

## 14. Hai chỗ chưa quyết

1. **Bước 2** — nếu bm25 trên đường node-query xếp hạng kém hơn `search_fts`: lùi về
   giữ hai đường, hay đầu tư sửa ranking cho đúng?
2. **Bước 7** — Chiêm nghiệm ra khỏi Nexus thì đi đâu: mini-app riêng, hay là một
   loại node sống trong Note/Things?

---

## 15. Đo, lúc viết tài liệu này

| | |
| --- | --- |
| Front-end Nexus | 6.581 dòng (kể cả test) |
| `src-tauri/src/timeline/` | 15.226 dòng |
| `vue-tsc` | sạch |
| Test TypeScript | 169 file / 1.922 test, pass |
| Phiên bản | 0.9.11 |

---

## 16. Cách gọi — 2026-09-22

**Event là thứ đã lên lịch. Moment là thứ đã xảy ra.**

Hai thứ này từng cùng tên *event*, mà nghĩa ngược nhau: một cuộc hẹn trên
Calendar có thể ở tương lai và lặp hằng tuần; một dòng trên dòng thời gian thì
chỉ là thứ đã xảy ra (prompt bóc tách loại hẳn "plans, intentions and anything
that has not happened yet"). Và cái thứ nhất còn chảy thành cái thứ hai — một
event không lặp lại thành một dòng `kind = event` trên timeline — nên
`events is:event` từng là "những event đến từ event".

| Khái niệm | Người dùng thấy (vi / en) | Viết trong truy vấn | Trên đĩa | Trong code (giữ nguyên) |
| --- | --- | --- | --- | --- |
| Lịch hẹn | sự kiện / event | `nodes is:event` | node `type: event` | `event` |
| Thứ đã xảy ra | khoảnh khắc / moment | `moments` (bí danh: `events`) | frontmatter `moments:` với thứ ghi tay hoặc đề xuất đã giữ; các loại khác suy ra từ trường của node | bảng `events` trong `timeline.db`, `Source::Events`, `timeline_write_event`, `EventsOverTime.vue` |
| Cụm ảnh chụp gần nhau | (không còn giao diện) | — | — | `MediaCluster`, trước là `MediaMoment` |

- **`events` vẫn là bí danh**, không bị từ chối như `notes` từng bị: thấu kính
  đã lưu và câu hỏi trợ lý đã viết dùng nó, và nó vẫn chỉ đúng một bảng.
  `search_gate.txt` xác nhận: 116 câu hỏi, chỉ một dòng đổi — câu từ chối.
- **Tên nội bộ không đổi**: không ai ngoài code đọc chúng. Code mới dùng tên
  đúng từ đầu (`timeline_update_moment`, không phải `…_event`).
- **"Dòng thời gian / Timeline"** là tên nơi chốn, không phải tên đơn vị, nên
  tab vẫn giữ tên đó.
- **"Moment" trong bản ghi âm** ("to point at a moment, cite …#t=START,END") là
  tiếng Anh thường, một thời điểm trong audio — không phải thuật ngữ này.

---

## 17. Vì sao mở Nexus lần đầu lâu — đo 2026-09-23

Ba thứ, đo trên vault thật (975 node, 10,8 MB chữ):

**1. 10,8 MB gửi qua IPC cho một biến không ai đọc.** Khi mount, `loadAllData` gọi
`get_nexus_items` *song song* với `get_nexus_graph_data`, rồi gán kết quả vào `allItems` —
một `ref` **chỉ được ghi, không chỗ nào đọc**. Danh sách nó từng phục vụ đã thành kết quả
tìm kiếm, và ô xem trước nó nuôi đã thành `edit-item`: `selectedItem` giờ chỉ được gán
`null`, nên cả khối template `v-if="selectedItem"` là code chết không bao giờ chạy được.
Riêng khâu serialize mất 189 ms ở bản debug, chưa kể truyền và parse. **Đã gỡ** cả lời gọi,
cả biến, cả ô xem trước, và cả lệnh `get_nexus_items` — nó không còn ai gọi.

**2. Đồ thị vẽ cả những node không ai viết.** `left_out_of_nexus` có danh sách loại trừ
riêng, không dùng danh sách của `db::internal` — đúng cái lỗi *"nửa luật được viết lại lần
nữa ở chỗ khác"* mà module đó ra đời để dẹp. Nên 320 trong 840 node của đồ thị là loại
`json`: state của RSS và các bản sao xung đột của nó, hình học whiteboard. Giờ dùng chung
`syn::tools::is_internal_type`: **840 → 511 node**, JSON của đồ thị 180 → 124 KB, và cái
người ta nhìn thấy là thứ họ đã viết.

**3. Bố cục lực chạy lại từ đầu, và chỉ lần đầu.** `getFilteredData` coi là *settled* khi
hơn một nửa số node có vị trí cũ nhớ lại được; lần đầu thì không có gì để nhớ, nên
`alpha(1)` và mô phỏng chạy **300 tick** — ở 60 fps là **~5 giây** đồ thị xoay trước khi
đứng yên. Số tick không đổi theo số node (`alphaDecay` cố định), nhưng chi phí mỗi tick thì
có: 632 ms CPU ở 840 node, 339 ms ở 511. Từ lần thứ hai trở đi vị trí được nhớ nên chỉ
`alpha(0.3)`.

Thứ (3) **chưa đụng vào**: nó là cảm giác của sản phẩm, không phải lỗi. Muốn ngắn lại thì
tăng `alphaDecay` (0,045 ≈ 150 tick ≈ 2,5 s), hoặc chạy trước vài chục tick không vẽ rồi mới
hiện — cả hai đều đánh đổi.

Phép đo giữ trong repo: `commands::nexus::what_opening_nexus_costs` (`#[ignore]`, chạy tay
với `SYN_EVAL_CACHE`).
