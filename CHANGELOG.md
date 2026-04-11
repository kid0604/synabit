# Synabit Changelog

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
