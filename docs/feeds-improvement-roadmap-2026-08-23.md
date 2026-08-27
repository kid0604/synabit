# Lộ trình cải tiến mini-app Feeds

*Lập ngày 2026-08-23. Dựa trên khảo sát toàn bộ `src-tauri/src/feed_engine/`, `src-tauri/src/commands/feeds.rs`, `src/mini-apps/feeds/` và đường sync của vault.*

## Bối cảnh

Engine đúng chuẩn (conditional GET, `feed-rs`, `ammonia`, FTS5, readability, scrape fallback) nhưng lớp trên có sáu tính năng gãy hoàn toàn, ảnh không bao giờ render được vì CSP, và toàn module chỉ có ba unit test — tất cả nằm trong `sanitizer.rs`. Mọi lỗi nghiêm trọng liệt kê dưới đây đều sẽ bị bắt bởi một test mười dòng, nên phần kiểm thử không phải hạng mục phụ mà là điều kiện để lộ trình này không lặp lại.

## Nguyên tắc sắp xếp

1. **Mất dữ liệu trước, tính năng sau.** Bất cứ thứ gì có thể xoá trạng thái người dùng hoặc làm hỏng file trong vault được ưu tiên tuyệt đối.
2. **Sửa cái đang nói dối trước cái đang thiếu.** Một nút báo "thành công" mà không làm gì tệ hơn một nút không tồn tại.
3. **Không xây tính năng mới trên nền chưa có test.** Mỗi giai đoạn kết thúc bằng bộ test của chính nó.
4. **Đừng chống lại kiến trúc sync.** Lớp sync merge văn bản theo ký tự; dữ liệu đổi liên tục không được nằm trong file sync. Đây là ràng buộc thiết kế, không phải thứ để lách.

## Tổng quan

| GĐ | Tên | Mục tiêu | Ước lượng |
| --- | --- | --- | --- |
| P0 | Chặn máu | Hết mất dữ liệu, hết tính năng gãy | 2–3 ngày |
| P1 | Đọc được thật sự | Dùng hằng ngày không ức chế | 6–8 ngày |
| P2 | Kiến trúc dữ liệu & sync | Đa thiết bị đúng nghĩa, không hỏng vault | 10–14 ngày |
| P3 | Ngang hàng thị trường | Đủ sức so với NetNewsWire/Miniflux | 8–10 ngày |
| P4 | Khác biệt hoá | Thứ reader độc lập không làm được | mở |

---

## P0 — Chặn máu (2–3 ngày) — ✅ đã xong 2026-08-23

Bảy hạng mục, diff đều nhỏ. Hết giai đoạn này Feeds đi từ *không dùng được nghiêm túc* sang *dùng được*.

Kèm 19 test mới: 18 trong `commands/feeds.rs`, 1 trong `search.rs`.

### P0.1 — Mở khoá ảnh trong CSP

`img-src` tại [tauri.conf.json:23](../src-tauri/tauri.conf.json) không có `https:`, nên mọi thumbnail và mọi `<img>` trong bài đều bị chặn; handler `@error` ẩn ảnh đi nên hỏng trong im lặng. Ba layout list được thiết kế xoay quanh thumbnail mà thumbnail không bao giờ hiển thị.

**Làm:** thêm `https:` vào riêng `img-src`, không đụng `connect-src`.

**Đánh đổi:** mở `https:` cũng mở đường cho tracking pixel. Chấp nhận tạm ở P0, giải quyết đúng ở P3.6 bằng proxy ảnh.

**Nghiệm thu:** thumbnail hiện trong cả ba layout; ảnh trong thân bài hiện.

### P0.2 — "Đánh dấu đã đọc" không được xoá sạch unread

`feed_mark_all_read` ([feeds.rs:604](../src-tauri/src/commands/feeds.rs)) chỉ nhận `source_id`. Khi người dùng đang chọn một category thì `sourceId` là `undefined`, lệnh chạy `UPDATE feed_articles SET is_read = 1` không điều kiện — mất trạng thái toàn bộ, không undo.

**Làm:** đổi chữ ký thành `source_ids: Option<Vec<String>>`; frontend tự map category → danh sách source. Gắn `UndoToast.vue` (đã dùng ở Note và Task) với snapshot id các bài vừa đổi để hoàn tác được trong 10 giây.

**Nghiệm thu:** đánh dấu category chỉ ảnh hưởng feed trong category đó; có nút hoàn tác.

### P0.3 — OPML import hoạt động thật

[ImportExportModal.vue:30](../src/mini-apps/feeds/components/ImportExportModal.vue) truyền **đường dẫn file** vào tham số `opmlContent`; backend cố parse đường dẫn như XML. Kể cả parse được thì [feeds.rs:1115](../src-tauri/src/commands/feeds.rs) cũng chỉ *trả về* danh sách chứ không ghi vào `sources.json`, còn frontend vứt luôn giá trị trả về rồi hiện `count: 0` kèm thông báo thành công.

**Làm:**
- Frontend đọc nội dung bằng `readTextFile` (`@tauri-apps/plugin-fs` đã được import sẵn trong file này cho `writeTextFile`).
- Backend persist: tạo category còn thiếu theo tên, khử trùng lặp theo URL đã có, append vào `sources.json`, trả về `{ added, skipped }`.
- **Không** gọi discovery cho từng feed lúc import — OPML đã có `xmlUrl` và `title`; một file 200 feed mà discovery tuần tự sẽ treo nhiều phút. Metadata còn thiếu để lần refresh đầu điền.

**Nghiệm thu:** import file OPML xuất từ Feedly/Inoreader/NetNewsWire, số feed hiện đúng, category giữ nguyên cây, import lần hai không nhân đôi.

### P0.4 — Lọc theo category

[feeds.rs:468](../src-tauri/src/commands/feeds.rs) có comment "Category filtering is handled by the frontend" nhưng frontend cũng không lọc. Bấm một category hiện toàn bộ bài của mọi feed.

**Làm:** bỏ `category_id` khỏi `ArticleFilter`, thay bằng `source_ids: Option<Vec<String>>`. Frontend map category → source ids. Cách này cũng gỡ được phụ thuộc `vault_path` khỏi một lệnh thuần DB.

**Nghiệm thu:** chọn category chỉ hiện bài của feed trong đó; số badge khớp số bài.

### P0.5 — View "Chưa đọc" và các badge đếm đúng

Type có `'unread'`, backend xử lý `is_read = 0`, i18n có key `feeds.unread` — nhưng `smartViews` ([FeedsSidebar.vue:49](../src/mini-apps/feeds/components/FeedsSidebar.vue)) không liệt kê. Không có cách nào lọc chưa đọc, tức là thiếu chức năng cốt lõi nhất của một RSS reader. Badge "Today" thì hiển thị tổng unread toàn cục, Starred và Read-later hardcode `0`.

**Làm:** thêm mục Unread vào sidebar; thêm lệnh `feed_get_view_counts` trả `{ today, unread, starred, readLater }` trong một truy vấn, thay ba lệnh đếm rời rạc hiện tại.

**Nghiệm thu:** bốn badge phản ánh đúng số bài của chính view đó.

### P0.6 — FTS5 đúng đắn

Không có trigger; index nạp thủ công lúc insert. Khi xoá feed hoặc cleanup xoá bài, dòng FTS mồ côi còn lại tới lần rebuild kế tiếp — mà SQLite tái sử dụng rowid, nên search trong khoảng đó có thể trả về **bài sai**. Ngoài ra query người dùng được truyền thẳng vào `MATCH`: gõ `"`, `-`, `*` hay chữ `AND` là ném syntax error, search im lặng không đổi kết quả.

**Làm:**
- Ba trigger `AFTER INSERT / UPDATE / DELETE ON feed_articles` theo đúng mẫu external-content của SQLite; tăng `FEEDS_FTS_SCHEMA_VERSION` để rebuild một lần.
- Xoá các câu insert FTS thủ công ở `insert_articles`, `scrape_refresh`, `feed_fetch_article_content` và bước rebuild trong `cleanup.rs` — sau khi có trigger chúng vừa thừa vừa là nguồn lệch.
- Hàm `build_fts_query`: tách token theo khoảng trắng, bỏ ký tự `"`, bọc từng token trong ngoặc kép, nối bằng khoảng trắng, thêm `*` cho token cuối để có prefix search.

**Nghiệm thu:** xoá một feed rồi search không còn trả về bài của feed đó; gõ `foo "bar` không văng lỗi.

### P0.7 — Lỗi phải nhìn thấy được

`RefreshResult.errors` được backend trả về đầy đủ rồi bị vứt ở [FeedsApp.vue:146](../src/mini-apps/feeds/FeedsApp.vue). Mọi lỗi khác đi vào `logger.error`. Refresh hỏng, thêm feed lỗi, search crash — người dùng chỉ thấy màn hình không đổi.

**Làm:** một composable `useFeedToast` tối giản (hoặc tái dùng khuôn `SyncConflictToast.vue`) hiển thị "N feed lỗi" kèm nút xem chi tiết.

**Nghiệm thu:** thêm một URL rác, người dùng thấy thông báo lỗi thay vì im lặng.

### Test đóng P0

- `commands::feeds`: mark-all-read có phạm vi, filter theo source_ids, OPML round-trip (import file export ra được chính nó).
- `db::schema`: xoá bài thì FTS sạch theo.
- `feed_engine::search`: `build_fts_query` với chuỗi độc hại.

---

## P1 — Đọc được thật sự (6–8 ngày) — ✅ đã xong 2026-08-23

Kèm 9 test mới: 3 cho `sanitizer`, 2 cho múi giờ, 4 cho backoff.

### P1.1 — Phân trang

`limit: 50, offset: 0` cứng tại [FeedsApp.vue:106](../src/mini-apps/feeds/FeedsApp.vue), không có infinite scroll. Bài thứ 51 trở đi chỉ tìm được bằng search.

**Làm:** `IntersectionObserver` ở cuối danh sách, nạp thêm theo `offset`. Chưa cần virtualization ở bước này (xem P3.1).

### P1.2 — Tuyệt đối hoá URL tương đối

Cả `parser.rs` lẫn `readability.rs` đều không resolve `<a href="/x">` và `<img src="/x.png">`; chỉ mỗi thumbnail meta được xử lý. Với site dùng đường dẫn tương đối — rất phổ biến — link bấm không ăn, ảnh gãy.

**Làm:** `sanitize_html` nhận thêm `base_url` và dùng `Builder::url_relative(UrlRelative::RewriteWithBase(Url::parse(base)?))`. `ammonia = "4"` và `url = "2"` đều đã là dependency. Một chỗ sửa, cả hai đường gọi cùng hết lỗi. Nhân tiện thay `resolve_url_simple` / `extract_base_url` viết tay bằng `Url::join` — bản viết tay không xử lý `./`, `../`, hay `<base>` tag.

### P1.3 — Tải toàn văn cho feed rút gọn

`feed_fetch_article_content` hiện chỉ chạy khi `contentType === 'scrape'`. Feed RSS chỉ trả tóm tắt thì không có cách nào mở rộng — trong khi NetNewsWire, Reeder, Miniflux, Inoreader đều có.

**Làm:** thêm cờ `fullTextFetch: bool` cho `FeedSource`; bỏ điều kiện `scrape`, cho phép gọi với bất kỳ bài nào content rỗng hoặc ngắn hơn ngưỡng; thêm nút "Tải toàn văn" trong `ReaderToolbar.vue` cho trường hợp thủ công.

### P1.4 — Hiển thị sức khoẻ feed

`lastError` được lưu nhưng không hiển thị ở đâu. Feed chết ba tuần trông y hệt feed khoẻ.

**Làm:** badge lỗi trong `FeedSourceItem.vue` với tooltip là thông điệp lỗi; thêm `error_count` và backoff luỹ thừa (5 phút → 6 giờ) để feed hỏng không bị gọi lại đúng nhịp mãi mãi.

### P1.5 — Màn hình cài đặt, và một nguồn sự thật cho mặc định

`saveConfig` không được gọi ở đâu; không có màn Settings. `showReadArticles`, `markReadOnScroll`, `globalUpdateInterval` khai báo nhưng không dùng. `defaultView` vô nghĩa vì `viewMode` khởi tạo từ `config` *trước khi* config được load ([FeedsApp.vue:47](../src/mini-apps/feeds/FeedsApp.vue)), và chế độ view người dùng chọn không được lưu. Mặc định hai đầu còn lệch nhau: Rust dùng `all`/30 ngày/500 bài/30 phút, TS dùng `magazine`/14 ngày/200 bài/60 phút — mà `"all"` thậm chí không nằm trong union type của `defaultView`.

**Làm:** một `FeedsSettingsModal.vue` theo khuôn `SettingsModal` của Finance và Messages; `watch` trên `viewMode` để persist; sửa `FeedConfig::default()` phía Rust thành nguồn sự thật duy nhất và cho TS khớp theo; xoá các trường không dùng hoặc cài đặt chúng cho tử tế.

### P1.6 — "Hôm nay" theo giờ địa phương

So sánh chuỗi `published_at >= "2026-08-23"` tại [feeds.rs:441](../src-tauri/src/commands/feeds.rs). Người dùng UTC+7 thấy "Hôm nay" lệch bảy tiếng, và bài có offset `+07:00` bị so sánh lexicographic sai ngữ nghĩa. App có locale `vi` nên đây không phải trường hợp hiếm.

**Làm:** frontend tính mốc nửa đêm địa phương, gửi xuống dạng instant UTC; backend so sánh instant chứ không so chuỗi ngày.

### P1.7 — Dọn kiểu dữ liệu

`FeedSource.feedType` phía TS khai `'rss'|'atom'|'json'|'youtube'|'reddit'` nhưng backend sinh `"scrape"` và `"unknown"`. `contentType` TS khai `'article'|'video'|'reddit_post'` còn backend ghi `"text/html"` / `"scrape"`. Union type hiện là trang trí.

**Làm:** cho union khớp thực tế backend, thêm `'scrape' | 'unknown'`, bỏ `youtube`/`reddit` cho tới khi P4.4 làm thật.

### Test đóng P1

- Golden-file cho `parser.rs`: RSS 2.0, Atom, JSON Feed, CDATA, encoding không phải UTF-8, entry thiếu `id`, entry thiếu ngày.
- `sanitize_html` với `base_url`: `/x.png`, `../x.png`, `//cdn/x.png`, URL tuyệt đối giữ nguyên.
- Backoff: feed lỗi n lần thì `next_retry_at` giãn đúng.

---

## P2 — Kiến trúc dữ liệu & sync (10–14 ngày) — ✅ đã xong 2026-08-23

Kèm 16 test mới: 7 cho việc tách state khỏi vault và lịch fetch, 9 cho đồng bộ
trạng thái đọc (`feed_engine/state_sync.rs`).

Đây là phần khiến "đa thiết bị" thành thật, và là phần rủi ro nhất — làm sau khi P0/P1 đã có lưới test.

### P2.1 — Tách state per-device khỏi file sync

`.json` là tài liệu syncable ([sync/utils.rs:63](../src-tauri/src/sync/utils.rs)) và được merge **theo ký tự** qua Loro. Mà `feed_refresh` ghi đè toàn bộ `sources.json` sau mỗi lần refresh để cập nhật `lastFetchedAt`/`etag`. Hai máy refresh cùng lúc sinh hai bản pretty-print khác nhau ở hàng chục vị trí timestamp; merge ký tự có thể tạo ra **JSON không hợp lệ**. Khi đó `read_json_file` trả `Err`, `feed_get_sources` fail, và frontend chỉ log lỗi rồi để danh sách feed trống — người dùng thấy như mất sạch subscription.

**Làm:** bảng SQLite mới `feed_source_state(source_id PK, etag, last_modified, last_fetched_at, last_error, error_count, next_retry_at)`. `sources.json` chỉ còn dữ liệu do người dùng quyết định: `id, url, siteUrl, title, description, categoryId, updateInterval, isPaused, fullTextFetch, addedAt, iconUrl`. File chỉ đổi khi người dùng thao tác, nên gần như không còn cơ hội xung đột.

**Nghiệm thu:** scenario test hai máy refresh đồng thời rồi sync — `sources.json` vẫn parse được và không mất feed nào.

### P2.2 — Đồng bộ trạng thái đọc

Trạng thái đọc/star/read-later nằm trong SQLite nên không sync. Đọc 50 bài trên desktop, mở Android thấy vẫn 50 bài chưa đọc. Với một app tự định vị local-first + sync, đây là lỗ hổng khái niệm chứ không phải tính năng thiếu.

**Làm:** log chỉ-ghi-thêm, mỗi thiết bị một file: `Feeds/state/<deviceId>.jsonl`. Mỗi máy chỉ ghi file của chính nó nên **không bao giờ có merge xung đột** — đúng hình dạng mà một lớp sync văn bản xử lý tốt. Đọc thì hợp nhất mọi file, last-write-wins theo timestamp mỗi khoá.

Khoá phải là `(sourceId, guid)` chứ **không** phải `feed_articles.id` — id là UUID sinh cục bộ nên khác nhau giữa các máy, còn `sourceId` đến từ `sources.json` đã sync nên giống nhau.

Nén định kỳ: thiết bị sở hữu file tự viết lại bản rút gọn của chính nó, bỏ qua entry đã bị chính nó ghi đè.

### P2.3 — Refresh nền

Toàn bộ vòng lặp auto-refresh nằm trong `onMounted` của [FeedsApp.vue:176](../src/mini-apps/feeds/FeedsApp.vue) — feed chỉ cập nhật khi đang mở tab Feeds. Mở app sau ba ngày là ngồi chờ.

**Làm:** scheduler chạy nền bằng `tauri::async_runtime::spawn` trong `setup()`, theo đúng khuôn heartbeat license tại [lib.rs:558](../src-tauri/src/lib.rs). Đánh thức mỗi 5 phút, tôn trọng `updateInterval` và `next_retry_at`, phát event cho frontend cập nhật badge.

**Lưu ý Android:** nền trên Android bị OS giới hạn; giữ nguyên hành vi refresh-khi-mở ở đó, đừng cố dựng lại `SyncWorker` đã bị gỡ.

### P2.4 — Fetch song song và bỏ trần 2MB

Vòng `for` tuần tự với timeout 30s mỗi source: 100 feed kèm vài feed chết là refresh mất nhiều phút. Ngoài ra mỗi source refresh lại ghi đè toàn bộ `sources.json`, kích hoạt watcher vault, kéo theo `loadData()` chạy lại. Trần 2MB tại [fetcher.rs:5](../src-tauri/src/feed_engine/fetcher.rs) khiến feed full-text của nhiều blog lỗi vĩnh viễn.

**Làm:** `futures::stream::iter(...).buffer_unordered(6)` (`futures = "0.3"` đã có sẵn); ghi state một lần sau khi cả lượt xong; nâng trần lên 16MB và kiểm tra kích thước theo luồng thay vì sau khi đã nạp hết. Bổ sung trần tương tự cho ba đường hiện **không có** giới hạn nào: discovery, scrape, và fetch-article-content — cả ba đang gọi `.text()` không giới hạn.

### P2.5 — Đừng kéo HTML về cho danh sách

`content` nằm inline trong bảng chính và mọi `SELECT` của list view đều kéo cả HTML. Với `max_articles_per_feed = 500` và feed full-text thì đây là chi phí lớn cho mỗi lần cuộn.

**Làm:** danh sách chỉ select các cột cần; tách `feed_article_content` nếu đo được lợi ích.

---

## P3 — Ngang hàng thị trường (8–10 ngày) — ✅ đã xong 2026-08-24

Kèm 18 test mới: 5 cho discovery không phụ thuộc thứ tự thuộc tính, 7 cho chặn
địa chỉ nội bộ và `Retry-After`, 5 cho cache ảnh, 1 cho sắp xếp.

- **P3.1 Virtualization** — sau khi bỏ trần 50, render 500 `ArticleCard` sẽ khựng. Theo `CLAUDE.md`, `content-visibility` là Baseline Newly available và chỉ dùng được khi việc thiếu nó chỉ chậm chứ không sai — đúng trường hợp này, giống `TaskListView.vue`.
- **P3.2 Luồng đọc** — đánh dấu đã đọc khi cuộn (config đã khai sẵn), nhớ vị trí đọc, phím `n` sang bài chưa đọc kế tiếp.
- **P3.3 Sắp xếp** — mới nhất/cũ nhất; key i18n `sort_newest`/`sort_oldest` đã tồn tại và đang chết.
- **P3.4 Bảng phím tắt** — `j/k/s/m/b/o/r/Esc` đã có nhưng không hiển thị ở đâu. Thêm overlay `?`.
- **P3.5 Accessibility** — các `aria-label` hiện là chuỗi sinh tự động vô nghĩa: `"Is Sidebar Open = !is Sidebar Open"`, `"Show Add Feed Modal = true"`, và nút X đóng modal ghi `"More Options"` — tệ hơn là không có label. Danh sách bài dùng `<div @click>` nên không điều hướng được bằng bàn phím. Thêm focus trap cho hai modal; `ImportExportModal` hiện không đóng được bằng Escape vì `@keydown` gắn trên div không focusable và modal đó không có input autofocus.
- **P3.6 Proxy và cache ảnh** — tải ảnh qua Rust, lưu vào vault, phục vụ qua `asset:`. Giải quyết đồng thời tracking pixel (thứ P0.1 tạm chấp nhận) và đọc offline có ảnh. Sau bước này có thể siết `img-src` trở lại.
- **P3.7 Discovery bằng parser thật** — [discovery.rs:105](../src-tauri/src/feed_engine/discovery.rs) dùng hai regex khớp đúng hai thứ tự thuộc tính; thứ tự khác là trượt, trong khi `scraper` đã là dependency và đã được dùng ở `scrape.rs`. Đồng thời dừng ngay khi probe thấy feed đầu tiên thay vì thử hết tám đường dẫn.
- **P3.8 Công dân mạng tử tế** — User-Agent thật kèm URL dự án thay vì giả mạo Chrome trên cả bốn client; tôn trọng `Retry-After`; chặn địa chỉ nội bộ và loopback trước khi fetch, vì fetch đang chạy trong tiến trình Rust đặc quyền.

---

## P4 — Khác biệt hoá (mở) — ✅ đã xong 2026-08-24

Kèm 27 test mới: 7 cho adapter YouTube/Reddit, 10 cho rule engine và thẻ,
3 cho selector scrape, 7 cho định vị highlight (vitest).

Một giới hạn đã biết: highlight nằm trong SQLite nên **chưa sync giữa các máy**.
Bản lưu bền là note trong vault. Nếu cần, dùng lại đúng cơ chế P2.2 —
`Feeds/highlights/<deviceId>.json` với cùng union và fingerprint.

Feeds sẽ không thắng bằng cách làm RSS tốt hơn Reeder. Ba thứ dưới đây là chỗ vault tạo ra lợi thế mà reader độc lập không có.

- **P4.1 Highlight → Note.** Bôi đen trong reader, lưu highlight, đổ vào note kèm backlink. Đây mới là tích hợp vault thật, chứ không phải clip nguyên bài như hiện tại. Readwise và Matter thu phí cho đúng thứ này.
- **P4.2 Rule engine.** Auto-star, auto-tag, mute từ khoá. Chi phí thấp vì FTS đã có sẵn. Miniflux và Inoreader đều có; NetNewsWire và Reeder thì không.
- **P4.3 Scrape v2.** Cho phép ghi đè selector theo từng site, lưu trong `sources.json`. Scrape fallback đang là điểm mạnh hiếm — NetNewsWire, Reeder, FreshRSS đều không có, Inoreader tính phí — nên đáng đầu tư thêm.
- **P4.4 Adapter YouTube/Reddit.** Type đã khai sẵn từ đầu; làm thật thì bỏ được `feed_type` chết.

## Chủ động không làm

- **Sync với tài khoản Feedly/Inoreader/Feedbin.** Trái định hướng local-first, và P2.2 đã giải quyết đúng vấn đề mà người dùng thật sự gặp.
- **Newsletter inbox.** Cần hạ tầng email, chi phí vận hành thường trực.
- **Podcast.** Cần player, hàng đợi, quản lý tải về — một mini-app riêng, không phải một tính năng của Feeds.

## Kiểm thử xuyên suốt

Toàn module hiện có ba test, tất cả trong `sanitizer.rs`, trong khi phần sync của cùng repo có cả `scenarios.rs` lẫn oracle. Mục tiêu tối thiểu khi kết thúc P2:

- Golden-file cho parser và scrape với fixture thật lưu trong repo.
- Test cấp lệnh chạy trên DB tạm cho toàn bộ `commands/feeds.rs`.
- OPML round-trip.
- Cleanup bảo toàn starred và read-later.
- Scenario hai thiết bị cho P2.1 và P2.2, theo khuôn `sync/scenarios.rs`.

## Rủi ro

| Rủi ro | Ảnh hưởng | Giảm thiểu |
| --- | --- | --- |
| P2.1 đổi hình dạng `sources.json` | Vault cũ không đọc được | Migration một chiều, đọc được cả hai dạng trong một bản phát hành |
| P2.2 phình `Feeds/state/` | Vault nặng dần | Nén định kỳ; chỉ ghi khi trạng thái thật sự đổi |
| P0.1 mở `img-src https:` | Feed theo dõi được người đọc | Chấp nhận có thời hạn; P3.6 đóng lại |
| Refresh nền trên Android | OS giết tiến trình, hao pin | Giữ refresh-khi-mở trên Android |
