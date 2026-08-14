# Synabit Android — Google Play Readiness Re-audit

**Ngày audit:** 24/07/2026 (Asia/Ho_Chi_Minh)  
**Phạm vi:** kiến trúc, source code, dữ liệu/sync, security/privacy, Android release engineering, UI/UX, accessibility và Google Play compliance  
**Snapshot:** `main` tại `6c9d8ed` (`origin/main` cùng commit), cộng 4 thay đổi Android chưa commit  
**Phiên bản:** `0.9.3`, `versionCode 9003`  
**Artifact chính:** `src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab`

---

## 1. Kết luận điều hành

### Điểm hiện tại: **40/100**

### Quyết định: **NO-GO — chưa sẵn sàng phát hành Google Play**

Synabit đã tiến một bước rõ rệt so với lần audit 23/07:

- Frontend production build đã chạy thành công.
- AAB release build thành công cho đủ 4 ABI.
- AAB đã được ký bằng release/upload key và xác minh được chữ ký.
- `targetSdk 36` đã đáp ứng cả yêu cầu Google Play có hiệu lực từ 31/08/2026.
- APK/native library vượt kiểm tra alignment 16 KB.
- `npm audit --omit=dev` hiện không báo vulnerability.
- Phần WorkManager/JNI background sync giả đã được loại khỏi working tree.
- CTA mua license ngoài app đã bắt đầu được ẩn trên Android.

Nhưng artifact release hiện tại vẫn không thể coi là release candidate vì có blocker runtime trực tiếp:

> R8 đã xóa `SecureStore.saveSecret()` và `SecureStore.getSecret()` khỏi DEX release, trong khi Rust gọi hai method này bằng tên qua JNI. AAB vẫn build và ký thành công, nhưng secure storage sẽ lỗi ở runtime.

Ngoài blocker trên, updater GitHub vẫn được nhúng và gọi trên Android dù Tauri updater không hỗ trợ Android; cách tự cập nhật ngoài Google Play cũng xung đột trực tiếp với Device and Network Abuse policy. Các quality gate vẫn đỏ: 68 TypeScript errors, 34 ESLint errors, unit test fail, Rust tests không compile và Android lint có 2 errors.

### Mức readiness thực tế

| Mốc | Trạng thái |
|---|---|
| Build được AAB ký release | **Đạt** |
| Upload thử artifact vào Play Console draft | **Có khả năng qua bước ingest kỹ thuật**, chưa xác minh Play Console |
| Internal testing bằng artifact hiện tại | **Chưa nên** |
| Closed/Open testing | **Chưa sẵn sàng** |
| Production rollout | **Không** |

Không nên upload/promote chính AAB hiện tại cho tester vì lỗi R8/SecureStore nằm ngay trong artifact đã ký.

---

## 2. Cách chấm điểm

| Nhóm | Trọng số | Điểm nhóm | Điểm quy đổi |
|---|---:|---:|---:|
| Kiến trúc & platform boundaries | 10% | 55/100 | 5.50 |
| Chất lượng source & tests | 20% | 28/100 | 5.60 |
| Dữ liệu, schema & sync | 15% | 45/100 | 6.75 |
| Security & privacy | 20% | 30/100 | 6.00 |
| Android release engineering | 15% | 60/100 | 9.00 |
| UI/UX & performance | 8% | 55/100 | 4.40 |
| Accessibility | 5% | 18/100 | 0.90 |
| Google Play compliance & operations | 7% | 27/100 | 1.89 |
| **Tổng** | **100%** |  | **40.04 → 40/100** |

Điểm tăng từ **29/100 lên 40/100** chủ yếu nhờ build/AAB/signing/API 36/16 KB/dependency audit. Điểm bị chặn mạnh bởi lỗi release runtime, updater Android, security boundary, privacy/Data Safety và toàn bộ test gates chưa xanh.

---

## 3. Trạng thái source và delta từ lần audit trước

### 3.1 Repository

- Branch: `main`
- HEAD: `6c9d8ed` — `Refactor and update sync mechanism, UI enhancements`
- `origin/main`: cùng HEAD
- Commit mới rất lớn: 136 files, khoảng 5.7k insertions và 6.5k deletions.
- Working tree chưa sạch:

```text
M src-tauri/gen/android/app/build.gradle.kts
M src-tauri/gen/android/app/src/main/java/com/synabit/app/MainActivity.kt
D src-tauri/gen/android/app/src/main/java/com/synabit/app/SyncWorker.kt
M src-tauri/src/jni.rs
```

4 thay đổi này loại:

- WorkManager dependency;
- việc schedule `SyncWorker` trong `MainActivity`;
- `SyncWorker.kt`;
- JNI headless sync no-op.

Đây là hướng đúng cho Android v1 nếu chưa có background engine hoàn chỉnh. Tuy nhiên thay đổi chưa commit, UI vẫn còn copy “Allow background syncing over mobile data”, và release behavior chưa được mô tả/test lại đầy đủ.

### 3.2 Version

- `package.json`: `0.9.3`
- `src-tauri/Cargo.toml`: `0.9.3`
- `tauri.conf.json`: `0.9.3`
- Android: `versionName 0.9.3`, `versionCode 9003`

Chưa có version bump cho release candidate mới. Bất kỳ artifact tiếp theo đưa lên Play sau code fix phải tăng `versionCode`.

---

## 4. Release artifact audit

### 4.1 AAB

```text
File: app-universal-release.aab
Size: 89,216,799 bytes (~85.1 MiB)
SHA-256: e0a66d7f900089d85a147f1907ad3d6735a8506559359b44f42dcbea8bfd087b
Version: 0.9.3 / 9003
minSdk: 24
targetSdk: 36
compileSdk: 36
```

`npx tauri android build --aab --ci` chạy thành công cho:

- `arm64-v8a`
- `armeabi-v7a`
- `x86`
- `x86_64`

AAB chứa bốn native libraries tương ứng. Google Play sẽ split theo ABI nên kích thước AAB 85.1 MiB không đồng nghĩa mỗi user tải toàn bộ bốn ABI.

### 4.2 Signing

Release signing config đã tồn tại và artifact xác minh được:

```text
Certificate DN:
CN=kid0604, OU=Synabit, O=Synabit, L=HCM, ST=HCM, C=VN

Key: RSA 2048
Certificate SHA-256:
4C:58:19:57:EC:3D:93:8E:EE:E4:58:18:6D:3A:E7:FD:
8F:CB:C7:B2:3F:0D:A8:45:58:AE:9D:55:98:00:A9:7D
```

Việc cần làm ngoài source:

- bật Play App Signing;
- backup upload key/keystore và password ngoài máy cá nhân;
- ghi lại certificate fingerprint trong runbook;
- không commit `keystore.properties`, keystore hay password;
- thiết lập signing secrets trong Android CI.

### 4.3 APK release và 16 KB

APK universal release được build thành công:

```text
Size: 199,231,348 bytes
SHA-256: 3c5fa4e08527e30528bb67090f06186dae2ec69ec83a098754c09b81fe81891d
APK Signature Scheme v2: pass
```

Kiểm tra:

- `zipalign -c -P 16 -v 4`: **pass**
- ARM64 ELF `PT_LOAD` alignment: **16384**
- NDK: 29
- Emulator audit đang chạy với `PAGE_SIZE=16384`

Như vậy phần packaging/alignment đáp ứng yêu cầu 16 KB về mặt static artifact. Google yêu cầu app target Android 15+ phải hỗ trợ 16 KB từ 01/11/2025.

### 4.4 Giới hạn smoke-test

Emulator hiện có `com.synabit.app` version 9003 được ký bằng debug key khác release key. Cài đè APK release sẽ fail signature mismatch và muốn tiếp tục phải uninstall app hiện tại, làm mất dữ liệu test.

Audit không xóa dữ liệu emulator. Vì vậy:

- build/sign/alignment: đã xác minh;
- clean install/launch/navigation/sync/rotation/process-death: **chưa được tính pass**;
- cần một AVD sạch hoặc applicationId test riêng trong CI.

---

## 5. Blocker P0

## P0-1 — SecureStore bị R8 xóa method khỏi release DEX

### Bằng chứng

Source Java có:

```java
public static boolean saveSecret(Context context, String key, String value)
public static String getSecret(Context context, String key)
```

Rust gọi bằng dynamic JNI lookup:

```rust
env.call_static_method(&jclass, "getSecret", ...)
env.call_static_method(&jclass, "saveSecret", ...)
```

Nhưng `proguard-rules.pro` chỉ là file template, không có keep rule. R8 không nhìn thấy Java call site nên xem các method là dead code.

Release mapping chỉ còn:

```text
com.synabit.app.SecureStore -> com.synabit.app.SecureStore:
```

Không có method member bên dưới. DEX strings chỉ còn class descriptor:

```text
Lcom/synabit/app/SecureStore;
```

Không còn:

- `saveSecret`
- `getSecret`
- `synabit_secure_secrets`

### Tác động

- Google OAuth token, E2EE password/key, global sync config, vault token và app-lock metadata không đọc/ghi được.
- JNI code có nhiều `.unwrap()`, nên `NoSuchMethodError`/JNI error có thể dẫn tới Rust panic; với release native process, rủi ro app abort là thực tế.
- `App.vue` gọi GDrive auth check và P2P auto reconnect ngay trong startup flow, nên lỗi không chỉ nằm ở một settings screen hiếm dùng.

### Fix bắt buộc

- Thêm `@androidx.annotation.Keep` cho class/method hoặc explicit R8 rules, ví dụ giữ chính xác các JNI entry points.
- Không keep toàn bộ app nếu có thể giữ hẹp.
- Chuyển dynamic JNI bridge thành một contract có test.
- Thay `.unwrap()` bằng typed error propagation và log không chứa secret.
- Sau build, tự động assert trong CI rằng DEX/mapping vẫn có hai method.
- Chạy instrumentation test thật: write → process death → read → migration → corrupted keystore behavior.

### Acceptance criteria

- Mapping/DEX release chứa `saveSecret` và `getSecret`.
- APK release trên clean 16 KB emulator khởi động được.
- Lưu/read token và E2EE secret qua process restart thành công.
- Không panic khi Java method/class lookup lỗi.

---

## P0-2 — GitHub/Tauri auto-updater đang được ship trên Android

### Kết luận trực tiếp

Đúng: updater hiện tại **không dùng được đúng cách trên Android**.

Tauri updater v2 công bố platform support:

| Platform | Support |
|---|---|
| Linux | ✓ |
| Windows | ✓ |
| macOS | ✓ |
| Android | x |
| iOS | x |

Tài liệu chính thức còn hướng dẫn:

- khai báo Rust dependency chỉ cho macOS/Windows/Linux;
- register plugin dưới `#[cfg(desktop)]`.

### Hiện trạng source

- `useAppUpdate.ts` import updater/process vô điều kiện.
- Auto-check chạy 10 giây sau mount trên mọi platform.
- UI có check/download/install/relaunch flow.
- Rust register `tauri_plugin_updater` và `tauri_plugin_process` vô điều kiện.
- Capability Android vẫn có `updater:default`, `process:default`.
- `tauri.conf.json` dùng GitHub `latest.json`.
- Chính config embedded trong AAB có updater active và endpoint GitHub.

### Policy

Google Play Device and Network Abuse policy quy định app phát hành qua Play không được tự modify/replace/update bằng cơ chế khác Play, và không được download executable DEX/JAR/.so từ nguồn khác Play.

Ngay cả khi Tauri call hiện tại chỉ fail “unsupported” trước bước cài, app vẫn:

- ship dead/unsupported update code;
- tự gọi GitHub sau startup;
- hiển thị UX update không thể hoàn tất;
- tăng diện tích capability/process không cần thiết;
- tạo policy-review risk.

### Kiến trúc đúng

**Desktop**

- Giữ Tauri updater + GitHub `latest.json`.
- Cargo dependency/register/plugin/capability chỉ cho desktop.

**Android**

- Dùng Google Play auto-update mặc định; hoặc
- tích hợp Play Core In-App Updates cho flexible/immediate update.
- Không download/cài desktop updater artifact từ GitHub.

### Fix bắt buộc

- Cargo target-specific dependency cho updater và process nếu process chỉ dùng cho updater.
- `#[cfg(desktop)]` khi register plugin.
- Android-specific Tauri config/capability không có updater/process.
- Frontend platform adapter; Android không import/call Tauri updater.
- Ẩn toàn bộ desktop update UI trên Android.
- Không embed GitHub update endpoint trong Android config.
- Privacy Policy Android không mô tả GitHub là update provider sau khi đã loại.

---

## P0-3 — Privacy Policy và Data Safety không khớp behavior

Privacy Policy đã bổ sung Google APIs, GitHub, OSM/OSRM, payment và licensing. Đây là tiến bộ. Tuy nhiên phần cốt lõi vẫn mâu thuẫn:

```text
“Your data is stored on your device, not on our servers.”
“We do not collect, transmit, or store your notes...”
“We do not have access to your data at any time.”
```

Trong source:

- app mặc định trỏ tới `sync.synabit.net:4433`;
- P2P/server adapter gửi encrypted sync payload tới mailbox relay/server;
- app gửi hashed HWID và device name tới license server;
- Google Drive upload/download vault files;
- map/geocoding/routing gọi third-party endpoints;
- updater gọi GitHub;
- license heartbeat/activation gọi `license.synabit.net`.

E2EE có thể làm nội dung không đọc được bởi server, nhưng dữ liệu vẫn được **transmit off device**. Metadata như IP, device name, mailbox activity, timestamps, payload size hoặc identifier vẫn cần đánh giá và disclosure.

Các vấn đề khác:

- Gọi HWID/device name là hoàn toàn “anonymous” và “not personal data” là khẳng định quá mạnh.
- Không mô tả sync relay: loại dữ liệu, encryption, retention, log, subprocessors, deletion.
- Không thấy privacy policy link/text trong UI app, trong khi Play yêu cầu policy có trong app và Play Console.
- Policy ghi cập nhật 10/05/2026 nhưng behavior đã thay đổi mạnh.
- Không có Data Safety artifact/mapping trong repo.
- Không có bằng chứng Data Safety form đã khai báo tổng hợp các behavior trên toàn bộ bản Android.

### Fix bắt buộc

- Viết lại policy theo data flow thực tế, không dùng câu tuyệt đối sai.
- Tách rõ local data, encrypted content, metadata, device identifiers và third-party processors.
- Mô tả relay/license/Google/OSM/OSRM/Nominatim/GitHub theo platform.
- Mô tả retention và deletion cho server-side license/sync metadata.
- Thêm link Privacy Policy dễ tìm trong app.
- Hoàn thành Data Safety form nhất quán với policy và artifact.
- Nếu không có account nhưng có license/device records, phải mô tả đúng quy trình xóa các records đó.

---

## P0-4 — Quality gates vẫn đỏ

| Gate | Kết quả |
|---|---|
| `npm run build` | **PASS** |
| `npm run type-check` | **FAIL — 68 errors** |
| `npm run lint` | **FAIL — 34 errors, 65 warnings** |
| `npm run test:unit -- --run` | **FAIL — 1/3 tests fail** |
| `cargo check --lib` | **PASS — 15 host warnings** |
| Android Rust release compile | **PASS — 27 warnings** |
| `cargo test --lib` | **FAIL TO COMPILE** |
| Android `lintUniversalRelease` | **FAIL — 2 errors, 34 warnings, 1 hint** |
| `npm audit --omit=dev` | **PASS — 0 vulnerabilities** |
| `git diff --check` | **PASS** |
| `cargo audit` | Không chạy — tool chưa cài |
| `cargo deny` | Không chạy — tool chưa cài |

Chi tiết:

- Unit test `NoteApp` fail vì `$t is not a function`.
- Rust tests fail vì `DocSyncPayload` không nằm trong scope ở test code.
- Android lint:
  - `MissingTvBanner`
  - `ImpliedTouchscreenHardware`

Một production build pass không thay thế type/test/lint gates. Với refactor sync rất lớn, test Rust còn không compile là blocker trực tiếp.

---

## P0-5 — Security boundary quá rộng so với HTML rendering

Capabilities hiện cấp:

- window `"*"`
- `fs:write-all`
- filesystem read `**`
- filesystem write `**`
- opener path `**`
- opener URL
- asset protocol scope `**`
- updater/process

Trong cùng webview, app render HTML bằng `v-html`. Một số flow có DOMPurify, nhưng ít nhất:

- Feed `ArticleReader.vue` render trực tiếp `article.content`/summary từ remote feed.
- `ProjectDashboard.vue` render `activeProject.content` trực tiếp.

Security của Tauri cần đánh giá theo chuỗi:

```text
untrusted/synced HTML
→ webview origin
→ Tauri IPC/plugin permissions
→ broad filesystem/opener capability
```

CSP hiện là một lớp giảm rủi ro, nhưng không phải lý do để bỏ sanitize và least privilege.

### Fix bắt buộc

- Sanitize tất cả HTML tại một trusted renderer duy nhất.
- Tách remote feed HTML khỏi privileged app context nếu có thể.
- Chặn event handlers, dangerous URLs, forms/iframes và unexpected SVG.
- Thay path `**` bằng app-data/vault scopes cụ thể.
- Tách capability desktop và mobile.
- Không cấp updater/process trên Android.
- Thêm negative security tests cho malicious feed/project/synced content.

---

## P0-6 — Monetization/Play Billing chưa chốt an toàn

Điểm tích cực: `LicenseModal` đã dùng `v-if="osType !== 'android'"` cho:

- trial CTA;
- divider;
- link “Purchase one here”.

Nhưng `osType` khởi tạo là chuỗi rỗng và được resolve async. Vì điều kiện là “không phải Android”, CTA desktop có thể xuất hiện trong khoảng đầu render; nếu OS plugin fail, `osType` vẫn rỗng và CTA có thể tồn tại lâu dài. Fallback chỉ set `isMobileOS`, không set `osType`.

App vẫn cho nhập external license key và Pro là app functionality/digital service. Điều này có thể phù hợp với consumption-only model nếu app Android không dẫn người dùng đi mua, nhưng phải chốt mô hình và tuân theo region/program hiện hành. Nếu muốn bán/nâng cấp trực tiếp trong app, phải dùng Play Billing hoặc chương trình alternative/external billing hợp lệ theo quốc gia.

### Fix bắt buộc

- Dùng positive desktop gate: chỉ render purchase/trial CTA khi platform đã resolve và là macOS/Windows/Linux.
- Android build không chứa link/copy dẫn tới external purchase nếu chưa tham gia chương trình tương ứng.
- Chốt một trong:
  - free Android;
  - consumption-only/existing entitlement;
  - Play Billing;
  - region-aware approved alternative billing program.
- Store listing và reviewer instructions phải khớp.
- Cung cấp review account/license nếu reviewer cần mở khóa core flows.

---

## 6. Findings P1

### P1-1 — SecureStore tự xóa toàn bộ secrets khi gặp bất kỳ exception

`getEncryptedPrefs()` catch mọi exception rồi gọi:

```java
context.deleteSharedPreferences(PREFS_FILENAME);
```

Điều này có thể xóa OAuth token, E2EE secret, sync config, vault tokens và app-lock metadata khi gặp:

- transient Android Keystore failure;
- restore sang thiết bị khác;
- key invalidation;
- malformed/corrupted preferences;
- vendor-specific crypto issue.

Không được biến mọi lỗi read/decrypt thành silent destructive recovery. Cần phân loại exception, surface recoverable state, yêu cầu re-auth/recovery có chủ đích và không xóa E2EE material mà không có cảnh báo.

### P1-2 — Migration plaintext secret vẫn có data-loss window

Migration gọi `saveSecret`, bỏ qua boolean/result rồi xóa file plaintext ngay:

```rust
let _ = env.call_static_method(... "saveSecret" ...);
let _ = std::fs::remove_file(path);
```

Nếu encrypted save trả `false`, dữ liệu gốc vẫn bị xóa. Chỉ xóa file cũ sau khi:

1. save trả true;
2. đọc lại;
3. parse và constant-time/semantic compare thành công.

### P1-3 — Android backup/restore chưa được định nghĩa

Manifest không có:

- `android:allowBackup`
- `android:dataExtractionRules`
- `android:fullBackupContent`

Với encrypted preferences/Keystore, backup data nhưng không backup được key có thể gây restore corruption và kích hoạt code xóa secrets. Cần explicit backup rules, exclude secure prefs/tokens/private DB phù hợp và test restore.

### P1-4 — AndroidX Security Crypto đang dùng alpha cũ

Dependency:

```text
androidx.security:security-crypto:1.1.0-alpha06
```

Android lint cảnh báo stable version tồn tại. Cần đánh giá migration lên stable API hoặc một Android Keystore design được support dài hạn.

### P1-5 — Database migrations chưa có version ledger

`schema.rs` ghi “Runs all migrations” nhưng không thấy:

- `PRAGMA user_version`;
- numbered transactional migrations;
- rollback/recovery policy;
- schema compatibility tests.

Đa phần logic dựa vào `CREATE TABLE IF NOT EXISTS`, flags trong `kv_store` và best-effort operations. Đây không đủ cho app có nhiều loại dữ liệu và sync.

Cũng không thấy `PRAGMA foreign_keys=ON`. Cần:

- migration ledger;
- transaction cho mỗi migration;
- backup trước destructive migration;
- upgrade tests từ mọi schema đã phát hành;
- foreign key strategy rõ ràng;
- corruption/rebuild tests cho FTS/cache.

### P1-6 — Sync refactor chưa có bằng chứng end-to-end

Kiến trúc coordinator/adapters/core là hướng tốt hơn trước, nhưng:

- Rust tests hiện không compile;
- chưa có E2E test multi-device;
- chưa có process-death/offline/conflict/idempotency tests;
- background worker bị gỡ nhưng UI/copy chưa đồng bộ;
- privacy chưa mô tả relay.

Các case tối thiểu:

- concurrent edits;
- duplicate/reordered messages;
- disconnect/reconnect;
- key rotation/revoked device;
- large asset;
- partial write;
- process death;
- app foreground/background;
- metered/unmetered behavior;
- relay unavailable.

### P1-7 — Background sync UX vẫn gây hiểu nhầm

Foreground timer chỉ chạy khi `document.visibilityState === 'visible'` và bị dừng khi app background. Nhưng UI ghi:

```text
Allow background syncing over mobile data
```

Sau khi bỏ WorkManager, copy đúng nên là foreground/when-app-is-open sync. Nếu cần background sync thật, phải thiết kế lại bằng Android scheduling constraints, secure credential access, battery/data policy và headless sync engine có test.

### P1-8 — Android TV declaration không hoàn chỉnh

Manifest khai báo Leanback launcher nhưng:

- thiếu TV banner;
- không khai báo touchscreen optional.

Nếu không support Android TV, xóa Leanback category/feature. Nếu support, phải làm đủ UX remote/D-pad, banner, focus navigation, orientation và TV testing.

### P1-9 — FileProvider expose external storage root

`file_paths.xml`:

```xml
<external-path name="my_images" path="." />
```

Provider không exported và dùng URI grants, nhưng root `.` rộng hơn nhu cầu. Hạn chế vào app-specific/cache/export directory cụ thể.

### P1-10 — Không có Android CI/release gate

Workflows hiện chỉ build desktop macOS/Windows/Linux. Không có job:

- type-check/lint/test;
- Android AAB release;
- Android lint;
- signing verification;
- R8/JNI keep assertion;
- 16 KB alignment;
- clean-emulator smoke;
- artifact hash/upload.

Một app đa nền tảng không nên phụ thuộc build release thủ công trên một máy.

### P1-11 — Google OAuth client secret vẫn là compile-time constant chung

`CLIENT_SECRET` dùng `env!` ở module chung và Android build báo constant không dùng. Desktop OAuth client secret nhúng trong distributed binary không thể là secret thực sự. Android path cần native/public-client PKCE configuration riêng và code/config phải được `cfg` theo platform để không vô tình ship desktop credential material.

`drive.readonly` là scope rộng/restricted; cần Google verification/justification phù hợp hoặc giảm scope.

### P1-12 — Accessibility vẫn rất thấp

Static inventory:

```text
<button>: 725
aria-label: 138
aria-label="More Options": 51
role="dialog" / aria-modal: 0
<img>: 29
alt=: 10
```

Nhiều label là mechanical/internal-state wording, ví dụ “More Options” gắn cho nút close hoặc label kiểu `Show Settings Modal = false`. Modal chưa có dialog semantics, focus trap, restore focus và escape behavior nhất quán.

Cần:

- semantic labels theo action;
- icon buttons có accessible name đúng;
- dialog role/title/focus trap;
- 48dp touch targets;
- keyboard/D-pad navigation;
- TalkBack test;
- dynamic type/font scaling;
- contrast và disabled/error state;
- landscape/tablet/foldable checks.

### P1-13 — Frontend performance/bundle

Build pass nhưng có chunk lớn:

| Chunk | Minified | Gzip |
|---|---:|---:|
| Tiptap editor | 1,568.77 kB | 502.34 kB |
| NoteApp | 1,059.74 kB | 304.64 kB |
| PDF worker | 1,232.30 kB | — |
| Mermaid core | 617.42 kB | 146.07 kB |
| Wardley | 615.46 kB | 147.95 kB |

Vite còn báo static + dynamic import không tách chunk như kỳ vọng. Trên Android WebView/RAM thấp, cần đo cold start, memory và interaction latency; lazy-load editor/PDF/diagram engines thực sự.

### P1-14 — Store assets và Play Console state chưa có bằng chứng

Repo có icon nhưng không thấy bộ:

- phone/tablet screenshots;
- feature graphic;
- store listing copy;
- Data Safety mapping;
- content rating answers;
- target audience/ads declaration;
- app access/reviewer instructions;
- pre-launch report;
- closed testing evidence.

Những mục này có thể tồn tại ngoài repo, nhưng chưa được cung cấp nên không thể tính là hoàn thành.

---

## 7. Kiến trúc tổng thể

### Điểm tốt

- Tauri/Rust core cho phép chia sẻ business logic đa nền tảng.
- Sync đã tiến về coordinator/core/adapters thay vì gọi rời rạc.
- Local-first và E2EE là hướng sản phẩm phù hợp.
- Android cleartext traffic release bị tắt.
- Secrets Android chuyển sang encrypted preferences + Android Keystore và synchronous `commit()`.
- Build đã bật R8/minification cho release.

### Vấn đề kiến trúc chính

Platform boundary chưa đủ chặt:

- desktop updater chạy vào Android build;
- desktop OAuth credential/config nằm trong module chung;
- capabilities dùng schema/default chung quá rộng;
- frontend phân nhánh platform bằng async runtime string thay vì compile-time adapter;
- mobile background lifecycle và desktop timer cùng tồn tại trong composable lớn.

Kiến trúc nên có explicit adapters:

```text
UpdateService
├── DesktopTauriUpdater
└── AndroidPlayUpdate / NoOp

SecretStore
├── DesktopKeyring
├── AndroidKeystore
└── iOSKeychain

SyncScheduler
├── DesktopForegroundScheduler
└── AndroidForegroundOnly hoặc AndroidBackgroundScheduler

Billing/Entitlement
├── DesktopLicense
└── AndroidPlay/ConsumptionOnly
```

Mỗi adapter cần compile-time selection, permission/capability riêng và test contract chung.

---

## 8. UI/UX assessment

### Tốt

- App có breadth chức năng mạnh và design language khá nhất quán.
- Mobile layout detection đã có OS + screen-size fallback.
- Foreground/background lifecycle sync chủ động giảm connection khi app ẩn.
- License UI đã bắt đầu nhận biết Android.
- Settings có cellular policy và local sync metrics.

### Chưa release-ready

- License modal có thể flash CTA desktop trước khi platform resolve.
- Update UI trên Android là dead-end.
- Background sync wording không khớp behavior.
- Error states thường log console hoặc generic string, chưa có recovery UX rõ.
- Modal semantics/focus chưa đạt.
- 51 nút có cùng accessible label “More Options”.
- App có nhiều feature/chunk nặng; chưa có bằng chứng test Android low/mid-tier.
- Không có evidence rotation, back gesture, process recreation, keyboard inset, tablet/foldable hoặc TalkBack.

---

## 9. Google Play policy/readiness matrix

| Hạng mục | Trạng thái | Nhận xét |
|---|---|---|
| AAB | Pass kỹ thuật | Build thành công |
| Release signing | Pass local | Cần Play App Signing/runbook |
| Target API | Pass | API 36 |
| 16 KB | Pass static | Zip/ELF pass; clean runtime smoke còn thiếu |
| Versioning | Chưa | Phải bump > 9003 cho artifact mới |
| Self-update policy | Fail | GitHub/Tauri updater vẫn ship Android |
| Payments | Conditional/Fail | Gate async chưa an toàn; model chưa chốt |
| Privacy policy | Fail | Mâu thuẫn sync/data flows; thiếu in-app link |
| Data Safety | Chưa xác minh | Không có artifact/Play Console evidence |
| App access | Chưa xác minh | License-gated flow cần reviewer access |
| Content rating/target audience | Chưa xác minh | Play Console |
| Android lint | Fail | 2 errors |
| Runtime smoke | Chưa pass | Không uninstall app debug trên emulator |
| Pre-launch report | Chưa có | Cần Play testing |
| Accessibility | Fail chất lượng | Coverage/semantics thấp |
| Dependency audit JS | Pass | 0 vulnerabilities |
| Rust supply-chain audit | Chưa | cargo-audit/deny chưa chạy |

### Target API

Google Play yêu cầu app mới/update target Android 16/API 36 từ 31/08/2026. Synabit đã ở API 36, nên phần này sẵn sàng.

---

## 10. Kế hoạch đưa điểm lên mức phát hành

## Phase 0 — Làm artifact có thể chạy (bắt buộc)

1. Fix R8 keep cho SecureStore/JNI.
2. Bỏ `.unwrap()` khỏi JNI secret bridge.
3. Fix migration save/verify/delete.
4. Không auto-delete secure preferences trên mọi exception.
5. Build AAB/APK mới, tăng `versionCode`.
6. Clean install trên AVD 16 KB và device thật.

**Mục tiêu:** từ 40 lên khoảng 48–52.

## Phase 1 — Loại policy blockers

1. Compile-gate Tauri updater cho desktop.
2. Android dùng Play update/no updater UI.
3. Chốt billing/consumption-only model.
4. Positive-gate desktop purchase CTA.
5. Viết lại Privacy Policy và thêm in-app link.
6. Hoàn thành Data Safety.
7. Giảm capabilities và sanitize HTML.

**Mục tiêu:** khoảng 60–68, đủ cân nhắc internal/closed testing.

## Phase 2 — Làm xanh quality gates

1. 0 type errors.
2. 0 lint errors.
3. Unit tests pass; tăng coverage cho stores/composables.
4. Rust tests compile và pass.
5. Android lint 0 errors.
6. cargo-audit/cargo-deny.
7. Android CI có signed test artifact, 16 KB và R8 assertions.

**Mục tiêu:** khoảng 72–80.

## Phase 3 — Product QA và Play operations

1. Sync E2E multi-device/conflict/offline/process-death.
2. Accessibility/TalkBack.
3. Low/mid-tier performance.
4. Tablet/foldable/rotation/back navigation.
5. Store assets, app access, content rating, pre-launch report.
6. Internal → closed → staged production rollout.

**Mục tiêu production:** tối thiểu 85/100 và không còn P0/P1 release blocker.

---

## 11. Checklist “ready for internal testing”

- [ ] SecureStore methods tồn tại trong release DEX.
- [ ] Clean install/launch trên 16 KB emulator.
- [ ] Secret write/read qua process death.
- [ ] Android updater GitHub bị loại hoàn toàn.
- [ ] External purchase CTA không bao giờ xuất hiện trên Android ngoài program hợp lệ.
- [ ] Privacy Policy đúng data flow và có link trong app.
- [ ] Type-check/lint/unit/Rust/Android lint pass.
- [ ] Sync manual/foreground smoke pass.
- [ ] Backup/restore behavior được định nghĩa.
- [ ] VersionCode tăng.
- [ ] Android CI lưu artifact/hash/mapping.

## 12. Checklist “ready for production”

- [ ] Tất cả checklist internal.
- [ ] Data Safety hoàn thành và khớp policy.
- [ ] Play App Signing/upload key backup.
- [ ] Store listing/screenshots/feature graphic.
- [ ] App access/reviewer credentials.
- [ ] Content rating/target audience/ads declarations.
- [ ] Pre-launch report không có crash/ANR blocker.
- [ ] Closed testing đủ device/API/ABI.
- [ ] TalkBack và accessibility pass.
- [ ] Sync E2E/conflict/offline pass.
- [ ] Staged rollout + crash/ANR monitoring + rollback runbook.

---

## 13. Nguồn policy/chính thức

- [Google Play target API requirements](https://support.google.com/googleplay/android-developer/answer/11926878?hl=en-GB_ALL)
- [Google Play Device and Network Abuse](https://support.google.com/googleplay/android-developer/answer/16559646?hl=en)
- [Google Play Payments policy](https://support.google.com/googleplay/android-developer/answer/9858738?hl=en)
- [Google Play Data Safety](https://support.google.com/googleplay/android-developer/answer/10787469?hl=en)
- [Google Play User Data / Privacy Policy](https://support.google.com/googleplay/android-developer/answer/10144311?hl=en)
- [Android 16 KB page-size support](https://developer.android.com/guide/practices/page-sizes)
- [Android Play Core In-App Updates](https://developer.android.com/guide/playcore/in-app-updates)
- [Tauri updater guide](https://v2.tauri.app/plugin/updater/)
- [Tauri updater supported platforms](https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/updater)

---

## 14. Final verdict

Synabit Android hiện đã có **release pipeline artifact ở mức sơ khai tốt hơn hẳn**: build được, ký được, target API 36 và 16 KB pass. Nhưng “build được AAB” chưa đồng nghĩa “AAB chạy đúng” hay “Play compliant”.

Blocker lớn nhất hiện tại là artifact release tự làm mất JNI methods của SecureStore do R8. Tiếp theo là updater GitHub trên Android, privacy/Data Safety sai với data flow, quality gates đỏ, capabilities quá rộng và billing gate chưa chắc chắn.

**Điểm: 40/100.**  
**Google Play production: NO-GO.**  
**Khuyến nghị:** chưa upload/promote AAB hiện tại; xử lý Phase 0 và Phase 1 trước, sau đó audit lại chính artifact mới trên clean 16 KB emulator và Play internal testing.
