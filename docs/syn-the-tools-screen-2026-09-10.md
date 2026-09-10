# Màn hình Tools: từ danh mục thành cái điều khiển được

*10/09/2026. Bắt đầu từ hai câu hỏi của người dùng: "bấm toggle ẩn hiện thì chả
có gì khác nhau" và "tao muốn on/off cũng ko biết làm ở đâu".*

## 1. Hai câu hỏi hoá ra là một

Tab Tools trả lời được *Syn với tới được những gì*, nhưng không trả lời được
*và tao đổi được không*. Nút duy nhất trên đó thì hỏng. Còn chỗ đổi được thì
nằm ở tab Permissions — mà chỉ tới được **sau khi** Syn đã hỏi và mình đã trả
lời. Trong 29 tool, chuyện đó xảy ra với đúng **một** cái.

## 2. Đo trước

29 tool = **16.101 ký tự ≈ 4.025 token, mỗi lượt**, gửi y hệt nhau dù câu hỏi
là gì.

| Nhóm | Tool | Ký tự | ~token | Phần |
| --- | ---: | ---: | ---: | ---: |
| Đọc vault | 15 | 7.150 | 1.787 | 44% |
| Ghi vault | 9 | 5.953 | 1.488 | 37% |
| Đổi hàng loạt | 4 | 2.196 | 549 | 14% |
| Trình duyệt | 1 | 802 | 200 | 5% |

Quét 136 run trong `Syn/runs`: **10 tool từng được gọi, 19 cái chưa bao giờ.**
19 cái đó là 9.039 ký tự — **56% payload**, trả mỗi lượt cho thứ chưa dùng lần
nào.

```
64  browse         3  create_node            1  remember
28  get_node       3  load_skill             1  search_feed_articles
26  query_nodes    2  search_files
11  update_node    6  list_schemas
```

## 3. Cái toggle hỏng vì cái gì

`<span class="block …" :class="… ? '' : 'line-clamp-2'">`.

`line-clamp` chạy được là nhờ đặt `display: -webkit-box`. `.block` và
`.line-clamp-2` cùng độ ưu tiên, và trong CSS đã build `.block` nằm sau (48899
so với 48701) — nên nó thắng, mọi mô tả luôn hiện đủ, mũi tên xoay bên cạnh
đoạn chữ đứng yên.

Không có gì bắt được lỗi này ngoài mắt người: nó type-check sạch, lint sạch,
cả hai class đều có thật. Nên test canh **hình dạng gây ra lỗi** — không đặt
utility `display` tĩnh lên một phần tử đang bị line-clamp.

## 4. Thiết kế

**Công tắc đặt trên tiêu đề nhóm.** Bốn cái, không phải 29. Nhóm vốn đã là chữ
mà thẻ xin phép đang dùng.

**Nhị phân.** Bật = như cũ, kể cả việc vẫn hỏi khi cần hỏi. Tắt = ghi `Never`
vào **chính** consent ledger mà tab Permissions đang hiện và revoke được. Một
bản ghi, hai chỗ nhìn. Không đẻ ra danh sách "tool đang bật" thứ hai.

**Tắt = không gửi mô tả**, chứ không phải gửi rồi từ chối lúc gọi. Từ chối lúc
gọi vẫn tốn token mô tả một thứ không dùng được, rồi tốn thêm một vòng để model
học ra điều đó. Và bỏ hẳn ra là phiên bản duy nhất người dùng **nhìn thấy**:
con số payload ở tab Prompt tụt ngay khi gạt công tắc.

**Mỗi dòng nói đã dùng bao nhiêu lần, gần nhất khi nào.** 29 thứ lạ hoắc là 29
quyết định; 19 dòng ghi "chưa dùng bao giờ" là một quyết định. Con số này để
**biết**, không phải để **quyết** — `restore_node` dùng đúng cái ngày cần nó.
Vì thế công tắc ở nhóm, con số ở dòng.

**Tool đã tắt bị làm mờ, không bị ẩn.** Một dòng biến mất khỏi danh sách thì
không phân biệt được với một dòng chưa từng tồn tại, mà lời hứa của màn hình này
là danh sách đầy đủ: *Syn không gọi được thứ gì không có ở đây.*

## 5. Phía Rust: ba chỗ phải sửa để công tắc có thật

1. **`Capability::scope_key()`** trước đây trả `None` cho ba nhánh vault. Scope
   key không chỉ là cách nhớ một câu trả lời — nó là chỗ duy nhất để **ghi** một
   lời từ chối. Không có nó, `record` xếp `Never` vào `"unscoped"` và `decide`
   không bao giờ ngó tới đó.
2. **`decide()`** đọc `Never` **trước**, chứ không phải sau cái nhánh cho-qua
   vault. Doc của chính nó đã viết *"A recorded `Never` refuses, for anything"*
   — câu đó sai với 28/29 tool cho tới hôm nay. Thứ tự là toàn bộ cách sửa.
3. **`can_be_remembered()`** tách khỏi `scope_key()`. Hai câu hỏi trông như một
   khi mới có ba nhánh có scope; chúng rời nhau ngay khi nhánh vault cần chỗ ghi
   một lời từ chối. **Một công tắc không phải một sự cho phép**: nhánh vault
   nhận `Never` và không nhận `Always`.

Và `VaultTools::definitions` lọc theo ledger, dùng **cùng một hàm**
`is_switched_off` mà catalogue dùng để làm mờ dòng — có test canh, vì hai vị từ
riêng biệt thì cái ngày chúng lệch nhau sẽ trông y hệt một công tắc không làm gì.

## 6. Cố tình không làm

- **Công tắc từng tool.** 29 quyết định, và ledger không có scope cho nó.
- **Trạng thái thứ ba "hỏi mỗi lần".** Công tắc mang ba nghĩa là công tắc phải
  nghĩ mới bấm được. Cái đó ở lại trên thẻ xin phép, chỗ câu hỏi thật sự được
  đặt ra.
- **Công tắc cho capability gắn với một host.** Catalogue dựng ra với đối số
  rỗng, nên `NetRead { domain }` tới đây với host là chuỗi rỗng; gạt nó sẽ ghi
  một lời từ chối vào hư không. Những cái đó quyết định trên thẻ, lúc đã biết
  host.

## 7. Chỗ nguy hiểm đã kiểm

- **Recipe.** `syn_recipe_problems` vẫn kiểm tên bước trên danh mục **đầy đủ**
  (catalogue trả về mọi tool, kèm cờ `offered`), nên tắt một nhóm không làm
  recipe hỏng lúc kiểm. Lúc chạy thì `decide` vẫn chặn — engine gọi nó trước mọi
  tool call, kể cả nhánh vault.
- **Tắt hết.** `tools: (!tools.is_empty()).then_some(&tools)` đã có sẵn: gửi
  `None` chứ không gửi `[]`, vì nhiều server nói API này từ chối `tools: []`.
- **Lời từ chối nói gì.** *"The user has said never to change a note in your
  vault. Do not ask again and do not look for another way."* — `describe()` đã
  phủ cả ba nhánh vault, không phải sửa gì.

## 8. Đo lại bằng gì

1. Có ai gạt công tắc nào không, sau một tuần. Không ai gạt thì nó đi theo
   `browse("1")` — và nên bỏ, theo đúng tiêu chuẩn đã áp cho `recall`.
2. `payload_cost().chars` thực tế trên vault đang dùng. Nếu vẫn 16.101 thì
   không ai tắt gì cả, và câu 1 đã trả lời rồi.
3. Số tool có `used > 0` sau một tháng. Nếu vẫn là 10, thì 19 cái kia không phải
   "chưa dùng" mà là "không dùng", và đó là câu hỏi khác: bỏ hẳn hay không.
