# Ngữ pháp truy vấn Synabit

**2026-09-20.** Đặc tả. Chưa có dòng code nào viết theo nó — đây là thứ để tinh chỉnh
trước, sửa sau.

---

## 1. Một câu

**Một biểu thức lọc kiểu Lucene, rồi một ống dẫn kiểu Splunk** — hai dòng dõi đã có,
ghép lại, không phát minh gì mới.

```
[nguồn]  <lọc>  |  <giai đoạn>  |  <giai đoạn>  …
```

---

## 2. Vì sao mượn, và mượn của ai

| Ngôn ngữ | Nửa lọc | Nửa biến đổi |
| --- | --- | --- |
| Lucene / Kibana | `key:value`, `AND OR NOT`, ngoặc, khoảng | — |
| GitHub, Gmail | `key:value`, `-x`, `OR` | — |
| Splunk (SPL) | `key=value` | `\| stats … \| where …` |
| Kusto (KQL) | tên bảng dẫn đầu | `\| where … \| summarize … by …` |

Splunk và Kusto **đều** là "một biểu thức lọc rồi ống dẫn". Biểu thức lọc được sao
chép nhiều nhất là kiểu Lucene/GitHub — và Synabit vốn đã dùng dở kiểu ấy
(`is:` `#tag` `status:` `-x`). Nên hình dạng này không phải lựa chọn thẩm mỹ: nó là
chỗ hai thứ đang có gặp nhau.

**Thứ không mượn:** SQL (sai hình dạng cho một thanh tìm kiếm), Datalog (mạnh nhưng
người thường không đọc nổi), bộ lọc bấm-chuột kiểu Notion (không lưu thành chữ được,
nên không thành thấu kính được).

---

## 3. Văn phạm

```
query      := [ source ] expr { "|" stage }

source     := "nodes" | "events"

expr       := or
or         := and { "OR" and }
and        := unary { [ "AND" ] unary }          -- nối ngầm là AND
unary      := [ "-" | "NOT" ] atom
atom       := "(" expr ")" | term

term       := field ":" value
            | "#" tagname
            | phrase
            | word                                -- khớp tên/tiêu đề

value      := compare | range | literal | call
compare    := ( ">" | ">=" | "<" | "<=" ) literal
range      := literal ".." literal
call       := name "(" [ literal { "," literal } ] ")"
literal    := word | phrase
phrase     := '"' … '"'
```

**Thứ tự ưu tiên:** `NOT` › `AND` › `OR`. Ngoặc đè lên tất cả. Giống mọi ngôn ngữ ở §2.

`OR` **phải viết hoa**, như Lucene — nếu không thì không phân biệt được với chữ "or"
trong một câu người ta đang tìm.

---

## 4. Nguồn

```
events  with:khánh when:2019
nodes   #gia-đình status:done
```

Không bắt buộc.

> **Sửa lại — 2026-09-20, lúc làm bước 2.** Chỗ này vốn viết *"mặc định `notes`"*.
> **Sai**, và sai vì viết trước khi đọc code: `when:` là từ mà dòng thời gian **được
> làm ra từ đó**, nên mặc định `notes` sẽ biến mọi câu `with:khánh` từng có thành một
> lời từ chối. Cái đúng: **nói ra thì theo lời nói, không nói thì từ khoá quyết định**
> — y như hôm nay. Nêu nguồn ra mặt đáng giá vì nó cho một câu **dùng từ của cả hai
> nửa** có đúng một nghĩa, chứ không phải vì đoán là sai khi chỉ có một nửa để đoán.

Và **một mình nó thì không phải nguồn, nó là một từ.** `nodes` với `events` là tiếng
Anh bình thường, mà mấy ô tìm kiếm chữ trần (`search_notes`, `search_tasks`…) đưa
thẳng thứ người ta gõ vào bộ phân tích này. Nuốt một từ đứng một mình ở đó sẽ lặng lẽ
biến một lượt tìm thành một lượt liệt kê tất cả. "Cả dòng thời gian" vẫn viết được:
`events sort:-when`, `events limit:200`.

Vì sao nêu ra mặt: hôm nay bảng được chọn **ngầm** theo từ khoá, nên
`is:note when:2019` lặng lẽ **vứt bỏ `is:note`** và trả về sự kiện. Kusto và Splunk
đều nêu nguồn, và đó là cách duy nhất câu ấy có một nghĩa.

Trên thanh, nguồn là **chip đầu tiên** — nó nói cho người đang nhìn biết họ đang xem
cái gì, điều mà cách ngầm không bao giờ nói được.

---

## 5. Trường, theo nguồn

Trường thuộc về nguồn, như cột thuộc về bảng trong KQL.

| `nodes` | `events` |
| --- | --- |
| `type:` (bí danh `is:`) | `when:` |
| `#tag` / `tag:` | `with:` — ai có mặt |
| `status:` | `place:` — ở đâu |
| `when:` — ngày của note | `about:` — về cái gì |
| *bất kỳ khoá frontmatter nào* | `shape:` — occasion/bond/spell/marker/noted/ending/chore |
| | `size:` — độ lớn |
| | `kind:` — nhãn tự do |
| | `note:` — ghi chú đã viết ra nó |

**`when:` là một từ cho cả hai nguồn.** Trên note nó là ngày của note; trên sự kiện là
quãng của nó. `date:` **bị xoá** — hôm nay nó chỉ nhận ba giá trị và **vứt im lặng**
mọi thứ khác, nên `is:note date:2026-06` trả về **toàn bộ** note.

Một trường không thuộc nguồn đang chọn là **lỗi được nói ra**, không phải bị bỏ qua.

---

## 6. Giá trị

### 6.1 Một luật, không ngoại lệ

| Viết | Nghĩa |
| --- | --- |
| `key:value` | **bằng** |
| `key:>4` `key:>=4` `key:<4` `key:<=4` | so sánh |
| `key:a..b` | khoảng, **gồm cả hai đầu** |

Hôm nay `magnitude:4` nghĩa là **≥4** còn `priority:3` nghĩa là **=3** — cùng hình
dạng, hai nghĩa. Luật mới bỏ ngoại lệ đó: muốn lớn hơn thì viết ra.

### 6.2 Ngày

```
when:2019            when:2019-05          when:2019-05-14
when:2019-05..2019-08
when:today   when:yesterday   when:last-year   when:this-month
when:~2012                       -- khoảng chừng
when:same-day-as(today)          -- "ngày này những năm trước"
```

Dấu khoảng là `..`. Dấu `/` cũ vẫn nhận, nhưng không còn là cách viết chính — trong
ngữ cảnh ngày nó đọc ra `dd/mm`.

`same-day-as()` là **hàm ngày đầu tiên**, và nó tồn tại để xoá một phép khỏi ống dẫn:
`anniversary` vốn không phải phép nguyên thuỷ, nó là một điều kiện ngày cộng một cột
dẫn xuất.

### 6.3 Độ lớn nhận cả hai thang

```
size:big        size:>4
```

`tiny` `small` `medium` `big` `huge` là **bí danh của ngưỡng số**, không phải một
khái niệm thứ hai. Thang chữ cho người, thang số cho máy, và chúng là một thứ.

> **Chưa chốt:** ngưỡng cụ thể của năm bậc. Phải đo trên vault thật, và §16 Bước 8 đã
> đo được rằng trên vault thiếu quan hệ thì độ lớn thoái hoá thành độ dài tiêu đề —
> nên định ngưỡng lúc này là định cho một phân bố đang hỏng.

> **Làm bước 2 — 2026-09-20.** Thang chữ **chưa làm**, vì lý do trên. Còn `size:4`
> trần thì **bị từ chối**, không phải đổi thành `=4`: độ lớn là một số thực tính ra,
> nên `= 4` là bộ lọc gần như không bao giờ khớp và không bao giờ nói vì sao. Câu từ
> chối chỉ luôn cách viết: *"write size:>=4 or size:<=4"*.

### 6.4 Hoa thường, dấu, ngoặc kép

- **So khớp không phân biệt hoa thường**, cả hai bên.
- Khoá luôn là chữ thường; giá trị **giữ nguyên như người viết** (hôm nay hầu hết bị
  ép về chữ thường, nên một tên riêng mất dạng).
- Cụm có dấu cách phải đặt trong ngoặc kép: `with:"Bùi Văn Phương"`.
- Một từ trần khớp **theo từ**, không theo chuỗi con — vault thật đã dạy điều này khi
  `ăn` khớp cả *«công **văn**»*.

---

## 7. Ống dẫn

Bảy **ô ngữ pháp**, đóng. Bên trong mỗi ô là **bảng tên hàm**, mở mãi mãi mà không
đụng một dấu phẩy nào của cú pháp.

| Ô | Ship trước | Thêm sau, không đổi cú pháp |
| --- | --- | --- |
| `\| stats <fn> by <khoá>` | `count` | `sum(x)` `avg(x)` `min(x)` `max(x)` |
| `\| seq <fn> by <khoá>` | `gaps` | `streak` `since-prev` `running` |
| `\| where <biểu thức>` | so sánh + `AND OR NOT` | — |
| `\| sort <khoá> [desc]` | ✓ | — |
| `\| head <n>` | ✓ | — |
| `\| explode <cái gì>` | `sentences` | `tags` `people` |
| `\| ask <n>` | ✓ | — |

`top <n> by <khoá>` là **đường ngọt** cho `sort <khoá> desc | head <n>`. Nó được nêu
tên vì nó là câu hỏi hay gặp, và nó **được ghi rõ là đường ngọt** — khác với
`anniversary`, thứ từng giả vờ là phép nguyên thuỷ.

### 7.1 Vì sao là ô chứ không phải là từ

```
gaps            streak          since-prev
  → ba từ ngữ pháp, không có trần

seq gaps by …   seq streak by …  seq since-prev by …
  → một ô, một bảng tên
```

Ngữ pháp **đắt và vĩnh viễn**: đổi là hỏng thấu kính người ta đã lưu. Bảng tên hàm
**rẻ**: thêm `sum` không làm hỏng gì của ai. Nên chốt ô rộng, ship ít hàm.

### 7.2 Mỗi giai đoạn làm gì với cột

Điều này quan trọng vì **bộ chọn hình dạng đọc cột** (bước 4).

| Giai đoạn | Cột ra |
| --- | --- |
| `stats count by month` | `month, count` |
| `stats sum(amount) by place` | `place, sum` |
| `seq gaps by who` | `who, times, first, last, span, quiet, longest` |
| `where …` `sort …` `head …` | **không đổi** |
| `explode sentences` | `day, note, text` |
| `ask <n>` | **không đổi**, ít dòng hơn |

### 7.3 Nhóm theo thời gian

`by day` `by week` `by month` `by year` là **khoá gom đặc biệt**: chúng gom theo
**cột ngày** của dòng, không phải theo một trường tên "month". Mọi khoá khác là tên
trường.

---

## 8. `limit:` `sort:` `columns:` khi có ống dẫn

Ba từ này là **của giai đoạn lọc**, không phải của cả câu.

- `limit:` áp lên **kết quả cuối cùng**. Giai đoạn SQL chạy tới một **trần nội bộ**
  (đề nghị 5.000 dòng), và **nói ra khi chạm trần**: *"đếm trên 5.000 sự kiện đầu"*.
  Thừa nhận cắt còn hơn im.
- `sort:` sắp **dòng nguồn**. Muốn sắp kết quả sau khi gom thì dùng `| sort`.
- `columns:` chọn cột **của nguồn**. Sau một `stats` hay `explode`, cột **ra** do giai
  đoạn quyết định.

> **Sửa lại — 2026-09-20, lúc làm bước 5.** Chỗ này vốn viết tiếp: *"nên `columns:` ở
> đầu câu không còn nghĩa — nói ra, đừng bỏ qua"*. **Sai**, và cái sai lộ ra ngay khi
> chạy: `| stats count by shape` bị từ chối với câu *"`shape` không phải một cột của
> câu trả lời này — xin nó bằng `columns:` trước đã"*, trong khi luật cũ lại **từ chối
> `columns:` khi có `stats`**. Hai câu từ chối bảo nhau làm hai việc ngược nhau.
>
> Cái đúng: `columns:` chọn cột **vào**, và đó chính là cách một trường trở nên gom
> được. `events … columns:when,shape | stats count by shape` chạy. Thứ `columns:`
> không làm là chọn cột **ra** sau một `stats` — mà đó không phải lỗi, đó chỉ là nghĩa
> của `stats`, và §7.2 đã ghi rồi.

---

## 9. Từ chối, thay vì lặng lẽ trả lời câu khác

Đây là luật §5.4 của tài liệu Timeline, áp cho ngữ pháp. Đo được hôm nay: **năm chỗ
đang vi phạm**.

| Gõ vào | Hôm nay | Phải thành |
| --- | --- | --- |
| `is:note date:2026-06` | **trả về mọi note** | `when:2026-06` chạy đúng |
| `is:note when:2019` | **bỏ `is:note`**, trả sự kiện | nguồn nêu rõ, hoặc báo lỗi |
| `limit:abc` | lọc thuộc tính tên `limit` | **báo lỗi** |
| `-#gia-đình` | loại trừ **chữ** "#gia-đình" | loại trừ **tag** |
| `-with:khánh` | lọc frontmatter tên `with` → không lọc gì | "không có Khánh" |
| `sort:gõnhầm` | sắp mặc định, im lặng | **báo lỗi** |

Thêm: **tên giai đoạn không biết** và **tên hàm không biết** đều báo lỗi, kèm gợi ý
tên gần nhất.

---

## 10. Chip

Mọi thứ trong §3 phải chip được — nếu không thì đường "chỉ bấm" gãy.

| Cấu trúc | Chip |
| --- | --- |
| `events` | chip nguồn, đứng đầu |
| `with:khánh` | một chip |
| `-with:khánh` | một chip, có dấu phủ định |
| `( a OR b )` | **một chip**, bên trong thấy cả hai và chữ OR |
| `\| stats count by month` | **một chip cho cả giai đoạn** |

Hôm nay chip là *một token*. Nhóm và giai đoạn làm vỡ giả định ấy, nên
`queryChips.ts` phải rộng ra. **Nhưng chữ vẫn là trạng thái duy nhất** — chip vẫn chỉ
là một lát của chữ, nên tính hai chiều không mất.

---

## 11. Cái gì hỏng, và đổi sang gì

Hôm nay **gần như chưa ai lưu thấu kính nào**. Đây là lúc rẻ nhất để đổi tên, và là
lý do phải chốt trước Bước 5.

| Cũ | Mới | Vì sao |
| --- | --- | --- |
| `date:today` | `when:today` | một từ cho một khái niệm |
| `where:hanoi` | `place:hanoi` | `where` để dành cho giai đoạn lọc |
| `magnitude:>4` | `size:>4` | "magnitude" là từ của máy |
| `magnitude:4` (nghĩa ≥4) | `size:>=4` | bỏ ngoại lệ |
| `date:…` | `when:…` | cái cũ **không làm gì cả** — xem §14.1 |
| `in:title` | *(giữ nguyên)* | nó **có** chạy — xem ghi chú §14.1 |
| `2016-05/2016-06` | `2016-05..2016-06` | `/` đọc ra `dd/mm`; cũ vẫn nhận |
| bảng chọn ngầm | `events` / `nodes` | xoá một lớp lỗi cả họ |
| `notes <câu hỏi>` | `nodes <câu hỏi>` | bảng ấy giữ cả người, cả sách, cả task |

---

## 12. Cố ý không thiết kế

- **Phép tập hợp** (*"gặp năm 2019 mà không gặp 2020"*). Chưa ai hỏi. Nếu tới thì nó
  là một ô mới, không phá ô nào.
- **Join hai nguồn.** Cùng lý do.
- **Ngưỡng năm bậc của `size:`.** Phải đo, và phân bố hiện đang hỏng (§16 Bước 8).
- **Ký tự đại diện** (`with:kh*`). Chưa thấy cần; thêm sau không phá gì.

---

## 13. Chỗ tao vẫn đang đoán

1. **Bảy ô là đủ rộng** — cơ sở là chúng là các họ của đại số quan hệ cộng hai thứ nó
   không có (nở dòng, gọi model). Đó là cơ sở, không phải bằng chứng.
2. **Nguồn đứng đầu không làm phiền người non-tech.** Lý lẽ: chip đầu tiên nói rõ
   đang xem gì, mà cách ngầm không nói. Chưa đo.
3. ~~**Chữ `notes` gọi đúng cái bảng nó trỏ vào.**~~ — **đã chốt 2026-09-20: `nodes`.**
   Bảng ấy giữ cả người, cả sách, cả task; `notes sort:title` trả về một người và một
   cuốn sách, mà chữ "notes" không hứa thế. Một chữ phải chịu được việc bị đọc bởi
   người **không viết ra câu truy vấn ấy**. Chữ cũ **từ chối kèm tên mới** ở đúng chỗ
   nó từng là nguồn — xem §25.
4. **`ask` nằm trong ngôn ngữ là đúng.** Nó nghĩa là **một thấu kính đã lưu có thể
   tốn tiền mỗi lần mở**. Hàng rào đề nghị: xem trước số dòng và chi phí trước khi
   chạy, **không bao giờ tự chạy khi mở thấu kính**, và một dấu riêng để nhìn là biết.

---

## 14. Chỗ nào trong code bị đụng

Đọc code ngày 2026-09-20, không phải trí nhớ.

### 14.1 Hai thứ đã chết sẵn trong ngữ pháp

Phát hiện trong lúc rà, và chúng đổi kế hoạch:

| Từ khoá | Tình trạng |
| --- | --- |
| `date:` | **phân tích rồi vứt** — không bộ chạy nào đọc `date_filter`. Ba giá trị "hợp lệ" cũng không làm gì cả |
| ~~`in:title`~~ | **tao đọc sai** — xem dưới |

Nên §9 nói `date:` *"chỉ nhận ba giá trị"* là còn nhẹ. Nó **không làm gì hết, với mọi
giá trị**. Xoá nó không mất chức năng nào, chỉ mất một cái bẫy.

> **Sửa lại — 2026-09-20.** Tao ghi `in:title` cũng chết. **Sai.** `db/search.rs`
> có đọc `title_only`; lệnh `grep -v "search.rs"` của tao loại nhầm cả
> `src/db/search.rs` lẫn `src/search.rs`. Trình biên dịch bắt được khi tao gỡ trường
> ấy đi. `in:title` **ở lại**.

### 14.2 Phía Rust — 12 file chạm vào `ParsedQuery`

| File | Đụng gì | Rủi ro |
| --- | --- | --- |
| `search.rs` | **chính bộ phân tích** | cao — struct phẳng 16 trường thành cây |
| `db/node_query.rs` | dựng SQL từ struct phẳng | **cao** — phải đi theo cây |
| `timeline/query.rs` | như trên, và **không đọc `type_filter`** (lỗi §9 số 2) | cao |
| `commands/nexus.rs` | định tuyến + 5 chỗ gọi | vừa |
| `syn/tools.rs` | công cụ trợ lý + **mô tả dạy cú pháp cho model** | **cao** |
| `syn/tempo.rs` | **dựng thẳng struct**, không qua chữ | cao — đổi struct là vỡ |
| `syn/rag.rs` | 3 chỗ gọi | vừa |
| `db/search.rs` | FTS, nhận `ParsedQuery` | vừa |
| `commands/{feeds,nodes,files}.rs`, `timeline/seal.rs` | dùng gián tiếp | thấp |

Chỗ dễ quên nhất là **mô tả công cụ** ở `syn/tools.rs`: nó là thứ **dạy cú pháp cho
model**. Đổi ngữ pháp mà quên nó thì trợ lý viết ra câu hỏng, và không test nào bắt
được vì model không chạy trong test.

### 14.3 Phía TypeScript

| File | Đụng gì |
| --- | --- |
| `shared/queryChips.ts` | `SINGULAR`, tách token — phải biết **nhóm** và **giai đoạn** |
| `shared/lenses.ts` | `SHELF_QUERY` (`is:lens …`) → `nodes type:lens …` |
| `task/query.ts` | dịch phương ngữ Tasks sang cú pháp engine |
| `task/composables/{useTaskFilters,useTaskSearch}.ts` | dựng và đọc chuỗi |
| `things/composables/{useThingsQuery,useThingsArrangement,useThingsViews}.ts` | dựng `type:` `sort:` `columns:` |
| `things/ThingsApp.vue` | `pinned:true sort:-updated_at limit:20` |
| `nexus/components/{LensBar,LensShelf}.vue` | chip, lưu, chạy |
| `i18n/*.json` | `query_placeholder: "type:book rating:>3"`, `search_placeholder` |

### 14.4 Dữ liệu đã lưu — **ba** loại node mang chuỗi truy vấn

Tao viết ở §11 rằng "gần như chưa ai lưu gì". Đúng với vault này, **sai với code**:

| Loại | Ai lưu | Trong vault thật |
| --- | --- | --- |
| `lens` | Nexus (bước 2) | **0** |
| `view` | Things — `Views/`, có trường `query` | **0** |
| `filter` | Tasks | **0** |

Không phải chưa có cơ chế lưu — **có ba, và vault này tình cờ trống cả ba**. Một vault
khác thì không. Nên vẫn phải đọc được chuỗi cũ, dù không phải viết migration cho
vault này.

---

## 15. Kế hoạch

### 15.1 Trước tiên: chụp ảnh hành vi hiện tại

Bước 9 của tài liệu Timeline đã chứng minh cách này chạy: một ảnh chụp 1307 dòng bắt
được một hồi quy mà 2340 unit test không bắt.

Trước khi đụng một dòng nào: **một test đặc tả chạy ~40 câu truy vấn thật** qua cả hai
bộ chạy và ghi lại kết quả. Mọi bước sau phải giữ ảnh ấy **không đổi một dòng**, trừ
đúng những dòng mà bước ấy cố ý đổi — và khi đổi thì phải nêu tên.

Không có ảnh này thì mọi bước dưới đây là làm mù.

### 15.2 Chiến lược đổi kiểu dữ liệu

Chỗ khó nhất là `ParsedQuery`: **struct phẳng, 16 trường công khai, và `tempo.rs`
dựng thẳng nó** chứ không qua chữ.

Đề nghị: **thêm bên cạnh, đừng thay tại chỗ.**

```
Term   = (trường, toán tử, giá trị)        -- rút từ 16 trường phẳng hiện tại
Expr   = And | Or | Not | Term             -- cây
Query  = { nguồn, lọc: Expr, giai_đoạn, sort, columns, limit, offset }
```

`ParsedQuery` **ở lại** như một **khung nhìn phẳng** dựng ra từ `Expr` khi biểu thức
tình cờ là một phép hội thuần tuý. Nhờ thế `tempo.rs`, `rag.rs` và `db/search.rs`
tiếp tục chạy trong suốt quá trình, và được chuyển sang cây **từng chỗ một** thay vì
tất cả trong một commit.

Khi câu hỏi có `OR`, khung nhìn phẳng không dựng được — và chỗ nào còn đòi nó thì
**báo lỗi rõ ràng**, không âm thầm làm rơi vế `OR`.

### 15.3 Bảy bước

| | Việc | Gate |
| --- | --- | --- |
| **0** | ~~Sửa năm lỗi §9~~ — **xong 2026-09-20** | ảnh chụp đổi **đúng 10 dòng**, 37 dòng còn lại không nhúc nhích |
| **1** | ~~`Term`/`Expr`/`Query` dựng bên cạnh; `ParsedQuery` thành khung nhìn phẳng~~ — **xong 2026-09-20** | ảnh chụp **không đổi một dòng** ✓ |
| **2** | ~~Nguồn đứng đầu, hợp nhất `when:`, đổi tên §11~~ — **xong 2026-09-20** | ảnh chụp đổi đúng chỗ đổi tên ✓ |
| **3** | ~~`OR` `NOT` ngoặc trong bộ phân tích và cả hai bộ chạy~~ — **xong 2026-09-20** | câu cũ không đổi; câu có `OR` chạy ✓ |
| **4** | ~~Chip biết nhóm~~ — **xong 2026-09-20** (giai đoạn theo bước 5) | vòng chữ→chip→chữ vẫn khít cho mọi câu ở §15.1 ✓ |
| **5** | ~~Dấu `\|` + `stats` + `sort`/`head`~~ — **xong 2026-09-20** | `bars` có dữ liệu ✓ |
| **6** | ~~`seq gaps` + `where` + `same-day-as()`~~ — **xong 2026-09-20** | ba panel thành thấu kính; cái thứ tư cần bước 7 |
| **7** | ~~`explode sentences` + `ask`~~ — **xong 2026-09-20** | bước duy nhất tiêu tiền |

**Bước 0 độc lập với mọi quyết định còn lại** — nó không đụng cú pháp, chỉ sửa chỗ
đang trả lời sai mà không báo. Làm được ngay cả khi §1–13 còn bàn tiếp.

**Bước 1 là bước không ai nhìn thấy gì** và là bước rủi ro nhất. Gate của nó khắt khe
nhất vì thế: **không một dòng nào được đổi**.

### 15.4 Ba chỗ dễ quên, ghi ra để khỏi quên

1. **Mô tả công cụ của trợ lý** (`syn/tools.rs`) — không test nào bắt được nếu sai.
2. **Chuỗi i18n dạy cú pháp** — `query_placeholder`, `search_placeholder`.
3. **Chuỗi cũ trong `view` và `filter` node** — vault này trống, vault khác thì không.

---

## 16. Nếu chỉ làm được một bước

Làm **bước 0**. Nó không đụng cú pháp, không cần chốt gì thêm, và nó gỡ đúng loại lỗi
mà cả tài liệu này tồn tại để ngăn: **lặng lẽ trả lời một câu hỏi khác**.

---

## 17. Bước 0 — đã làm, 2026-09-20

**Ảnh chụp trước, sửa sau**, đúng thứ tự §15.1. `src/search_gate.rs` chạy 47 câu hỏi
qua cả hai bộ chạy trên một vault cố định và so với `search_gate.txt`.

Ảnh chụp lộ ra **hai khiếm khuyết của chính nó** trước khi lộ ra lỗi nào của ngữ pháp:

- nó **sắp tên trước khi cắt**, nên `sort:` và `sort:-` ra cùng một dòng — tức là nó
  mù với đúng thứ `sort:` làm;
- **FTS không được dựng** trong vault test, nên mọi từ trần trả về 0 — nó đang ghi lại
  một lỗ của bộ khung, không phải hành vi của ngữ pháp.

Sửa hai chỗ đó rồi mới ghi mốc. Một ảnh chụp không nhìn thấy thứ nó phải canh thì tệ
hơn không có, vì nó cho cảm giác an toàn.

### Mười dòng đổi, ba mươi bảy dòng đứng yên

| Câu | Trước | Sau |
| --- | --- | --- |
| `date:today` | **7 (mọi thứ)** | 0 — nay là bộ lọc thuộc tính thường |
| `date:2026-06` | từ chối "không có gì để khớp" | 0 |
| `is:note date:2026-06` | **2 (mọi note)** | 0 |
| `is:note when:2019` | 3 — **bỏ `is:note`** | **2** — chỉ sự kiện từ note |
| `#gia-đình when:2019` | 3 — **bỏ tag** | **từ chối, có lý do** |
| `status:done when:2019` | 3 — **bỏ status** | **từ chối, có lý do** |
| `limit:abc` | 0 — lọc thuộc tính tên `limit` | **từ chối: 'abc' không phải số dòng** |
| `-#gia-đình` | từ chối | **6** — loại trừ đúng tag |
| `sort:tiêu_đề` | 0 — lọc thuộc tính tên `sort` | **từ chối, có lý do** |
| `columns:tiêu_đề` | 0 | **từ chối, có lý do** |

### Ba chỗ tao đọc sai, và cái gì bắt được

1. **`in:title` không chết.** Tao viết ở §14.1 rằng nó chết như `date:`. Lệnh
   `grep -v "search.rs"` loại nhầm cả `src/db/search.rs` — nơi thật sự đọc
   `title_only`. **Trình biên dịch bắt được** khi tao gỡ trường ấy. §14.1 đã sửa.

2. **Một test khẳng định thứ không ai đọc.** `test_date_filter` kiểm rằng parser
   *lưu* `date_filter` — và không bộ chạy nào đọc nó. Một test xanh trong khi tính
   năng chết hẳn. Thay bằng test về hành vi.

3. **Mô tả công cụ có ngân sách ký tự**, và thêm chữ vào nó làm đỏ một test: nó trả
   giá **mỗi lượt nói chuyện**. Phải viết ngắn lại chứ không phải nâng trần.

### Còn lại của bước 0

`-with:khánh` và `-when:2019` vẫn lặng lẽ thành loại trừ thuộc tính frontmatter. Phủ
định từ khoá dòng thời gian **cần cây biểu thức** (`NOT` của §3), nên nó thuộc bước 3,
không sửa được ở đây mà không đụng cú pháp.

---

## 18. Bước 1 — đã làm, 2026-09-20

**Không ai nhìn thấy gì.** 47 dòng ảnh chụp: không dòng nào nhúc nhích. Đó là toàn bộ
gate của bước này và nó đã qua.

### Cây là cách đọc, struct phẳng là hình chiếu

`src/query.rs` mới: `Field` · `Value` · `Term` · `Expr` · `Query`. `search.rs` giữ
`ParsedQuery`, nhưng nó **không còn tự đọc chữ nữa** — `parse_query` giờ đúng một
dòng:

```rust
pub fn parse_query(raw: &str) -> ParsedQuery {
    ParsedQuery::of(crate::query::parse(raw))
}
```

Điểm mấu chốt là **hình chiếu, không phải bộ đọc thứ hai**. Nếu `query::parse` và
`parse_query` cùng đọc chữ, chúng sẽ trôi ra khỏi nhau — và bước 3 sẽ sửa một bên rồi
tự hỏi vì sao bên kia vẫn trả lời câu cũ. Giờ chỉ có **một cách đọc**, mười sáu trường
phẳng chỉ là một góc nhìn của nó.

### Cái mà cây nói được mà struct phẳng không nói được

Struct phẳng chỉ biết nói một câu: **tất cả những thứ này, cùng lúc**. Nó có ba danh
sách loại trừ riêng (`exclude_terms`, `property_exclusions`, `tag_exclusions`) vì nó
không có chỗ để nói "không". Cây nói một lần:

```
-status:done   →   Not(Term(Prop("status"), "done"))
-#gia-đình     →   Not(Term(Tag,            "gia-đình"))
-báo           →   Not(Term(Text,           "báo"))
```

Và đúng chỗ ấy là lý do `-with:khánh` đang trả lời sai: struct phẳng **có ô** cho
"note nào có khoá frontmatter `with` khác khánh", và **không có ô** cho thứ người ta
định hỏi. Bước 3 sẽ sửa được vì giờ có chỗ để đặt.

### `Or` chưa phân tích được, nhưng đã từ chối được

`Expr::Or` có mặt trong cây dù chưa câu nào dựng ra nó. Nó không phải giàn giáo chết:
`ParsedQuery::of` **đã từ chối nó bằng lời**, và có test.

Đó là toàn bộ an toàn của bước 3. Ngày `OR` bắt đầu phân tích được, mười hai file còn
đang đọc khung nhìn phẳng sẽ **kêu lên**, chứ không lặng lẽ giữ một vế và trả lời một
câu hỏi nhỏ hơn câu được hỏi. Đúng loại lỗi cả tài liệu này tồn tại để ngăn.

### 961 câu hỏi, không câu nào đọc khác đi

Ảnh chụp 47 dòng là gate, nhưng 47 câu không đủ để tin rằng một bộ phân tích viết lại
đọc y như cũ. Nên: lấy `parse_query` **ở HEAD** ra khỏi git, đặt cạnh bản mới trong một
test dùng một lần, chạy **961 câu** (22 khoá × 22 giá trị × có/không dấu trừ, cộng 40
câu quái) qua cả hai, so `{:?}` từng ký tự.

Không câu nào khác. Test ấy đã xoá — giữ lại nghĩa là nuôi một bản sao của bộ đọc đã
chết.

### Một lỗi sập hẳn, tìm thấy trên đường đi

`unquoted()` cắt **byte**:

```rust
value[1..value.len() - 1]      // đọc ổn với "x", và panic với “x”
```

Dấu ngoặc cong là 3 byte, nên byte 1 nằm **giữa** nó. `with:“Khánh”` làm sập
`parse_query` — và bàn phím iOS/macOS tự đổi `"` thành `“` mà không hỏi ai. Mọi người
gọi đều dính: thanh tìm kiếm, thấu kính đã lưu, công cụ của trợ lý.

Đã đổi sang đi theo **ký tự**. Không đổi dòng nào của ảnh chụp, nên nó không nằm trong
gate — nhưng nó là một cú sập, nên nó được ghi ra đây.

### Cố ý chưa có trong `Query`

`source` (§4) và ống dẫn `|` (§7) **không** được thêm vào `Query` lần này, dù §15.2 vẽ
chúng. Một trường không bộ phân tích nào ghi vào được là giàn giáo chết, và giàn giáo
chết là thứ không ai dám gỡ về sau. Bước 2 và bước 5 thêm chúng **khi có chữ để đọc
vào đó**.

### Đo

| | |
| --- | --- |
| Ảnh chụp | 47 dòng, **0 đổi** |
| Test Rust | 2365 (thêm 6) |
| Test TypeScript | 1868, không đổi |
| Clippy | sạch trên `query.rs`, `search.rs`, `search_gate.rs` |
| `query.rs` | 511 dòng, trong đó 6 test |
| `search.rs` | −175 dòng (130 thêm, 305 bớt — vòng lặp token sang `query.rs`) |

---

## 19. Bước 2 — đã làm, 2026-09-20

**Nguồn đứng đầu, `when:` cho cả hai bảng, và hai chữ đổi tên.**

### Ảnh chụp đổi ở đâu

Ảnh chụp dài ra 47 → 58 dòng. Mười một dòng mới đều là câu **chưa hỏi được** trước
bước này:

| Câu | Trước | Sau |
| --- | --- | --- |
| `notes when:2019` | không viết được | **1** — note mang ngày ấy |
| `events when:2019` | không viết được | 3 |
| `events limit:5` | không viết được | **4** — cả dòng thời gian |
| `notes with:khánh` | không viết được | **từ chối: `with:` hỏi về sự kiện** |
| `events #gia-đình` | không viết được | từ chối: `#tag` hỏi về note |
| `events is:note` | không viết được | 3 |

Và bảy dòng cũ đổi nghĩa, đúng bảy chỗ §11 nói:

| Câu | Trước | Sau |
| --- | --- | --- |
| `where:"Hà Nội"` | 1 sự kiện | **từ chối: 'where' is now 'place'** |
| `place:"Hà Nội"` | lọc thuộc tính `place` → 0 | **1** |
| `magnitude:>2` | 2 | **từ chối: 'magnitude' is now 'size'** |
| `size:>2` | lọc thuộc tính `size` → 0 | **2** |
| `magnitude:2` (nghĩa ≥2) | 2 | `size:2` → **từ chối, chỉ cách viết** |
| `when:2019..2021` | **từ chối** — `..` chưa đọc được | **4** |
| `when:2016..2026 gặp` | từ chối | **1** |

### Một chữ đổi tên không được phép chỉ là xoá

Xoá một từ khoá **không làm nó thôi phân tích** — nó làm từ ấy phân tích thành *một
khoá frontmatter cùng tên*. `where:hanoi` sẽ trả lời **0** và không nói gì. Nên hai
chữ cũ nằm trong một bảng `RENAMED` và **từ chối kèm tên mới**, chứ không biến mất.

Đây cũng là đường di cư cho §14.4: một thấu kính cũ ở vault khác mang `magnitude:>4`
sẽ **hiện lên câu "magnitude is now size"**, không phải trả lời sai trong im lặng.

### Nguồn: nói ra thì theo lời, không nói thì theo từ khoá

Đặc tả §4 viết "mặc định `notes`". Đọc code rồi thì thấy **sai** — xem ô sửa lại ở §4.
Cái làm được: nguồn nói ra **đè lên** suy đoán. Nhờ thế có ba thứ trước đây không nói
được, và không mất thứ nào:

1. `notes when:2019` — note **mang ngày ấy**. Hôm qua không viết được câu này.
2. `events limit:5` — cả dòng thời gian.
3. Một trường không thuộc nguồn là **lỗi nói ra thành lời**, cả hai chiều.

### `when:` phải thật sự trả lời được, chứ không chỉ được trỏ vào

§11 trỏ `date:today` sang `when:today`. Nhưng `when::parse` **chưa từng đọc được chữ
`today`** — nó chỉ đọc `2016-05-14`, `2016-05`, `2016`, `a/b`, `~x`. Trỏ người ta sang
một cách viết không chạy thì tệ hơn là không trỏ. Nên `when:` học thêm `today`,
`yesterday`, `tomorrow`, `this-week`, `last-week`, `this-month`, `last-month`,
`this-year`, `last-year` — và `..` bên cạnh `/`.

Ngày "hôm nay" là **tham số** (`parse_on`), không phải đồng hồ đọc bên trong, nên test
của `last-year` là test số học chứ không phải test may mắn cho tới tháng Mười Hai.

### Một lỗi đồng thuận, do chính test bắt được

Cho `when:` đọc được `yesterday` làm **đỏ một test không liên quan gì tới truy vấn**:
`quiet::a_stretch_that_is_not_a_stretch_is_refused_rather_than_written`.

Nó đúng. Một **hush** và một **seal** lưu quãng thời gian **y như chữ người ta viết**,
rồi đọc lại bằng cách phân tích lại mỗi lần. Ghi `yesterday` vào đó thì cái quãng bị
giấu **trượt đi một ngày mỗi sáng** — lặng lẽ, vì không ai đọc lại cho tới lúc có thứ
bị giấu mà lẽ ra không được giấu. Với seal thì tệ hơn hush: `last-year` trong một seal
sẽ **rời khỏi đúng cái năm nó sinh ra để che** vào mỗi tháng Giêng.

Nên `when` tách làm hai cửa:

| | đọc được | ai dùng |
| --- | --- | --- |
| `parse` | mọi thứ, kể cả chữ trượt theo ngày | truy vấn |
| `parse_written` | chỉ thứ viết ra hẳn | **seal, hush** — file sống lâu hơn cái ngày viết nó |

Tao **không** tự đi sửa định dạng file đồng thuận trong bước này. Tầng 2 là quyết định
của con người; đổi cách lưu nó là việc riêng, không phải việc kèm theo một bước ngữ
pháp.

### Ba câu lỗi từng dạy sai cú pháp

Ba chỗ mỗi chỗ giữ một danh sách ví dụ riêng, và **hai trong ba** vẫn mời
`2016-05-01/2016-06-30` (dấu §11 vừa thay) với `"last year"` (cách viết chưa bao giờ
phân tích được). Một câu báo lỗi dạy sai cú pháp còn tệ hơn câu không dạy gì. Giờ một
hằng, một chỗ: `when::HOW_TO_WRITE_ONE`.

### Hai chỗ §15.4 dặn, đã kiểm

1. **Mô tả công cụ** (`syn/tools.rs`): nó **không** dạy model từ khoá dòng thời gian —
   model đi cửa `timeline` riêng. Nên không có gì phải đổi, và ngân sách 19.800 ký tự
   không nhúc nhích. Đã kiểm chứ không đoán.
2. **Chuỗi i18n**: `query_placeholder` là `type:book rating:>3` — không dính chữ nào bị
   đổi tên.

### Đo

| | |
| --- | --- |
| Ảnh chụp | 47 → **58 dòng**, 11 dòng mới, 7 dòng đổi, 40 dòng đứng yên |
| Test Rust | 2371 (thêm 6) |
| Test TypeScript | 1868, không đổi |
| Clippy | không cảnh báo mới trên file nào đụng tới |

---

## 20. Bước 3 — đã làm, 2026-09-20

**`OR`, `NOT`, ngoặc — trong bộ phân tích và trong *cả hai* bộ chạy.**

### Ảnh chụp: 58 → 75 dòng, và **đúng hai dòng cũ đổi**

Hai dòng ấy là món nợ bước 0 đã ghi ra và không trả được:

| Câu | Trước | Sau |
| --- | --- | --- |
| `-with:khánh` | **notes 7** — "note có khoá `with` khác khánh" | **events 3** — sự kiện không có Khánh |
| `-when:2019` | notes 7 | **events 1** — sự kiện ngoài 2019 |

Mười bảy dòng mới là những câu **chưa viết được**:

```
#gia-đình OR #công-việc              → notes    2
is:task OR is:book                   → notes    3
with:khánh OR with:minh              → events   2
(with:khánh OR with:minh) when:2019  → events   2
is:note (#gia-đình OR #công-việc)    → notes    2
#gia-đình or #công-việc              → notes    0   ← chữ thường là một TỪ
(#gia-đình                           → từ chối: ngoặc mở mà không đóng
#gia-đình)                           → từ chối: ')' không dính vào đâu
#gia-đình OR                         → từ chối: 'OR' cần hai vế
```

### Không có danh sách "cái gì phủ định được" nữa

Đây là gốc của lỗi `-with:khánh`. Bộ đọc cũ **tự biết một danh sách nhỏ** những
thứ có thể đứng sau dấu trừ — một tag, một khoá tra được, một từ — và `with:`
không nằm trong danh sách ấy, nên câu hỏi lặng lẽ thành *"note có khoá frontmatter
`with` khác khánh"*.

Giờ `-x` **bóc dấu trừ rồi đọc `x` như một điều kiện bình thường**. Không còn danh
sách. **Hỏi được gì thì bỏ-hỏi được nấy** — kể cả những từ khoá chưa ai viết ra.

### `NOT` của SQL không phải `NOT` của người

Chỗ này là lý do `-status:done` xưa nay phải viết tay thành `IS NULL OR <>`.

So sánh với một khoá frontmatter không có là `NULL`, `NOT NULL` cũng là `NULL`, mà
mệnh đề `WHERE` thì vứt `NULL` đi. Nên một task **chưa từng có status** sẽ bị
`-status:done` loại ra — đúng ngược với nghĩa của chữ "chưa xong".

Cây có `NOT` ở mọi chỗ, nên luật này phải là **một luật**, không phải một ngoại lệ
viết tay ở từng trường:

```sql
COALESCE(<điều kiện>, 0) = 0     -- "không biết" tính là "không đúng"
```

Một dòng, ở cả hai bộ chạy, thay cho ba chỗ viết tay khác nhau.

### Từ trần thành câu truy vấn con, không còn là một vòng đi-về

Trước: câu nào có từ thì **hỏi FTS trước**, lấy về một danh sách id, rồi
`AND id IN (…)`. Cách ấy chỉ nói được "và", nên một từ **không thể** nằm trong một
nhóm `OR` hay dưới một `NOT`. Nó còn có trần `MAX_QUERY_LIMIT * 4` id.

Giờ mỗi từ là một điều kiện tự đứng được:

```sql
id IN (SELECT item_id FROM search_index WHERE search_index MATCH ?)
```

Không vòng đi-về, không trần, và **ghép vào đâu cũng được**. Định nghĩa "khớp một
từ" vẫn đúng một chỗ (`fts_match_for`), dùng chung với `search_fts`.

Sửa kèm: hàm dựng biểu thức FTS **không escape dấu nháy**, nên `foo"bar` đóng chuỗi
FTS5 sớm và cả câu thành lỗi cú pháp. Giờ nháy trong từ được nhân đôi, như FTS5 quy
định.

### `match_any` thôi là một cờ, nó thành câu chữ

Trợ lý có một bước lùi: hỏi `query_nodes` mà đòi **mọi** từ thì trả về 0, nên thử lại
với **bất kỳ** từ nào. Nó vốn là `parsed.match_any = true` — một cờ chỉ đường FTS
đọc.

Có `OR` rồi thì nó là chính cái nó vẫn có nghĩa: `Query::any_word()` gom các từ trần
thành **một nhóm `OR`**, mọi bộ lọc khác vẫn bắt buộc. Cờ biến mất khỏi đường này.

### Một lỗi tiếng Việt, tìm thấy bằng chính ảnh chụp

Thêm dòng `when:2019 (gặp OR ăn)` vào ảnh chụp thì nó trả **1**, trong khi vault có
«**Ăn** tối với Minh» đúng năm 2019.

Đo, không đoán:

```
lower("Ăn tối")   = "Ăn tối"      ← không đổi
lower("Gặp Khánh") = "gặp khánh"
lower("ĂN")       = "Ăn"          ← chỉ chữ N bị hạ
```

**`lower()` của SQLite chỉ biết ASCII.** `Ă`, `Đ`, `Ô`… không bao giờ thành chữ
thường. Mà tiêu đề sự kiện là **câu**, nên chữ đầu viết hoa — tức là **mọi từ tiếng
Việt mở đầu một tiêu đề đều không tìm được trên dòng thời gian**. `đi` không thấy
«Đi chơi». Chỉ dính đường LIKE của dòng thời gian; phía note đi qua FTS5, vốn biết
Unicode.

Đã ghi **một dòng riêng trong ảnh chụp để nó không chìm**:

```
when:2016..2026 ăn                 → events   0 []
```

**Không sửa trong bước này**, và nói rõ vì sao: nó là một thay đổi riêng, không phải
thứ nhét kèm một bước ngữ pháp.

> **Đã sửa — 2026-09-20. Xem §26.** Và cách sửa **không** phải cột chuẩn hoá như tao
> đoán ở đây: cột ấy chữa được dòng thời gian và để **phía node vẫn hỏng**.

### Mô tả công cụ: đo rồi mới quyết, và quyết là không đụng

§15.4 dặn nhớ `syn/tools.rs`. Đo:

```
chars 19799 of 19800
```

**Thừa đúng một ký tự.** Dạy `OR` cho model nghĩa là phải cắt một thứ đang có ích.
Và cái model cần `OR` để làm thì nó **đã có**: `query_nodes` tự nới sang "bất kỳ từ
nào" khi đòi mọi từ ra 0. Nên không đụng — và ghi ra đây để lần sau khỏi tưởng là
quên.

### Còn nợ sang bước 4

Thanh chip chưa biết nhóm. Gõ `(#a OR #b)` thì câu trả lời **đúng**, nhưng chip vẽ
ra trông lạ. Vòng chữ→chip→chữ vẫn khít (chữ vẫn là trạng thái duy nhất), nên không
có gì hỏng — đó đúng là việc của bước 4.

### Đo

| | |
| --- | --- |
| Ảnh chụp | 58 → **75 dòng**; 2 dòng cũ đổi (đúng món nợ bước 0), 17 dòng mới |
| Test Rust | 2377 (thêm 6) |
| Test TypeScript | 1868, không đổi |
| Clippy | không cảnh báo mới trên file nào đụng tới |
| `ParsedQuery` | thôi là thứ hai bộ chạy đọc — chỉ còn đường FTS dùng |

---

## 21. Bước 4 — đã làm, 2026-09-20

**Chip thôi là một token.**

### Định nghĩa mới, và nó rút ra từ một câu hỏi

Chip cũ là *một token*. Điều đó đúng chừng nào câu hỏi chỉ nói được "tất cả những
thứ này". Có `OR` và ngoặc rồi thì nó sai — và sai ở đúng chỗ đáng sợ.

Chip là **thứ bấm × là gỡ đi được**. Nên chip phải là một mảnh **gỡ đi mà không đổi
nghĩa phần còn lại**.

- `(#a OR #b)` là một mảnh như thế. Gỡ nửa của nó thì `OR` mất một vế.
- `#a OR #b` **trần** cũng là một mảnh: `OR` buộc lỏng nhất, nên cả câu là **một**
  phép tuyển, không có mảnh nào nhỏ hơn để gỡ.
- Ngoặc là cách người ta nói khác đi — và khi nói thì được trả chip lại:
  `(#a OR #b) with:khánh` là **hai** chip.

### Hai lỗi cũ của thanh chip, lộ ra khi viết lại

**1. `-with:khánh` và `with:khánh` vẽ ra **giống hệt nhau**.** Dấu trừ bị bóc đi để
tìm khoá rồi **không bao giờ được vẽ lại**:

```ts
const key = token.slice(0, at).toLowerCase().replace(/^-/, '');
```

Hai câu hỏi ngược nhau, một bức tranh. Giờ chip mang `negated` và thanh vẽ dấu `−`.

**2. Bấm vào đồ thị có thể lặng lẽ đổi câu hỏi đang có.** Thanh đang là `#a OR #b`,
bấm một người → nối thêm → `#a OR #b with:khánh`. Mà `OR` buộc lỏng nhất, nên câu ấy
là *"`#a`, **hoặc** `#b` cùng với Khánh"* — không phải thứ ai định hỏi. Giờ
`withFilter`/`withTag` **đóng ngoặc cho phép tuyển trần trước khi nối**:
`( #a OR #b ) with:khánh`.

Cái thứ hai đáng kể hơn cái thứ nhất, vì bấm trên đồ thị là **đường dùng chính** của
thanh này — §6.1 gọi nó là cái biên lai.

### Gate: chạy trên chính bộ câu của Rust

§15.3 nói gate của bước này là *vòng chữ→chip→chữ vẫn khít cho mọi câu ở §15.1*. Nên
làm cho nó đúng nghĩa đen: `queryChipsGate.spec.ts` **đọc `search_gate.txt`** —
đúng cái file vàng mà bộ phân tích và cả hai bộ chạy bị ghim vào — và chạy cả 75 câu
qua thanh chip. Câu nào thêm bên Rust để ghim một hình dạng ngữ pháp mới thì **tự nó
sang đây**.

### Gate đầu tiên tao viết **không có răng**

Viết xong vòng chữ→chip→chữ, tao thử **phá `joinedByOr`** cho nó luôn trả `false`.
Gate vẫn **xanh**.

Đúng thôi: cắt `#a OR #b` thành **ba** mảnh thì ghép lại vẫn ra đúng từng ấy chữ.
Vòng tròn khít là điều kiện **cần**, không phải **đủ**. Cái sai chỉ lộ ra lúc **gỡ**
một mảnh.

Nên gate có thêm mệnh đề có răng:

> Gỡ **bất kỳ** chip nào khỏi một câu hợp lệ thì phần còn lại **vẫn là một câu** —
> ngoặc cân, và không `OR` nào mất vế.

Thử lại cùng phép phá ấy:

```
× leaves a question behind whichever chip is taken off
  #gia-đình OR #công-việc → without "#gia-đình" → OR #công-việc
```

Đây là **lần thứ hai** trong tài liệu này một ảnh chụp lộ ra khiếm khuyết của chính
nó trước khi lộ ra lỗi nào của sản phẩm (lần đầu: §17, nó sắp tên trước khi cắt nên
mù với `sort:`). Bài học không đổi: **một ảnh chụp không nhìn thấy thứ nó phải canh
thì tệ hơn không có**, vì nó cho cảm giác an toàn.

### Ba luật chép từ bên Rust, và vì sao phải chép

`tokenise` bên TS giờ **tách ngoặc y như `query.rs`**, kể cả luật "ngoặc dính ngay
sau một cái tên là ngoặc của cái tên" (`when:same-day-as(today)` vẫn là một token).
Cộng với `SINGULAR` và luật `OR` viết hoa, là ba chỗ hai bên phải đồng ý.

Không có cách nào ép hai ngôn ngữ dùng chung một bộ tách từ ở đây, nên cả ba đều có
test ở **cả hai bên**, và gate đọc file vàng chính là dây nối: nếu bên TS tách khác
đi, vòng tròn trên 75 câu ấy sẽ lệch.

### Chưa làm, và nói rõ

**Chip giai đoạn** (`| stats count by month`) chưa có, vì `|` chưa phân tích được.
Dựng ô chứa cho một hình dạng chưa câu nào viết ra được thì đúng là thứ bước 1 đã cố
ý tránh. Nó đi cùng bước 5.

### Đo

| | |
| --- | --- |
| Gate chip | **75 câu** đọc thẳng từ `search_gate.txt`, 5 mệnh đề |
| Kiểm tra gate có răng | phá `joinedByOr` → gate đỏ đúng chỗ |
| Test TypeScript | 1885 (thêm 17) |
| Test Rust | 2377, không đổi |
| `vue-tsc` | sạch |

---

## 22. Bước 5 — đã làm, 2026-09-20

**`|`, `stats count by …`, `sort`, `head` — và `bars` cuối cùng có dữ liệu.**

Ảnh chụp 75 → 90 dòng. Không dòng cũ nào đổi.

```
events when:2016..2026 | stats count by month              → events 2 [2021-03 · 2019-11]
events when:2016..2026 | stats count by month | top 1 by count → events 2 [2019-11]
notes columns:title,date | stats count by year             → notes  2 [2019 · 2021] — 5 with no year
is:task | stats count by status → từ chối: 'status' không phải cột của câu trả lời này
is:note | stats count by month  → notes  2 [2021-03 · 2019-11]
events when:2016..2026 | wibble → từ chối: 'wibble' không phải việc một câu hỏi làm được
```

### Ống dẫn chạy trong Rust, không phải trong SQL

`stats count by month` **là** một `GROUP BY`, và viết thế thì nhanh hơn. Nhưng ô ngay
cạnh nó là `seq gaps by with` — bao lâu giữa lần gặp này với lần gặp trước — mà thứ
ấy **không** là `GROUP BY` trong phương ngữ nào app này dám dựa vào. Viết nửa ống dẫn
bằng SQL và nửa bằng Rust nghĩa là **hai chỗ** một giai đoạn có thể mang nghĩa, và
hai chỗ ấy sẽ trôi ra khỏi nhau.

Nên: cơ sở dữ liệu trả lời câu hỏi, ống dẫn trả lời *làm gì với câu trả lời*, trên
những dòng đã cầm trong tay. §8 định giá chỗ ấy một cách sòng phẳng — nửa lọc chạy
tới **trần 5.000 dòng**, và chạm trần thì **nói ra**.

### Câu trả lời tự khai ra thứ nó bỏ lại

Hai chỗ một câu trả lời có thể **đúng mà vẫn lừa người đọc**:

1. Chạm trần 5.000 — "đếm trên 5.000 sự kiện đầu" là câu trả lời tốt; **trình bày nó
   như một phép đếm** thì không.
2. Gom theo `status` mà có dòng không có `status` nào. Bỏ im lặng chính là cách một
   phép đếm trên 3 thứ trả về 1.

Nên `QueryResult` có thêm một trường `note`, và thanh vẽ nó dưới câu trả lời:

```
is:task columns:title,status | stats count by status
  → notes 1 [done] — 1 with no status
```

Luật trần được **bóc ra thành hàm** (`hit_the_ceiling`) chứ không viết thẳng vào hai
bộ chạy — một luật không gọi được là một luật không test được, mà luật này chỉ nổ
trên vault to hơn bất kỳ vault nào test dựng ra.

### §8 tự mâu thuẫn, và chạy mới lộ

Luật cũ: *"`columns:` khi có `stats` thì vô nghĩa — từ chối"*. Nhưng `stats count by
shape` lại bị từ chối với câu *"`shape` không phải cột của câu trả lời này — xin nó
bằng `columns:` trước đã"*.

**Hai câu từ chối bảo nhau làm hai việc ngược nhau.** Không test nào bắt được; chỉ
đọc hai dòng ảnh chụp cạnh nhau mới thấy.

Cái đúng ghi ở ô sửa lại trong §8: `columns:` chọn cột **vào**, và đó chính là cách
một trường trở nên gom được.

### Đo trên vault thật, như mọi bước

```
events when:2024..2026 | stats count by month | top 6 by count
  2026-05  51      2026-04  19
  2026-06  35      2026-09  13
  2026-07  22      2026-08  12

events when:2016..2026 columns:when,shape | stats count by shape
  chore 102   occasion 52   spell 7   marker 1
```

162 sự kiện, `chore` **102/162 = 63%** — cùng tỉ lệ mà cả phiên này đã đo được nhiều
lần. Trần không chạm (162 ≪ 5.000), nên `note` rỗng, đúng như phải thế.

### `bars`, và vì sao nó không đến sớm hơn

`shapeFor` nhận ra **một đống nhãn kèm một con số**: đúng hai cột, cột thứ hai tên là
`count`/`sum`/`avg`/`min`/`max`, **và** ô trong nó là số. Cả hai điều kiện, vì
`columns:title,priority` cũng là hai cột với số ở cột hai — mà đó là bảng công việc,
không phải biểu đồ.

Đọc **trước** phép thử ngày, cố ý: `stats count by month` có ngày dọc một bên, và vẽ
nó thành dòng thời gian sẽ khoe cái nhãn và giấu đúng cái vừa được đếm.

Cột được chia theo **cái lớn nhất**, không theo tổng: câu hỏi là *cái nào to hơn*, mà
thang chia theo tổng thì hai mươi cái đè bẹp nhau hết.

### Chip giai đoạn — món nợ bước 4, trả ở đây

Bước 4 để lại chip giai đoạn vì `|` chưa phân tích được. Giờ có rồi:

- `| stats count by month` là **một chip**, và dấu `|` thuộc về nó — gỡ bước cuối đi
  mà để lại `|` lủng lẳng thì máy từ chối.
- Và một lỗi cùng họ với lỗi `OR` của bước 4: **bấm một người trên đồ thị khi đang có
  ống dẫn** vốn sẽ nối vào cuối → `… | stats count by month with:khánh`, tức là ba từ
  nữa của giai đoạn. Giờ nó nối vào **câu hỏi**, trước dấu `|` đầu tiên.

### Chưa làm

`stats sum(x)`/`avg(x)` — bảng tên hàm mở, thêm sau không đụng cú pháp (§7.1). `seq`,
`where`, `explode`, `ask` là bước 6 và 7.

### Đo

| | |
| --- | --- |
| Ảnh chụp | 75 → **90 dòng**, không dòng cũ nào đổi |
| Test Rust | 2383 (thêm 6) |
| Test TypeScript | 1897 (thêm 12) |
| `vue-tsc` | sạch |
| Trần ống dẫn | 5.000 dòng, và nói ra khi chạm |

---

## 23. Bước 6 — đã làm, 2026-09-20

**`| seq gaps by …`, `| where …`, `when:same-day-as(…)`.** Ảnh chụp 90 → 105 dòng,
không dòng cũ nào đổi.

### Gate: panel viết tay, viết lại thành câu hỏi

Đây là phép thử của cả thiết kế (§5.3 tài liệu thấu kính). Không phải "một câu hỏi
na ná", mà **cùng một luật, cùng những người**.

`timeline/silence.rs` là một module viết tay với ba ngưỡng trong đó:

```rust
const ENOUGH_TIMES: usize = 5;
const ENOUGH_MONTHS: i64 = 183;
const SHORTEST_WORTH_SAYING: i64 = 90;
if quiet_for <= longest_before.max(SHORTEST_WORTH_SAYING) { return None }
```

Cùng ba ngưỡng ấy, viết thành câu hỏi:

```
events | seq gaps by who
      | where times >= 5 and span >= 183d and quiet > longest and quiet > 90d
```

`max` của hai cái cuối chính là hai phép so sánh nối bằng `and` — cùng một thứ. Có
test dựng đủ ba trường hợp phân biệt (gặp nhiều và mới gặp; gặp nhiều mà lâu rồi
không; mới quen được hai ngày) và câu hỏi chọn đúng một người như module chọn.

| Panel | Câu hỏi | |
| --- | --- | --- |
| Ngày này năm xưa | `events when:same-day-as(today)` | ✓ |
| Khoảng lặng | dòng trên | ✓ đúng từng ngưỡng |
| Chuyện gì đã xảy ra với | `events columns:when,about \| seq gaps by about \| where quiet > 6mo` | ✓ |
| Một năm bằng lời mày | `… \| explode sentences \| ask 15` | bước 7 |

Ba trên bốn. Cái thứ tư cần `sentences` và `ask`, đúng như §15.3 chia.

**Nói cho rõ chỗ không thay thế được:** luật "mỗi năm chỉ nhắc một lần" của
`onthisday.rs` và lớp đồng thuận của `quiet.rs` **không** phải truy vấn và không nên
thành truy vấn. Chúng là tầng 2 — quyết định của con người. Cái thành thấu kính là
**câu hỏi**, không phải cái quyền im lặng.

### Ba chỗ đặc tả sai, lộ ra khi chạy

**1. `same-day-as()` không phải một dáng của `when:`.** Một `when:` là một **quãng**
— hai đầu. "Ngày này những năm xưa" là **một ngày trong năm, bỏ năm đi**, và không
trả lời được bằng cùng phép so sánh. Nên nó là một trường riêng (`Field::SameDay`),
dù vẫn **viết** là `when:same-day-as(x)` đúng như §6.2.

**2. §7.2 thiếu một cột.** Danh sách đặc tả là `who, times, first, last, quiet,
longest`. Nhưng panel không chịu đi nếu thiếu `span` — nó **không nói gì** về người
mới quen hai tuần, dù khoảng lặng có to đến đâu, mà `where` thì không trừ được cột
này cho cột kia. Đã thêm `span`. Danh sách ấy viết trước khi đọc luật của panel.

**3. Chỉ có `who` là cột, không có `place` và `about`.** Nên `seq gaps by about` —
việc gì đã nguội — là câu hỏi ngôn ngữ **không hỏi được**. Đã thêm cả ba vai.

### Một câu trả lời bằng uuid không thay thế được cái panel nào

Chạy `seq gaps by who` lần đầu ra thế này:

```
uuid-khanh   1   2019-11-05 …
uuid-minh    1   2019-11-05 …
```

Đúng số, sai ngôn ngữ. Panel hiện tên người; thấu kính hiện uuid thì **không phải
cùng một thứ**, nó là cùng câu trả lời viết bằng thứ tiếng không ai nói.

Đường vào đã đổi tên thành danh tính (`identity_of`); giờ có đường về, chạy **trước**
ống dẫn — vì `seq gaps by who` gom theo thứ nằm trong cột ấy, và gom theo uuid là gom
theo uuid.

Làm việc ấy lộ ra **`names_for` và `identity_of` chưa bao giờ là nghịch đảo của
nhau**: `identity_of` lấy `$.node_id` trước, còn `names_for` chỉ tra `id` với
`stable_id`. Một danh tính lấy từ `node_id` thì tra ngược không ra. Đã sửa.

### Đo trên vault thật, và con số làm tao dừng lại

```
events when:2016..2026 | seq gaps by who | sort times desc | head 8
  → ["who","times","first","last","span","quiet","longest"]
    total 1   note "161 with no who"
```

**161 trên 162 sự kiện không nêu tên ai.** Một sự kiện duy nhất có liên kết `with`.

Cơ chế đúng, test xanh, câu hỏi viết được — và **không có gì để hỏi**. Nửa cảm xúc
của dòng thời gian không nghẽn ở ngôn ngữ nữa; nó nghẽn ở chỗ **sự kiện không mang
người**. Đây cùng một tỉ lệ phiên này đã đo lúc chạy bóc tách (*1 trên 20 đề xuất nêu
tên một người*), giờ đo trên cả chỉ mục.

Ngôn ngữ giờ hỏi được. Dữ liệu chưa trả lời được. Đó là hai việc khác nhau, và bước
này chỉ làm xong việc thứ nhất.

`same-day-as` thì có dữ liệu:

```
events when:same-day-as(2026-05-01)              → 1 ["2026-05-01","meeting",…]
events when:same-day-as(2026-05-01) | stats count by year → ["2026", "1"]
```

### `| where` là bộ đọc thứ hai, và cố ý

Nó **không** dùng lại bộ đọc của nửa lọc, vì hai bên hỏi hai thứ khác nhau: nửa lọc
hỏi về **một node hay một sự kiện**; `where` hỏi về **một dòng của câu trả lời** —
các cột của nó, theo tên. `quiet > longest` so hai cột **của cùng một dòng**, câu
mà nửa lọc không phát biểu nổi.

`6mo` `2y` `3w` `30d` đọc ra **ngày**, vì `quiet` đếm bằng ngày. Tháng là 30 ngày,
năm là 365: đây là so **độ dài**, không phải gọi tên ngày tháng, và lịch thật sẽ làm
phép so phụ thuộc vào tháng nào mà không ai đang nói đến.

### Đo

| | |
| --- | --- |
| Ảnh chụp | 90 → **105 dòng**, không dòng cũ nào đổi |
| Test Rust | 2392 (thêm 9) |
| Test TypeScript | 1897, không đổi |
| Clippy | không cảnh báo mới |
| Vault thật | **161/162 sự kiện không nêu tên ai** |

---

## 24. Bước 7 — đã làm, 2026-09-20

**`| explode sentences` và `| ask n`.** Bước duy nhất tiêu tiền. Ảnh chụp 105 → 113
dòng, không dòng cũ nào đổi.

### Bốn trên bốn

```
Ngày này năm xưa       events when:same-day-as(today)
Khoảng lặng            events | seq gaps by who
                             | where times >= 5 and span >= 183d
                                   and quiet > longest and quiet > 90d
Chuyện gì xảy ra với   events columns:when,about | seq gaps by about | where quiet > 6mo
Một năm bằng lời mày   events when:2026 | explode sentences | ask 15
```

Phép thử của cả tài liệu thấu kính (§5.3): *"nếu tám panel không viết lại được thành
thấu kính thì thiết kế này sai."* Tám trên tám giờ viết được.

### Hàng rào dựng vào hình dạng, không dán thêm

Câu hỏi khó nhất của bước này không phải "làm sao gọi model", mà **"làm sao một thấu
kính đã lưu không âm thầm tiêu tiền khi mở ra"**.

Trả lời bằng kiểu dữ liệu, không bằng kỷ luật:

```rust
pub struct Around<'a> {
    pub words: Option<&'a dyn Words>,   // đọc chữ trong vault
    pub asker: Option<&'a dyn Asks>,    // tiêu tiền
}
```

Đường thường truyền `asker: None`. Nên `| ask` ở đó **không chạy được** — không phải
vì ai đó nhớ kiểm tra, mà vì **không có chỗ để tiêu**. Nó từ chối, và lời từ chối
mang theo giá:

```
`ask 1` would send 4 lines to a model. A question that spends money is not run
by opening it — ask for it deliberately.
```

Lời từ chối **chính là** cái xem trước §13.3 đòi. Và cửa thứ hai là một lệnh riêng
(`ask_node_query`) với một nút riêng trên thanh — không ai đi tới hoá đơn bằng cách
bấm đúng cái nút vẫn bấm.

### Lớp đồng thuận: chỗ duy nhất của bước này không được sai

`| explode sentences` là phần đầu tiên của ngôn ngữ chạm tới **chữ người ta viết**,
chứ không phải tiêu đề với ngày tháng trong chỉ mục. Mọi thứ ở trên nó — seal, hush —
tồn tại để quyết định app được nhìn gì. Một ngôn ngữ đi vòng qua đó **không phải một
tính năng có lỗi, nó là một lỗ thủng trong cái lớp ấy**, mà cái lớp ấy là lý do toàn
bộ chuyện này được phép đến gần một cuốn nhật ký.

Nên `Words` là một **trait**, không phải một `&DbBridge`. Truyền cơ sở dữ liệu thì
"nhớ lọc đồng thuận" thành một luật ai đó phải nhớ; truyền cái này thì nó thành luật
người gọi **không tránh được việc đã nghĩ tới**.

Và viết thành test, không phải thành ghi chú — `commands::nexus::consent_gate`, năm
cái, **kể cả cái đối chứng** (không có nó thì bốn cái kia xanh trên một lỗi trả về
rỗng cho mọi thứ):

| | |
| --- | --- |
| note thường | trả về câu của nó |
| note bị seal | **không gì** |
| note trong quãng bị seal | **không gì** |
| một câu bị hush | câu ấy biến mất, những câu khác ở lại |
| một quãng bị hush | **không gì** |

Thứ tự là tất cả: bỏ **trước khi** dựng bất cứ thứ gì từ chúng. Không phải lọc khỏi
câu trả lời — **không bao giờ được đọc**. Luật ấy của `timeline::year` (*"nên nó
không chỉ bị bỏ khỏi câu trả lời — nó chưa bao giờ được gửi đi"*), và một thấu kính
chạm tới cùng những câu ấy phải giữ nguyên luật.

### Model chỉ trỏ, không nói

Cùng một giao thức `timeline::year` dùng, giờ dùng chung một hàm: một danh sách đánh
số đi ra, những con số đi về. **Trong câu trả lời không có chỗ nào để đặt văn xuôi.**
Đó không phải lựa chọn thẩm mỹ — nó là thứ làm cho một câu bịa ra thành **không thể**
thay vì **ít khả năng**.

Một con số không ai mời thì bị bỏ, không vòng lại — model đếm sai cũng không chạm
được dòng nó chưa từng được xem. Có test.

Hai chỗ tiết kiệm mà `year` đã học trước:
- Danh sách đã đủ ngắn thì **không hỏi** — hỏi chỉ tốn một cuộc gọi để nghe lại đúng
  danh sách ấy.
- Một note được đọc **một lần**, dù bao nhiêu dòng dẫn về nó. Ba sự kiện một ngày đến
  từ một note; không chặn thì câu của nó hiện ba lần — với người là lặp, với model là
  **ba phiếu cho cùng một dòng**. Đây là lỗi ảnh chụp bắt được: `events when:2019 |
  explode sentences` ra **6** dòng, phải là **2**.

### `ask` chỉ được hỏi một lần

Hai cuộc gọi model trong một câu hỏi thì tốn gấp đôi và giải thích được một nửa. Từ
chối, có lý do.

### Đo

| | |
| --- | --- |
| Ảnh chụp | 105 → **113 dòng**, không dòng cũ nào đổi |
| Test Rust | 2403 (thêm 11) |
| Test TypeScript | 1899 (thêm 2) |
| Cổng đồng thuận | 5 test, có đối chứng |
| `vue-tsc` | sạch |

---

## 25. `notes` → `nodes` — 2026-09-20

Câu hỏi treo ở §13.3 đã chốt. Bảng ấy giữ **cả người, cả sách, cả task**, nên
`notes sort:title` trả về một người và một cuốn sách — thứ chữ "notes" không hứa. Một
chữ trong ngôn ngữ phải chịu được việc bị đọc bởi **người không viết ra câu ấy**.

Ảnh chụp 113 → 115 dòng. Mọi dòng có `notes …` đổi thành `nodes …`, cộng hai dòng mới
ghi lại chính phép đổi tên:

```
notes when:2019   → từ chối: 'notes' is now 'nodes'
notes             → nodes 2   ← một mình nó vẫn là từ người ta đang tìm
```

**Vì sao phải từ chối chứ không lặng lẽ bỏ.** Xoá `notes` khỏi bảng nguồn thì nó
không thôi phân tích — nó thành **một từ trần**. Và khi ấy `notes when:2019` biến
từ *"node mang ngày 2019"* thành *"tìm chữ notes"* cộng `when:2019`, mà câu ấy không
còn từ nào của node nên nó **đổi luôn sang hỏi sự kiện**. Đúng loại đổi nghĩa trong
im lặng mà §9 tồn tại để chặn — và lần này nó đổi cả **bảng**.

Nên chữ cũ nằm riêng một chỗ (`RENAMED_SOURCE`), không chung với `where:`/`magnitude:`
của §11, vì nó không phải một `key:` — nó là **từ đầu tiên của câu, và chỉ ở đó**.
Một mình nó vẫn là tiếng Anh bình thường, đúng như `nodes` và `events` vẫn thế.

Kèm theo: mấy câu báo lỗi nói *"this question is about notes"* và *"asks about a
note"* giờ nói **node** — chúng nói về `#tag` với `status:`, vốn là trường của node
chứ không riêng gì note.

---

## 26. `lower()` chỉ biết tiếng Anh — 2026-09-20

`lower()` của SQLite gấp chữ ASCII và **không gấp gì khác**. Đo:

```
lower("Ăn tối")    = "Ăn tối"      không đổi
lower("Gặp Khánh") = "gặp khánh"   chỉ chữ G, vì G là ASCII
lower("ĂN")        = "Ăn"          chỉ chữ N
```

Mọi chữ cái tiếng Việt không đồng thời là chữ tiếng Anh — `Ă Â Đ Ê Ô Ơ Ư` và các
dạng có dấu — trở ra **y như lúc đi vào**. Nên mọi phép so trong app này từng hạ chữ
cả hai vế thật ra đang so **chữ hạ bằng Rust** với **chữ hạ bằng ASCII**, mà hai thứ
ấy khác nhau đúng ở những từ vault này viết bằng.

### Nó rộng hơn chỗ §20 ghi

§20 ghi nó như một lỗi của **dòng thời gian**. Đo lại thì không phải. Một node có
thẻ `Gia-Đình`:

```
is:book      → 1     ← nó vẫn ở đó
#gia-đình    → 0
#Gia-Đình    → 0
#GIA-ĐÌNH    → 0
author:Đặng  → 0
```

**Không tìm được bằng bất kỳ cách viết nào.** Không phải "khó tìm" — không tồn tại
với mọi câu hỏi về thẻ hay về thuộc tính.

### Cách sửa tao đoán ở §20 là sai

§20 đề nghị một **cột đã chuẩn hoá** trên bảng `events`. Cột ấy chữa được dòng thời
gian và **để nguyên phía node**: thẻ, status, mọi giá trị frontmatter được so **tại
chỗ**, và không có chỗ nào để đặt bản sao thứ hai của từng cái.

Cái sửa đúng là **dạy SQL một phép hạ chữ biết hơn hai mươi sáu chữ cái**:
`db::text` đăng ký hai hàm vô hướng lên mọi kết nối —

| | |
| --- | --- |
| `vlower(x)` | hạ chữ Unicode |
| `vwords(x)` | như trên, cộng đệm khoảng trắng và biến dấu câu thành khoảng trắng |

Dùng y như `lower()` vẫn được dùng, và **không bao giờ trôi khỏi phía Rust vì nó
chính là phía Rust**.

Giá phải trả: hàm tự viết thì không đánh chỉ mục được. Trước đó không có chỉ mục nào
trên `lower(title)` cả, nên hôm nay không mất gì — và một vault đủ to để cần chỉ mục
chính là thứ sẽ báo cho ta biết.

Kèm theo: mười sáu lời gọi `replace()` lồng nhau dựng vào SQL của **mọi** câu truy
vấn dòng thời gian có chữ, giờ là một danh sách đọc một lần mỗi dòng.

### Đo

Ảnh chụp đổi **đúng hai dòng** — đúng hai dòng §20 ghi ra để nó không chìm:

| Câu | Trước | Sau |
| --- | --- | --- |
| `when:2016..2026 ăn` | **0** | **1** [Ăn tối với Minh] |
| `when:2019 (gặp OR ăn)` | 1 | **2** |

Vault thật:

```
ăn   →  1  ["Cam ăn miếng táo đầu tiên"]
đi   →  1  ["Cam lần đầu đi trung tâm thương mại"]
văn  →  5  ["Làm công văn…", "Onboard Bùi Văn Phương", …]
ĂN   →  1  ["Cam ăn miếng táo đầu tiên"]
```

`ăn` **không** dính 5 sự kiện «văn» — bảo đảm cũ còn nguyên, và được canh ở chỗ có
một chữ «văn» thật để sai: test của `timeline::query`, giờ thêm cả «Ăn» viết hoa.

| | |
| --- | --- |
| Test Rust | 2409 (thêm 5) |
| Test TypeScript | 1899, không đổi |
| Phụ thuộc | `rusqlite` thêm feature `functions` |

---

## 27. Rà soát trước khi dùng thật — 2026-09-20

Bảy bước xong không có nghĩa là dùng được. Rà soát tìm **bốn lỗi**, trong đó **ba cái
nằm ở khoảng giữa cỗ máy và màn hình** — chỗ không bước nào trong bảy bước nhìn tới.

### 1. Mọi lời từ chối hiện ra là `[object Object]`

Nặng nhất, và mỉa mai nhất. Cả bảy bước đặt cược vào *"từ chối kèm một câu dạy
người ta"*. Câu ấy **không bao giờ tới được màn hình**.

`AppError` tuần tự hoá thành `{ code, message }` — cố ý, để người gọi rẽ theo `code`.
Nên thứ một lệnh Tauri ném ra là một **object**, mà `String(object)` là
`"[object Object]"`. Ba mặt của cỗ máy này — `LensBar`, Things, và khối truy vấn
trong note — đều làm đúng thế.

Test lẽ ra phải bắt được thì **giả lập sai hình dạng**: nó ném `new Error(...)`, thứ
Tauri không bao giờ ném. Xanh trên một hình dạng không tồn tại. Đây là lần thứ ba
trong tài liệu này một test xanh che một thứ đã chết.

Mà app **đã từng gặp lỗi này rồi**: mini-app Messages viết đúng hai dòng chữa nó
**bốn lần trong bốn file**, không ai rút ra. Giờ là `utils/said.ts`, một chỗ.

### 2. Ô tìm kiếm lặng lẽ trả về cả vault

`search_fts` — ô tìm trong Nexus, bộ lọc đồ thị, thanh bên của Note/Task/File/QuickCap
— đọc **8 trên 24 trường** của `ParsedQuery` và bỏ phần còn lại **trong im lặng**.
Quan trọng nhất: nó **chưa bao giờ đọc `refused`**.

Đo trên ba note:

| Gõ vào ô tìm | Đúng ra | Nó trả về |
| --- | --- | --- |
| `#alpha OR #beta` | 2 | **3 — cả vault** |
| `when:2019` | 0 hoặc từ chối | **3 — cả vault** |
| `NOT #alpha` | 2 | **3 — cả vault** |
| `#alpha \| stats count by month` | từ chối | 1, ống dẫn bị vứt |

Một bộ lọc lặng lẽ tìm ra **mọi thứ** cũng là lỗi y như một bộ lọc lặng lẽ tìm ra
**không gì** — câu trả lời là *"đây, vault của mày"* thay vì *"tao không hỏi được
câu đó"*. Và bảy bước vừa rồi **làm nó rộng ra**: giờ có rất nhiều từ khoá gõ vào
được mà đường ấy không hiểu.

Giờ nó từ chối, và nói **hỏi ở đâu được**: *"`when:` is more than this search box can
ask. Ask it in the query bar."* `ParsedQuery` thêm một trường `has_pipeline`, vì hình
dạng phẳng không chỗ nào chứa giai đoạn — nhưng chứa được **sự kiện là có giai đoạn**,
và thế là đủ để từ chối.

### 3. Gõ nửa chừng một dấu ngoặc thì danh sách trống

Things và Tasks chạy truy vấn **sống**, một nhịp debounce sau mỗi phím. Nên lúc gõ
`Họp (Khánh` — trước khi kịp đóng ngoặc — câu bị từ chối, danh sách bị xoá và một
dòng lỗi hiện ra **ngay dưới con trỏ**. Dấu ngoặc trong tiêu đề tiếng Việt là chuyện
thường. Đây là lỗi **bước 3 tạo ra** khi biến ngoặc thành ngữ pháp.

`tokenize` vốn đã có đúng luật cho chuyện này, cho dấu nháy: *"người ta đang viết dở,
và viết lại thứ người ta đang gõ là điều duy nhất một thanh như thế không được làm."*
Giờ ngoặc bỏ ngỏ **ở cuối** được coi là đã đóng, và `OR` cuối câu cũng thế — nó trả
lời đúng thứ danh sách đang hiện một phím trước đó.

Ngoặc **đóng vào hư không** thì khác: `#a)` không phải viết dở, nó sai. Vẫn từ chối.

### 4. Lời từ chối là tiếng Anh trong một app tiếng Việt

**Chưa sửa.** Mọi câu từ chối của cỗ máy — thứ dạy người ta nhiều nhất — đều là tiếng
Anh cứng trong Rust, trong khi cả giao diện có en/vi đầy đủ. Sửa đúng là đổi từ chối
từ **một chuỗi** thành **một mã cộng tham số**, rồi dịch ở phía màn hình. Đó là một
việc riêng, không phải thứ nhét vào cuối một buổi rà soát.

### Không phải lỗi, đã kiểm

| | |
| --- | --- |
| Truy vấn đã lưu mang cú pháp cũ | vault thật: **0** node `lens`/`view`/`filter`, 0 thuộc tính `query` |
| Things/Task/Note tự dựng truy vấn | chỉ dùng `type:` `is:` `sort:` `columns:` — không chữ nào bị đổi tên |
| Things có nuốt lời từ chối không | không, nó hiện ra (và giờ hiện đúng chữ) |
| Đường tốn tiền có tự chạy được không | không — `asker: None` trên mọi đường thường |

### Đo

| | |
| --- | --- |
| Test Rust | 2410 (thêm 1) |
| Test TypeScript | 1903 (thêm 4) |
| Ảnh chụp | 116 dòng; 2 dòng đổi (ngoặc và `OR` viết dở), 2 dòng mới |
| `vue-tsc` | sạch |
