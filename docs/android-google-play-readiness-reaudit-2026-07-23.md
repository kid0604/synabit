# Synabit Android — Re-audit Google Play Readiness

**Ngày re-audit:** 23/07/2026  
**Snapshot:** branch `main`, commit nền `332309e` / tag `v0.9.3`  
**Version:** `0.9.3`, versionCode `9003`  
**Package:** `com.synabit.app`  
**Kết luận:** **NO-GO — chưa thể upload một release hợp lệ lên Google Play**  
**Điểm readiness:** **29/100**

> Đây là re-audit của working tree hiện tại, không phải chứng nhận của Google. Điểm được chấm theo khả năng ship an toàn, build lặp lại và vượt review; không chấm theo số lượng code đã viết.

## 1. Kết luận điều hành

Synabit Android đang ở mức **late alpha / stabilization**, chưa đạt internal release candidate:

- Có tiến triển thật ở Android secure storage và việc thêm accessible names.
- Nhưng production frontend build hiện bị phá bởi markup không hợp lệ.
- Không tạo được AAB; không có release artifact để upload hoặc chạy Play pre-launch report.
- Quality gates chính đều đỏ: TypeScript, ESLint, frontend unit test, Rust test và Android Lint.
- Background sync Android vẫn không hoạt động.
- SecureStore mới chưa an toàn ở release/minified build và migration có khả năng làm mất secret.
- Privacy policy, Data Safety và luồng thanh toán vẫn không khớp hành vi thực tế.
- Snapshot là một working tree rất lớn, chưa commit: **118 tracked files thay đổi/xóa + 10 untracked files**, với khoảng **2.072 dòng thêm và 7.326 dòng xóa**.

**Trả lời ngắn:** chưa sẵn sàng lên Google Play. Thậm chí chưa nên đưa bản hiện tại lên Internal testing, vì không build được AAB và artifact debug còn lại không đại diện cho source mới nhất.

## 2. Thay đổi so với audit 22/07/2026

Điểm trước là **32/100**; điểm hiện tại là **29/100**.

| Hạng mục | Trạng thái mới | Đánh giá |
|---|---|---|
| Android Keystore | Đã thêm `SecureStore.java` và đường JNI từ Rust | Đúng hướng, nhưng chưa release-safe |
| Accessibility | `aria-label` tăng từ 0 lên 137 | Có nỗ lực, nhưng phần lớn được sinh máy móc, nhiều label sai nghĩa |
| Lint/typecheck scripts | Đã có `npm run lint` và `npm run type-check` | Tốt cho quy trình, nhưng cả hai đang fail |
| Rust compile | `cargo check` vẫn pass, warnings giảm 27 → 15 | Tiến bộ |
| Frontend production build | Từ pass thành **fail cú pháp** | Regression nghiêm trọng |
| Android Lint | Vẫn 2 errors; warnings tăng 34 → 35 | Chưa cải thiện gate |
| AAB | Trước bị chặn ở toolchain/plugin; nay bị chặn sớm ở frontend | Vẫn không có artifact |
| Background sync | Không thay đổi bản chất | Vẫn no-op và load sai native library |
| Privacy/billing/capabilities | Không có fix đủ điều kiện đóng blocker | Vẫn P0 |

Việc thêm secure storage và accessibility không đủ bù regression build. Do đó readiness giảm 3 điểm.

## 3. Scorecard

Điểm tổng là weighted score, ưu tiên build, security, data integrity và Android runtime.

| Hạng mục | Điểm | Trạng thái | Kết luận |
|---|---:|---|---|
| Kiến trúc & maintainability | 48/100 | Rủi ro cao | Stack tốt nhưng refactor sync lớn, boundary chưa ổn định, working tree quá rộng |
| Source quality & testing | 10/100 | Blocker | Build/typecheck/lint/unit/Rust tests đều đỏ |
| Data integrity & migrations | 45/100 | Rủi ro cao | Nền tảng SQLite/CRDT tốt; migration/recovery/source-of-truth chưa đủ |
| Security & privacy | 28/100 | Blocker | SecureStore là tiến bộ, nhưng migration/R8/backup/capabilities/privacy vẫn nguy hiểm |
| Android engineering | 22/100 | Blocker | Không có AAB, background worker hỏng, lint fail, thiếu signing pipeline |
| UI/UX | 52/100 | Cần cải thiện | Visual cũ khá tốt; snapshot hiện tại không render được để nghiệm thu |
| Accessibility | 17/100 | Blocker chất lượng | Có label nhưng sai semantics; chưa có dialog/focus/TalkBack proof |
| Google Play compliance | 21/100 | Blocker | Billing, privacy/Data Safety, artifact/signing và declarations chưa đạt |

**Điểm tổng: 29/100 — NO-GO.**

## 4. Evidence từ các quality gate

| Gate | Kết quả hiện tại | Mức độ |
|---|---|---|
| `npm run build` | **Fail** tại `WhiteboardToolbar.vue:193`, `Illegal '/' in tags` | P0 |
| `npx tauri android build --aab --target aarch64 --ci` | **Fail** vì `beforeBuildCommand` fail cùng lỗi trên | P0 |
| `npm run type-check` | **Fail: 68 TypeScript errors** | P0 |
| `npm run lint` | **Fail: 100 problems — 35 errors, 65 warnings** | P0 |
| `npm run test:unit -- --run` | **Fail: 1/3 tests**; `NoteApp` thiếu mock `$t` | P0 |
| `cargo check --manifest-path src-tauri/Cargo.toml --lib` | Pass, 15 warnings | Pass có cảnh báo |
| `cargo test ... --lib` | **Fail compile**; thiếu `DocSyncPayload` trong test `sync/core/apply.rs` | P0 |
| `./gradlew :app:lintArmDebug --no-daemon` | **Fail: 2 errors, 35 warnings, 1 hint** | P0 |
| `npm audit --omit=dev` | **5 vulnerabilities: 3 high, 2 moderate** | P1/High |
| `git diff --check` | **Fail** do trailing whitespace | P1 |
| Signed release AAB | Không tồn tại | P0 |
| Android CI | Không có release gate đầy đủ | P0 |

### 4.1 TypeScript errors không chỉ là unused code

Các lỗi đáng chú ý:

- `setTimeout` được gọi trên object không có property tương ứng.
- `ReadableStream` async iterator không khớp target/type.
- `SynSettings` thiếu `num_ctx` và `max_history_messages`.
- Tiptap Details/Location/Whiteboard component type mismatch.
- Quick Capture dùng `NodeType` không tương thích.
- Settings tab `"license"` không nằm trong union type; `ConfirmModal` thiếu prop `show`.
- Whiteboard edge labels không khớp model.

Do Vite chủ yếu transpile, production build không thể thay thế `vue-tsc` như một type safety gate.

### 4.2 Frontend markup regression

Ít nhất ba lỗi được tạo trong đợt sửa accessibility:

- `WhiteboardToolbar.vue:193` có fragment `/ aria-label="$emit">`, làm Vite dừng build.
- `ImageFileViewer.vue:73` có `@load=""`.
- Dynamic slot ở `App.vue` gây Vue parser errors trong ESLint.

Đây là dấu hiệu thay đổi accessibility đã được áp dụng cơ học nhưng chưa được review hoặc chạy gate sau mỗi batch.

## 5. Release artifact và build engineering

### 5.1 Không có artifact có thể upload

Trong build outputs chỉ có:

- `app-arm64-debug.apk`
- Kích thước: **966.706.589 bytes, khoảng 922 MiB**
- Thời điểm: 22/07/2026 23:33
- ABI: chỉ arm64 ở artifact này
- Native library: `libsynabit_lib.so`, khoảng 481 MiB do debug symbols

Không tìm thấy `.aab`.

Bản debug APK này được tạo trước các thay đổi cuối làm hỏng frontend, nên không đại diện cho source đang audit. Không được dùng nó để kết luận release hiện tại chạy được.

### 5.2 16 KB page-size

`zipalign -P 16 -c -v 4` pass trên APK debug còn lại. Đây là tín hiệu tốt, nhưng chỉ là **conditional evidence**:

- App có native Rust library, nên final release phải kiểm tra cả ZIP alignment và ELF LOAD segment alignment.
- Cần kiểm lại trên signed release AAB/APK do bundletool sinh.
- Google yêu cầu app/update target Android 15+ support 16 KB trên thiết bị 64-bit từ 01/11/2025.

Tham khảo: [Android 16 KB page-size guide](https://developer.android.com/guide/practices/page-sizes).

### 5.3 Signing và Play App Signing

`build.gradle.kts` chưa có `signingConfigs`/`signingConfig`. Repo chưa có pipeline:

- Tạo/đọc upload key từ CI secret store.
- Sign và verify AAB.
- Upload Internal track.
- Register Play App Signing SHA-1/SHA-256 với Google OAuth/deep links.
- Retain mapping file/native symbols cho crash deobfuscation.

Google Play yêu cầu upload bundle được ký bằng upload key và khuyến nghị Play App Signing: [official guide](https://support.google.com/googleplay/android-developer/answer/9842756?hl=en).

## 6. Kiến trúc và maintainability

### 6.1 Điểm tốt

- Vue 3 + TypeScript + Tauri 2 + Rust là stack hợp lý cho sản phẩm đa nền tảng.
- Rust giữ filesystem/database/sync logic ngoài renderer.
- SQLite có WAL, FTS5, indexes và CRDT compaction.
- Sync crypto dùng primitive hiện đại như XChaCha20-Poly1305/Argon2 và có zeroization ở một số path.
- CSP không cho inline script/eval; Android release đặt cleartext traffic false.
- Refactor sync mới đang hướng tới coordinator/adapter/core rõ hơn.

### 6.2 Vấn đề

#### ARCH-01 — Snapshot không phải release branch ổn định — P0

Working tree có 128 entry thay đổi, trong đó 10 file untracked. SecureStore và một số file refactor quan trọng còn untracked. Một build từ máy khác hoặc clean checkout sẽ không chứa đầy đủ implementation đang được đánh giá.

**Yêu cầu đóng:** clean branch/PR, review theo từng concern, CI chạy trên clean checkout, tag immutable.

#### ARCH-02 — App shell và feature components quá lớn — P1

`App.vue`, Settings, Quick Capture, Note/Whiteboard/Finance và nhiều Rust command module đang sở hữu quá nhiều state/lifecycle. Điều này làm những batch edit xuyên hàng trăm component dễ gây regression như hiện tại.

**Khuyến nghị:** tách shell services (`Vault`, `Sync`, `License`, `Update`), typed feature boundary và component nhỏ hơn; tránh batch regex sửa UI mà không AST/template validation.

#### ARCH-03 — IPC và state còn stringly typed — P1

Frontend gọi Tauri command/event qua string, một số path dùng `any`; model Settings/Whiteboard/TipTap đang drift. Cần typed IPC gateway hoặc generated bindings và contract tests.

#### ARCH-04 — Hai thế hệ sync chưa được ổn định — P0

Repo đang chuyển từ P2P/composable cũ sang coordinator/adapter mới, trong khi UI, README và Android worker vẫn dùng thuật ngữ/cấu hình cũ. Rust host check pass không chứng minh Android/headless path hoạt động.

## 7. Android secure storage — tiến bộ nhưng chưa đóng blocker

### 7.1 Điều đã làm đúng

`SecureStore.java` dùng:

- `MasterKey` AES-256-GCM.
- `EncryptedSharedPreferences`.
- AES256-SIV cho key name và AES256-GCM cho value.
- Rust Android path gọi Java secure store qua JNI.
- Có migration từ `synabit_secrets.json` cũ.

Đây là cải thiện rõ rệt so với plaintext JSON.

### 7.2 SEC-01 — R8 có thể rename/remove `SecureStore` — P0

Release bật `isMinifyEnabled = true`. Rust chỉ biết class và methods qua tên string:

`com.synabit.app.SecureStore`, `getSecret`, `saveSecret`.

ProGuard hiện chỉ keep `TauriActivity`; không có rule cho `SecureStore`. R8 không thấy Java caller trực tiếp và có thể rename/remove class hoặc methods, làm release JNI lookup fail.

**Fix:** thêm keep rule chính xác cho class/static methods; build minified release; instrumentation test đọc/ghi/migrate trên APK do bundletool sinh.

### 7.3 SEC-02 — Migration có cửa sổ mất toàn bộ secrets — P0

`saveSecret()` dùng `SharedPreferences.apply()`, là ghi bất đồng bộ, rồi trả `true`. Rust sau đó xóa ngay plaintext file mà:

- Không kiểm boolean result của JNI migration.
- Không đọc lại encrypted value.
- Không chờ dữ liệu persist.

Nếu process chết/crash trong cửa sổ này, user có thể mất OAuth token, E2EE key, sync config và PIN/protection metadata.

**Fix:** dùng durable commit hoặc storage transaction tương đương; chỉ xóa bản cũ sau save success + readback + parse + fsync/atomic marker; giữ recovery copy có lifecycle rõ; test kill-process tại mọi migration step.

### 7.4 SEC-03 — JNI `unwrap()` có thể làm native process panic — P0

Đường load/save dùng nhiều `unwrap()` cho VM attach, class loader, class/method lookup, Java string và call result. Keystore invalidation, R8, OEM/provider error hoặc Java exception có thể biến thành native crash thay vì typed error.

`getSecret()` phía Java còn biến mọi exception thành chuỗi rỗng, không phân biệt:

- Chưa có secret.
- Keystore hỏng/invalidate.
- Encrypted prefs không đọc được.
- Class/API runtime failure.

**Fix:** trả result có error code; clear pending Java exception; không panic qua FFI; có recovery UX thay vì tự tạo secrets mới và làm mất khả năng decrypt sync data.

### 7.5 SEC-04 — Backup/restore chưa được thiết kế — P0

Manifest chưa khai báo:

- `android:allowBackup`
- `android:dataExtractionRules`
- `android:fullBackupContent`

EncryptedSharedPreferences không tự giải quyết restore: preference ciphertext có thể được restore nhưng Android Keystore key không đi cùng. Điều này gây secret không đọc được sau restore/device transfer.

**Fix:** loại secure prefs, key material và sensitive DB/cache khỏi backup; hoặc thiết kế recovery key rõ ràng. Test cloud restore và device-to-device transfer trên API mới/cũ.

### 7.6 SEC-05 — Dependency security-crypto cũ — P1

App dùng `androidx.security:security-crypto:1.1.0-alpha06`; Android Lint báo stable `1.1.0` đã có. Implementation mới còn untracked và chưa có Android instrumentation test.

Ít nhất cần dùng stable dependency, đánh giá API deprecation/migration path và khóa behavior bằng test. Với thiết kế lâu dài, có thể cân nhắc direct Keystore + versioned AES-GCM envelope thay vì phụ thuộc toàn blob vào EncryptedSharedPreferences.

## 8. Background sync Android vẫn không hoạt động

### AND-01 — Load sai native library — P0

`SyncWorker.kt` gọi:

`System.loadLibrary("synabit")`

Nhưng Cargo `[lib]` tên `synabit_lib` và APK chứa:

`lib/arm64-v8a/libsynabit_lib.so`

Static initializer log lỗi rồi tiếp tục. Khi gọi native method, `UnsatisfiedLinkError` không phải `Exception`, nên `catch (Exception)` trong `doWork()` không đảm bảo bắt được.

### AND-02 — JNI implementation là no-op — P0

`Java_com_synabit_app_SyncWorker_runHeadlessSync` chỉ:

- Đọc ba string.
- Tạo Tokio runtime.
- Log server address.
- Có comment `Headless sync logic to be implemented`.

Không gọi sync coordinator, không load E2EE/device credentials, không push/pull và không trả typed result. Kotlin vẫn log “completed successfully” và trả `Result.success()`.

### AND-03 — Schedule không khớp setting — P0

`MainActivity` luôn enqueue periodic work 15 phút khi activity được tạo:

- Không kiểm user đã bật sync chưa.
- Không cancel khi disconnect/disable.
- UI cho 1–60 phút và foreground default 5 phút.
- WorkManager periodic minimum là 15 phút.
- Không phản ánh network/cellular/battery policy từ user setting.

**Quyết định release thực dụng:** hoặc implement và test end-to-end thật, hoặc tắt hoàn toàn background worker/schedule/copy trong Android v1. Không ship một worker báo success nhưng không sync.

## 9. Security boundary

### SEC-06 — Tauri capabilities quá rộng — P0/High

`capabilities/default.json` cấp cho window `*`:

- `fs:write-all`
- `fs:allow-read` path `**`
- `fs:allow-write` path `**`
- `opener:allow-open-path` path `**`
- updater/process permissions

`tauri.conf.json` còn bật asset protocol scope `**`.

Nếu renderer bị XSS hoặc dependency compromise, attacker có thể vượt qua path validation ở Rust command layer. Đây là rủi ro đặc biệt cao vì app render RSS/Markdown/preview content.

**Fix:** capability riêng cho Android; bỏ write-all/process/updater; chỉ expose app-data và user-approved vault; mọi write/delete đi qua audited Rust commands; asset scope chỉ đúng preview roots.

### SEC-07 — Untrusted HTML vẫn có đường không sanitize — P0/High

`ArticleReader.vue` render:

`v-html="article.content || article.summary || ''"`

Ngoài ra còn nhiều `v-html` path trong editor/preview. DOMPurify có được dùng ở một số chỗ, nhưng không có sanitizer boundary bắt buộc cho mọi untrusted source.

Kết hợp remote HTML + wildcard native capabilities là một trust-boundary failure.

**Fix:** centralized sanitizer allowlist; strip event handlers, form, iframe, unsafe URL/style; test XSS/mXSS corpus; content preview không được có quyền native filesystem.

### SEC-08 — Dependency advisories — P1/High

`npm audit --omit=dev` báo 5 vulnerabilities:

- 3 high.
- 2 moderate.
- Bao gồm `dompurify`, `linkify-it`, `markdown-it`, `undici`, `vite`.
- Fix available.

Không nên waive các advisory liên quan HTML/network/toolchain khi renderer có native bridge rộng.

### SEC-09 — FileProvider scope quá rộng — P1

`file_paths.xml` expose `<external-path path="." />`. Provider không exported, nhưng URI grant bug có blast radius toàn external storage.

**Fix:** chỉ expose app-specific share/export directory, read-only và file do user chọn.

## 10. Privacy, OAuth và dữ liệu remote

### 10.1 Privacy policy không khớp runtime — P0

Policy hiện nói:

- Content không được collect/transmit/store.
- Data chỉ nằm trên device.
- Không share với third party ngoài Google/payment.

Nhưng code có:

- Synabit sync relay/server.
- License API nhận HWID, device name, license key và heartbeat.
- Google Drive file content/metadata và OAuth token.
- `drive.readonly` có thể đọc toàn bộ Drive files/metadata user cho phép.
- OSRM nhận route coordinates.
- OpenStreetMap nhận tile coordinates/IP/network metadata.
- GitHub updater request nếu updater vẫn chạy.

Privacy policy phải phản ánh chính xác data types, purpose, recipient, encryption, retention, deletion và optionality. “Payload encrypted” không có nghĩa server không xử lý metadata.

Google yêu cầu Data Safety chính xác và nhất quán với privacy policy, kể cả app ở closed/open/production tracks: [Data Safety guidance](https://support.google.com/googleplay/android-developer/answer/10787469?hl=en).

### 10.2 Google Drive restricted scope — P0/P1 policy

`drive.readonly` là restricted scope. Google khuyến nghị downscope sang `drive.file` khi có thể; restricted scope có thể yêu cầu OAuth verification và annual security assessment tùy deployment/use case.

Ngoài ra `CLIENT_SECRET` desktop được compile bằng `env!` trong module chung. Cần xác minh Android binary không chứa secret và tách OAuth client theo platform.

**Fix:** ưu tiên `drive.file` + system picker/app folder; cfg-gate desktop-only secret; Android OAuth client riêng; registered signing fingerprints; verified domain/privacy URL; chuẩn bị verification evidence.

Tham khảo: [Google OAuth verification requirements](https://support.google.com/cloud/answer/13464321?hl=en).

### 10.3 Deep link

Manifest có hai intent-filter custom scheme `com.synabit.app`. Custom scheme có thể bị app khác claim; PKCE giảm rủi ro code theft nhưng không loại phishing/redirect ambiguity.

**Fix:** bỏ filter trùng; giới hạn host/path; cân nhắc verified HTTPS App Link/AppAuth; test malicious handler và cancellation.

## 11. Data architecture và integrity

### 11.1 Nền tảng tốt

- Vault filesystem giữ Markdown/JSON/assets portable.
- SQLite cache/index dùng WAL, indexes và FTS5.
- CRDT có update log/compaction.
- Sync có encrypted payload và một số atomic temp-file/rename path.
- Có path traversal/symlink protection ở một số command.

### 11.2 DATA-01 — Chưa có migration framework toàn schema — P1

Schema chủ yếu dựa vào `CREATE TABLE IF NOT EXISTS`, legacy cleanup và ad-hoc `ALTER`. Chỉ FTS có version riêng; chưa có `PRAGMA user_version`/migration ledger bao toàn database.

**Rủi ro:** upgrade bị gián đoạn tạo state nửa cũ/nửa mới; khó support nhiều release; không có golden upgrade fixtures.

**Fix:** numbered migrations trong transaction, preflight/invariant check, backup trước destructive migration, upgrade tests từ mọi version còn support.

### 11.3 DATA-02 — Referential integrity yếu — P1

Các bảng liên kết như node blocks/edges, CRDT updates và document paths thiếu foreign key/cascade rõ ràng; chưa có bằng chứng bật `PRAGMA foreign_keys=ON`. Timestamp/JSON shapes chưa chuẩn hóa.

### 11.4 DATA-03 — Nhiều source-of-truth — P1

Nội dung có thể nằm đồng thời ở vault file, SQLite nodes/content, whiteboard table, FTS và CRDT snapshot/update. Chưa có tài liệu invariant xác định:

- Source authoritative.
- Thứ tự commit file/DB/CRDT.
- Recovery sau crash.
- Rename/delete conflict.
- Rebuild derived data.

### 11.5 DATA-04 — Delete/restore chưa đủ an toàn — P1

Với sync đa thiết bị, cần trash/undo, tombstone versioning, retention và restore tests. Direct delete + propagation có thể tạo data loss không thể phục hồi.

## 12. UI/UX và accessibility

### 12.1 UI/UX

Evidence từ APK debug trước vẫn cho thấy visual language, dark/light tokens, cards, editor và bottom navigation khá polished. Tuy nhiên không thể nghiệm thu UI snapshot hiện tại vì source không build.

Các vấn đề cũ chưa có bằng chứng được đóng:

- Global horizontal swipe có thể xung đột editor/whiteboard/map/graph.
- `h-screen`/bottom nav/safe-area/keyboard dễ che content.
- Nexus graph và labels quá nhỏ trên phone.
- Settings là modal desktop dày đặc thay vì mobile settings route.
- Sync terminology “P2P/server/relay” không nhất quán.
- Chưa có device matrix cho low-memory phone, tablet/foldable, Android 7–16.

### 12.2 Accessibility batch hiện tại chưa đạt chất lượng — P0 quality

Static count:

- 725 `<button>`.
- 137 `aria-label`.
- 51 label đúng literal `"More Options"`.
- 29 `<img>`, chỉ 10 `alt`.
- 0 `role="dialog"`, `aria-modal` hoặc focus-trap usage.

Label sai/khó hiểu với TalkBack gồm:

- Nút close/back được gọi `"More Options"`.
- `"Show Settings Modal = false"`.
- `"Is Sidebar Open = !is Sidebar Open"`.
- `"Store.sync All Sources"`.
- `"Store.search Query.value ="`.
- `"Selected File = null"`.
- `"View Mode ="`.

Accessible name phải mô tả hành động/trạng thái theo ngôn ngữ user, không phải expression trong `@click`.

**Fix:** review thủ công theo component; semantic dialog/heading/nav/main; label-for/id; pressed/expanded/selected state; focus trap/restore; live regions; TalkBack, Switch Access, keyboard, 200% font và contrast tests.

## 13. Android Lint và manifest

Android Lint: **2 errors, 35 warnings, 1 hint**.

Hai errors vẫn từ Android TV declaration:

- `MissingTvBanner`
- `ImpliedTouchscreenHardware`

Manifest có `LEANBACK_LAUNCHER`/leanback feature nhưng app chưa có TV banner, D-pad/focus QA hoặc TV UX.

**Fix nhanh cho phone release:** bỏ Leanback launcher và TV feature khỏi manifest. Nếu muốn TV, tách track/form factor và làm đầy đủ TV quality.

Các manifest concerns khác:

- Chưa có backup/data extraction rules.
- Hai OAuth custom-scheme filters trùng.
- FileProvider external path quá rộng.
- Notification permission flow chưa được chứng minh trên Android 13+.

## 14. Monetization và Google Play Payments

App có trial/license, khóa app về read-only và hiển thị:

`Purchase one here` → `https://synabit.net/pricing`

Không thấy Google Play Billing dependency hoặc region/program gating.

Theo Play Payments policy, app phân phối qua Play thu tiền để mở digital functionality/cloud productivity phải dùng Play Billing, trừ khi thuộc exception hoặc đã enroll đúng chương trình/vùng và đáp ứng API/UX/reporting/fee requirements. App không được dùng cùng một external purchase CTA toàn cầu như hiện tại.

Tham khảo: [Google Play Payments policy](https://support.google.com/googleplay/android-developer/answer/9858738?hl=en).

**Các lựa chọn an toàn:**

1. Tích hợp Play Billing trên Android, verify purchase/entitlement server-side.
2. Ship Android beta miễn phí, bỏ external purchase CTA và paid entitlement.
3. Geofence + enroll đúng từng external/alternative billing program, implement toàn bộ program requirements.

Cho release đầu, lựa chọn 1 hoặc 2 ít rủi ro nhất.

## 15. Google Play checklist tại 23/07/2026

| Yêu cầu | Trạng thái | Audit |
|---|---|---|
| Target API | **Pass** | `targetSdk=36`, `compileSdk=36`; đáp ứng mốc API 36 từ 31/08/2026 |
| Min SDK | Config pass | `minSdk=24`, nhưng chưa test đủ API 24–36 |
| Android App Bundle | **Fail** | Không build được AAB |
| Release signing | **Fail** | Không có signing pipeline/upload key proof |
| Play App Signing | Chưa xác minh | Cần cấu hình trong Play Console |
| 64-bit ABI | Conditional | Debug arm64 có; final AAB chưa kiểm |
| 16 KB page size | Conditional | Stale debug APK zipalign pass; final release chưa kiểm ELF/AAB |
| Android Lint | **Fail** | 2 errors |
| Functional background sync | **Fail** | Load sai lib + JNI no-op |
| Secure secret migration | **Fail** | R8/backup/durability/error handling chưa đạt |
| Privacy policy URL/content | **Fail** | Nội dung repo sai runtime; public verified URL chưa xác minh |
| Data Safety | **Fail/chưa chuẩn bị** | Cần data inventory và declaration khớp app/server/SDK |
| OAuth verification | **Fail/chưa xác minh** | `drive.readonly` restricted |
| Payments | **Fail cho global release** | External pricing CTA, không Play Billing/gating |
| Android updater | **Fail/rủi ro** | Tauri GitHub updater không có Android guard |
| Accessibility | **Fail quality gate** | Labels sai, thiếu dialog/focus/TalkBack proof |
| Store listing assets | Chưa sẵn sàng | Không có bộ listing/feature graphic/screenshots được nghiệm thu |
| Content rating/target audience/app access | Chưa xác minh | Cần hoàn tất Play Console |
| Financial features declaration | Chưa xác minh | Finance mini-app không đồng nghĩa financial service, nhưng form vẫn phải khai |
| Developer verification/package registration | Chưa xác minh | Play packages phải được register/verified theo mốc 30/09/2026 |
| Closed testing | Có thể bắt buộc | Personal account mới cần 12 opted-in testers trong 14 ngày liên tục |
| Android CI/staged rollout | **Fail** | Chưa có internal/closed/production pipeline |

Nguồn policy chính:

- [Target API level policy](https://support.google.com/googleplay/android-developer/answer/16561298?hl=en)
- [Android App Bundle requirement](https://support.google.com/googleplay/android-developer/answer/9844679?hl=en)
- [Data Safety](https://support.google.com/googleplay/android-developer/answer/10787469?hl=en)
- [Payments policy](https://support.google.com/googleplay/android-developer/answer/9858738?hl=en)
- [Closed testing for new personal accounts](https://support.google.com/googleplay/android-developer/answer/14151465?hl=en-EN)
- [Registering Play package names](https://support.google.com/googleplay/android-developer/answer/16984799?hl=en)

## 16. P0 blockers theo thứ tự xử lý

1. **Khôi phục production build:** sửa malformed Vue markup; build phải pass từ clean checkout.
2. **Đưa gates về xanh:** typecheck, lint, frontend tests, Rust tests, Android Lint.
3. **Tạo clean release candidate:** commit đầy đủ SecureStore/refactor, review, versionCode mới.
4. **Tạo signed AAB reproducibly:** CI + upload key + verify + bundletool install.
5. **Chốt background sync:** implement end-to-end hoặc remove khỏi Android v1.
6. **Hoàn thiện secure storage:** R8 keep, durable migration, typed JNI errors, backup rules, restore tests.
7. **Thu hẹp Tauri capabilities và sanitize remote HTML.**
8. **Sửa privacy policy/Data Safety inventory** theo runtime/server thực tế.
9. **Chốt monetization:** Play Billing hoặc beta không có sale CTA.
10. **OAuth hardening/verification:** downscope Drive, Android client, signing fingerprints, deep link.
11. **Accessibility pass thực:** semantics/focus/TalkBack/font scale.
12. **Play Console/internal track:** declarations, listing, pre-launch report, device catalog, size.

## 17. Definition of Done

Chỉ gọi là “ready to submit” khi:

- [ ] Working tree clean, release tag immutable, versionCode tăng.
- [ ] Signed release AAB tạo được trên clean Android CI.
- [ ] AAB install được qua bundletool/Internal testing.
- [ ] Final bundle pass 64-bit, 16 KB và download-size checks.
- [ ] TypeScript, ESLint, unit, Rust tests và Android Lint đều xanh.
- [ ] Không còn worker giả-success/no-op.
- [ ] Secret migration không thể mất dữ liệu khi process bị kill.
- [ ] R8-minified release đọc/ghi SecureStore được.
- [ ] Backup/restore/device-transfer behavior được test.
- [ ] Wildcard FS/opener/asset capabilities được loại bỏ.
- [ ] Remote HTML được sanitize và dependency high advisories được xử lý.
- [ ] Privacy policy public URL và Data Safety khớp code/server.
- [ ] Billing/entitlement compliant ở tất cả region được phát hành.
- [ ] OAuth client/scopes/verification/signing fingerprints hoàn tất.
- [ ] TalkBack, 200% font, keyboard, safe-area, gesture nav pass.
- [ ] Upgrade/migration/offline/conflict/delete/restore không mất dữ liệu.
- [ ] Play pre-launch report không còn blocker crash/ANR/security.
- [ ] Có staged rollout, monitoring, rollback và incident owner.

## 18. Đề xuất release strategy

Đường ngắn nhất để lên Play an toàn là một **Android beta scope nhỏ**:

- Chỉ giữ local notes/tasks/quick capture và feature đã test.
- Tắt background sync cho đến khi headless coordinator hoàn chỉnh.
- Tắt Tauri updater/process trên Android.
- Bỏ Drive full-browser `drive.readonly`; dùng `drive.file` nếu cần.
- Beta miễn phí hoặc Play Billing hoàn chỉnh; không external pricing CTA global.
- Bỏ Android TV declaration khỏi phone build.
- Hạn chế mini-app nặng nếu final bundle vượt size/performance budget.

Sau khi AAB, security, privacy và quality gates xanh, đưa lên Internal testing; sau đó closed test, sửa pre-launch/crash/ANR, rồi mới staged production.

---

## Final assessment

Synabit có nền tảng sản phẩm và kiến trúc đa nền tảng đáng giữ, nhưng snapshot Android hiện tại **không phải release candidate**. SecureStore và accessibility là tiến bộ ở mức implementation draft; chưa phải bằng chứng production. Regression frontend khiến app không build, background sync vẫn giả-success, security boundary quá rộng và policy/billing chưa hợp lệ.

**Kết luận cuối: 29/100, NO-GO. Chưa sẵn sàng upload Google Play, kể cả Internal testing, cho đến khi có signed AAB từ clean source và toàn bộ P0 kỹ thuật/policy tối thiểu được đóng.**
