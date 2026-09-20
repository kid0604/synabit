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

source     := "notes" | "events"

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
notes   #gia-đình status:done
```

Không bắt buộc; **mặc định `notes`**, nên mọi câu đang chạy hôm nay vẫn chạy.

Vì sao nêu ra mặt: hôm nay bảng được chọn **ngầm** theo từ khoá, nên
`is:note when:2019` lặng lẽ **vứt bỏ `is:note`** và trả về sự kiện. Kusto và Splunk
đều nêu nguồn, và đó là cách duy nhất câu ấy có một nghĩa.

Trên thanh, nguồn là **chip đầu tiên** — nó nói cho người đang nhìn biết họ đang xem
cái gì, điều mà cách ngầm không bao giờ nói được.

---

## 5. Trường, theo nguồn

Trường thuộc về nguồn, như cột thuộc về bảng trong KQL.

| `notes` | `events` |
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
| `seq gaps by with` | `who, times, first, last, quiet, longest` |
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
- `columns:` chọn cột **của nguồn**. Sau một `stats` hay `explode`, cột do giai đoạn
  quyết định, nên `columns:` ở đầu câu không còn nghĩa — **nói ra**, đừng bỏ qua.

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
| bảng chọn ngầm | `events` / `notes` | xoá một lớp lỗi cả họ |

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
3. **`ask` nằm trong ngôn ngữ là đúng.** Nó nghĩa là **một thấu kính đã lưu có thể
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
| `shared/lenses.ts` | `SHELF_QUERY` (`is:lens …`) → `notes type:lens …` |
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
| **1** | `Term`/`Expr`/`Query` dựng bên cạnh; `ParsedQuery` thành khung nhìn phẳng | ảnh chụp **không đổi một dòng** |
| **2** | Nguồn đứng đầu, hợp nhất `when:`, đổi tên §11 | ảnh chụp đổi đúng chỗ đổi tên |
| **3** | `OR` `NOT` ngoặc trong bộ phân tích và cả hai bộ chạy | câu cũ không đổi; câu có `OR` chạy |
| **4** | Chip biết nhóm và giai đoạn | vòng chữ→chip→chữ vẫn khít cho mọi câu ở §15.1 |
| **5** | Dấu `\|` + `stats` + `sort`/`head` | `bars` có dữ liệu |
| **6** | `seq gaps` + `where` + `same-day-as()` | bốn panel cảm xúc thành thấu kính |
| **7** | `explode sentences` + `ask` | bước duy nhất tiêu tiền |

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