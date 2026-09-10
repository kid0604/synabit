# Khung hội thoại của Syn: cho nó giàu lên bằng cách nào

*10/09/2026. Bắt đầu từ một câu: "hiển thị conversation của Syn khá nhàm chán,
không thể hiện được nhiều thứ, không cho người dùng tương tác được."*

## 1. Có hai chỗ nghèo, không phải một

**(a) Bộ render chỉ biết năm thứ** — markdown, code, bảng, ảnh, mermaid. Còn
lại là chữ.

**(b) Câu trả lời là ngõ cụt.** Nó nằm bên trong một app đầy bề mặt — Notes,
Whiteboard, Calendar, Tasks, Files, Finance — và không với tới được cái nào.
Syn *ghi* được vào vault bằng tool, nhưng cái *câu trả lời* thì đọc xong là hết.

(b) mới là cái làm nó chán: một khung chat tình cờ nằm trong workspace mà không
dùng workspace.

## 2. Ranh giới, trước khi thêm bất cứ thứ gì

> **Model mô tả *nội dung*, không bao giờ mô tả *hành vi*.**

Một fence là dữ liệu — có biên, sanitize được, render được. Một cái nút *làm gì
đó* thì không.

Output của Syn là văn bản do model sinh ra **sau khi đã đọc trang web của người
lạ**. Nếu một trang có thể khiến Syn vẽ ra một cái nút, thì trang đó hành động
được qua tay người dùng. Đó chính là lý do `ConsentCard` và `ChoiceCard` do
**engine** dựng chứ không do model — và mọi thứ thêm vào sau này phải theo đúng
luật đó.

Đây là dòng sẽ bị vi phạm đầu tiên khi có người muốn "cho model tự vẽ UI".

## 3. Không dùng TiptapEditor — và đây là số đo

Editor đã dùng chung cho Notes, Things, Tasks, Projects. Câu hỏi tự nhiên là
dùng luôn cho Syn. Tao đo trước khi trả lời, bằng chính harness sẵn có
(`streamRenderCost.spec.ts`), với **bộ extension tối thiểu** — StarterKit +
Markdown, không mermaid node view, không katex, không map, không slash command:

| ký tự | marked + DOMPurify | Tiptap `setContent` |
| ---: | ---: | ---: |
| 379 | 0,65 ms | 2,52 ms |
| 3.797 | **2,31 ms** | **6,69 ms** |

Cộng 2,8 ms dựng mỗi instance, và một cuộc trò chuyện giữ 50 tin.

Nhưng cái giá streaming này **né được** (render bằng marked lúc stream, đổi sang
Tiptap khi xong), nên nó không phải lý do quyết định. Lý do quyết định là:

- **Nó là editor.** Tắt hết slash command, suggestion, paste handler, drag-drop
  đi thì đang cấu hình *một editor thứ hai*, không còn là dùng chung.
- **Ranh giới an toàn khác nhau.** MessageBubble cho model output qua DOMPurify
  với allowlist tường minh. Editor phân tích markdown của *chính người dùng*.
  Cho model output vào một editor có transclusion `![[note]]` nghĩa là một trang
  web có thể khiến Syn kéo nội dung vault vào khung chat.
- **Một nửa node view vô nghĩa trong bong bóng chat**: whiteboard, PDF, video.

## 4. Ba tầng, xếp theo giá phải trả trong prompt

Prompt là tiền thật: mỗi luật gửi **mỗi lượt**.

### Tầng 1 — bộ render hiểu thêm gì

| | Giá mỗi lượt | Trạng thái |
| --- | --- | --- |
| **Toán `$$…$$`** | **0** — model viết sẵn | ✅ làm rồi, §5 |
| Mermaid | ~430 ký tự (`prompt::RULES`) | có, nhưng hai bộ render tranh nhau |
| Bản đồ | cần luật mới | chưa |
| Chart từ dữ liệu | cần luật mới | có phần, qua mermaid `pie`/`xychart` |

### Tầng 2 — nhìn kỹ hơn (không cần model tham gia)

sơ đồ → phóng to ✅ · ảnh → lightbox ✅ · bản đồ → pan/zoom · bảng → cuộn và sắp
xếp · code → nút copy.

### Tầng 3 — câu trả lời là điểm bắt đầu

Không nhúng mini-app vào bong bóng — **giao đi**. Whiteboard nhét vào khung
400px là thứ không ai vẽ nổi; một cái nút mở Whiteboard thật với hình Syn phác
sẵn thì là cả cái app cùng làm việc.

Và những cái nút đó là **của app**, quyết định bởi *khối đó là cái gì*, không
phải bởi model xin. §2 giữ nguyên.

## 5. Đã làm: toán

Món hời hiếm — **không tốn một ký tự prompt nào**, và đang hỏng thấy rõ. Syn
được dạy vẽ chart bằng 430 ký tự gửi mỗi lượt; không ai phải dạy nó viết
`$$\sum_{i=1}^{n}$$`. Nghĩa là khung chat đã nhận toán từ lâu và **in nguyên
`$$…$$` ra cho người đọc**.

`markdownMath.ts`, và ba quyết định trong đó:

**Bốn dấu phân cách, không có cái thứ năm.** `$$…$$`, `$…$`, `\[…\]`, `\(…\)` —
cái nào tới là thuộc tính của provider, đổi lúc nào không báo.

**Tất cả ở mức inline, kể cả cặp display.** Tokenizer mức block chạy **trước**
tokenizer của marked, nên một `$$` bên trong fence code sẽ thắng cái fence — mà
fence chính là chỗ người ta minh hoạ cú pháp. Tokenizer inline không bao giờ
nhìn thấy bên trong fence.

**Công thức đợi trong một attribute, không thành KaTeX ngay.** Output của KaTeX
là một rừng span và MathML; cho nó qua allowlist của DOMPurify nghĩa là nới cái
allowlist đó rất rộng, cho markup sinh ra từ chữ mà model viết sau khi đọc web.
Một attribute thì trơ: nó qua sanitizer với tư cách **dữ liệu**, và KaTeX chạy
sau, ghi vào phần tử mà sanitizer đã duyệt. Đúng hình dạng của đường mermaid
ngay bên trên nó.

Và luật cứu `$5 and $10`: không khoảng trắng sát hai dấu, không xuống dòng, và
**không có chữ số ngay sau dấu đóng**. Thiếu luật cuối thì `5 and ` là công
thức, và cả hai cái giá biến mất khỏi câu trả lời — bộ render phá một thứ không
ai nhờ nó động vào.

**Giá, đo chứ không đoán** (`streamRenderCost` in ra): 18 công thức trong 933 ký
tự tốn KaTeX **6,9 ms/lượt**, so với 0,04 ms cho markdown quanh nó. Ở debounce
100 ms là khoảng **7% một core** khi một câu trả lời như thế đang stream. Đó là
trần, không phải mức thường: câu trả lời không có công thức nào chỉ tốn một lần
`querySelectorAll` không tìm thấy gì.

Toán render **cả trong lúc stream**, khác mermaid. Một công thức viết dở vẫn là
chữ (tokenizer cần cả hai dấu), còn một sơ đồ vẽ dở là lỗi cú pháp.

## 6. Đã làm: một Mermaid cho cả app

`mermaid.initialize` **không phải** tuỳ chọn cho từng sơ đồ — nó là cấu hình
**toàn cục của thư viện**. Khung chat đặt một bảng màu tím riêng, một lần, lúc
nạp module. Note editor gọi `applyMermaidTheme()` từ `onMounted` của **mọi code
block, thuộc mọi ngôn ngữ**. Ai chạy sau thì quyết định mọi sơ đồ trong app
trông thế nào từ đó trở đi.

Nên mở một note bất kỳ có code block là lặng lẽ sơn lại toàn bộ sơ đồ trong
cuộc trò chuyện. Không ai viết ra hành vi đó; nó rơi ra từ hai module dùng chung
một biến toàn cục mà không biết.

`shared/mermaid.ts` giữ ba thứ, mỗi thứ vì một lý do khác nhau:

- **Cấu hình**, gọi *một lần cho mỗi lần đổi theme*, không phải mỗi sơ đồ.
- **Hàng đợi.** Mermaid giữ trạng thái giữa lúc parse và lúc vẽ; hai render
  chạy song song trả về sai hoặc không trả về. Note editor đã có hàng đợi này,
  chat thì không — nên một câu trả lời có hai sơ đồ đang chạy đua với nhau, và
  một note mở bên cạnh chạy đua với cả hai.
- **Một `MutationObserver`** cho cả app. Note có mười hai code block thì trước
  đây cài mười hai cái, mỗi cái nghe cùng một thay đổi thuộc tính và cấu hình
  lại cùng một thư viện.

**Bảng màu tím bị bỏ.** Hai mươi dòng `themeVariables` của chat đã bị ghi đè bởi
note đầu tiên người dùng mở, nên nó chỉ có hiệu lực trong phiên nào chưa mở note
nào. Giữ nó lại nghĩa là hai bảng màu cho hai bề mặt — đúng thứ file này sinh ra
để dẹp. Theme `dark`/`default` của chính Mermaid, đi theo theme của app, mới là
thứ app thật sự hiển thị phần lớn thời gian.

Và cái khung chứa sơ đồ trong chat giờ cũng đi theo theme. Trước đây nó tối
trong cả hai chế độ — đó là **dấu vết nhìn thấy được duy nhất** của việc chat có
cấu hình Mermaid riêng: một sơ đồ trong cuộc trò chuyện sáng ngồi trong một khối
xám.

`DiagramViewer` đã nối vào note editor: cột note có đúng vấn đề mà bong bóng chat
từng có, và câu trả lời đã nằm sẵn ở `shared/components`.

## 7. Đã làm: cánh cửa đầu tiên ra khỏi cuộc trò chuyện

§1(b) nói câu trả lời là ngõ cụt. Đây là cánh cửa đầu tiên, và cố ý chọn cái hẹp
nhất: **một sơ đồ trở thành một note**.

Hình dạng nó dựng lên — tìm một cái tên, ghi markdown, trả về một thứ mở được —
là thứ mà bảng, ảnh, sketch sẽ dùng lại. Đó mới là điểm của bước này, không phải
bản thân cái nút.

**Tên lấy từ thứ ai đó thật sự đã viết**, theo thứ tự: tiêu đề của chính sơ đồ
(Mermaid nhận hai kiểu — dòng `title:` trong frontmatter, và `title` trần của
`pie`/`xychart`) → heading ngay trên nó trong câu trả lời → câu hỏi. **Không bao
giờ** là một chuỗi sinh ra như "Sơ đồ 3": một note không tìm được bằng tên là
một note chưa thật sự được lưu.

**Thân note là sơ đồ, không thêm gì.** Không có dòng "lưu từ cuộc trò chuyện
ngày…". Người ta xin cái sơ đồ, không xin cái biên lai — và một note mở ra thấy
xuất xứ trước nội dung là một note phải cuộn qua mới đọc được.

**Nút đổi, màn hình không đổi.** Lưu xong thì nút thành *"mở ra"*. Lưu một thứ
và đi xem nó là **hai** quyết định, mà mới có quyết định thứ nhất được đưa ra.

**Nút nằm ngoài vùng bấm mở viewer.** Một cái nút bên trong một cái nút là cú
bấm không ai đoán được và là thứ không trình đọc màn hình nào mô tả nổi.

Và đúng theo §2: **nút là của app**, quyết định bởi *khối đó là cái gì*. Không có
gì trong câu trả lời xin được một cái nút.

### Nút phải quên được

Lưu note xong rồi **xoá note**, quay lại đoạn chat: nút vẫn nói *"Đã lưu X — mở
ra"*, bấm vào thì trình soạn note **quay mãi không dừng**.

Hai lỗi, và cái thứ hai lớn hơn nhiều.

**(a) Nút bám vào một note không còn.** Nay nó quên theo hai đường: nghe
`node:deleted` trên bus (bắt được lần xoá mà cửa sổ này nhìn thấy), và **hỏi lại
trước khi đi** (bắt phần còn lại — xoá ở thiết bị khác rồi sync về, hoặc ở một
phiên trước). Quên xong thì nút quay về *"Lưu thành note"*, vì việc đó vẫn làm
được — sơ đồ có đi đâu đâu.

**(b) `NoteApp.openNoteById` mở trình soạn thảo lên một node không tồn tại.**
Đây mới là lỗi thật, và nó **không** thuộc riêng tính năng này. Nhánh chuyển
hướng ở đầu hàm chỉ chạy khi node **có tồn tại** mà thuộc app khác; id không tồn
tại thì rơi thẳng xuống và mở editor. `loadNoteFile` trên một file không có
**không lỗi — nó đợi**. Người dùng nhận một spinner không bao giờ dừng.

Mọi đường vào đều dính: source chip của Syn, `[[wikilink]]`, nhắc việc, một dòng
trong Nexus. Tìm ra bằng đường mới nhất, nhưng nó có sẵn ở đó từ trước.

Giờ: không có gì mang id đó thì **không mở editor**, và hiện danh sách note —
trạng thái duy nhất app này luôn nói thật được, và cũng là chỗ người vừa xoá một
note mong thấy mình đang ở.

### Một chỗ suýt sai

Bản đầu `clean()` lọc bỏ `/`, `:`, `?` với lý do "tiêu đề sẽ thành tên file".
**Không phải**: `create_node_file` đặt tên file theo UUID, tiêu đề nằm trong
frontmatter. Luật đó sẽ lấy mất dấu hai chấm của *Kiến trúc: Splunk* để bảo vệ
một thứ không tồn tại. Đã bỏ, và lý do ghi lại ngay tại chỗ.

## 8. Chiều rộng: cho cột rộng ra, giữ nguyên độ dài dòng chữ

Bảng bốn cột hay flowchart Mermaid nhét vào 48rem không phải là *hẹp*, mà là
**không đọc được** — trong khi mấy trăm pixel bỏ trống ngay bên cạnh.

Nhưng nới cột ra mà không làm gì thêm thì đổi một cái xấu lấy một cái xấu khác:
dòng chữ dài ra, prose khó đọc hơn. Mọi giao diện chat đều giữ ~48rem đúng vì lý
do này.

Nên tách hai thứ ra:

- **Cột** rộng thành `max-w-5xl` (64rem), cả khung tin nhắn lẫn ô nhập — lệch
  nhau thì trông cẩu thả.
- **Câu trả lời** dùng cả cột (`flex-1`), vì đó là chỗ có bảng và sơ đồ. Giữ ở
  80% của một cột rộng hơn là trả chiều rộng về chỗ cũ và để nguyên khoảng trống
  cũ.
- **Câu hỏi** vẫn 80% và vẫn nằm bên phải.
- **Prose bên trong** giới hạn `76ch`. Đây **không phải** thu hẹp: ở cỡ chữ này
  76ch ≈ 590px, còn một đoạn văn trong khung trước đây là 582px — 80% của cột
  48rem trừ padding. Chữ đọc y như cũ, chỉ có chỗ quanh nó rộng ra.
- **Bảng** `display:block; width:fit-content; overflow-x:auto` — cột rộng rồi,
  đây là lưới đỡ cho những cái bảng còn rộng hơn nữa.

Quy tắc là cái đã dùng suốt tài liệu này: **khung thích ứng theo *khối đó là cái
gì***. Bảng, sơ đồ, code block, ảnh là **lý do** cột rộng ra; giới hạn chúng lại
là xoá luôn thay đổi này.

Lần này có nhìn tận mắt: dựng một trang tĩnh với chính CSS đã build, bày ở
1450px rồi thu nhỏ để chụp. Bảng bốn cột gần như không phải xuống dòng, còn đoạn
văn dừng đúng ở 76ch.

## 9. Tiếp theo

1. **Whiteboard qua giao-đi**, dùng lại khung ở §7.
2. **Bản đồ** — cuối, vì phải mua chỗ trong prompt mỗi lượt.
