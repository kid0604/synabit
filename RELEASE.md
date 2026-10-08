# Hướng dẫn Build Release & Đóng gói (Synabit)

Tài liệu này lưu trữ các câu lệnh và quy trình cần thiết để đóng gói (build) ứng dụng Synabit lên môi trường Production, đặc biệt là Google Play Store.

## 1. Android (Google Play Store)

Google Play yêu cầu định dạng `.aab` (Android App Bundle) và bắt buộc phải được ký bằng Keystore. File keystore (`synabit-release-key.jks`) và mật khẩu (`keystore.properties`) đã được thiết lập sẵn trong dự án.

**Câu lệnh build:**
```bash
npm run build:android
```
Hoặc lệnh gốc:
```bash
npm run tauri android build -- --aab
```

**Thành quả (Output):**
File `.aab` sẽ được tạo ra tại đường dẫn:
`src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab`

Upload file này lên **Google Play Console** > **App bundle explorer** (hoặc tạo bản Release mới).

## 2. Desktop (Windows / macOS / Linux)

**Bản phát hành chính thức đi qua CI, không build tay.** Workflow `.github/workflows/release.yml` chỉ build sau khi commit được tag đã qua toàn bộ `checks.yml`, và ký file cập nhật cho updater.

```bash
git tag v0.9.20
git push origin v0.9.20
```

CI tạo một GitHub Release dạng **draft**. Kiểm tra file đính kèm rồi bấm *Publish*: chỉ khi đó `releases/latest/download/latest.json` mới trỏ tới bản mới và updater trong app mới thấy nó.

Bản macOS hiện chưa được ký Developer ID hay notarize, nên lần mở đầu tiên người dùng phải chuột phải → Open.

Build tay bên dưới chỉ để thử trên máy mình:

```bash
npm run build:desktop
```
Hoặc lệnh gốc:
```bash
npm run tauri build
```

**Thành quả (Output):**
Các file cài đặt (`.dmg`, `.exe`, `.deb`...) sẽ được tạo ra tại thư mục:
`src-tauri/target/release/bundle/`

## 3. Lưu ý bảo mật
Tuyệt đối **KHÔNG** commit các file sau lên Git (đã được cấu hình sẵn trong `.gitignore`):
- `src-tauri/synabit-release-key.jks` (Chữ ký điện tử)
- `src-tauri/gen/android/keystore.properties` (Mật khẩu chữ ký)

Bạn nên sao lưu 2 file trên ra ổ cứng ngoài hoặc Cloud cá nhân.
