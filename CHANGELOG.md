# Synabit Changelog

## [Unreleased] - Syn ngồi vào máy (macOS)

Tất cả đều **tắt cho tới khi bạn bật**: lần đầu Syn cần một việc, một thẻ hỏi; "Không bao giờ" trong màn quyền tắt hẳn việc đó. Phòng kín chỉ chạy trên macOS 14 trở lên. App tự tấn công phòng lần đầu cần tới nó, và làm lại mỗi khi app hay macOS đổi phiên bản; một phép thử lọt là phòng bị tắt và màn "Máy" nói rõ.

### Added
- **Phòng kín (M1, M2):** Syn chạy được chương trình thật trên Mac (python, ffmpeg, sqlite3…) trong một phòng không có mạng, không thấy file nào ngoài phòng. Đưa note hoặc file vào phòng, giữ kết quả lại trong vault. Dùng được từ Telegram và từ routine, nếu bạn đã cho phép luôn.
- **Xem máy (M0.5):** tìm file trên Mac theo tên, xem pin, đĩa, bộ nhớ.
- **Gói Python cho phòng (M3):** Syn xin một danh sách gói, bạn duyệt trên thẻ mỗi lần. App cài từ PyPI ngoài phòng; phòng vẫn không có mạng.
- **Cửa thư mục (M4):** cho phòng đọc một thư mục trên Mac, do bạn tự chọn trong hộp thoại; phòng không bao giờ sửa được nó. **Cái giá:** những gì trong thư mục đó có thể được gửi tới nhà cung cấp model khi Syn đọc. Mặc định chỉ dùng từ app; dùng từ Telegram và routine là công tắc riêng của từng thư mục.
- **Diễn tập rồi áp dụng (M5):** muốn dọn, đổi tên, sắp xếp một thư mục thì Syn làm trên bản sao, bạn xem từng thay đổi trên thẻ rồi chọn cái nào áp dụng. Không gì bị xoá hẳn: file bỏ đi vào Thùng rác, và mỗi đợt hoàn tác được trong 30 ngày.
- **Người phụ việc (M6):** để tải dữ liệu công khai, Syn gửi một phụ việc lên mạng chỉ với một câu mục tiêu, hiện trên thẻ. Phụ việc không có trí nhớ, note hay vault của bạn. **Cái giá:** câu mục tiêu đó đi ra ngoài.
- **Việc dài (M7):** nén video dài hay chuyển đổi lớn chạy tiếp sau khi Syn trả lời. Bạn được báo khi xong, và dừng được từ màn "Máy". Việc của routine chạy ở mức ưu tiên nền, chỉ khi máy cắm sạc và không bận.
- **Mở và chỉ chỗ (M9):** Syn mở một tài liệu bằng app mặc định, hoặc chỉ một file trong Finder. Chỉ khi bạn ngồi ở máy. Không bao giờ mở app, script hay địa chỉ web.
- **Routine đọc được connector** mà bạn đã cho phép luôn. Ví dụ: tóm tắt bài mới trên PubMed mỗi sáng.
- **Dừng tất cả:** trong khay hệ thống và màn "Máy". Thoát app cũng dừng mọi phòng.

### Changed
- Quyền chạy một lệnh ngoài phòng giờ chỉ có hiệu lực cho đúng một lần chạy, và chỉ trao được trong hộp thoại của hệ thống. Chưa có tool nào dùng nó.
- Connector đọc được chữ của một phần tử XML có thuộc tính, như `<PMID Version="1">`.

### Chưa có
- Chạy lệnh ngoài phòng (`machine_host`), Shortcuts, và cài phần mềm vào máy (bậc 5).
- Xem lại ảnh hay slide đã dựng (`machine_look`), vì còn chờ `look_at_file`.
- Linux và Windows không có phòng; Android giữ nguyên.

## [0.0.3] - 2026-04-12
### Added
- **Nexus Full-Page UI:** Refactored Nexus layout thay vì dùng slide-over chuyển sang dạng màn hình toàn cảnh focus 100%, có hỗ trợ hiển thị đẹp mắt thông qua Tailwind Typography và tích hợp bộ phân giải DOMPurify/Marked an toàn tĩnh.
- **Nexus File Tracker:** Hệ thống Nexus giờ đây quét cả những file đính kèm/import ngoài lề để trả về kết quả tra cứu.
- **Advanced Rust Search Engine:** Tái cấu trúc cơ chế tra cứu từ `.contains` sang hệ Ranking Score và AND-terms (Tìm chia tách cụm từ). Cho phép đánh trọng số xếp hạng và dùng lệnh Lọc nhanh như: `is:note`, `is:task`.
- **Edit Gateway:** Bắt cầu trực tiếp qua cổng Event `@edit-item` từ kết quả tra cứu Nexus chuyển dời thẳng trạng thái đến App Mini tương ứng để sửa thông tin.
- Cải thiện lỗi Race Condition search chậm (ID Tracking) trong ô Input của Nexus.


## [0.0.2] - 2026-04-10
### Added
- **Nexus Command Center:** Ra mắt giao diện Dashboard Nexus - Cỗ máy tìm kiếm tổng hợp toàn mảng dữ liệu.
- **Omni-Search:** Áp dụng hệ thống Index Rust đa luồng, hỗ trợ search theo thời gian thực (Full-text và #Tags) cho Notes, Tasks và QuickCaps.
- **Slide-over Preview Panel:** Trải nghiệm xem trước tài liệu/note ngay trong Nexus thông qua bảng trượt cạnh phải, giữ nguyên thao tác người dùng không bị phân tâm.
- **Vault Stats Dashboard:** Thống kê định lượng dữ liệu và phân bố toàn bộ Tag đang có trong hệ thống bằng Tag Cloud.
- **Tasks App Enhancements:** Thêm schema động cho Task (Custom properties hỗ trợ yaml), UI Kanban chi tiết.
- **Filters nâng cao:** Hỗ trợ tính năng lọc Task theo [Today, This Week, Overdue,...].
- Cải thiện render Markdown: Sửa lỗi hiển thị HTML rỗng và ảnh cục bộ `asset://` trong màn hình preview.

### Changed
- Khung tìm kiếm QuickCap và Notes được tối ưu tốc độ từ Frontend xuống Backend.
- Viền CSS Focus trong các Input được làm mịn.
