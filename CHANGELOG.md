# Synabit Changelog

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
