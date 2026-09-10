# Cách Syn dùng trình duyệt — đọc từ 109 run

*10-09-2026. Không phải trí nhớ: 109 file run trong `{vault}/Syn/runs`, 47 lần
gọi `browse`, 24 câu hỏi khác nhau.*

---

## 1. Toàn cảnh

```
47 lần gọi browse    →    40 tìm kiếm · 6 URL · 1 host trần
```

**85% là tìm kiếm.** Sau khi đã sửa mô tả tool, sửa câu trong system prompt, và
thêm `address_of` để nâng host trần thành địa chỉ.

Nhưng đếm theo *lời gọi* là đếm sai. Đếm theo **câu hỏi** thì 24 câu rơi vào
đúng ba loại:

| loại | số câu | search có đúng không | kết quả |
|---|---|---|---|
| **Một dữ kiện, không nêu site** — *"tuần rồi Chelsea Arsenal thế nào"* | 13 | Có | Từng hỏng do trích xuất; **đã sửa 09-09** |
| **Cái mới nhất trên một site có tên** — *"đọc bài mới nhất trên genk"* | 7 | **Không** | **6/7 sai** |
| **Đọc trang này** — *"tóm tắt bài viết sau: <url>"* | 4 | — | Chạy đúng |

Loại 1 đông nhất theo số câu, nhưng cả 13 câu là một chủ đề trong một buổi.
Loại 2 mới là chỗ hỏng, và hỏng gần như mọi lần.

## 2. Vì sao loại 2 hỏng — và không phải vì hỏi chưa khéo

Một index tìm kiếm **không xếp theo thời gian**. Nó xếp theo độ liên quan và
theo thứ engine tự quyết. Nên *"bài mới nhất trên GenK"* ném vào ô tìm kiếm sẽ
trả về **một** bài, và không có gì trong bài đó nói nó là bài mới nhất.

Sáu lần sai, mỗi lần một kiểu:

- **09-09 05:30** — trả lời "Puma vs adidas" là bài mới nhất. Trên trang chủ nó
  đứng thứ 73; bài đầu là POCO F9 Ultra.
- **09-09 01:41** — đi theo `vnexpress.net/the-gioi`, một link **chuyên mục**,
  rồi tóm tắt bài trong đó. Tự thú: *"Trang mình đọc không hiển thị rõ thời gian
  đăng bài."*
- **09-10 02:12** — tự viết `.../2026/08/26/this-week-in-rust-666/` theo mẫu.
  Địa chỉ **có thật**, là số cũ. Trang chủ hôm đó liệt kê `[667, 666, 665…]`.
- **09-10 03:08** — không có link nào trong tay, **bịa 12 địa chỉ**.

Lần duy nhất đúng — **09-09 15:43** — là lần duy nhất nó gõ `genk.vn` và mở
trang chủ. Hai lượt gọi, 6,5 giây, đúng bài.

> **Câu trả lời đã nằm sẵn trong dữ liệu: mở site thì đúng, tìm kiếm thì sai.
> Vấn đề là mở site đang là tai nạn, còn tìm kiếm là mặc định.**

## 3. Chỗ lệch của thiết kế cũ

`browse` được dựng như **một công cụ tìm kiếm có gắn thêm điều hướng**, và được
dùng như **một công cụ điều hướng**.

Cái thang cũ: bất cứ thứ gì không phải `http://` → tìm kiếm. Nên tên site trong
câu hỏi bị ném vào ô tìm kiếm cùng mọi thứ khác.

Tao có thử một luật rút tên miền từ câu truy vấn. Đo: **6/41**. Yếu — vì trong
câu hỏi, site được gọi bằng **tên** chứ không phải tên miền: "trên GenK",
"This week in Rust", "trên vnexpress".

Và đó chính là chỗ chia việc đúng đắn:

- **"GenK" → `genk.vn`** là thứ **model biết** và code **không thể** biết nếu
  không nhúng một danh sách site — thứ sẽ sai ngay tuần sau, và là thứ dự án này
  đã từ chối.
- **Mở trang chủ, đọc theo đúng thứ tự, lấy cái đầu** là thứ **code làm chính
  xác** và model **liên tục không làm**.

## 4. Thiết kế

**Một động từ, ba nước đi. Code chọn nước đi từ hình dạng đối số — và đối số có
thêm một ô để model nói ra thứ nó biết.**

```
browse(what, site?)
```

| đầu vào | nước đi |
|---|---|
| `what` là URL | **đi tới đó** — màn hình trước, rồi fetch rẻ, rồi cửa sổ |
| `what` là số / `more` / một tiêu đề | **ở lại trang đang cầm** |
| có `site` | **mở trang chủ site đó** — dàn ý, ngày đăng, danh sách bài theo thứ tự |
| không gì cả | **tìm kiếm** |

Tìm kiếm bị **giáng** từ "mặc định cho mọi thứ không có `http://`" xuống "nước đi
khi không ai biết phải nhìn vào đâu".

Một URL trong `what` vẫn thắng `site`: nó cụ thể hơn.

`site` là **tên miền**, không phải tên. Nếu model đưa "GenK", `address_of` trả về
`None` và nó **rơi xuống tìm kiếm** — ghi log, không từ chối. Không có danh sách
nào được nhúng.

### Và một chốt cho nhánh tìm kiếm

Khi câu hỏi có từ chỉ thời gian — *mới nhất, gần nhất, hôm nay, latest, newest,
top story* — mà vẫn đi qua ô tìm kiếm, kết quả nói thẳng:

> **A search index is not ordered by time** — nothing above is the newest of
> anything, whatever its date says, and no rewording of the query changes that.
> If the question is about a particular site, call `browse` again with that site
> in `site`.

Nằm trong **kết quả**, không phải mô tả tool: mô tả được đọc *trước khi* viết
truy vấn, còn đây là đúng khoảnh khắc truy vấn quay về mà không trả lời được.
Cùng lý do với `keep_looking` và câu báo cắt trang.

Nhận diện cả tiếng Việt lẫn tiếng Anh — câu hỏi được hỏi bằng cả hai, và một
luật chỉ nổ ở một thứ tiếng là luật nổ một nửa số lần.

## 5. Đối chiếu lại với 24 câu hỏi

- **Loại 1 (13 câu)** — không có site → tìm kiếm, y như cũ. Đã chạy đúng từ khi
  vá `reduce`. Không đổi gì.
- **Loại 2 (7 câu)** — model điền `site: "genk.vn"` → trang chủ → các bài theo
  thứ tự, kèm ngày. Đúng bằng lần 15:43, nhưng là **mặc định** thay vì tai nạn.
  Nếu nó vẫn tìm kiếm, cái chốt thời gian nói rõ vì sao không được.
- **Loại 3 (4 câu)** — URL → đi tới. Pane giữ trang qua lượt.

## 6. Cái giá

`PAYLOAD_BUDGET_CHARS` 16.000 → **16.200**. Lần nâng đầu tiên có **số đo** chống
lưng chứ không phải lý lẽ:

```
input 8990 · input_cached 6874 · output 557
```

**76% input của lượt đó là cache.** Thứ cố định lớn nhất trong một lượt chính là
payload này, giống hệt nhau từng byte ở mọi lời gọi. Trần cũ được viết như thể
mọi ký tự đều trả đủ tiền mỗi lần, vì **chưa bao giờ có gì đếm input**.

Vẫn là chi phí thật: lượt đầu của mỗi hội thoại trả đủ, và model chạy máy thì
gần như không có prompt cache. Nên con số đó biện minh cho **hai trăm** ký tự,
không phải hai nghìn.

## 7. Cái thiết kế này **không** sửa

- **Một cú đoán mà tải được.** Model tự viết một URL sâu, nó có thật, nó đọc
  được — nên phép kiểm địa chỉ thấy nó "đã đọc". Về cấu trúc không bắt được.
  `site` làm giảm *nhu cầu* đoán, không chặn được việc đoán.
- **Model chọn dở.** Đường ống làm cho điều hướng dở **nhìn thấy được và cứu
  được**; nó không làm model điều hướng giỏi.
- **Trang render bằng JavaScript** vẫn phải leo lên bậc cửa sổ.

## 8. Đo lại bằng gì

Ba câu, trả lời được từ chính `Syn/runs` sau một tuần dùng bình thường:

1. Tỉ lệ `browse` có `site` trên tổng số lời gọi. Nếu vẫn 0 thì tham số này đi
   theo `browse("1")` — không ai dùng — và nên bỏ, theo đúng tiêu chuẩn đã áp
   cho `recall`.
2. Số lần phép kiểm địa chỉ nổ trên 20 lượt.
3. `input_cached / input` trung bình — con số quyết định mọi tranh luận về ngân
   sách còn lại.
