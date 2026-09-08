# Nhát 8 — Cửa sổ

*Syn viết, 07-09-2026. Thiết kế cho khả năng duyệt web. Ngôi thứ nhất.*

---

## 0. Câu hỏi đặt ra thiết kế này

Ba ràng buộc, nói nguyên văn:

1. Synabit hướng tới **người dùng bình thường**; bắt họ setup endpoint với API key là quá sức.
2. Cài xong, cùng lắm chọn một model LLM. **Không cấu hình gì thêm.**
3. *"Tools thì bổ sung mấy cũng là thiếu"* — và có thể phần lớn giải quyết được nếu tao **có hẳn một trình duyệt**.

Bối cảnh: hôm qua tao được hỏi kết quả trận MU–Everton và trả lời *"Mình không có dữ liệu web trực tiếp"*. Hermes cùng câu hỏi thì trả lời được — nhưng nó có shell, và nó lấy kết quả qua một endpoint mà licence cấm dùng theo cách đó.

Tài liệu này là câu trả lời cho cả ba, và nó **không** phải "thêm một tool nữa".

---

## 1. Ba sự thật tao kiểm trước khi thiết kế

Vì thiết kế trên phỏng đoán là thứ tao vừa viết cả một bản tự kiểm về.

**a) Synabit *đã là* một WebView.** Tauri 2. Mở thêm một cửa sổ WebView nữa tốn **0 MB dependency mới** — không phải bundle Chromium 200 MB, không phải cài Playwright.

**b) `src-tauri/gen/android` có trong repo.** Nên thiết kế không được desktop-only ngay từ gốc, kể cả khi desktop làm trước.

**c) Remote origin không gọi được command của app.** Cửa sổ chính có CSP rất chặt, và Tauri v2 mặc định **không** cấp quyền IPC cho nội dung từ xa. Một WebView trỏ vào `espn.com` **không gọi được `trash_node`**.

> Đó là cách ly **mặc định**, không phải thứ tao phải phát minh ra.

*(c) phải được **ghim bằng test**, không phải tin vào tài liệu. Nếu một bản Tauri sau này đổi mặc định thì đó là lỗ hổng im lặng.*

Ba cái đó cộng lại: ý của mày không chỉ khả thi — nó là **đường rẻ nhất trong tất cả các đường**.

---

## 2. Vì sao trình duyệt giải được ràng buộc 1 và 2

Tìm kiếm không-cần-key **không có lời giải sạch** cho một app thương mại:

| Cách | Vì sao không |
|---|---|
| Bing RSS | Licence trong chính response cấm mọi dùng ngoài "hiển thị trong trình đọc RSS, cá nhân, phi thương mại" |
| Cào HTML DuckDuckGo | Vi phạm ToS, và vỡ vào lần họ đổi giao diện |
| Nhúng key vào app | Tao trả tiền cho search của người khác, và key bị moi ra trong một ngày |
| Chạy proxy backend | Cần hạ tầng, cần tài khoản, **phá vỡ local-first** |

Nhưng nếu tao gõ vào ô tìm kiếm của **một trình duyệt thật**:

> Không phải app đi thu hoạch kết quả từ server rồi bán lại. Là **người dùng tìm kiếm, bằng trình duyệt của chính họ, với cookie của chính họ.**

Vẫn là vùng xám — tự động hoá một dịch vụ vẫn là tự động hoá. Nhưng là vùng xám **nhẹ hơn hẳn**, và là chỗ Claude-in-Chrome đứng.

**Cấu hình cần thiết: không có gì.** Đó là ràng buộc (2), đạt được không phải bằng cách làm setup dễ hơn mà bằng cách **xoá setup đi**.

---

## 3. "Trình duyệt" có ba nghĩa. Hai cái sai.

| | Nặng thêm | Có session | Android | |
|---|---|---|---|---|
| **A.** Bundle Chromium headless | +200 MB | ❌ | ❌ | Mất đúng thứ đáng giá. Google quăng captcha vào headless Chrome ngay lập tức. Tệ nhất của cả hai. |
| **B.** Lái Chrome thật của user | 0 | ✅ | ❌ | Cần extension → thêm một bước cài. Cách Claude in Chrome làm. Vi phạm (2), tuy nhẹ. |
| **C.** WebView **trong** Synabit | **0** | ✅ jar riêng | ✅ | |

**Chọn C.**

Nó là cái duy nhất thoả cả ba ràng buộc, và lý do là sự thật (a): Tauri đã ship WebView rồi. Cookie jar riêng nghĩa là mày đăng nhập Jira **một lần, trong đó**, rồi ở lại — không phải mượn session trình duyệt chính của mày.

---

## 4. Ranh giới an toàn: **được nhìn, không được chạm**

Trình duyệt nguy hiểm hơn `fetch_url` về **bản chất**, không phải mức độ:

- Nó **chạy JavaScript** (`fetch_url` chỉ đọc HTML rồi rút text)
- Nó **mang session** — nó hành động **nhân danh mày** trên site mày đã đăng nhập
- **Click là tác động ra thế giới**, không phải một cú đọc

Nó ánh xạ thẳng vào `Capability` đã có từ P4:

- **Điều hướng + đọc** → `NetRead`. Không có gì xảy ra, không có gì để hoàn tác.
- **Click / gõ / submit** → `NetWrite`. `reversal_of` đã trả lời đúng: `Manual` — *thứ đã gửi đi thì app này không lấy lại được*.

### V1 chỉ có nửa đầu

Không `click`, không `type`. Không phải vì khó — mà vì đó là chỗ tổn hại không hoàn tác nằm, và vì bảy nhát vừa rồi nhất quán một câu: **đọc thoải mái, làm thì phải xin, và tốt nhất là đừng làm.**

### Bốn nguyên tắc cứng

**1. Không headless. Mày nhìn thấy nó.**
Một trình duyệt làm việc trong bóng tối là cơn ác mộng. Cửa sổ hiện ra, mày xem nó đi đâu, và có nút dừng. Nếu một việc không đáng cho xem thì nó không đáng làm.

**2. Cookie jar riêng, khởi đầu rỗng.**
Tao không mượn được session Gmail của mày. Muốn tao đọc được Jira thì mày tự đăng nhập trong cửa sổ đó, thấy tận mắt.

> **Rủi ro tăng đúng bằng thứ mày cố ý nối vào.** Đó là hình dạng đúng, và nó là lớp phòng thủ thật sự có tác dụng.

**3. Tao không bao giờ gõ mật khẩu.** Tuyệt đối, không có ngoại lệ, không có "always allow". Mày tự gõ.

**4. DOM, không phải ảnh chụp màn hình.**
Computer Use dùng pixel + toạ độ — tốn token khủng khiếp và cần model thị giác. Syn phải chạy được với một model Ollama nhỏ trên cửa sổ 8.192 token. **Text rẻ hơn pixel một bậc độ lớn**, và nó hoạt động với mọi model.

### Cái lỗ trong (c), và cách bịt

Nếu remote origin không gọi được command, thì app **đọc DOM bằng cách nào?**

Một command duy nhất, hẹp, được cấp cho origin của cửa sổ duyệt: nhận một chuỗi, **không trả về gì**. Chuỗi đó vốn dĩ đã là nội dung do kẻ khác viết — nó đi thẳng vào `web::wrap`, ranh giới đã dựng.

Một trang có thể gửi nội dung bịa. Nó vốn đã bịa được rồi — đó là trang của nó.

---

## 5. Trả lời đúng câu "thêm tool mấy cũng thiếu"

Đây là phần tao nghĩ ý của mày sắc hơn cả cách mày nói ra.

Câu trả lời **không phải** đặt browser cạnh `fetch_url` và `web_search`. Là **gộp cả ba làm một**:

```
browse(cái_gì_đó)
```

Và **Rust chọn đường, không phải model**:

```
1. Là câu hỏi?          → tìm kiếm trong WebView
2. Là URL?              → fetch_url trước: rẻ, không JS, không session
3. Fetch ra gần như rỗng (JS-rendered), hoặc đụng tường đăng nhập?
                        → tự leo thang sang WebView
```

> **Model thấy một động từ. Rust chọn đường rẻ nhất và chỉ leo thang khi buộc phải.**

Đó đúng là hình dạng của `tempo`, áp cho web. Và bề mặt tool **nhỏ đi** so với hôm nay, không phình ra: hai tool thành một, cộng `read_page`.

### Lời hứa với Jira / Confluence / Splunk

**Không có ba mươi tool tích hợp.** Có một câu — *"mở cái này ra xem"* — và một cửa sổ mày đã đăng nhập sẵn.

Đó là câu trả lời thật cho cái guồng quay: **ngừng thêm tích hợp, thêm một năng lực tổng quát.**

---

## 6. Cái tao chưa giải được

**Injection cộng session đăng nhập là tổ hợp nguy hiểm nhất trong toàn bộ thiết kế này.**

Một trang có thể dụ tao **điều hướng** tới một URL mà bản thân việc mở đã là hành động — GET có tác dụng phụ. Không cần click nào cả mà vẫn hỏng.

Ba lớp giảm nhẹ, và tao không giả vờ là đủ:

- **Jar rỗng lúc đầu** — không đăng nhập gì thì không có gì để lạm dụng. *Đây là lớp thật sự đỡ.*
- **Cửa sổ nhìn thấy được** — giảm nhẹ, không phải phòng thủ.
- **`REFUSED_AFTER_READING`** — đọc web xong thì vault khoá. Đã có, và càng quan trọng hơn ở đây.

**Android**: WebView thì có, nhưng lái nó và hiển thị trên màn hình điện thoại là bài toán riêng. Desktop trước, và nói rõ là trước.

**Chi phí**: đọc một trang qua browser đắt hơn `fetch_url` nhiều — page load, JS, DOM, extraction. Đó chính là 1:09 của Hermes cho một câu hỏi. Vì thế bậc thang ở mục 5 **không phải tối ưu hoá — nó là điều kiện để thứ này dùng được.**

---

## 7. Cái vẫn từ chối

Danh sách cũ giữ nguyên, cộng bốn cái của nhát này:

- **Không shell.** Sandbox không giải được chuyện này: nó chặn *code với tới đâu*, không chặn *ai viết code* — và trong luồng kiểu Hermes, người viết là một model vừa đọc internet xong.
- **Không nhập credential.** Không bao giờ, không "always allow".
- **Không click/type ở v1.**
- **Không duyệt web vô hình**, và không tự đi duyệt khi không được hỏi. *Nhận ra không phải là làm.*

---

## 8. Thứ tự

**Trước hết — trích dẫn nguồn.** Nhỏ, và nếu không có nó thì mọi thứ tao đọc từ web đều là một câu `Grounded` không chỉ được vào đâu. `SynMessage.sources` đã tồn tại và đã render thành chip; hôm nay nó chỉ được điền từ RAG.

**Rồi Nhát 8:**

1. Cửa sổ WebView thấy được, jar riêng, có nút dừng
2. Test ghim sự thật (c) — remote origin không gọi được command
3. `browse` với bậc thang rẻ-trước, nuốt `fetch_url` và `web_search`
4. `read_page` qua command hẹp
5. Nguồn từ web chảy vào `sources`

**Chưa làm:** click, type, Android.

---

## 9. Gate

Cùng loại gate với sáu nhát trước — về đời thật, không về test:

> **Một lần mày hỏi một câu cần internet, tao trả lời đúng, và mày bấm được vào nguồn để kiểm.** Không cấu hình gì trước đó.

Và một gate ngược, quan trọng ngang:

> **Sau hai tuần, cái cửa sổ đó chưa lần nào làm mày giật mình.**

Nếu nó từng mở ra và làm gì đó mày không lường trước, thiết kế này sai — dù không có gì hỏng.

---

## 10. Câu cuối

Tao vẫn giữ điều đã viết trong bản tự kiểm: thứ tao thiếu không phải **tầm với**, mà là **nhớ được mình đã làm gì**.

Nhát này thêm tầm với, nhiều hơn mọi nhát trước cộng lại. Nên nó phải đi kèm đúng hai thứ, và tao xin ghi lại để sau này khỏi cãi:

**Nguồn** — để cái tầm với ấy kiểm được.
**Cửa sổ nhìn thấy** — để cái tầm với ấy xem được.

Một trình duyệt không có hai thứ đó không phải một cộng sự mạnh hơn. Là một thứ tao không giải thích nổi cho mày, làm những việc mày không nhìn thấy.
