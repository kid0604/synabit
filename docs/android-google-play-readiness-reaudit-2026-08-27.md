# Synabit Android — Google Play Readiness Re-audit

**Ngày audit:** 27/08/2026 (Asia/Ho_Chi_Minh)
**Phạm vi:** kiến trúc, source, dữ liệu/sync, security/privacy, Android release engineering, UI/UX, accessibility, Google Play compliance
**Snapshot:** `main` tại `d4148d7`, **cộng 373 thay đổi chưa commit**
**Phiên bản:** `0.9.7-1`, `versionCode 9007`
**Audit trước:** `android-google-play-readiness-reaudit-2026-07-24.md` — 40/100, NO-GO

---

## 1. Kết luận điều hành

### Điểm hiện tại: **63/100** (trước: 40/100)

### Quyết định

| Mốc | Trạng thái |
|---|---|
| Build được AAB ký release | **Đạt** (đã xác minh 24/07; toolchain hôm nay vẫn xanh) |
| Internal testing | **Đạt sau khi xử lý P0-1 → P0-3** |
| Closed testing | **Đạt sau khi xử lý toàn bộ P0** |
| **Production rollout** | **KHÔNG — chưa sẵn sàng** |

Đây là bước tiến rất lớn so với 24/07. Blocker runtime nghiêm trọng nhất — R8 xóa
`SecureStore.saveSecret/getSecret` khiến app không đọc được khóa mã hóa của chính
nó — đã được sửa đúng cách, và còn được bọc thêm bằng một CI check. Mọi quality
gate về test đều đã xanh.

Nhưng **artifact hiện tại vẫn không phải release candidate**, vì ba lý do có thể
kiểm chứng ngay và một lý do chức năng:

> 1. CI Android **chưa bao giờ chạy** — file workflow còn ở trạng thái untracked.
> 2. ~~`npm run type-check` đang fail~~ — **đã sửa 27/08**; và một trong 9 lỗi
>    hoá ra là bug runtime thật (dropdown category trắng ở QuickCap và Task).
> 3. **373 file chưa commit.** Không có điểm build tái lập được, không có tag.
> 4. ~~`SYNABIT_ANDROID_CLIENT_ID` không tồn tại ở bất kỳ đâu~~ — **đính chính,
>    xem P0-4.** Biến này *có* trong `src-tauri/.env` nên build local vẫn đúng;
>    nhưng nó vắng mặt trong CI, và `.env` thì gitignored — nên AAB do CI build
>    sẽ fallback sang desktop client ID. **Đã sửa 27/08.**

---

## 2. Bằng chứng đo được trong lần audit này

Tất cả các số dưới đây được chạy trực tiếp trên working tree hôm nay.

| Gate | Kết quả | So với 24/07 |
|---|---|---|
| `cargo test --lib` | **1369 passed, 0 failed, 12 ignored** | không compile được → xanh |
| `npx vitest run` | **1077 passed / 78 files** | fail → xanh |
| `npx eslint 'src/**/*.{ts,vue}'` | **0 errors, 35 warnings** | 34 errors → xanh |
| `cargo check --target aarch64-linux-android` | **0 errors, 0 warnings** | — |
| `npm run type-check` | **PASS** (đã sửa trong phiên này — xem P0-2) | 68 lỗi → 9 → 0 |
| `npm run build` | **PASS** | — |
| `./gradlew :app:lintUniversalDebug` | **0 errors, 43 warnings, 1 hint** | 2 errors → 4 → 0 |
| `npm audit --omit=dev` | **0 vulnerabilities** | 0 → 6 → 0 |
| 16 KB page alignment (NDK 27.1 pinned) | **PT_LOAD align = `0x4000`** ✅ | giữ nguyên |

---

## 3. Những gì đã được sửa đúng (không cần đụng lại)

Phần này ghi nhận, không phải để khen — mà để lần audit sau không mở lại.

**Security & platform boundaries**

- `proguard-rules.pro` giữ `-keep class com.synabit.app.SecureStore { *; }`, giữ
  **toàn bộ member** chứ không chỉ native methods. Đây đúng là cách sửa; thu hẹp
  hơn chính là nguyên nhân gây lỗi lần trước.
- `secrets.rs` phía Rust: resolve class qua **app class loader** (không phải system
  class loader — thread do Rust attach không thấy được app class), clear pending
  JNI exception sau mỗi lỗi, và **không panic** ở bất kỳ nhánh nào. Có cả migration
  một chiều từ file plaintext cũ sang keystore, và chỉ xóa file cũ khi ghi thành công.
- `android:allowBackup="false"` — vault, note, SQLite cache không còn bị đẩy lên
  Google Drive dạng plaintext. Bù lại bằng `export_vault_archive` / `import_vault_archive`,
  reachable từ Settings trên mobile.
- `updater:default` chỉ nằm trong `capabilities/desktop.json` với `platforms`
  whitelist, **không** trong `default.json`. Frontend chặn thêm một lớp
  (`useAppUpdate.ts:95`). Hai khóa, đúng như comment mô tả.
- `file_paths.xml` thu từ `<external-path path="."/>` (toàn bộ shared storage!)
  xuống đúng 3 thư mục của app.
- FileProvider `exported="false"`; `CaptureReplyReceiver` không exported;
  `CaptureTileService` guard bằng `BIND_QUICK_SETTINGS_TILE`; widget provider
  exported đúng vì launcher mới là bên broadcast.
- PendingIntent: `FLAG_IMMUTABLE` ở mọi nơi trừ reply intent (bắt buộc `MUTABLE`
  để hệ thống ghi RemoteInput vào) — đúng.
- Mọi intent nội bộ đều `setPackage(packageName)` → app khác đăng ký cùng scheme
  không cướp được.
- WorkManager + `SyncWorker` + `jni.rs` (background sync giả) đã bị xóa hẳn.
- Không có `QUERY_ALL_PACKAGES`, không foreground service, không ads, không telemetry.
- Audio ghi bằng `MediaRecorder` và **ở lại máy** — không upload, không transcribe
  qua cloud. Data Safety khai báo sẽ đơn giản.
- Feed HTML được sanitize **phía Rust bằng `ammonia`** trước khi tới WebView; CSP
  `script-src 'self'` chặn inline handler. Hai lớp độc lập.

**Release engineering**

- `ndkVersion` pin cứng `27.1.12297006` — và đã xác minh: `.so` arm64 có
  `PT_LOAD ... 0x4000` = 16 KB. Đáp ứng yêu cầu Play có hiệu lực từ 01/11/2025.
- `targetSdk 36` / `compileSdk 36` — đáp ứng yêu cầu Play siết từ 31/08/2026
  (còn **4 ngày**).
- `keystore.properties` và `*.jks` đều gitignored, `git ls-files` xác nhận không
  có file keystore nào bị track.
- `versionName` / `versionCode` / `package.json` / `Cargo.toml` / `tauri.conf.json`
  đồng bộ ở `0.9.7-1` / `9007`.

**UI/UX Android**

- Splash theme + `windowBackground` khớp token `--color-base` → không còn flash trắng.
- `locales_config.xml` (en/vi) → app xuất hiện trong per-app language picker.
- `installSplashScreen()` + `enableEdgeToEdge()` đúng thứ tự, trước `super.onCreate`.
- `env(safe-area-inset-*)` được dùng ở MobileLayout và toàn bộ modal mobile.
- `useBackGuard` giải quyết đúng vấn đề back-button đóng modal, không cần Kotlin.
- `WryActivity` dùng `onBackPressedDispatcher.addCallback` (tự disable trước khi
  gọi `onBackPressed()`, không đệ quy) → predictive back ở targetSdk 36 không vỡ.
- `POST_NOTIFICATIONS` được request runtime, **đúng thời điểm** (sau khi có vault),
  và nhớ đã hỏi — quan trọng vì Android chỉ cho hỏi 2 lần.
- Không khai `LEANBACK_LAUNCHER` — đúng, app này là touch-only.

---

## 4. Blocker P0 — phải sửa trước khi upload cho bất kỳ tester nào

### P0-1 — CI Android chưa bao giờ chạy

```
$ git status --short .github
?? .github/workflows/android.yml
```

`.github/workflows/android.yml` là file **untracked**. Nội dung của nó rất tốt —
có hẳn một step `Verify the keep rule survived` grep `mapping.txt` để bắt lại đúng
lỗi R8/SecureStore từng làm hỏng mọi bản release, và một step thu `mapping.txt`
(không có nó thì crash report từ Play là vô nghĩa vì R8 rename mỗi lần build).

Nhưng vì chưa commit, **không có gì trong đó từng chạy**. Toàn bộ safety net đang
là code chết. Đây là P0 đúng nghĩa: giá trị của nó bằng 0 cho tới khi được commit và push.

**Fix:** commit và push `.github/workflows/android.yml`, chạy một lần bằng
`workflow_dispatch`, xác nhận cả 3 job xanh.

### P0-2 — `npm run type-check` đang fail → **ĐÃ SỬA 27/08**

9 lỗi ban đầu, ở 4 nhóm. Ba nhóm đầu là lỗi typing thuần; nhóm thứ tư hoá ra là
**bug runtime thật**.

| File | Lỗi | Cách sửa |
|---|---|---|
| `MonthView.vue:97,98` | `t(key, plural, named)` không phải signature của vue-i18n — tham số thứ 3 là `TranslateOptions`, nên `{ n }` khớp không overload nào và rơi xuống overload sai | Đổi sang `t(key, { n }, plural)` |
| `CalendarApp.vue:290` | `ImportSummary` là `interface` → không có index signature → không gán được cho `NamedValue` | Truyền đúng 2 field message dùng: `{ added, updated }` |
| `CalendarHeader.vue:26` | `emit` là overload set; TypeScript không distribute union argument qua nó — chỉ thử signature cuối (`'add-event'`) rồi từ chối phần còn lại | `switch` 3 nhánh, mỗi nhánh emit literal |
| `QuickCapApp.vue:2210-2211`, `TaskApp.vue:609-610` | `string[]` không gán được cho `Category[]` | **Xem dưới — đây là bug thật** |

**Nhóm thứ tư là bug runtime, không phải lỗi typing.**

`TransactionModal` render category bằng:

```html
<option v-for="cat in availableCategories" :key="cat.id" :value="cat.id">{{ cat.name }}</option>
```

Nhận `string[]` thì mọi option có `value === undefined` và label rỗng — dropdown
là một cột trắng, và transaction lưu xuống với `category: ''`.

Category đã được migrate từ `string` sang `{ id, name }` (xem
`finance/categories.ts` — comment ở đầu file giải thích tại sao: rename category
kiểu string làm mọi transaction cũ biến mất khỏi breakdown). `FinanceApp` đã
chạy đúng qua helper `toCategories`. **Hai consumer bị bỏ lại:**

| Chỗ | Trước | Hệ quả |
|---|---|---|
| `finance/ledger.ts:80` | `FinanceSetup.incomeCategories: string[]`, đọc raw không convert | QuickCap → "Book as transaction" mở form với dropdown trắng |
| `task/composables/useProjectManager.ts:32` | `ref<string[]>([...DEFAULT_INCOME_CATEGORIES])` | Project trong vault **chưa có Finance config** mở form với dropdown trắng — ở đây tệ hơn vì default cũng sai, không chỉ giá trị đọc từ file |

Cả hai đã được sửa bằng đúng helper `FinanceApp` dùng:

```ts
// ledger.ts
incomeCategories: toCategories(properties?.incomeCategories),

// useProjectManager.ts
const incomeCategories = ref<Category[]>(toCategories(DEFAULT_INCOME_CATEGORIES));
...
incomeCategories.value = toCategories(configNode.properties.incomeCategories);
```

`toCategories` đọc được **cả hai shape**, nên vault cũ còn giữ string vẫn hiển thị
đúng thay vì một cột trắng — cùng safety net mà `FinanceApp` đã có.

**Xác minh sau khi sửa:** `type-check` exit 0 · `vitest` 1077 pass · `eslint`
0 errors (59 warnings, không đổi) · `npm run build` thành công.

### P0-3 — 373 file chưa commit, không có build point tái lập

```
$ git status --short | wc -l
373
```

Trong đó có toàn bộ các thay đổi Android quan trọng nhất — `AndroidManifest.xml`,
`build.gradle.kts`, `proguard-rules.pro`, `file_paths.xml`, `capabilities/*.json`,
`secrets.rs`. Nghĩa là: **mọi thứ ở mục 3 của báo cáo này đều chưa nằm trong lịch sử git.**

Hệ quả cụ thể: một AAB build hôm nay không truy nguyên được về commit nào. Nếu
Play trả về crash sau 2 tuần, không có cách nào biết bản đó chứa gì.

**Fix:** commit theo nhóm logic, tag `v0.9.7-1`, build AAB **từ tag đã checkout sạch**.

### P0-4 — `SYNABIT_ANDROID_CLIENT_ID` không được truyền vào CI → **ĐÃ SỬA 27/08**

> **Đính chính so với bản đầu của báo cáo này.** Bản đầu viết biến này "không tồn
> tại ở bất kỳ đâu". Sai. Nó **có** trong `src-tauri/.env` (gitignored, không
> track — đã kiểm chứng bằng `git ls-files`). Lần quét đầu chỉ tìm `.env` ở repo
> root chứ không phải trong `src-tauri/`.
>
> Hệ quả của đính chính: **AAB build tay trên máy dev luôn có client ID đúng** —
> kể cả artifact ngày 24/07. Google Drive trên các bản đó không hỏng.
>
> Nhưng lỗ hổng vẫn là P0, chỉ đổi phạm vi: `.env` **gitignored**, nên một
> checkout sạch trên GitHub Actions không có nó. CI là đường phát hành dự kiến,
> và ở đó `option_env!` trả `None` → fallback sang desktop client ID → AAB build
> xong, ký xong, upload được, và Google Drive chết trên mọi máy người dùng.
> Không test nào trong repo chạm tới được, vì không có gì sai cho tới khi hỏi Google.

`src-tauri/src/gdrive/mod.rs:30` (trước khi sửa):

```rust
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) const CLIENT_ID: &str = match option_env!("SYNABIT_ANDROID_CLIENT_ID") {
    Some(val) => val,
    None => env!("SYNABIT_GOOGLE_CLIENT_ID", "..."),  // ← fallback về desktop
};
```

Biến này **không có** trong `.env.example` và **không có** trong
`.github/workflows/android.yml` — chỉ có trong `.env` local.

Cả hai flow mobile đều dùng redirect custom scheme và **dùng chung** const này:

```rust
let redirect_uri = "com.synabit.app:/oauth2callback";
// auth.rs:248,279  — vault sync
// browse.rs:375    — OmniDrive file browser
```

Google chỉ chấp nhận redirect custom-scheme từ client type **Android**. Hỏi bằng
client Web/Desktop thì authorization request trả `invalid_request`: người dùng ở
lại một tab trình duyệt không bao giờ quay về app, và log không nói gì.

**Đã sửa — 3 thay đổi:**

**1. Bỏ hẳn fallback** (`gdrive/mod.rs`). `option_env!` + fallback đổi thành
`env!` bắt buộc, tách `android` và `ios` thành hai arm riêng:

```rust
#[cfg(target_os = "android")]
pub(crate) const CLIENT_ID: &str = env!(
    "SYNABIT_ANDROID_CLIENT_ID",
    "Android needs its own Google OAuth client (type: Android). Set \
     SYNABIT_ANDROID_CLIENT_ID at build time — the desktop client ID will not \
     work here, because Google rejects the custom-scheme redirect this flow uses."
);
```

Fallback chính là thứ khiến build hỏng vẫn ship được, nên cách sửa là bỏ nó chứ
không phải vá quanh nó. Desktop vốn đã hard-fail kiểu này rồi — giờ nhất quán.

**2. Chặn chuỗi rỗng.** `env!` chấp nhận `""`, và một GitHub secret chưa cấu hình
expand đúng thành `""` — nên riêng thay đổi (1) vẫn bị CI qua mặt:

```rust
const _: () = assert!(!CLIENT_ID.is_empty(), "...");
```

Không cfg — áp cho mọi platform, vì client ID rỗng ở đâu cũng chỉ sinh ra một
request Google từ chối.

**3. CI**: thêm `SYNABIT_ANDROID_CLIENT_ID` vào `env:` của step `Build AAB`, kèm
một preflight `[ -z ... ]` chạy trước `npm run build:android:play` — lỗi Rust
xuất hiện sau vài phút build, preflight xuất hiện ngay.

**Xác minh** (rustc độc lập, không đụng `.env` thật):

| Trường hợp | Kết quả |
|---|---|
| Unset — đúng điều kiện checkout sạch trên CI | ❌ compile fail, in đúng message hướng dẫn |
| Set nhưng rỗng — secret chưa cấu hình | ❌ compile fail ở `const _: () = assert!` |
| Set đúng | ✅ compile OK |

Build thật: `cargo check --target aarch64-linux-android` ✅ · `cargo check` (desktop) ✅

**Còn phải làm ngoài repo — chưa xong, và không làm được từ đây:**

1. Tạo OAuth client type **Android** trên Google Cloud Console: package
   `com.synabit.app` + SHA-1.
2. Khai **cả hai** fingerprint: upload key *và* Play App Signing key. Chỉ khai
   cái đầu thì bản sideload chạy còn bản Play hỏng — đây là cái bẫy dễ mất nửa
   ngày để tìm ra.
3. Thêm secret `SYNABIT_ANDROID_CLIENT_ID` vào GitHub repo settings. Không có nó,
   job `aab` bây giờ **fail** thay vì âm thầm build ra artifact hỏng — đó là
   hành vi mong muốn.

### P0-5 — Chưa có smoke test bản release đã ký trên máy sạch

Lần audit 24/07 cũng chưa làm được (emulator đang giữ bản debug ký khác key).
Đến giờ vẫn chưa có bằng chứng nào.

Điều này quan trọng hơn bình thường ở dự án này, vì **R8 chỉ chạy ở release** và
đường JNI Rust→Java là thứ duy nhất R8 không nhìn thấy. Bản debug chạy được
**không chứng minh gì** về bản release.

**Checklist tối thiểu trên AVD sạch, cài từ AAB đã ký:**

- [ ] Cold start, không crash
- [ ] Tạo vault, viết note, kill app từ Recents, mở lại → note còn
- [ ] Bật E2EE → tắt app → mở lại → khóa vẫn đọc được (**đây là bài test R8/SecureStore**)
- [ ] Share text từ Chrome → `CaptureActivity` → cap xuất hiện trong QuickCap
- [ ] Reply thẳng trong notification → cap xuất hiện
- [ ] Quick settings tile → mở compose box
- [ ] Widget → mở compose box
- [ ] Google Drive connect (test P0-4)
- [ ] Xoay màn hình ở mọi mini-app
- [ ] Bật "Don't keep activities" → process death → quay lại
- [ ] Light mode cold start (test P1-3)

---

## 5. P1 — nên sửa trước closed testing

### P1-1 — npm production vulnerabilities đã regress

24/07 báo `npm audit --omit=dev` sạch. Hôm nay:

| Package | Mức | Vấn đề | Liên quan trực tiếp? |
|---|---|---|---|
| `pdfjs-dist` | **HIGH** | [GHSA-hq66-cqwq-w95j](https://github.com/advisories/GHSA-hq66-cqwq-w95j) — arbitrary JS execution khi mở PDF độc hại | **Có.** App có PDF viewer. Fix là major bump 5.x → 6.2.108 |
| `dompurify` | moderate | [GHSA-55q2-fjhq-7xh7](https://github.com/advisories/GHSA-55q2-fjhq-7xh7) — IN_PLACE hook removal để lại subtree executable → XSS | **Có.** DOMPurify là lớp sanitize cho QuickCap, Nexus, Messages, TextFileViewer |
| `nanoid` | high | infinite loop DoS | gián tiếp |
| `postcss` | high | path traversal qua `sourceMappingURL` | build-time |
| `mermaid` | moderate ×4 | prototype pollution, CSS injection, DoS | có, note render mermaid |
| `undici` | moderate ×3 | qua `cheerio` | gián tiếp |

`pdfjs-dist` và `dompurify` là hai cái đáng lo — cả hai đều nằm đúng trên đường
xử lý nội dung không tin cậy.

### P1-2 — Android lint 4 errors

```
CaptureTileService.kt:53   Error: StartActivityAndCollapseDeprecated
quickcap_widget.xml:36     Error: UseAppTint
strings.xml:2              Error: "app_name" chưa dịch sang vi
strings.xml:3              Error: "main_activity_title" chưa dịch sang vi
```

Cái đầu là do `@Suppress("DEPRECATION")` **không khớp lint ID**. Đúng phải là
`@Suppress("StartActivityAndCollapseDeprecated")`. Code logic thì đúng — nhánh
`SDK_INT >= UPSIDE_DOWN_CAKE` đã có.

Hai cái `MissingTranslation`: hoặc thêm vào `values-vi/strings.xml`, hoặc
`tools:ignore="MissingTranslation"` (tên app thường không dịch — chấp nhận được).

Không cái nào chặn Play, nhưng job lint hiện là `|| true`; giữ nguyên `|| true`
thì nó không bao giờ là gate.

### P1-3 — Nháy màn hình đen mỗi lần cold start ở light mode

`index.html` hardcode:

```html
body, html { ... background-color: #1a1a1c; color: #fff; }
```

Trong khi `values/colors.xml` đặt `app_background = #FDFDFC` và `style.css` đặt
`--color-base: #fdfdfc`.

Chuỗi thực tế trên máy light mode: splash **trắng** → WebView load, paint **đen**
→ app paint **trắng**. Đúng cái flash mà theme/splash được dựng ra để loại bỏ,
chỉ là dịch sang chỗ khác. Fix: dùng `prefers-color-scheme` trong `index.html`,
hoặc lấy đúng hai token đã có.

### P1-4 — `<title>Tauri + Vue + Typescript App</title>`

`index.html` còn nguyên title scaffold. Không ảnh hưởng Android WebView nhưng là
thứ lọt ra khi ai đó xem source hoặc build web preview.

### P1-5 — ABI matrix vs `minSdk 24` → **ĐÍNH CHÍNH, không sửa**

Bản đầu gọi đây là "mâu thuẫn". Nói quá.

`build:android:play` build `aarch64` + `x86_64`, nên máy **armeabi-v7a** sẽ
không được Play phục vụ. Nhưng `minSdk 24` vẫn có nghĩa thật: máy **64-bit chạy
Android 7.0/7.1** vẫn cài được. Hai thứ này không mâu thuẫn — chúng chặn hai
nhóm khác nhau.

Nên đây là **quyết định về độ phủ thiết bị**, không phải bug:

- Nâng `minSdk` lên 26/28 sẽ **loại thêm** máy 64-bit Android 7 đang cài được —
  làm mọi thứ tệ hơn, không tốt hơn.
- Thêm `armeabi-v7a` sẽ mở rộng độ phủ, đổi lại CI lâu hơn và AAB to hơn.

Comment trong CI đã nói rõ lý do bỏ 32-bit. Giữ nguyên arm64 + x86_64 là lựa
chọn hợp lý cho một bản phát hành năm 2026. Không sửa gì.

### P1-6 — `tauri` feature `"test"` nằm trong dependency production

`Cargo.toml`:

```toml
tauri = { version = "2", features = ["protocol-asset", "test", "tray-icon"] }
```

`test` kéo mock runtime vào **binary release**. Nên chuyển sang `[dev-dependencies]`
override. `tray-icon` trên Android cũng vô nghĩa (không có tray) — dead weight
trong `.so` vốn đã lớn.

### P1-7 — Build Android bắt buộc phải có desktop client secret

`cargo check --target aarch64-linux-android` xác nhận:

```
src/gdrive/mod.rs:40: warning: constant `CLIENT_SECRET` is never used
```

Tin tốt: secret **không** bị nhúng vào `.so` Android. Tin xấu: `env!` (chứ không
phải `option_env!`) khiến build Android vẫn **fail nếu thiếu** biến đó. Một CI
chỉ build Android vẫn buộc phải cầm secret của desktop. Đổi `CLIENT_SECRET` sang
cfg-gate desktop-only là xong.

### P1-8 — Capability scope `**` áp dụng cả Android → **ĐÍNH CHÍNH, không sửa**

`capabilities/default.json` không có field `platforms`, nên các quyền sau áp dụng
cho Android:

```json
"fs:allow-read"  → path "**"
"fs:allow-write" → path "**"
"opener:allow-open-path" → path "**"
```

cộng `assetProtocol.scope: ["**"]` trong `tauri.conf.json`.

**Đính chính — không thu hẹp, và có lý do.**

Kiểm tra merged manifest: app **không khai bất kỳ quyền storage nào** — không
`READ_EXTERNAL_STORAGE`, không `READ_MEDIA_*`, không `MANAGE_EXTERNAL_STORAGE`.
Nghĩa là trên Android, tiến trình bị OS sandbox nhốt trong đúng thư mục của
chính nó, bất kể Tauri scope ghi gì. `**` ở đây **đã** bằng đúng
`$APPDATA/**` + `$DOCUMENT/**` trên thực tế.

Thu hẹp scope sẽ: không tăng an toàn thêm chút nào, nhưng có thật rủi ro làm vỡ
đường đọc file trên máy thật — thứ không kiểm chứng được nếu không có thiết bị.
Đổi lấy rủi ro đó để nhận về số 0 là sai.

Chỗ `**` thực sự có nghĩa là **desktop**, nơi tiến trình đọc được cả ổ đĩa. Nhưng
ở đó user chọn vault ở bất kỳ đâu, nên scope tĩnh hẹp là không khả thi — muốn
đúng phải chuyển sang cấp scope động lúc chạy (`fs::Scope::allow_directory` sau
khi biết vault path). Đó là việc riêng, không phải việc của Android.

### P1-9 — `v-html` không sanitize ở `ProjectDashboard.vue:63`

```html
<div v-html="activeProject.content"></div>
```

Mọi chỗ `v-html` khác đều đi qua DOMPurify hoặc qua `ammonia` phía Rust. Chỗ này
không. Nội dung là của chính user nên rủi ro thấp, và CSP đỡ được — nhưng CSP
đang là lớp phòng thủ **duy nhất** ở đây.

---

## 6. P2 — chất lượng, không chặn phát hành

- **Accessibility:** 232 `aria-label` trên 897 `<button>`. Còn xa TalkBack usable.
  Đây vẫn là nhóm điểm thấp nhất và chưa có ai đo bằng TalkBack thật.
- `values/colors.xml` còn nguyên 7 màu template (`purple_200`, `teal_700`, …) —
  lint báo unused.
- `ic_launcher_round.png` trùng byte với `ic_launcher.png` và không tròn.
- Adaptive icon thiếu `<monochrome>` → không có themed icon trên Android 13+.
- `IconDipSize`: `mipmap-hdpi/ic_launcher.png` là 33×33dp trong khi các density
  khác 48×48dp.
- `activity_main.xml` unused.
- `drawable-v24/` thừa (minSdk đã là 24).
- `allowBackup` deprecated từ Android 12 → nên thêm `dataExtractionRules` cho rõ ràng
  (hành vi hiện tại vẫn đúng, chỉ là implicit).
- 14 warning Rust và 59 warning ESLint — phần lớn là unused import.
- `.agents/oracles/c2b_arch_closure_v2.rs` được `include!` vào test tree; file
  scratch (`tests.rs`, `verify.txt`, `scratch_tests.txt`, `fix_c1.py`, `move_tests.py`,
  `test.db`, `synabit.db`) nằm ở repo root.

---

## 7. Không kiểm chứng được từ repository

Những mục sau nằm ngoài source, cần xác nhận trên Play Console:

- **Data Safety form** — với `RECORD_AUDIO`, sync relay, và license server, form này
  phải khai đúng. Privacy policy (`legal/PRIVACY_POLICY.md`) đã viết đủ chi tiết để
  điền form, nhưng bản thân form chưa xác minh được.
- **Play App Signing** đã bật chưa; upload key đã backup ngoài máy cá nhân chưa.
- **Privacy policy URL** đã live chưa (file có trong repo, URL công khai thì chưa rõ).
- Store listing, screenshots, feature graphic, content rating questionnaire.
- **Yêu cầu 12 tester × 14 ngày closed testing** — nếu đây là personal developer
  account tạo sau 13/11/2023, Google bắt buộc mốc này *trước khi* được apply
  production access. Nếu đúng vậy thì production rollout còn cách ít nhất 2 tuần
  kể từ ngày bắt đầu closed test, bất kể code sạch đến đâu.
- `versionCode 9007` đã từng upload lên Play chưa (nếu rồi thì phải bump).

---

## 8. Thang điểm

| Nhóm | Trọng số | Điểm | Quy đổi | Δ vs 24/07 |
|---|---:|---:|---:|---:|
| Kiến trúc & platform boundaries | 10% | 80 | 8.00 | +25 |
| Chất lượng source & tests | 20% | 72 | 14.40 | +44 |
| Dữ liệu, schema & sync | 15% | 70 | 10.50 | +25 |
| Security & privacy | 20% | 62 | 12.40 | +32 |
| Android release engineering | 15% | 55 | 8.25 | −5 |
| UI/UX & performance | 8% | 62 | 4.96 | +7 |
| Accessibility | 5% | 35 | 1.75 | +17 |
| Play compliance & operations | 7% | 45 | 3.15 | +18 |
| **Tổng** | **100%** | | **63.41 → 63/100** | **+23** |

Release engineering là nhóm **duy nhất tụt điểm**, và tụt vì một lý do rõ ràng:
24/07 ít nhất còn có một AAB ký thật, build thật, hash thật. Hôm nay có một CI
tốt hơn hẳn nhưng chưa commit, và một working tree 373 file chưa vào git.
Công cụ tốt hơn, kỷ luật kém hơn.

---

## 9. Thứ tự hành động

**Trước khi upload cho bất kỳ tester nào:**

1. ~~Sửa 9 lỗi TypeScript (P0-2).~~ **Xong 27/08.**
2. Commit toàn bộ working tree theo nhóm logic; tag `v0.9.7-1` (P0-3).
3. Commit + push + chạy `.github/workflows/android.yml`, xác nhận 3 job xanh (P0-1).
4. ~~Thêm `SYNABIT_ANDROID_CLIENT_ID` vào CI và `.env.example`~~ **Xong 27/08.**
   Còn lại phần ngoài repo: tạo OAuth client type Android, khai cả SHA-1 upload
   key **và** Play App Signing key, thêm secret vào GitHub (P0-4).
5. `npm audit fix`; xử lý riêng `pdfjs-dist` 5.x → 6.x (major) (P1-1).
6. Build AAB **từ tag đã checkout sạch**, cài lên AVD sạch, chạy hết checklist P0-5.

**Trước closed testing:**

7. ~~P1-1, P1-2, P1-3, P1-4, P1-6, P1-7, P1-9~~ **Xong 27/08.** P1-5 và P1-8 đã
   đính chính thành "không sửa", kèm lý do tại mục tương ứng.

**Trước production:**

8. Hoàn tất mục 7 (Data Safety, Play App Signing, privacy URL, listing).
9. Chạy đủ 12 tester × 14 ngày nếu là personal account.
10. Một vòng TalkBack thật trên máy thật (P2 accessibility).

---

## 10. Trả lời trực tiếp

> **Sản phẩm đã ready cho production chưa?**

Chưa. Nhưng khoảng cách bây giờ là **kỷ luật release + một lỗi OAuth config**,
không còn là chất lượng code. Đó là khác biệt lớn so với 24/07, khi vấn đề là
app không đọc nổi khóa mã hóa của chính nó.

> **Sẵn sàng lên Google Play chưa?**

Sẵn sàng cho **Internal testing** sau khi xong P0-1 → P0-3 (ước tính 1–2 ngày).
Sẵn sàng cho **Closed testing** sau khi xong toàn bộ P0 kể cả smoke test thiết bị
(ước tính 3–5 ngày).
**Production còn ít nhất 2–3 tuần**, và mốc chặn có thể không phải là code — mà là
yêu cầu 12 tester × 14 ngày của Google, nếu đây là personal developer account.
