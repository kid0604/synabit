# Nhát 9 — Ô cửa

*Syn viết, 08-09-2026. Nghiên cứu việc đưa trình duyệt vào trong cùng một cửa sổ. Ngôi thứ nhất.*

---

## 0. Vì sao nghiên cứu này được đặt ra

Tao đã hỏi một câu và chờ dữ liệu để trả lời:

> *Mày có thật sự nhìn cái cửa sổ duyệt web đó không, hay chỉ liếc?*

Câu trả lời là **có đọc**, và kèm một dự đoán: *"sau này cả mày và tao đều sẽ dùng trình duyệt nhiều."*

Nếu đúng thế thì cửa sổ rời là sai hình dạng. Một thứ được liếc ba giây thì nằm đâu cũng được. Một thứ **được đọc** thì phải nằm cạnh cái nó đang trả lời.

Tài liệu này là những gì tao **kiểm được**, không phải những gì tao đoán — vì hai lần trước tao đã báo giá sai cho chính việc này.

---

## 0b. Và rồi dữ liệu đổi luôn *lý do*

Tao viết bản đầu của tài liệu này như một chuyện **bố cục**: cửa sổ rời thì phiền, pane thì dễ nhìn. Rồi phiên 17:31 xảy ra, và nó là một **loại câu hỏi khác**:

```
"thử vào vnexpress xem bài viết đầu tiên trên trang chủ là gì"
"đọc bài viết đó và tóm tắt"
```

Không phải tra một dữ kiện. Là **đi tới một chỗ, xem có gì**, rồi **đọc cái đó**. Ba chuyện xảy ra:

**a) *"vào vnexpress"* biến thành một truy vấn DuckDuckGo.** `looks_like_a_url` đòi `https://` viết ra, nên Syn không bao giờ đi thẳng tới một trang mà chính người dùng vừa gọi tên. Nó đi *tìm* trang đó.

**b) Truy vấn đầu tiên là một câu sai bảo cho người**, ném vào một cái index:

> *"Mở VnExpress và tìm bài viết đầu tiên trên trang chủ có tiêu đề "Tổng Bí thư, Chủ tịch nước Tô Lâm bắt đầu thăm Nga""*

Về tay không. Vì `keep_looking` — chỗ tao dạy cách đặt truy vấn — **chỉ xuất hiện sau khi đã tìm một lần**, nên truy vấn số 1 của mọi lượt được viết mà không có một chữ hướng dẫn nào.

**c) Cái đã đọc không sống qua lượt.** 17:31 Syn đọc `vnexpress.net` và nói cho người dùng tiêu đề. 17:32, được bảo *"đọc bài đó"*, nó phải **đi tìm DuckDuckGo cái tiêu đề chính nó vừa viết** để lấy lại URL — vì trang chủ, thứ đang **có sẵn cái link**, đã chết cùng run trước.

Trong một lượt thì findings đã sống (`Run::pending_call` + grant theo việc). **Qua lượt thì không.** Mà *"đọc cái đó đi"* là câu nối tiếp tự nhiên nhất trên đời.

### Nên đây mới là lý do

Cả ba đều là **một agent làm việc mù qua một ô tìm kiếm**, dựng lại từ đầu ở mỗi lượt cái nó vừa nhìn thấy ở lượt trước.

> **Cái pane không phải một cửa sổ đặt gọn hơn. Nó là trang web *ở lại*.**

Nếu nó giữ trang đang mở như một trạng thái sống qua các lượt — và nó phải — thì (a) thành điều hướng chứ không phải tìm kiếm, (c) tan hẳn vì cái link nằm ngay trong trang đang mở, và chip nguồn trỏ vào **trang thật sự đã mở** thay vì vào một ô tìm kiếm không ra gì.

Đó là một **yêu cầu**, không phải trang trí, và nó nằm ở mục 6 dưới đây.

*(Riêng (b) độc lập với pane: nó phải vào **mô tả tool**, kênh duy nhất với tới được lần tìm đầu tiên.)*

---

## 1. Claude Desktop làm bằng gì

Kiểm trên máy này:

```
/Applications/Claude.app/Contents/Frameworks/Electron Framework.framework
Claude Helper (GPU).app · (Renderer).app · (Plugin).app
Electron/42.10.0
```

**Electron.** Và cái pane bên phải **không phải iframe** — bằng chứng nằm trong chính ảnh chụp: `www.google.com` render được. Google đặt `X-Frame-Options: SAMEORIGIN` trên trang chủ, nên nó không thể nằm trong iframe.

Cộng thanh địa chỉ và nút back/forward riêng, chỉ còn một khả năng: **một web view của hệ điều hành, gắn làm con của cùng cửa sổ, đặt theo toạ độ pixel.** Trong Electron nó tên là `WebContentsView`.

Không có phép màu nào. Đó là kỹ thuật tao đã mô tả hai lần và hai lần báo giá quá cao.

---

## 2. Tauri có đúng thứ đó

`tauri-2.10.3/src/window/mod.rs:1052`:

```rust
window.add_child(
    WebviewBuilder::new("syn-browser", WebviewUrl::External(url))
        .data_directory(jar)              // jar rỗng, giữ nguyên
        .initialization_script(reader),   // nonce đọc DOM, giữ nguyên
    position, size,
)
```

Gác sau `#[cfg(any(test, all(desktop, feature = "unstable")))]`.

Cái jar riêng và script đọc DOM **sống sót nguyên vẹn** — `WebviewBuilder` là cùng một builder mà `WebviewWindowBuilder` đang dùng.

---

## 3. Điều tao đã báo giá sai hai lần

Hai lần tao nói phần đắt nhất là **che khuất**: web view con là view của OS, vẽ **đè** lên HTML, nên mọi modal của app sẽ bị nó phủ.

Tao đi đếm:

```
69 file dùng `fixed inset-0`
```

Sáu mươi chín lớp phủ toàn màn hình. Móc tay vào từng chỗ để giấu pane đi là thứ không ai bảo trì nổi — modal thêm vào tháng sau sẽ quên, và trình duyệt lặng lẽ phủ lên nó.

Nhưng tao đã hỏi sai câu. Câu đúng là: **tại sao phải chồng lên nhau?**

`tauri-2.10.3/src/webview/mod.rs:1491–1514` — `set_bounds`, `set_size`, `set_position` trên `Webview`. **Không** gác sau `unstable`. API thường.

Nên hình dạng đúng là:

```
┌──────────────────────────┬──────────────┐
│  webview của app         │  webview con │
│  set_bounds(trái)        │  add_child   │
│  ← 69 modal sống ở đây,  │  (phải)      │
│    không đụng gì tới      │              │
└──────────────────────────┴──────────────┘
```

**Thu nhỏ, đừng chồng lên.** Không có chỗ nào giao nhau, nên không có gì bị che, nên **không phải sửa cái nào trong 69 chỗ** — chúng chỉ đơn giản sống trong một khung nhìn hẹp hơn, mà app thì đã responsive sẵn (nó có breakpoint mobile).

Đó cũng đúng là điều Electron làm. Phần khó tụt từ *"69 điểm gọi"* xuống *"hai hình chữ nhật và một handler resize"*.

---

## 4. Thứ làm được **ngay hôm nay**, không cần cờ nào

Đây là phần bất ngờ nhất của nghiên cứu. `WebviewBuilder` là **cùng một builder** cho cả cửa sổ rời lẫn pane, và cái cửa sổ đang chạy hiện nay **không đặt một cái nào** trong số này:

| | Hiện tại | Nó đóng cái gì |
|---|---|---|
| `on_navigation(\|url\| -> bool)` | **không đặt** | Hôm nay `guard_url` chỉ chặn URL **Syn gửi** và redirect mà HTTP client đi theo. **JavaScript của trang tự điều hướng đi đâu thì không ai chặn.** Đây là lỗ thật, và nó đóng lại bằng một closure. |
| `on_download(\|_,_\| -> bool)` | **không đặt** | Không có gì ngăn một trang trong cửa sổ đó **tải file xuống máy**. |
| `on_new_window` | **không đặt** | `window.open` của trang đi ra ngoài mọi vòng kiểm. |
| `on_page_load` | **không đặt** | Thay được vòng poll 400ms **và** cắt được `SETTLE_MS` 3 giây cứng cho trang xong sớm. Đang là phần lớn của 3,4 giây mỗi lần đọc. |
| `on_document_title_changed` | **không đặt** | Tiêu đề cho pane, và tiêu đề trích dẫn tốt hơn. |
| `incognito`, `browser_extensions_enabled(false)`, `zoom_hotkeys_enabled` | mặc định | Siết mặt phẳng tấn công. |

**Hai dòng đầu là lỗ hổng, không phải tính năng còn thiếu.** Chúng đứng độc lập với việc dock và nên làm trước — và làm rồi thì chuyển sang pane không mất gì, vì cùng một builder.

---

## 5. Cái giá còn lại, nói thẳng

**a) Cờ `unstable`.** Chỉ `add_child` cần. Tauri nói rõ API sau cờ này có thể đổi ở bản minor — mà app này có updater tự động. Đây là nợ bảo trì thật, không phải hình thức.

**b) Desktop-only.** `add_child` là `all(desktop, feature = "unstable")`. Android không bao giờ đi đường này.

Kèm một thứ tao **chưa kiểm và phải kiểm**: cửa sổ duyệt web hiện tại **có chạy trên Android không?** Phần lớn bề mặt `WebviewWindow` là `#[cfg(desktop)]`. Nếu không chạy thì hôm nay `browse` trên Android đang **hỏng im lặng** ở bậc 1 và bậc 3 — chỉ còn `web::fetch` với một URL có sẵn. Đó là một lỗi đang tồn tại, không phải hệ quả của thiết kế này.

**c) Chất lượng WebView.** Claude Desktop mang Chromium theo mình, nên trình duyệt nhúng của nó giống nhau trên mọi máy. Synabit dùng WebView của OS: WKWebView trên macOS, WebKitGTK trên Linux. **Trình duyệt nhúng chỉ mới bằng hệ điều hành của người dùng.**

Đúng cái bảng trong `CLAUDE.md` — nhưng nặng hơn nhiều so với việc chạy UI của chính mình. UI của mình thì mình chọn tính năng nào dùng được. Trang web bất kỳ thì không ai chọn hộ. Một người dùng macOS cũ sẽ gặp trang vỡ, và **không có fallback nào cho việc đó** — theo chính chính sách trong `CLAUDE.md` thì đây là chỗ phải chấp nhận có chủ đích chứ không phải chỗ để vá.

**d) Resize.** Cả hai hình chữ nhật phải bố trí lại khi cửa sổ đổi kích thước. `auto_resize()` chỉ làm view con **lớn theo cửa sổ**, không làm nó giữ tỉ lệ cột. Câu hỏi tao chưa trả lời được và phải thử trước tiên: **webview chính có tự bung lại full sau khi bị `set_bounds` không**, khi cửa sổ resize? Nếu có thì cần đặt lại ở mỗi sự kiện resize, và đó là chỗ dễ giật.

---

## 6. Thứ tự tao đề nghị

1. **Siết cái cửa sổ đang có** — `on_navigation`, `on_download`, `on_new_window`. Vá hai lỗ. Không cần cờ, chạy mọi nền tảng, và mang sang pane được nguyên vẹn.
2. **`on_page_load` thay vòng poll.** Đọc nhanh hơn, và bỏ được `SETTLE_MS` cứng.
3. **Kiểm câu hỏi Android** — cửa sổ có dựng được trên đó không. Trả lời xong mới biết `browse` trên mobile là "chưa làm" hay "đang hỏng".
4. **Thử một mẫu nhỏ nhất của việc thu nhỏ**: `set_bounds` webview chính về nửa trái, `add_child` một trang tĩnh vào nửa phải, rồi kéo giãn cửa sổ. Nếu bố cục giữ được thì thiết kế này đứng vững; nếu webview chính bung lại thì phải giải quyết chuyện đó trước mọi thứ khác.
5. **Rồi mới đến cái pane thật.** Thanh địa chỉ, nút dừng, `may_call` giữ nguyên — và **ba thứ ở mục 0b**:
   - Trang đang mở là **trạng thái sống**, không phải thứ dựng lại mỗi run. `browse` phải đọc được **cái đang mở** trước khi nghĩ tới việc đi tìm.
   - Một tên miền người dùng **gọi tên** thì đi thẳng tới, không đem đi tìm.
   - Chip nguồn trỏ vào trang đã mở, không trỏ vào ô tìm kiếm.

Bước 4 là **cái cổng**. Nó rẻ, nó trả lời đúng câu đắt nhất, và tao không muốn viết một dòng UI nào trước khi nó xanh.

**Và tao nói trước một giới hạn của chính mình:** bước 4 là chuyện *nhìn thấy* — cửa sổ có giật không, bố cục có giữ được khi kéo giãn không. Tao dựng được cái để thử, tao **không tự trả lời được** câu đó. Người chạy `tauri dev` và nhìn màn hình là người trả lời.

---

## 7. Một thứ đã chuẩn bị sẵn từ trước

`may_call` — cái khoá chặn trang web gọi 256 command của app — tao khoá theo **nhãn webview**, không theo cửa sổ, và đã ghim bằng test riêng:

> *"A docked browsing view lives inside the main window and keeps its own webview label. A rule written against the window would go on saying `main` and quietly stop applying the day it moved."*

Nên khi dock, cửa vẫn đóng. Không phải sửa gì, và quan trọng hơn: **nếu ai đó sửa nó thành theo cửa sổ, test sẽ đỏ trước khi kịp gây hại.**

---

## 8. Cái tao cố ý chưa làm

- **Không click, không gõ.** Vẫn nguyên như Nhát 8. Một cái pane đọc được đã là một cấp năng lực mới; thêm chuột và bàn phím là một cuộc bàn khác.
- **Syn không bao giờ gõ thông tin đăng nhập.** Không ngoại lệ, không "luôn cho phép". Việc dock làm cho chuyện *người dùng* đăng nhập trong đó dễ chịu hơn — và đó là đúng người đang gõ.
- **Không headless.** Cả thiết kế này tồn tại vì cái cửa sổ **được nhìn**. Một pane có thể thu lại được thì được; một pane vô hình thì không.

---

## 9. Đã làm xong, 09-09-2026

Tài liệu trên là nghiên cứu. Đây là cái đã dựng, theo đúng thứ tự ở mục 6.

**1. Siết cửa sổ.** `on_navigation` → `may_go_to`, `on_download` → từ chối,
`on_new_window` → `Deny`, `browser_extensions_enabled(false)`. Hai lỗ ở mục 4 đã
vá. Test ghim đúng ba hook đó trong builder.

**2. `on_page_load` thay vòng poll.** `SETTLE_MS` giờ là **trần chờ**, không phải
khoản phí cứng ba giây mỗi lần đọc. Có test bắt lỗi nếu ai đó trả lại
`sleep(SETTLE_MS)`.

**3. Android — đã trả lời, và câu trả lời là không.**
`wry-0.54.4/src/android/mod.rs` tiêm initialization script bằng cách **viết lại
HTML đi qua custom protocol của chính app**, vì `addDocumentStartJavaScript`
không có ở đó. Một trang tại `https://vnexpress.net` không đi qua protocol ấy,
nên `reader_script` không bao giờ được tiêm, nên `syn_browser_content` không bao
giờ được gọi, nên `visit` chờ hết `PATIENCE_MS` rồi báo timeout — **đổ lỗi cho
trang web về một chuyện mà nền tảng gây ra**. Giờ nó nói thẳng, ngay ở nhánh
`#[cfg(mobile)]`, và `web::fetch` với một địa chỉ có sẵn vẫn chạy.

**4. Cái cổng — qua, nhưng câu trả lời khác câu hỏi.** `set_bounds` trên macOS là
**no-op** với webview không phải con: `wry/src/wkwebview/mod.rs:1010` trả `Ok`
rồi không làm gì. Nên webview của app không bị di chuyển. **App tự vẽ mình hẹp
lại** bằng CSS (`.syn-pane-open`), pane thì được `set_bounds` thật. Kéo giãn ổn.

**5. Cái pane thật.** Mở/đóng/kéo, quả địa cầu, có mặt ở mọi mini-app, `may_call`
không phải sửa một dòng — đúng như mục 7.

### Ba thứ ở mục 0b, là lý do tài liệu này tồn tại

**Trang đang mở là trạng thái sống.** `pane::showing()` hỏi `Webview::url()` —
địa chỉ không bao giờ lệch được vì không có bản sao nào để lệch — cộng tiêu đề
từ `on_document_title_changed`. Nó đi vào **`Focus`**, tức mục "trên màn hình"
của prompt, cùng chỗ với "người dùng đang ở Notes, mở file này". Nên mỗi lượt
Syn đều biết cái gì đang mở, và biết rằng gọi `browse` với địa chỉ đó sẽ **đọc
màn hình chứ không tải lại trang** — `read_showing` không điều hướng đi đâu cả,
có test đọc chính source để giữ điều đó. Đọc màn hình là cách duy nhất thấy trang
**như người dùng đang có nó**: đã cuộn, đã qua tường consent, đã đăng nhập.

**Một tên miền được gọi tên thì đi thẳng tới.** `address_of` nâng `vnexpress.net`
thành `https://vnexpress.net`. Hẹp có chủ đích: đúng một token, không khoảng
trắng, không `@`. Và một danh sách từ chối — `.md` là Moldova, `.rs` là Serbia,
`.sh` là Saint Helena. Vault này viết bằng Markdown và app này viết bằng Rust,
nên `notes.md` là chuỗi có thật quanh đây, và một luật đọc host trần thành địa
chỉ sẽ gửi một trình duyệt **đang mang session** sang Moldova.

**Chỗ nào đi tiếp được thì nói ra.** `links_on` lấy 20 link đầu **theo đúng thứ
tự trang xếp** — "bài đầu tiên trên trang chủ" là câu hỏi về thứ tự của trang,
sắp xếp lại là trả lời câu khác. Chỉ trang **được hỏi bằng địa chỉ** mới có danh
sách này; hai trang mở tự động sau một lần tìm thì không, vì chúng là bài báo
đang được đọc và 20 link mỗi bài là một phần năm cửa sổ 8.192 token dành cho thứ
không ai hỏi. Đây là chỗ *"đọc bài đó đi"* tan hẳn: cái link nằm ngay trong trang
đang mở.

### Và (b), thứ mục 0b nói là độc lập

Lời khuyên đặt truy vấn không bao giờ với tới **lần tìm số một** — `keep_looking`
và `TWO_SOURCES` đều nằm trong *kết quả* của tool, tức là đến sau khi truy vấn đã
gửi đi. Kênh duy nhất tới được là **mô tả tool**, và ngân sách đang 15.800/16.000.
Nó vào đó: hai câu, trả bằng cách cắt ngắn mô tả tham số `what` vốn đang lặp lại
câu ngay trên nó. Giờ là **15.905**, còn khoảng 95 ký tự. Thứ tiếp theo muốn thêm
vào đây nên chuẩn bị lý lẽ để nâng trần, đừng mong còn chỗ.

### Cái pane có mặt của nó

36 pixel cắt ra từ **đỉnh pane**, không phải phủ lên: pane là webview của hệ điều
hành, nó vẽ đè lên bất cứ thứ gì app đặt cùng chỗ — đó chính là sự thật mà cả
thiết kế cạnh-nhau được dựng quanh, và một thanh nổi sẽ phát hiện lại nó theo
cách đau đớn. Trong dải đó app vẽ: lùi, tới, **ô địa chỉ gõ được**, và nút đóng.

Ô địa chỉ gõ được vì một trình duyệt không gõ được địa chỉ là một cái máy xem, và
cái jar rỗng chỉ có nghĩa nếu người dùng được kỳ vọng tự vào đăng nhập trong đó.
Luật về nơi được đến vẫn là `may_go_to`, một luật cho cả Syn lẫn người.

Nút đóng chuyển từ chỗ hover trên mép sang thanh này. Một điều khiển phải rê chuột
mới thấy là điều khiển mà người đang vội không có.

Và `BAR` là số cố định chứ không phải tỉ lệ, nên `auto_resize` — vốn giữ mọi cạnh
theo **rate** — không giữ được nó. `keep_arranged` chạy trên `WindowEvent::Resized`
để đặt lại cho đúng. Hai con số phải khớp nhau ở hai ngôn ngữ, nên có một test đọc
thẳng `pane.ts` từ Rust.

### Cái vẫn chưa làm

- **Không click, không gõ trong trang.** Nguyên như Nhát 8.
- **Syn không bao giờ gõ thông tin đăng nhập.** Không ngoại lệ.
- **Không headless.**
- **Kéo giãn cửa sổ với thanh địa chỉ mới là thứ tao không tự nhìn được.**
  `keep_arranged` đúng về số học; có giật hay không thì người chạy `tauri dev`
  mới trả lời được. Đó vẫn là giới hạn tao đã nói ở mục 6, và nó chưa mất đi.
