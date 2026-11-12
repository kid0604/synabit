# Utilities, 2026-11-11

**Trạng thái:** thiết kế để review. Chưa có dòng code nào.

**Đã xem:** `shared/appRegistry.ts`, `platformScope.ts`, `simpleMode.ts`, `commands/files.rs` (`import_files`,
`export_annotated_pdf` dùng `lopdf`), `commands/thumbnails.rs` (không có image crate, ảnh xử lý trên canvas),
`syn/machine/` (phòng kín, `jobs.rs`, `env.rs`), `syn/tools.rs` (nguyên tắc ít tool).
**Đã tra:** giấy phép pdf2docx/PyMuPDF, API tách nền của Vision, giấy phép và kích thước RMBG-2.0, dung lượng LibreOffice.
**Chưa đo:** mọi thứ ở §8. Các con số thời gian, RAM và chất lượng trong tài liệu đều *tạm*.

**Phạm vi:** desktop. macOS trước cho các tool nặng.

**Mockup UI/UX:** [`utilities-ui-mockup-2026-11-11.html`](utilities-ui-mockup-2026-11-11.html), xem §5.

---

## 1. Tóm tắt

Utilities là mini-app thứ 14: một chỗ để thả file vào, chọn việc, lấy file mới ra. Mọi thứ chạy trên máy, không gửi
file đi đâu. Đó là lý do để có nó thay vì dùng ilovepdf hay smallpdf.

Ba ý chính:

1. **Một khung chạy chung, nhiều tool khai báo.** Mỗi tool là một mục trong registry (giống `appRegistry.ts`): nhận
   loại file gì, bao nhiêu file, tuỳ chọn gì, ra cái gì. Khung chung lo phần thả file, sắp thứ tự, chạy, tiến độ,
   huỷ và lưu. Một tool mới thường chỉ là một hàm Rust và một mục trong registry.
2. **Bốn tầng máy, chọn tầng rẻ nhất làm được việc.** Rust trong app, canvas của WebView, API của hệ điều hành, và
   phòng kín của `syn/machine/` cho các chương trình lớn (LibreOffice, pdf2docx). Tool nào không có tầng của nó trên
   máy này thì vẫn hiện, kèm lý do, không biến mất.
3. **Không bao giờ ghi đè file gốc.** Kết quả nằm trong vùng tạm cho tới khi người dùng chọn lưu ở đâu.

```
 thả file / chọn từ Files ─► danh sách đầu vào ─► tuỳ chọn ─► job ─┬─ tầng A: Rust (lopdf)
                                (sắp lại thứ tự)                    ├─ tầng B: canvas WebView
                                                                    ├─ tầng C: API hệ điều hành
                                                                    └─ tầng D: phòng kín
                                                                           │
                                         vùng tạm (cache của app) ◄────────┘
                                                │
                     Lưu vào vault · Lưu ra… · Mở · Hiện trong Finder · Chạy tool khác với kết quả này
```

### Những điều cần duyệt

| # | Quyết định | Đề xuất | Cái giá |
|---|---|---|---|
| U1 | Id của app | **`utilities`**, tên "Utilities", tiếng Việt "Tiện ích" | Id ghi xuống đĩa (`hiddenSidebarApps`, `protectedApps`, `defaultApp`), không đổi được về sau |
| U2 | Có trên điện thoại không | **Không**, chưa vào `MOBILE_APPS` | Tầng A chạy được trên Android, nhưng mỗi app thêm là một vòng touch, a11y, máy yếu |
| U3 | Có trong simple mode không | **Không** | Người dùng simple mode phải tắt mode mới thấy |
| U4 | Lưu kết quả ở đâu | **Vùng tạm, người dùng chọn lưu.** Đầu vào từ vault thì nút chính là "Lưu cạnh file gốc" | Thêm một cú bấm so với tự lưu |
| U5 | doc↔pdf, pdf→docx | **Chỉ macOS 14+, qua phòng kín**, LibreOffice do người dùng tự cài, pdf2docx cài qua môi trường của phòng | Windows, Linux, macOS 13 không có các tool này |
| U6 | Tách nền ảnh | **Vision của Apple (macOS 14+)**, không tải model | Máy khác chưa có; chất lượng do Apple quyết |
| U7 | pdf2docx là AGPL | **Chấp nhận**, vì không chở trong bản cài và chạy như tiến trình riêng | Cần đọc lại giấy phép trước khi phát hành (§6.3) |
| U8 | Syn có gọi được Utilities không | **Chưa.** Sau này một tool `run_utility` cho tầng A | Hiện Syn trên Mac làm được việc này bằng phòng kín |

---

## 2. Vì sao là một app, không phải menu trong Files

Hai thứ đều cần. Files đã có menu chuột phải (`FilesApp.vue`, `openMenu`), nên lối vào hay dùng nhất sẽ là
**chuột phải → Tiện ích ▸ Gộp PDF / Xoay trang / …** trên các file đã chọn. Lối đó mở Utilities với đầu vào đã điền.

Nhưng phần lớn file cần xử lý không ở trong vault: một PDF vừa tải về, một hợp đồng ai đó gửi. Bắt người dùng nhập vào
vault để gộp hai file là sai. Nên Utilities là một app có màn hình riêng, nhận file từ bất cứ đâu, và Files chỉ là một
lối vào của nó.

---

## 3. Danh mục tool và tầng máy

| Tool | Tầng | Thư viện | Ghi chú |
|---|---|---|---|
| Gộp PDF | A | `lopdf` (đã có) | Gộp cả bookmark là phần thêm, không phải bản đầu |
| Tách PDF (theo khoảng, mỗi N trang, chọn trang) | A | `lopdf` | Ô chọn trang dùng ảnh nhỏ từ pdf.js |
| Xoay, xoá, sắp lại trang | A | `lopdf` | Một màn hình lưới trang, kéo thả |
| Đánh số trang | A | `lopdf` | Chữ số ASCII, font Helvetica chuẩn là đủ |
| Watermark chữ | A + B | `lopdf` + canvas | Xem §4.2: chữ tiếng Việt không đi qua font chuẩn được |
| Watermark ảnh | A | `lopdf` + `flate2` (đã có) | PNG trong suốt cần SMask |
| Ảnh → PDF | A + B | `lopdf` | JPEG nhúng nguyên byte (DCTDecode); định dạng khác qua canvas ra JPEG/raw trước |
| PDF → ảnh | B | `pdfjs-dist` (đã có) | Chọn DPI, PNG hoặc JPEG |
| Đổi cỡ, đổi định dạng, nén ảnh | B | canvas | Giống `thumbnails.rs`: không thêm image crate. HEIC tuỳ WebView giải được hay không |
| Tách nền ảnh | C | Vision, `VNGenerateForegroundInstanceMaskRequest` | macOS 14+. `objc2` đã có; thêm `objc2-vision` |
| Word, Excel, PowerPoint → PDF | D | LibreOffice headless | Người dùng tự cài (~300 MB tải, gần 1 GB khi cài) |
| PDF → Word | D | pdf2docx | Chỉ PDF có chữ. PDF scan cần OCR, chưa làm |
| Nén PDF | D | pikepdf + Pillow | Nén lại ảnh trong PDF. Ghostscript cũng là AGPL, và nặng hơn |
| Khoá / mở khoá PDF | A hoặc D | `lopdf` hay pikepdf | Chưa kiểm `lopdf` 0.34 mã hoá được tới đâu |
| PDF → văn bản | A | `file_text.rs` (đã có) | Cùng đường đọc chữ mà Files dùng để index |
| Cắt ảnh, xoá thông tin ảnh | B | canvas, `shared/exif.ts` (đã có) | Vẽ lại qua canvas là bỏ hết EXIF; HEIC giữ hướng ảnh trước khi bỏ |
| Nhận dạng chữ (PDF scan) | C | Vision, `VNRecognizeTextRequest` | Chưa kiểm tiếng Việt được hỗ trợ từ macOS nào |
| Ký tên | A + B | `perfect-freehand` (đã có) + `stamp.rs` | Chữ ký vẽ ra ảnh rồi đóng như watermark ảnh. Chữ ký lưu lại thì để trong Safe? Chưa quyết |
| Video và âm thanh | D | ffmpeg trong phòng kín | Phòng kín đã chạy được ffmpeg (Phụ lục A của `syn-at-the-machine`) |

Tầng nào rẻ thì dùng tầng đó, theo thứ tự A → B → C → D. Tầng A chạy ở mọi nơi kể cả Android, không cần mạng, không
cần cài gì. Tầng D mạnh nhất nhưng chỉ có trên macOS 14+ và cần cài thêm.

### 3.1. Vì sao không làm PDF trong JavaScript (pdf-lib)

pdf-lib làm được gộp, tách, watermark có font tiếng Việt (qua fontkit). Nhưng:

- `lopdf` đã ở trong bản build, Android cũng có. Thêm pdf-lib là hai bộ PDF trong một app.
- File 200 MB phải đi qua IPC vào bộ nhớ của WebView rồi đi ra lại. Ở Rust, file đi từ đĩa vào đĩa.
- `run_utility` cho Syn (U8) chỉ gọi được code Rust.

### 3.2. Vì sao không tách nền bằng model trong WebView

RMBG-2.0 cho chất lượng tốt nhất nhưng chỉ được dùng phi thương mại, và bản ONNX fp16 nặng khoảng 490 MB. Chạy model
cỡ đó trong WebView cần WebGPU, mà `modern-web-guidance` không có hướng dẫn nào cho nó, và macOS 13 với Safari 16.4
không có. Vision của Apple có sẵn trong hệ điều hành, chạy trên Neural Engine, không tải gì. Khi cần máy khác, đường
thứ hai là `rembg` với model u2net trong phòng kín, không phải WebView.

---

## 4. Kiến trúc

### 4.1. Front end

```
src/mini-apps/utilities/
  UtilitiesApp.vue        lưới tool theo nhóm (PDF, Ảnh, Chuyển đổi), ô tìm, tool dùng gần đây
  tools.ts                registry: id, tên, icon, nhóm, loại file nhận, min/max file, component tuỳ chọn
  ToolRunner.vue          khung chung: thả file → danh sách (kéo để sắp) → tuỳ chọn → Chạy → kết quả
  options/                component tuỳ chọn riêng, lazy: PageGrid.vue (tách, xoá, xoay), WatermarkOptions.vue, …
  composables/useJob.ts   gọi lệnh, nghe sự kiện tiến độ, huỷ
```

Thêm một dòng vào `BUILT_IN_APPS`, icon `Wrench` của lucide. Route, More menu, khoá app đi theo registry như mọi app.

Registry ở TypeScript chỉ giữ phần giao diện. **Tool nào chạy được trên máy này thì Rust nói**, qua
`utility_catalog()`: mỗi tool một trạng thái `ready`, `needs_setup { what }` (chưa cài LibreOffice, chưa có môi trường)
hay `unavailable { why }` (Windows, macOS 13). Một test so id hai bên, giống cách `folderForType` được giữ khớp với
`syn/tools.rs`.

### 4.2. Watermark chữ tiếng Việt

Font chuẩn của PDF (Helvetica, Times) chỉ có bảng mã Latin-1, không có "ạ", "ữ". Muốn chữ thật thì phải nhúng và cắt
font TrueType, việc mà `lopdf` không làm. Cách rẻ hơn: canvas vẽ chữ bằng font của hệ thống ra ảnh RGBA, Rust nén bằng
`flate2`, nhúng làm ảnh có SMask, đặt lên mỗi trang với độ trong suốt và góc xoay đã chọn. Chữ watermark thành ảnh,
không chọn hay copy được. Với watermark thì đó là điều muốn có.

### 4.3. Rust

```
src-tauri/src/utilities/
  mod.rs        catalog, job, vùng tạm, dọn dẹp
  pdf.rs        gộp, tách, xoay, xoá, sắp trang, đánh số, nhúng ảnh
  stamp.rs      watermark (ảnh RGBA → XObject + SMask)
  vision.rs     #[cfg(target_os = "macos")] tách nền
  room.rs       #[cfg(target_os = "macos")] công thức chạy trong phòng kín
src-tauri/src/commands/utilities.rs
  utility_catalog, utility_run, utility_cancel, utility_save, utility_discard
```

**Job.** `utility_run(tool, inputs, options)` trả `job_id` ngay, chạy trên tokio, phát `utilities:progress
{ job_id, done, total, note }`. Huỷ được giữa chừng. Mỗi lúc chạy vài job, theo `MOST_AT_ONCE` như `machine::jobs`.

**Vùng tạm.** `app_cache_dir()/utilities/<job_id>/`. Không nằm trong vault, nên không bị sync mang đi. Xoá khi lưu
xong, khi bỏ, và khi mở app thì xoá các thư mục cũ hơn một ngày.

**Lưu.** `utility_save(job_id, dest)` với ba đích: cạnh file gốc trong vault (rồi index như `import_files`), vault
`assets/`, hay một đường dẫn từ hộp thoại lưu. Tên trùng thì thêm hậu tố, không bao giờ ghi đè, giống `import_files`.

**Đầu vào là file của người lạ.** Một PDF tải về có thể được làm ra để đánh sập trình đọc. Tầng A đọc bằng `lopdf`
(Rust, không có mã C), lỗi trả về chứ không panic. Tầng D chạy LibreOffice và pdf2docx trong phòng kín, không mạng, chỉ
thấy thư mục của job. Ở đây phòng kín có lợi theo hướng ngược với Syn: nó giữ một trình đọc tài liệu bị lừa khỏi máy
của người dùng.

### 4.4. Phòng kín cho Utilities

Phòng hiện nay gắn với hội thoại của Syn, và được thiết kế cho lệnh do model viết. Utilities cần một phòng không gắn
hội thoại, chạy **lệnh do app viết sẵn** với tham số đã kiểm, và không có gì phải đánh dấu "lời người ngoài" vì đầu
ra là file, không đi vào model. Cần:

- Một loại phòng `Rooms::utility()` dùng chung profile kín, không cửa thư mục, chỉ thư mục job được đọc ghi.
- Môi trường riêng cho Utilities trong `env.rs` (pdf2docx, pikepdf, Pillow), lockfile do app ghim, cài một lần khi
  người dùng bấm "Cài", qua phòng chuẩn bị như `machine_env`.
- LibreOffice: tìm ở `/Applications/LibreOffice.app` (`find.rs` đã tìm chương trình trên máy), chạy
  `soffice --headless --convert-to pdf` với `-env:UserInstallation` trỏ vào thư mục job.
- I8 vẫn áp dụng: job Utilities cũng bị dừng khi một phòng có mạng đang mở.

---

## 5. Giao diện

**Mockup bấm được:** [`utilities-ui-mockup-2026-11-11.html`](utilities-ui-mockup-2026-11-11.html). Mở thẳng trong trình duyệt, hoặc
`http://localhost:1420/docs/utilities-ui-mockup-2026-11-11.html` khi dev server đang chạy. Màu, chữ, nút lấy từ
`src/style.css` và `AppHeader.vue`, có cả chế độ tối (nút trăng ở dưới thanh app). Thanh "Mockup ▸" ở góc phải dưới
nhảy thẳng tới từng màn, nó không thuộc thiết kế.

### 5.1. Học gì từ ilovepdf và tinywow, bỏ gì

| Họ làm | Utilities làm | Vì sao |
|---|---|---|
| Trang chủ là lưới tool, mỗi tool một màu | Giữ lưới. Màu theo **nhóm** (PDF đỏ, Chuyển đổi xanh, Ảnh tím, Video cam), không theo tool | 28 màu là nhiễu; 4 màu giúp mắt tìm nhóm. Màu chỉ ở ô icon, phần còn lại vẫn đơn sắc như cả app |
| Chọn tool trước, rồi chọn file | Làm được cả hai chiều. **Thả file trước** thì app hỏi "Làm gì với 3 PDF này?" | Trên desktop, file thường đã nằm sẵn trong Finder. Người ta biết mình có gì trước khi biết tool tên gì |
| Một màn tải file, một màn tuỳ chọn, một màn tải về | **Một màn**, đổi trạng thái tại chỗ: trống → có file → đang chạy → xong | Không có upload, nên không cần màn chờ riêng. Sửa tuỳ chọn rồi chạy lại không phải chọn file lại |
| "Tiếp tục với tool khác" sau khi xong | Giữ, và **kết quả đi thẳng** sang tool kế tiếp, không lưu ra rồi chọn lại | Gộp → nén → khoá là chuỗi hay gặp |
| Quảng cáo, giới hạn file, đăng nhập, xoá file sau 1 giờ | Không có. Thay bằng một dòng: "Mọi việc chạy trên máy này" | Đây là lý do để có Utilities |
| Tool hiện ra dù không chạy được, bấm vào mới biết | Nhãn ngay trên thẻ: **Cần cài**, **Cần macOS 14** | Không để người dùng chọn file xong mới biết |

### 5.2. Bố cục

```
┌────┬──────────────┬───────────────────────────────────────────────────────────────┐
│rail│ Tiện ích     │ ‹ › 🔧 Tiện ích                                   [Chọn file…] │
│    │ [Tìm…     /] ├───────────────────────────────────────────────────────────────┤
│    │ ĐÃ GHIM      │ ┌ Thả file vào bất cứ đâu trong Tiện ích ─────────────────────┐ │
│    │  Gộp PDF     │ └──────────────────────────────────────────────────────────────┘ │
│    │  Nén PDF     │ ● PDF  Sắp xếp, chỉnh và bảo vệ                                 │
│    │ DÙNG GẦN ĐÂY │ [Gộp PDF ★] [Tách PDF] [Sắp xếp trang] [Xoay] [Nén · Cần cài] … │
│    │  Watermark   │ ● Chuyển đổi                                                    │
│    │ NHÓM         │ [Word → PDF · Cần cài] [Ảnh → PDF] [PDF → Word · Cần cài] …     │
│    │  Tất cả  28  │ ● Ảnh                                                           │
│    │  PDF     11  │ [Tách nền] [Nén ảnh] [Đổi cỡ] [Cắt] [Đổi định dạng] [Xoá EXIF]  │
│    │ CHƯA LƯU     │ ● Video và âm thanh                                             │
│    │  Hop-dong…   │ [Video → GIF · Cần macOS 14] …                                  │
└────┴──────────────┴───────────────────────────────────────────────────────────────┘
```

Thanh bên theo khuôn của Files và Things (rộng 256px, `surface-alt`, nhãn mục chữ hoa nhỏ). Header theo `AppHeader`:
tiêu đề bên trái, **một** nút chính có chữ ở cuối bên phải.

- **Ghim**: ngôi sao hiện khi rê chuột lên thẻ. Tool ghim lên đầu thanh bên. Mặc định ghim Gộp PDF, Nén PDF, Tách nền.
- **Dùng gần đây**: ba tool, không lặp tool đã ghim.
- **Kết quả chưa lưu**: kết quả còn trong vùng tạm, kèm giờ còn lại. Bấm vào thì mở lại màn kết quả. Đóng app không mất.
- **Tìm**: phím `/`, khớp tên, mô tả và id, bỏ dấu ("nen" ra "Nén PDF", "word" ra cả hai chiều). Gõ tìm khi đang ở trong
  một tool thì quay về lưới.

### 5.3. Một tool: một màn, bốn trạng thái

```
 ‹ › [icon] Gộp PDF · Ghép nhiều PDF thành một…            + Thêm file   [Gộp 3 file]
 ┌──────────────────────────────────────────────────────┬──────────────────────────┐
 │ 3 file · 20 trang · kéo để đổi thứ tự  [Sắp theo tên]│ TUỲ CHỌN                 │
 │ ┌1──┐ ┌2──┐ ┌3──┐ ┌ + ┐                              │ ☑ Giữ mục lục            │
 │ │pg │ │pg │ │pg │ │   │                              │ ☐ Mục lục theo tên file  │
 │ └───┘ └───┘ └───┘ └───┘                              │                          │
 │                                                      │ ĐẦU RA                   │
 │                                                      │ Tên file [...-gop.pdf]   │
 └──────────────────────────────────────────────────────┴──────────────────────────┘
```

1. **Trống.** Một vùng thả lớn với icon của tool, "Chọn file…" (hộp thoại hệ thống) và "Chọn từ Tệp" (vault). Ghi rõ
   nhận loại file nào, và "File gốc không bao giờ bị thay đổi". Chưa có cột tuỳ chọn: chưa có gì để chỉnh.
2. **Có file.** Trái là chỗ làm việc, phải là cột tuỳ chọn 316px, cuối cột là **Đầu ra** (tên file sẽ tạo). Nút chính
   trên header mang động từ và số lượng: "Gộp 3 file", "Tách nền 2 ảnh", "Đóng watermark". Nút mờ khi chưa đủ file
   (`min`), và "Thêm file" ẩn khi đã đủ `max`.
3. **Đang chạy.** Chỗ làm việc mờ đi, ở giữa là thẻ tiến độ có đơn vị thật ("Trang 9 / 20", "Ảnh 2 / 5") và nút Huỷ.
   Cột tuỳ chọn vẫn thấy nhưng không sửa được. Việc dưới một giây thì không hiện thẻ, để khỏi nháy.
4. **Xong.** Thay cả màn bằng kết quả: ảnh nhỏ, tên file sửa được tại chỗ, số trang và dung lượng (nén thì có
   "4,1 MB → 1,6 MB (−61%)"), dòng "Chưa lưu. Tự xoá sau 24 giờ". Hàng nút lưu (§5.4). Dưới cùng là **Dùng tiếp với
   kết quả này**. Nút chính trên header đổi thành "Sửa và chạy lại", quay về trạng thái 2 với file và tuỳ chọn cũ.

Chỗ làm việc đổi theo loại tool, khung thì không:

| Kiểu | Tool | Chỗ làm việc |
|---|---|---|
| Danh sách file | Gộp, Nén, Khoá, Ảnh → PDF, Nén ảnh, Đổi định dạng | Thẻ file kéo để sắp, số thứ tự khi thứ tự có nghĩa (gộp) |
| Lưới trang | Tách, Sắp xếp trang, Xoay, Xoá trang | Mọi trang của một PDF. Tách: đường cắt và màu theo nhóm file sẽ tạo; chế độ "Chọn trang" thì bấm để chọn, ⇧ chọn dải |
| Xem trước | Watermark, Đánh số trang, Ký tên | Trang 1 lớn, cập nhật khi kéo thanh trượt; cột phải là vài trang nhỏ |
| Trước / sau | Tách nền, Nén ảnh, Cắt ảnh | Kéo thanh giữa để so; nền ca-rô cho phần trong suốt |

### 5.4. Lưu: hai nút, tuỳ file đến từ đâu

| Đầu vào | Nút chính | Nút phụ |
|---|---|---|
| Từ vault (qua Files hay "Chọn từ Tệp") | **Lưu cạnh file gốc** | Lưu ra… |
| Từ ngoài (Finder, hộp thoại) | **Lưu…** (hộp thoại, nhớ thư mục lần trước) | Thêm vào vault |
| Nhiều file ra (tách, xử lý hàng loạt) | **Lưu tất cả vào thư mục…** | Lưu thành .zip |

Thêm "Mở" (bằng app mặc định của hệ thống) và "Chép" (file vào clipboard, để dán vào mail hay chat). Lưu xong hiện
toast "Đã lưu vào …" với nút "Hiện trong Finder". Không có lựa chọn ghi đè file gốc ở đâu cả.

### 5.5. Thả file trước

Kéo file vào bất cứ đâu trong Utilities (ở màn chính) thì cả cửa sổ viền xanh, "Thả để xem làm được gì". Thả xong mở
một hộp nhỏ:

```
 Làm gì với 3 PDF này?                                   ✕
 [Hop-dong….pdf] [Phu-luc-A.pdf] [Bien-ban….pdf]
 ▸ Gộp PDF              Ghép nhiều PDF thành một…        1
   Nén PDF   Cần cài    Giảm dung lượng…                 2
   Tách từng file       …                                3
   …
 ↑↓ để chọn · ↵ để mở · số để mở nhanh
```

Danh sách lọc theo loại và số file (một PDF thì không có Gộp), xếp theo việc người dùng hay làm với loại đó, rồi theo
thứ tự mặc định. Chọn một tool thì vào thẳng trạng thái "có file". File lẫn nhiều loại (PDF và ảnh) thì gợi ý theo loại
chiếm nhiều nhất, và nói rõ file nào bị bỏ ra.

Ba lối khác dẫn vào cùng hộp này: nút "Chọn file…" trên header màn chính, menu chuột phải **Tiện ích ▸** trong Files
(bỏ qua hộp, vào thẳng tool đã chọn), và sau này kéo file thả lên icon Utilities trên thanh app.

### 5.6. Tool chưa chạy được

- **Cần cài** (nhãn vàng): bấm vào thấy một thẻ giải thích thay cho vùng thả. LibreOffice thì app không tự cài: chỉ
  đường tải, nút "Kiểm tra lại". Gói Python thì có nút "Cài (60 MB)" với tiến độ, cài xong vào thẳng tool. Thẻ nói
  thêm tool nào khác cũng dùng chung thứ đó, và vì sao an toàn ("mở trong phòng kín, không có mạng").
- **Cần macOS 14** (nhãn xám, thẻ mờ): vẫn bấm được để đọc lý do. Không giấu, vì giấu thì người dùng nghĩ app không có.
- Nút chính trên header ẩn ở cả hai trạng thái.

### 5.7. Những điều nhỏ mà quan trọng

- **Tên file ra** theo khuôn `<tên gốc>-<việc>.<đuôi>`, chữ việc bằng tiếng Việt không dấu (`-gop`, `-nen`,
  `-tach-nen`), sửa được trước khi chạy và sau khi xong.
- **Phím**: `/` tìm, `Esc` đóng hộp hoặc về lưới, `⌘↵` chạy, `⌘S` lưu, `⌘O` chọn file.
- **Hàng loạt**: tool một-file-vào-một-file-ra (nén, xoay, tách nền) nhận nhiều file và chạy lần lượt; màn kết quả là
  danh sách, mỗi dòng một file, có dòng lỗi riêng nếu một file hỏng mà các file khác vẫn xong.
- **Lỗi** nói bằng lời của việc, không bằng lời của thư viện: "File này có mật khẩu. Dùng Mở khoá PDF trước", "Trang
  14 hỏng, đã bỏ qua", kèm nút dẫn tới tool sửa được nếu có.
- **Không có màn cài đặt riêng** cho Utilities. Gỡ gói đã cài nằm trong Cài đặt chung, mục Utilities.
- **Khoá app** hoạt động như mọi app (`protectedApps`). Utilities không đọc vault ngoài file người dùng đưa vào.

### 5.8. Danh mục theo đợt

Mockup vẽ đủ 28 tool để thấy lưới khi đầy. Bản thật chỉ hiện tool đã làm xong; không có thẻ "Sắp có".

| Đợt | Tool |
|---|---|
| U-1 | Gộp, Tách, Sắp xếp trang, Xoay, PDF → Ảnh, Ảnh → PDF |
| U-2 | Watermark, Đánh số trang, Khoá / Mở khoá, PDF → Văn bản, Nén ảnh, Đổi cỡ, Cắt, Đổi định dạng ảnh, Xoá thông tin ảnh |
| U-3 | Tách nền, Nhận dạng chữ (Vision) |
| U-4 | Word/Excel/PowerPoint → PDF, PDF → Word, Nén PDF |
| Sau | Ký tên, Video và âm thanh (ffmpeg trong phòng kín) |

---

## 6. Rủi ro

### 6.1. LibreOffice trong Seatbelt

Chưa ai chạy thử. LibreOffice đọc `~/Library`, font hệ thống, có thể cần dịch vụ hệ thống mà profile kín chặn. Nếu
không chạy được trong phòng thì có hai lối: nới profile riêng cho nó (phải qua bộ tự kiểm tra), hoặc bỏ U5 khỏi bản đầu.
Không chạy LibreOffice ngoài phòng với một file lạ.

### 6.2. Chất lượng PDF → Word

pdf2docx dựng lại bố cục từ toạ độ chữ. Văn bản đơn giản ra tốt; bảng phức tạp, nhiều cột, chữ trong ảnh thì không.
PDF scan ra file Word trống. Bản đầu phải nói rõ điều này trên màn tool, và phát hiện PDF không có chữ (`file_text.rs`
đã đọc được chữ của PDF) để báo trước thay vì trả một file rỗng.

### 6.3. Giấy phép

pdf2docx và PyMuPDF là AGPL hoặc giấy phép thương mại của Artifex. Synabit không chở chúng: máy người dùng tải về qua
`uv`, chạy như tiến trình riêng, không có dịch vụ mạng. Theo cách đọc thông thường thì AGPL không lan sang app trong
trường hợp này, nhưng đây không phải ý kiến pháp lý. Đọc lại trước khi phát hành, và ghi vào `THIRD_PARTY_LICENSES.md`.
RMBG-2.0 không dùng (phi thương mại).

### 6.4. File lớn

`lopdf` nạp cả file vào RAM. PDF scan 500 trang có thể vài trăm MB. Cần đo (§8), và đặt trần rõ ràng thay vì để app chết.

---

## 7. Các mốc

| Mốc | Nội dung | Cỡ |
|---|---|---|
| U-0 | Đo §8. Không viết code của app | S |
| U-1 | Khung: registry, `ToolRunner`, job, vùng tạm, lưu. Tool: gộp, tách, xoay/xoá/sắp trang, PDF → ảnh, ảnh → PDF | L |
| U-2 | Watermark chữ và ảnh, đánh số trang, khoá/mở khoá, PDF → văn bản, nén/đổi cỡ/cắt/đổi định dạng ảnh, xoá thông tin ảnh. Menu Tiện ích trong Files | M |
| U-3 | macOS: tách nền và nhận dạng chữ bằng Vision | S |
| U-4 | macOS 14+: phòng Utilities, môi trường, Office → PDF, PDF → Word, nén PDF | L |
| U-5 | `run_utility` cho Syn, chỉ tầng A, mọi nền tảng | M |

U-1 và U-2 không đụng `syn/machine/`, nên không chờ nhánh `syn-at-the-machine`. U-4 thì chờ.

**Trạng thái U-1, 2026-11-11 (chưa commit):** có code cho cả mốc. Rust: `src-tauri/src/utilities/` (`pdf.rs` một hàm
`assemble` cho gộp, tách, sắp xếp, xoay; `images.rs` JPEG → PDF không giải mã, đọc hướng EXIF; `workspace.rs` vùng tạm)
và `commands/utilities.rs`; ba lệnh lưu ra ngoài nằm trong `gate::CHOSEN_ARGS`. Front end: `src/mini-apps/utilities/`.

- **Đã kiểm:** 28 test Rust; 24 test front end; PDF thật (tạo bằng `cupsfilter`) gộp 3 file, 20 trang trong 16 ms, tách
  đôi có xoay, PDFKit của macOS mở được, chữ tiếng Việt còn nguyên; một JPEG thật thành PDF A4 đúng hướng. Giao diện chạy
  qua đủ sáu tool trên IPC giả lập, cả sáng lẫn tối.
- **Chưa kiểm:** chưa chạy trong app Tauri thật (hộp thoại Rust, asset protocol với thư mục cache, kéo thả file từ
  Finder, IPC gửi byte thô); PDF lớn (§8 mục 1, có sẵn test `real_files` để đo); PDF có mật khẩu (`lopdf` 0.34 chỉ giải
  RC4, AES sẽ báo "có mật khẩu"); HEIC trên macOS 13.
- **Khác thiết kế:** "Chọn từ Tệp" và menu chuột phải trong Files để sang U-2. Gộp chưa giữ bookmark. PDF → Ảnh vẽ bằng
  pdf.js trong WebView, khoảng 0,6 giây một trang ở 150 DPI trong trình duyệt thử.

**Trạng thái U-2, 2026-11-11 (chưa commit):** mười tool mới — Watermark, Đánh số trang, Khoá PDF, Mở khoá PDF,
PDF → Văn bản, Nén ảnh, Đổi cỡ ảnh, Cắt ảnh, Đổi định dạng ảnh, Xoá thông tin ảnh — và mục "Tiện ích" trong menu chuột
phải của Files, đưa file vault sang với nút "Lưu cạnh file gốc". Rust mới: `stamp.rs` (watermark là ảnh có mặt nạ,
số trang là chữ Helvetica, đều theo hướng trang đang hiển thị), `crypt.rs` (mở RC4, AES-128, AES-256; khoá bằng AES-256
R6; gắn vào lúc `lopdf` nạp file nên đọc được cả PDF mã hoá có object stream), `metadata.rs` (bỏ EXIF/XMP/IPTC khỏi
JPEG và PNG không mã hoá lại, giữ hướng ảnh).

- **Đã kiểm:** 46 test Rust, 31 test front end. PDFKit của macOS: file PDFKit khoá (AES-128, R4) mình mở được; file
  mình khoá (AES-256, R6) PDFKit mở bằng đúng mật khẩu, từ chối mật khẩu sai. Watermark và số trang trên PDF thật, kể cả
  trang xoay 90°, PDFKit vẽ đúng chỗ. Giao diện chạy qua cả mười tool trên IPC giả lập.
- **Phát hiện:** Preview (PDFKit) không nhận mật khẩu có dấu cho AES-256, dù chuẩn cho phép và pdf.js mở được. Màn Khoá
  PDF cảnh báo khi mật khẩu có ký tự ngoài ASCII. `lopdf::extract_text` trả rỗng với PDF tạo bằng `cupsfilter`, nên
  PDF → Văn bản dùng pdf.js; tìm kiếm toàn văn của Files (`file_text.rs`) dùng cùng hàm đó, có thể cũng bỏ sót những
  PDF này — chưa kiểm trên file thật khác.
- **Chưa kiểm:** như U-1, chưa chạy trong app Tauri thật; WebP chỉ được ghi khi WebView ghi được (Safari 26 trở lên).
  "Chọn từ Tệp" trong vùng thả vẫn chưa có; lối vào từ vault là menu chuột phải của Files.

**Trạng thái U-3, 2026-11-11 (chưa commit):** ba tool dùng Vision của macOS — Tách nền (macOS 14+), Nhận dạng chữ
(PDF scan thành PDF tìm được chữ, và/hoặc .txt), Lấy chữ từ ảnh. Rust: `vision.rs` gọi Vision qua `msg_send!` của
`objc2` và vài hàm C của CoreGraphics/CoreVideo, **không thêm crate**; `searchable.rs` đặt chữ vô hình (Type0,
`Identity-H`, `ToUnicode` U+n) lên trang theo đúng hướng trang. Máy không có Vision thấy thẻ mờ với nhãn "Chỉ macOS" /
"Cần macOS 14" và lời giải thích; các tool đó không được gợi ý khi thả file hay trong menu Files.

- **Đã đo trên máy này (macOS 26.5):** Vision có `vi-VT`. Một trang 1700×2200 đọc trong khoảng 0,2 giây, chữ tiếng Việt
  có dấu ra đúng hết. Tách nền ảnh 1024×1024: mặt nạ sạch, phủ 50% ảnh. PDF tìm được chữ: PDFKit và pdf.js đều đọc ra
  chữ, và `findString` của PDFKit khoanh đúng chỗ chữ trên ảnh scan.
- **Đã kiểm:** 53 test Rust, 25 test front end của Utilities. Giao diện: xem trước trước/sau bằng thanh kéo, đổi nền,
  chạy hàng loạt bỏ qua ảnh không có chủ thể và báo lại; Nhận dạng chữ trên 15 trang (IPC giả lập, mặt nạ thật).
- **Chưa kiểm:** macOS 13 và 14 thật (chỉ dựa vào việc lớp `VNGenerateForegroundInstanceMaskRequest` có hay không);
  ảnh chụp người thật nhiều chủ thể; bản scan xấu (nghiêng, mờ); PDF scan rất nhiều trang (mỗi trang gửi khoảng 15 MB
  điểm ảnh sang Rust). Vẫn chưa chạy trong app Tauri thật.

**Trạng thái U-4, 2026-11-12 (chưa commit):** ba tool — Nén PDF, Office → PDF, PDF → Word.

- **Nén PDF khác thiết kế:** không dùng pikepdf trong phòng. Rust (`compress.rs`) lấy ảnh trong PDF ra, WebView giải mã
  và nén lại bằng canvas theo mức (200/150/100 dpi ở cỡ trang, JPEG 0,85/0,7/0,5), Rust đặt ảnh nhỏ hơn vào chỗ cũ.
  Chạy mọi nền tảng, không cài gì. File không nhẹ hơn thì báo "đã gọn" thay vì trả file to hơn.
- **Office → PDF:** LibreOffice (người dùng tự cài) chạy trong phòng kín của `syn/machine`. Đo trên macOS 26.5 bằng cách
  bỏ từng quyền: ngoài đọc và chạy gói `LibreOffice.app` (`Profile::programs`), nó chỉ cần ghi `/private/tmp` và các
  file `OSL_PIPE_*` (ống một-phiên-bản của nó; thư mục tạm của phòng quá dài cho socket), tạo socket Unix ở đúng đường
  dẫn đó, và tra `com.apple.coreservices.launchservicesd`. Không mạng, không dịch vụ nào khác (`Profile::office`). Phòng
  của Syn không có các quyền này. Word 2 giây, Excel 1,3 giây; chữ tiếng Việt đúng.
- **PDF → Word:** pdf2docx cài một lần qua phòng chỉ nối kho gói Python (`machine::env`), 19 giây trên mạng này; mỗi lần
  chuyển chạy trong phòng kín, PDF 3 trang mất 20 giây; chữ tiếng Việt đúng.
- **Đã kiểm:** 58 test Rust của Utilities, 119 test của `syn/machine` (cả profile và tự kiểm), 41 test front end; PDF
  ảnh 2,37 MB nén còn 216 KB và PDFKit vẽ đúng; trên giao diện, ảnh 4000×4000 được WebView nén thật từ 2,37 MB còn 180 KB;
  thẻ "Cần cài" và nút Cài pdf2docx.
- **Chưa kiểm:** PowerPoint thật (chỉ thử Word và Excel); file Office có font lạ; macOS 14 và 15 thật với quyền
  LibreOffice đã đo trên 26.5; một ống `OSL_PIPE` bỏ lại nếu LibreOffice bị giết giữa chừng (mỗi lần chuyển dùng phòng
  tên riêng nên không vướng nhau, nhưng file ống cũ không được dọn); vẫn chưa chạy trong app Tauri thật.

**Audit trước U-5, 2026-11-12 (chưa commit):** đọc lại toàn bộ U-1…U-4 (ba lượt đọc độc lập: lõi PDF/ảnh, lệnh Rust và
phòng, front end) rồi sửa.

- **Lõi:** PDF mã hoá — độ dài khoá RC4 lấy theo `/Length`, kiểm khoá AES, stream nằm trong ObjStm được giải mã sau khi
  nạp; nhận dạng chữ xếp dòng theo hàng (bộ so sánh cũ không có thứ tự toàn phần, `sort_by` có thể panic); trang xoay
  đọc `/Rotate` thừa kế; trang lặp lại khi sắp xếp bỏ `/Annots` dùng chung; giới hạn số điểm ảnh khi đóng dấu và khi
  giải nén ảnh; xoá EXIF xử lý đúng hướng ảnh, MPF, PNG độ dài hỏng; JPEG nén lại không mang EXIF cũ; tên file ≤ 200 byte.
- **Lệnh Rust:** một lần chạy hỏng hay bị huỷ luôn dọn thư mục của nó (`Running` + `Drop`); cờ dừng đăng ký từ lúc tạo
  job nên huỷ sớm không bị mất; tiến độ theo từng file không nhảy lùi; `utility_open` chỉ mở các đuôi tool tạo ra
  (output có thể do trang ghi, mở `.command` là chạy nó); thư mục chọn qua hộp thoại gắn với lệnh đã xin nó
  (`take_for`); lưu .zip kiểm hết output trước khi ghi.
- **Phòng và Vision:** quote đường dẫn khi gọi LibreOffice (đường dẫn có dấu `'`), `UserInstallation` là URL chuẩn
  (đường dẫn có dấu cách); luật ghi `OSL_PIPE_*` khớp đúng tên ống, không phải tiền tố; trả `CVPixelBuffer` của mặt nạ.
- **Front end:** rời một tool (tool khác, trang chủ, mở kết quả ở thanh bên, mở từ Files) **huỷ** lần chạy đang dở —
  trước đây nó chạy tiếp ngầm, đọc tuỳ chọn của tool mới và ghi vào thanh tiến độ của màn mới; kết quả về sau khi đã
  huỷ bị bỏ. App được keep-alive nên watcher route chỉ nghe route của mình (trước đây sang app khác là đóng tool) và
  phím tắt `/`, ⌘↩, Esc chỉ bắt khi app đang hiện. Khi đang chạy: không nhận file thả vào, file và tuỳ chọn `inert`
  (không với tới bằng bàn phím), lớp "đang chạy" nằm ngoài vùng cuộn nên luôn thấy. File kéo từ Finder thả lên thẻ file
  hay thẻ trang không bị nuốt mất. Đọc file xong muộn không còn reset tuỳ chọn của tool sau. Ô số để trống không cho
  chạy. Ảnh thu nhỏ vẽ tuần tự (pdf.js không cho hai lần vẽ trên một canvas). Mở PDF hỏng thì huỷ task pdf.js; PDF kết
  quả được đóng khi rời màn kết quả. "Nhỏ hơn x%" của Nén PDF chỉ tính file thật sự nén được. Xem trước tách nền không
  giữ ảnh của tấm trước khi tấm này không có chủ thể. Xem trước watermark ẩn dấu khi chọn "trừ trang đầu", logo hỏng
  không làm vỡ. Mật khẩu xoá khi rời tool. Cắt ảnh: ⌥ + mũi tên đổi cỡ, nút "File khác"; Sắp xếp trang: ⌥← ⌥→.
- **Đã kiểm:** 64 test Rust của Utilities, 119 của `syn/machine`, 23 của `app_shell`, 32 test front end (thêm
  `workbench.spec.ts` cho vòng đời lần chạy); `real_office` qua đường dẫn có dấu cách. vue-tsc, eslint sạch; clippy
  không cảnh báo nào trong file của Utilities.
- **Để lại, có lý do:**
  - pdf.js 6 bản chuẩn dùng `Promise.withResolvers` (Safari 17.4) và `Promise.try` (Safari 18.2), dưới mức sàn 16.4 —
    chung cho cả Files, cần một việc riêng.
  - Quyền fs-plugin cho trang ghi `$APPCACHE`/`$APPDATA` (job.json, môi trường pdf2docx) có từ trước.
  - Font chữ vô hình không nhúng (Acrobat có thể cảnh báo).
  - `pdf_picture` đọc lại PDF cho mỗi ảnh.
  - HEIC không đọc được trên macOS 13; file `content://` của Android bị bỏ qua.
  - Không huỷ được giữa chừng một lần chạy trong phòng (LibreOffice, pdf2docx); bộ tự kiểm của phòng chưa phủ profile
    `programs`/`office`; lookup `launchservicesd` chưa được thử xem có mở được app khác không.

---

## 8. Bước 0: cần đo trước

1. Gộp 10 PDF tổng 300 MB và tách một PDF 500 trang bằng `lopdf`: thời gian, RAM đỉnh, file ra mở được trong Preview,
   Acrobat, pdf.js.
2. Watermark qua canvas: chữ tiếng Việt có dấu, xoay 45°, độ trong 30%, trên PDF có trang xoay sẵn và MediaBox lệch gốc.
3. Vision tách nền trên 20 ảnh: người, đồ vật, ảnh chụp màn hình, ảnh nhiều vật. Đo thời gian trên M1 và máy Intel nếu có.
4. LibreOffice trong phòng kín (§6.1): có chạy không, font tiếng Việt có ra đúng không, mất bao lâu cho một docx 20 trang.
5. pdf2docx trên 10 PDF tiếng Việt có thật: hoá đơn, hợp đồng, bài báo hai cột, một PDF scan.
6. `lopdf` 0.34 mở và ghi PDF có mật khẩu được tới đâu.
