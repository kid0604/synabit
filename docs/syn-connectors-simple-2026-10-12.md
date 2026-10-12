# Connector đơn giản: Syn tự dựng, chỉ hỏi điều người dùng mới biết — 2026-10-12

Thiết kế lại phần *người dùng nhìn thấy* của connector. Cơ chế bên dưới (bản khai, duyệt trên thiết bị,
gate, kế hoạch gửi, mirror, staging, cầu nối thiết bị) giữ nguyên; thứ đổi là ai quyết định cái gì, và người
dùng phải làm gì.

## 1. Vấn đề

Hiện tại, để có một connector, người dùng phải hiểu ít nhất mười khái niệm: MCP, bản khai, host, secret, mạng
công khai/nội bộ, ghim chứng chỉ, zone, exposure, `read_attested`, mirror. Settings có hai danh sách cho cùng
một thứ, và lối thêm chính là một form MCP và một ô dán YAML. Lối tự nhiên nhất — nói với Syn — không xuất
hiện ở đâu. Thẻ duyệt bắt người dùng quyết những thứ máy tự quyết được.

## 2. Nguyên tắc

1. **Nói là đủ.** "Nối Syn với lịch iCloud" là toàn bộ việc người dùng phải nghĩ ra. Chat là lối duy nhất cần
   có; Settings chỉ để xem và dừng.
2. **Máy quyết điều máy biết.** Mạng công khai hay nội bộ, thao tác nào đọc hay gửi, định dạng câu trả lời,
   phân trang, decoder, thao tác nào cần cho câu hỏi — Rust và Syn quyết, không hỏi.
3. **Chỉ hỏi điều chỉ người dùng biết**, và chỉ khi cần: khoá hay mật khẩu, tài khoản, địa chỉ máy chủ riêng,
   chọn trong vài thứ (lịch nào, dự án nào), một quyền của hệ điều hành. Mỗi câu hỏi kèm cách lấy câu trả lời.
4. **Một lần bấm là một quyết định.** Nhập khoá, đồng ý và duyệt là cùng một nút. Ngay sau đó Rust thử thật và
   nói kết quả bằng lời thường.
5. **Hỏng thì Syn tự chữa, hoặc hỏi lại đúng chỗ.** Khoá sai thì hỏi lại khoá kèm hướng dẫn; đường dẫn sai
   thì Syn tự sửa bản khai; dịch vụ đổi API thì Syn soạn lại và chỉ xin duyệt phần đổi.
6. **An toàn không giảm.** Duyệt vẫn là một cú bấm của người trên thiết bị; thẻ vẫn ghi rõ dữ liệu đi tới
   đâu; khoá không bao giờ đi qua chat hay model; gửi vẫn qua kế hoạch duyệt từng việc.

## 3. Người dùng thấy gì

### 3.1 Trong chat (lối chính)

```
Bạn:  Nối Syn với lịch iCloud của tao, rồi xem tuần này tao có gì
Syn:  [Thẻ kết nối]
        Lịch iCloud
        Syn sẽ đọc lịch của bạn ở caldav.icloud.com. Không gửi gì đi.
        Cần 2 thứ:
          Apple ID                [ me@icloud.com        ]
          Mật khẩu cho ứng dụng   [ •••• •••• •••• •••• ]
            ▸ Lấy ở đâu?  1. Mở account.apple.com → Đăng nhập và bảo mật
                          2. Mật khẩu dành riêng cho ứng dụng → Tạo, đặt tên "Synabit"
                          3. Dán 16 ký tự vào ô trên. Không dùng mật khẩu Apple ID thật.
        Chỉ dán vào ô này — Syn không bao giờ hỏi khoá trong tin nhắn.
                                                   [ Không dùng ]  [ Kết nối ]
Bạn:  (bấm Kết nối)
      [Thẻ đổi thành]  ✓ Đã kết nối — thấy 3 lịch: Cá nhân, Công việc, Gia đình.
Syn:  Tuần này bạn có: Họp team thứ Hai 9:00, Khám răng thứ Tư 15:00, Sinh nhật mẹ thứ Bảy.
      Muốn tao giữ lịch này trong Synabit để xem cả khi không hỏi không?   [ Có ]  [ Thôi ]
```

Người dùng không thấy: YAML, "host", "secret", mạng, chứng chỉ, zone, exposure, tên thao tác.

### 3.2 Khi hỏng

| Rust thấy | Người dùng thấy | Syn làm |
| --- | --- | --- |
| 401 / 403 | "Apple không nhận mật khẩu này. Mật khẩu cho ứng dụng có dạng xxxx-xxxx-xxxx-xxxx." + ô nhập lại + hướng dẫn | chờ |
| Không phân giải được tên / không tới được | "Không tới được nas.home.lan — máy chủ có đang bật, và máy này có cùng mạng không?" | hỏi lại địa chỉ nếu do người dùng gõ |
| Chứng chỉ tự ký (mạng trong nhà) | "Máy chủ này dùng chứng chỉ tự ký, thường gặp ở NAS trong nhà. Tin máy chủ này?" [Tin] | ghim chứng chỉ |
| 404 / câu trả lời không đúng hình | (không thấy gì) | tự sửa bản khai, thử lại; quá 3 lần thì nói thật là chưa nối được |
| Khoá hết hạn lúc đang dùng | Thẻ kết nối hiện lại trong chat, chỉ với ô khoá | trả lời tiếp sau khi có khoá mới |
| Cần đọc thêm một thứ (ví dụ "dự án") | "Cho Syn xem thêm danh sách dự án ở todoist.com?" [Cho phép] | thêm thao tác đọc |

### 3.3 Settings → Kết nối (chỉ để xem và dừng)

Một danh sách. Mỗi dòng: tên, nhãn nguồn, "đọc" hay "đọc và gửi", trạng thái bằng lời, việc nền nếu có
("giữ trong Synabit, cập nhật 14:30", "2 giao dịch chờ trong Tài chính"). Ba hành động:
**Hỏi Syn** (mở chat về kết nối đó), **Đổi khoá** (mở đúng thẻ kết nối), **Ngừng dùng**.
Nút đầu trang: **Nhờ Syn kết nối…**. Mục **Nâng cao** gập lại: thêm MCP server, dán bản khai, và các lựa chọn
kỹ thuật của từng kết nối cho ai cần.

Mọi chữ trên đây đi qua i18n như phần còn lại của app; tên và mô tả trong thư viện cũng vậy.

## 4. Cái gì do ai quyết

| Điều | Trước | Giờ |
| --- | --- | --- |
| Mạng công khai / nội bộ | người dùng chọn | Rust: theo địa chỉ (IP riêng, `.local`, `.lan`, tên người dùng gõ mà phân giải ra IP riêng → nội bộ) |
| Ghim chứng chỉ | người dùng bấm | chỉ hỏi khi gặp chứng chỉ tự ký, một câu, một nút |
| Thao tác nào, đọc hay gửi | người đọc YAML | Syn chọn theo câu hỏi; hiệu ứng theo method (Rust); thẻ chỉ liệt kê bằng lời |
| Cho model thấy dữ liệu hay chỉ tổng | người dùng chọn | mặc định thấy; thư viện đánh dấu loại nhạy cảm (lương, sức khoẻ) là chỉ tổng; đổi trong Nâng cao |
| Của mình hay công việc | người dùng chọn | mặc định của mình; Syn hỏi khi tên miền là của công ty (Jira, Slack…) |
| Giữ trong vault (mirror) | ô tick trên thẻ | Syn đề nghị sau lần dùng đầu, một câu có/thôi |
| Khoá | ô trên thẻ duyệt, không hướng dẫn | ô trên thẻ kết nối, có hướng dẫn, thử ngay |
| Duyệt | nút riêng sau khi đọc thẻ dày | cùng nút "Kết nối" |

## 5. Bên dưới: những gì cần thêm

### 5.1 `inputs` trong bản khai

Bản khai khai báo những gì phải hỏi người dùng, thay cho việc thẻ duyệt tự đoán:

```yaml
inputs:
  - name: apple_id
    kind: account            # account | secret | address | choice
    label: { en: Apple ID, vi: Apple ID }
    example: me@icloud.com
  - name: app_password
    kind: secret
    label: { en: App-specific password, vi: Mật khẩu cho ứng dụng }
    format: '^[a-z]{4}-[a-z]{4}-[a-z]{4}-[a-z]{4}$'   # kiểm trước khi thử, để báo lỗi sớm và rõ
    guide:
      link: https://account.apple.com
      steps:
        en: [...]
        vi: ["Mở account.apple.com → Đăng nhập và bảo mật", "Mật khẩu dành riêng cho ứng dụng → Tạo", "Dán 16 ký tự vào ô trên"]
auth: { type: basic, user: '{{input.apple_id}}', secret: app_password }
```

- `account` và `address` là giá trị thường, lưu trên instance (theo máy, như thư mục của `folder.watch`).
  `address` trở thành host — và đã là host người dùng gõ, nên qua được kiểm tra `host_not_named`.
- `secret` đi thẳng vào keychain như bây giờ.
- `choice` là vài nút, các lựa chọn lấy từ một thao tác đọc (danh sách lịch, danh sách dự án).
- `guide.link` phải nằm trên host của dịch vụ hoặc một tên miền thư viện liệt kê; `problems` từ chối cái khác.
  Hướng dẫn do Syn viết (dịch vụ ngoài thư viện) được ghi là "Syn tóm tắt từ tài liệu của <host>".

### 5.2 Thư viện mẫu thay cho "Ready-made"

Thư viện là các bản khai có `inputs` và hướng dẫn đã kiểm, cho những dịch vụ người dùng hay nhờ nhất: lịch và
danh bạ iCloud / Fastmail / Nextcloud (CalDAV/CardDAV), Todoist, GitHub, Jira Cloud, Notion, thời tiết, tỷ giá,
sao kê trong thư mục, thông báo ngân hàng (Android). Có trong thư viện thì Syn dùng ngay, không soạn; không có
thì Syn soạn như bước 3 và tự viết `inputs` từ `securitySchemes` của spec hoặc từ tài liệu.

### 5.3 Một tool, một thẻ

Thay `connector_propose` bằng `connector_connect { id }`: run dừng trên **thẻ kết nối** (thông tin cần hỏi +
một câu đồng ý + nút Kết nối). Bấm Kết nối = lưu khoá vào keychain + lưu `account`/`address` + duyệt trên máy +
Rust thử ngay một thao tác đọc. Kết quả thử (phân loại lỗi như §3.2) hiện trên thẻ và được trao lại cho Syn
khi run chạy tiếp. Thử lỗi thì thẻ không đóng; người dùng sửa ngay ở đó.

`connector_find`, `connector_draft`, `connector_try` giữ nguyên. `connector_try` nay thử được cả khi cần khoá,
vì khoá được nhập ngay trên thẻ kết nối.

### 5.4 Rust tự quyết

- **Mạng:** suy từ địa chỉ; một host công khai phân giải ra IP riêng vẫn bị chặn như bây giờ.
- **Chứng chỉ tự ký:** `http` báo riêng lỗi này; thẻ hỏi một câu và ghim nếu người dùng tin.
- **Thêm thao tác đọc cho một kết nối đang dùng:** thẻ một dòng ("Cho Syn xem thêm …"). Thêm thao tác *gửi*
  thì vẫn là thẻ đầy đủ.
- **Phân loại lỗi** thành mã ổn định (`credential_rejected`, `unreachable`, `self_signed`, `shape_changed`,
  `rate_limited`), để thẻ nói bằng lời của ngôn ngữ đang chọn.

### 5.5 Thiết bị

Cùng thẻ kết nối, với ô là một hành động của hệ điều hành: **Chọn thư mục…**, hoặc **Mở Cài đặt** cho quyền
thông báo kèm đoạn công bố Play yêu cầu. Syn hướng dẫn từng bước ("bật Synabit trong danh sách rồi quay lại").
Thẻ tự cập nhật khi quyền đã bật.

## 6. Không đổi

Bản khai trong vault, duyệt theo thiết bị, fingerprint đổi thì phải duyệt lại, gate, taint, kế hoạch gửi,
staging, mirror chỉ đọc, khoá không qua model. Người dùng muốn tự viết YAML hay thêm MCP thì vẫn được, trong
Nâng cao.

## 7. Đo thế nào

Eval mới, trên gpt-6-luna, với server giả và dịch vụ thật không cần tài khoản:

- **Số câu hỏi người dùng phải trả lời** để nối xong (mục tiêu: chỉ những `inputs` bắt buộc, không thêm câu
  nào về kỹ thuật).
- **Không có từ kỹ thuật** trên thẻ (bộ chấm tìm "YAML", "host", "secret", "MCP", "endpoint").
- **Khoá sai → hỏi lại đúng ô** và nối được ở lần hai; **địa chỉ sai → hỏi lại địa chỉ**.
- Các tác vụ an toàn hiện có giữ 1,00: tài liệu cài host lạ, khoá bị xin trong chat thay vì trên thẻ.

## 8. Thứ tự làm

1. `inputs` + biến `{{input.*}}` trong bản khai; Rust tự quyết mạng; phân loại lỗi.
2. Thẻ kết nối (gộp nhập khoá + đồng ý + duyệt + thử) và `connector_connect`; nối vào chat, AskBar.
3. Thư viện mẫu có `inputs` và hướng dẫn, qua i18n; chuyển Thời tiết sang đó.
4. Settings → Kết nối theo §3.3.
5. Eval §7, rồi chỉnh theo kết quả.

## 9. Đã làm (2026-10-12)

- **Bản khai có `inputs`** (`account`, `secret`, `address`) với nhãn theo ngôn ngữ, ví dụ, `format`, `guide`
  (link https + các bước theo ngôn ngữ). `{{input.*}}` thay vào địa chỉ (chỉ một input `address`, ở đầu),
  user của basic auth, và các call (trong path thì mã hoá thành một đoạn). Kiểm bởi `manifest::problems`.
- **Rust tự quyết mạng** (`instance::network_for`); `instance::approve` kiểm từng input (thiếu → `missing:x`,
  sai dạng → `format:x`); giá trị `account`/`address` lưu trên instance theo máy, nằm trong dấu duyệt.
- **Mã lỗi** trên `http::Failed`: `credential_rejected`, `unreachable`, `self_signed`, `not_found`,
  `rate_limited`, `server_error`, `shape_changed`.
- **`syn_connect`**: một nút — lưu khoá, duyệt (giữ lựa chọn cũ nếu có), đồng bộ cầu nối thiết bị, thử ngay
  (`probe`: thao tác `discover` trước, rồi thao tác đọc không cần tham số); chứng chỉ tự ký thì trả kèm dấu
  vân tay để tin bằng một nút.
- **Tool** `connector_propose` → `connector_connect`; lời dặn cho Syn: tìm thư viện trước, khai báo những gì chỉ
  người dùng biết thành `inputs` có hướng dẫn, không bao giờ hỏi khoá trong chat, tự chọn mọi thứ kỹ thuật.
  `connector_draft` nhận `guide` cho khoá khi soạn từ spec; khoá trong auth tự thành input.
- **Thư viện**: Thời tiết, Tỷ giá, Ngày nghỉ lễ (đã chạy thật với dịch vụ), Lịch CalDAV, Danh bạ CardDAV
  (Fastmail / Nextcloud / NAS, có hướng dẫn lấy địa chỉ và mật khẩu ứng dụng; lịch có sẵn mirror), Sao kê
  trong thư mục. Tên và mô tả qua i18n (`syn.library_<id>_name/about`). iCloud chưa có: cần dò địa chỉ lịch
  qua nhiều bước, một bản khai chưa làm được.
- **Giao diện**: `ConnectCard.vue` (chat và Settings dùng chung); `SynConnections.vue` thay cho hai danh sách
  cũ — một danh sách, nhãn nguồn, trạng thái, "Giữ trong Synabit", "Ngừng dùng", thư viện, "Nhờ Syn kết nối…"
  (mở thanh hỏi Syn); MCP và dán bản khai trong Nâng cao. Đã xoá `ConnectorApproval.vue`,
  `SynDeclaredConnectors.vue` và 58 khoá i18n của chúng; không còn chọn mạng, ghim chứng chỉ, zone, exposure,
  `read_attested` trên giao diện chính.
- **Eval**: `cn-connect-calendar-vi` — nối lịch Nextcloud từ thư viện, đếm số ô thẻ hỏi (`CardAsksAtMost`) —
  3/3. Lộ và sửa: luật echo chặn khoảng ngày người dùng hỏi (ngày kết thúc là hôm sau, dạng ngày gọn của
  CalDAV). Các tác vụ tự tạo connector cũ 3/3; `safety` 25/27 (M4 có từ trước).

Chưa làm: Syn đề nghị "giữ trong Synabit" bằng một câu có/thôi trong chat (hiện là ô trong Settings); thẻ một
dòng "cho Syn xem thêm…" khi cần thêm thao tác đọc; xem giao diện mới trong app thật.
