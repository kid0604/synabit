# Kế hoạch nâng chất lượng hình tương tác của Syn, 2026-10-28

**Nền:** bảy câu hỏi người dùng hỏi cả Syn, ChatGPT và Gemini rồi so ảnh chụp (2026-10-27 và
2026-10-28), cộng hai vòng "bốn câu khó" trong `eval-agent-2026-10-01.md`. Tài liệu này nói *vì sao
hình của Syn kém, sửa ở tầng nào, theo thứ tự nào, đo bằng gì*.

**Phạm vi:** chất lượng hình Syn vẽ qua `visual_guide` / `show_visual` của Lát 1
(`syn-visuals-plan-2026-10-26.md`). Không đổi giao thức `synvisual`, khung cách ly hay CSP.

**Nguyên tắc chốt:** không vá theo từng ví dụ. Mỗi thay đổi phải chung cho mọi câu hỏi, và chỉ được
giữ nếu điểm của **cả bộ đo** tăng — kể cả phần giữ lại không ai dùng để sửa. Từ nay không thêm luật
vào guide vì một hình hỏng; hình hỏng là một dòng trong bộ đo.

**Model:** eval chạy trên `gpt-6-luna` (chốt 2026-10-28, dù app đã dùng `gpt-6.1-terra`). Hình đúng
trên luna thì đúng trên model mạnh hơn; điều ngược lại không chắc — câu thuế đúng trên terra, sai
trên luna.

---

## Tiến độ

| Phần | Trạng thái | Lệch so với kế hoạch |
| --- | --- | --- |
| PR Q0 — hai lỗi nhỏ | Chưa làm | — |
| PR Q1 — tự kiểm trong kit, chạy bằng Playwright | Chưa làm | — |
| PR Q2 — bộ đo và nền | Chưa làm | — |
| PR Q3 — bản thiết kế và kiểu giải thích | Chưa làm | — |
| PR Q4 — vòng tự kiểm trước khi giữ | Chưa làm | — |
| PR Q5–Q7 — thành phần trong kit | Chưa làm | — |

---

## 0. Tóm tắt

Bảy ví dụ cho thấy hình của Syn hỏng ở **ba tầng**, và cả ba có gốc chung: model viết lại mọi thứ
từ con số không mỗi lần, theo một khuôn duy nhất (một khung hình + vài thanh trượt), và **không bao
giờ thấy hình mình vẽ ra**.

| Tầng | Syn hỏng bao nhiêu lần / 7 | Gốc | Sửa bằng |
| --- | --- | --- | --- |
| 1. Không trúng ý câu hỏi | 4 hỏng, 3 nửa | Vẽ *chủ đề* thay vì *điều câu hỏi muốn thấy*; không chọn kiểu giải thích | Bản thiết kế bắt buộc + danh sách kiểu giải thích (Q3) |
| 2. Trình bày kém | 7 / 7, mỗi lần một kiểu | Tự viết trục, lề, chú thích, nút bấm từ đầu | Thành phần và bố cục trong kit (Q5–Q7) |
| 3. Sai hoặc tự mâu thuẫn | 4 / 7 (luna) | Suy ra công thức trong đầu; vẽ và tính từ hai nguồn khác nhau | Một mô hình cho mọi cách hiện + thế ngược để kiểm (Q3, Q7) + vòng tự kiểm (Q4) |

Đo trước (Q1, Q2), rồi mới sửa. Thứ tự sửa theo đòn bẩy: bản thiết kế (rẻ, đánh vào tầng 1), vòng
tự kiểm (bắt được lỗi của mọi tầng mà không cần biết câu hỏi), rồi thành phần kit (đắt nhất, xoá
tầng 2 theo cấu trúc).

---

## 1. Bằng chứng

Mỗi dòng là một câu hỏi mà người dùng chạy cả ba bên. Số đã được tính lại tay; hình của Syn đã được
mở và bấm thử từ vault.

| Câu | ChatGPT | Gemini | Syn | Syn hỏng ở tầng |
| --- | --- | --- | --- | --- |
| Dijkstra từng bước | Màu theo trạng thái, khoảng cách trên đỉnh, phép `min(4, 2+1)` trong dòng bước | 26 bước chi tiết, tự chạy; vòng tròn lạc dưới C | Đúng kết quả; bước 0 bảng ghi B=4, C=2 trong khi chữ ghi "∞"; mũi tên trên đồ thị vô hướng; không tô trạng thái | 1, 2, 3 |
| Con lắc đơn | 2,25 s ở 35°, 1,2 m — chu kỳ *thật*; giá treo, cân bằng, cung biên độ | Thanh năng lượng sai thang (2,52/2,83 J vẽ dài 56%) | Số to là 2,006 s gần đúng, số đúng 2,042 s nằm chữ nhỏ; năng lượng "1,006" do Euler bán ẩn | 1, 2, 3 |
| Fourier + Gibbs | Ô phóng to điểm nhảy, đỉnh 1,18 ở x≈0,31 — đúng | Vọt 0,26% và "hội tụ đều" — sai; phổ có hài chẵn; LaTeX không dựng | Đúng nhưng không thấy Gibbs: không phóng to, không đo đỉnh vọt | 1, 2 |
| Bayes y khoa | Chia 100 / 9.900 thành bốn ô — thấy vì sao | Đặt ngộ nhận "99%" lên đầu | Đúng ở mọi thanh trượt; thấy 99 vs 495 nhưng không thấy 495 từ đâu | ½, 2 |
| Thuế, luna | Đúng hết, vùng thu ngân sách tô | Đúng số; đơn vị "triệu đ" sai; gọi độ dốc là "co giãn" | `pc = (t+120)/5` thiếu hệ số 3 → 0,4 / 1,6 / 102,4 / 0,8 sai; đường cung gõ tay lệch; chấm không nằm trên giao điểm | 1, 2, 3 |
| Thuế, terra | (như trên) | (như trên) | Đúng hết; nhưng cảnh báo số dán lên cả sáu số đúng | ½, 2 |
| Trôi dạt di truyền | N=20 và N=200 cạnh nhau, đường H với lý thuyết | H quan sát bám lý thuyết; ô "Cố định: 1, Mất: 0" ngược bảng "7 / 9" | Lấy mẫu nhị thức đúng; "đã cố định 17/20" gộp cả mất alen; không có H, không so hai N | ½, 2, 3 |

Cộng thêm từ hai lần hỏi khoản vay (2026-10-28): câu trả lời bịa 568,1 / 376,9 triệu khi hình nói
579,7 / 451,9; hình vẽ hai thước đo trên một trục, rồi trên một thang không có trục; một lần vẽ dữ
liệu theo triệu trên trục theo đồng.

**Điều người dùng nhận ra và cần giữ:** ChatGPT không thắng nhờ biết nhiều hơn. Lần nào nó cũng chọn
*kiểu giải thích* đúng với câu hỏi — đi từng bước, phóng to hiện tượng, tách thành phần, tô vùng,
đặt cạnh nhau — và hai màn của nó trông như cùng một bộ thành phần. Gemini thì cho thấy mặt kia:
nhiều khung, nhiều số, và càng nhiều chỗ sai không ai kiểm.

---

## 2. Năm thay đổi

### 2.1 Bản thiết kế trước khi vẽ, và một danh sách ngắn kiểu giải thích (tầng 1)

Syn làm đúng ý khi câu hỏi tự nói ra ý ("cho tao hiểu vì sao" ở câu Bayes), và vẽ chủ đề khi câu
hỏi chỉ nêu tên ("Fourier và Gibbs"). Nó không tự đặt câu hỏi đó. Bản thiết kế bắt nó đặt.

`show_visual` nhận thêm trường bắt buộc `brief`:

```json
{
  "point": "Đỉnh vọt ở điểm nhảy không thấp đi khi N tăng, chỉ hẹp lại",
  "pattern": "zoom",
  "proof": [{ "name": "overshoot", "value": 1.1823, "from": "compute" }]
}
```

- `point` — một câu: hình phải cho người xem thấy điều gì. Không phải tên chủ đề.
- `pattern` — một trong danh sách dưới. Mỗi kiểu có các phần bắt buộc, guide liệt kê.
- `proof` — những con số chứng minh `point`, tính bằng `compute` trước khi vẽ. Hình phải hiện
  chúng (kiểm ở Q4 qua `synabit.report`).

| Kiểu | Khi câu hỏi | Phần bắt buộc | Ví dụ |
| --- | --- | --- | --- |
| `step` | "từng bước", thuật toán, quy trình | Bước 0 là trạng thái ban đầu; mỗi trạng thái một màu có chú thích; cái vừa đổi được đánh dấu; dòng bước có phép tính | Dijkstra |
| `compare` | "vì sao A khác B", "so sánh", ảnh hưởng của một tham số | Hai (hoặc vài) trường hợp chạy cạnh nhau cùng thang; một số đo chung cho cả hai | Trôi dạt N=20 vs N=200 |
| `decompose` | "gồm những gì", "từ đâu ra", xác suất có điều kiện | Toàn thể chia thành phần, mỗi phần có số và tổng khớp | Bayes 100 / 9.900 |
| `zoom` | tên một hiện tượng, một điểm đặc biệt | Toàn cảnh + ô phóng to vào hiện tượng + số đo của nó | Gibbs |
| `areas` | "ai chịu bao nhiêu", "mất bao nhiêu" | Vùng tô có số, cùng thang với đường | Thuế |
| `explore` | "cho tao chỉnh", mô phỏng một hệ | Trạng thái + đại lượng người xem hỏi + đối chiếu với lý thuyết hoặc bất biến | Con lắc |

Danh sách giữ ngắn. Một kiểu mới chỉ được thêm khi bộ đo có một nhóm câu hỏi mà không kiểu nào
hợp, và thêm kèm câu hỏi giữ lại để kiểm.

`visual_guide` nhận thêm `pattern` và trả phần của kiểu đó cùng **một hình mẫu viết tay** cho kiểu
đó. Model bắt chước ví dụ mạnh hơn làm theo luật; sáu hình mẫu là cái giá cố định, không lớn lên
theo số câu hỏi.

`brief` được lưu trong frontmatter của node hình, để bộ đo chấm "hình có cho thấy `point` không".

### 2.2 Một mô hình, nhiều cách hiện; thế ngược để kiểm (tầng 3)

Luật "tham chiếu tính từ cùng định nghĩa" hiện chỉ có trong `simulation.md`. Câu thuế cho thấy nó
đúng cho mọi hình: đường cung vẽ từ toạ độ gõ tay, giá từ một công thức suy tay khác, và hai thứ
không khớp. Chuyển lên `core.md`:

- Định nghĩa mô hình **một lần** — `demand(p)`, `supply(p)`, `step(state)` — và tính *mọi thứ* hiện
  ra từ nó: đường, điểm, vùng, số.
- Điểm cân bằng, đỉnh, nghiệm: **giải bằng số** từ mô hình (`synabit.solve`, Q7), không suy công
  thức đóng bằng tay.
- Mỗi số suy ra được **thế ngược** vào phương trình gốc bằng `synabit.expect` (Q4 chạy chúng). Câu
  thuế của luna: thế giá 24,4 → Qd = 51,2, Qs(22,4) = 47,2 — lệch, bắt được.

`compute` chỉ bảo vệ phép tính, không bảo vệ công thức: luna gọi `compute` đúng hai lần với công
thức sai. Nên `proof` trong bản thiết kế phải được tính từ *cùng* hàm với hình, và Q4 so hai bên.

### 2.3 Vòng tự kiểm: model thấy hình mình vẽ (mọi tầng)

Hôm nay `check.rs` đọc HTML để chặn chỗ không an toàn, và không biết hình chạy ra sao. Thêm một
bước **chạy hình trong khung ẩn** trước khi giữ, đo những thứ chung cho mọi hình, trả về như
`problems` của `check.rs`:

| Kiểm | Bắt được (trong bảy ví dụ) |
| --- | --- |
| Lỗi JS, `synabit/error` | — |
| `NaN`, `Infinity` trong thuộc tính hoặc chữ | — |
| Số trong chữ có hơn 4 chữ số thập phân | "4.166666666666667y" (khoản vay, terra) |
| Còn `$…$`, `\(…\)` thô trong chữ đã dựng | LaTeX của Gemini |
| Một đường (`path`, `polyline`) nhiều điểm mà dải dọc bằng 0 | Khoản vay vẽ theo triệu trên trục đồng |
| Chữ tràn ra ngoài `svg` hoặc khung | Nhãn trục lãi kép bị cắt |
| Hai nhãn chữ trong cùng `svg` đè lên nhau | Nhãn trọng số Dijkstra; nhãn cân bằng của Gemini |
| Nút hoặc thanh trượt không đổi gì khi dùng | — |
| `fill` là màu cố định trong khi kit có màu theo theme | Thanh Bayes không đổi theme |
| `synabit.expect` thất bại | Thuế luna; Dijkstra bước 0; con lắc (chu kỳ hiện so với chu kỳ đo) |
| Số trong `brief.proof` không có trong `synabit.report` | Khoản vay (câu trả lời khác hình) |

Phần kiểm nằm trong `kit.js` (`synabit.lint()`, chạy sau lần vẽ đầu và sau khi thử từng điều
khiển), nên cùng một mã chạy trong app và trong eval. Kết quả đi về host bằng `synabit/lint`.

`synabit.report({...})` khai các số chính của hình. Chúng vào kết quả của `show_visual`, tức là vào
những gì run đã đọc — nên câu trả lời trích số từ hình thì có nguồn, bịa số khác thì bị cảnh báo
số bắt. Việc này thay bản vá "mở `compute` khi đã vẽ" của 2026-10-28 ở tầng tổng quát hơn.

**Chỗ chạy:** trong app, host dựng một iframe ẩn cùng giao thức `synvisual` (cùng CSP, cùng sandbox),
engine chờ `synabit/lint` với hạn 3 giây; quá hạn thì giữ hình, ghi vào run là "chưa tự kiểm".
Trong eval (`MockRuntime`, không có webview): một script Playwright WebKit chạy file HTML đứng riêng
(`assets::standalone`, đã có cho `SYN_EVAL_KEEP`). Playwright đã có trong repo cho e2e.

### 2.4 Thành phần và bố cục trong kit (tầng 2)

Syn hỏng trình bày cả bảy lần, mỗi lần một kiểu khác; luật trong guide không chạy kịp. Thành phần
xoá cả nhóm lỗi theo cấu trúc, và làm mọi hình nhất quán.

| Thành phần | Lo | Xoá lỗi |
| --- | --- | --- |
| `synabit.chart(el, {panels, x, series, areas})` | Trục, đơn vị, vạch tròn (`nice`), định dạng số theo locale, lề tự rộng theo nhãn, chú thích có ô màu, nhiều bảng chồng nhau mỗi bảng một thước đo, vùng tô giữa hai đường | Hai thước đo một trục; thang không trục; nhãn cắt; "1π"; "0k"; chú thích không màu |
| `synabit.controls(el, [{id, label, min, max, step, unit, help, presets}])` | Thanh trượt có nhãn, giá trị, đơn vị, khoảng, một dòng giải thích, nút đặt nhanh | Thanh trượt chật, "N" trơn, nhãn xuống dòng |
| `synabit.stats(el, [{label, value, unit, digits, note}])` | Ô số phụ, làm tròn có chủ ý, dòng công thức dưới số | Số to lặp lại giá trị thanh trượt; chính xác giả; dòng chữ dồn |
| `synabit.layout('side' \| 'stack' \| 'compare')` | Khung hai nửa (điều khiển / sân khấu), xếp chồng, hai bản cạnh nhau | Khung 640 px trống nửa; điều khiển và hình rời rạc |
| `synabit.stepper(el, {steps, render})` | Lùi / tiếp / tự chạy / thanh tiến trình; bước 0 bắt buộc là trạng thái ban đầu | Bảng bước 0 lệch chữ |
| `synabit.graph(el, {nodes, edges, directed, state})` | Đồ thị có hướng hay không, màu theo trạng thái kèm chú thích, nhãn trọng số trong ô | Mũi tên trên đồ thị vô hướng; nhãn bị đè |
| `synabit.sim({state, step, method, invariants})` | Velocity Verlet / RK4, bước cố định, chạy / dừng / đặt lại, hiện bất biến | Euler bán ẩn; năng lượng "1,006" |
| `synabit.solve`, `synabit.integrate` | Nghiệm (chia đôi, Newton), tích phân số | Suy công thức đóng bằng tay |

Viết d3 tay vẫn được cho những gì thành phần không phủ. Mỗi thành phần dùng tính năng Baseline
Widely available (Safari 16.4, `CLAUDE.md`), có test trong `visualKit.spec.ts`, và có trang mẫu trong
`/spike/kit`. `kit-sim.js` của Lát 2 chuyển vào đây thành `synabit.sim`.

### 2.5 Bộ đo (để biết bốn thay đổi trên có giúp thật không)

Một nhóm eval mới, `visual-quality`, chạy trên luna, 3 lần mỗi câu.

- **32 câu**, trải sáu kiểu giải thích và ít nhất tám lĩnh vực (thuật toán, vật lý, toán, xác suất,
  kinh tế, sinh học, tài chính, hoá). **22 câu phát triển** — gồm bảy câu ở §1 — được nhìn và dùng
  để sửa. **10 câu giữ lại**: chỉ đọc điểm tổng, không bao giờ mở hình hỏng để sửa theo.
- Mỗi câu có đáp số biết trước (như bảy câu ở §1: 16,7%, 25,2 / 23,2, 2,041 s, A→G = 11…).
- Ba điểm cho mỗi hình:
  1. **Tự kiểm** — tỷ lệ hình qua `synabit.lint` (chạy bằng Playwright ở mọi PR, kể cả trước Q4).
  2. **Đúng** — đáp số xuất hiện trong chữ đã dựng của hình hoặc trong `report`, và câu trả lời
     không có số nào ngược với hình.
  3. **Chấm theo bảng** — luna (có thị giác) chấm ảnh chụp hình theo bảng tiêu chí cố định: tầng 1
     (có cho thấy `point` / điều câu hỏi muốn không), tầng 2 (trình bày), mỗi tầng 0–2.
- **Hiệu chỉnh người chấm** trước khi tin nó: chấm 21 ảnh chụp của §1 (lưu ở
  `src-tauri/src/syn/eval/calibration/`, không vào bản build). Người chấm phải xếp thứ tự ba màn
  khớp với nhận xét ở §1 trong ít nhất 5 / 7 câu, và phải chấm màn Fourier của Gemini thấp ở tầng 3
  dù nó đẹp. Không đạt thì sửa bảng tiêu chí trước khi đo, không sau.

---

## 3. Các PR

### PR Q0 — Hai lỗi nhỏ, làm ngay

1. **Cảnh báo số nhân đơn vị hai lần.** "1,2 nghìn đồng" được đọc là 1.200, trong khi `compute` trả
   1,2 tính theo nghìn; câu thuế của terra đúng hết mà bị dán cảnh báo lên cả sáu số. Một số có chữ
   đơn vị được coi là có nguồn nếu số viết ra (trước khi nhân) có trong nguồn. `answer.rs`
   `unbacked`, `Figure` giữ thêm giá trị chưa nhân.
2. **`compute` mở muộn.** Bản 2026-10-28 mở nó sau khi `show_visual` giữ hình, nhưng số cần tính nằm
   trong chính hình sắp vẽ; terra phải tự gọi `find_tools`. Mở khi `visual_guide` được đọc.

Mỗi lỗi một test, đỏ khi tắt bản sửa.

### PR Q1 — Tự kiểm trong kit, chạy được ngoài app

- `kit.js`: `synabit.lint()`, `synabit.expect(name, actual, expected, tolerance)`,
  `synabit.report(values)`, thông điệp `synabit/lint`.
- `scripts/visual-trial.mjs`: Playwright WebKit mở từng file HTML đứng riêng trong một thư mục, chờ
  `synabit/lint`, thử từng điều khiển, chụp ảnh, ghi một JSON.
- Test: một file HTML cố định cho mỗi dòng của bảng §2.3, dựng lại từ hình hỏng thật trong vault;
  mỗi file phải bị bắt đúng lỗi của nó, và một hình tốt không bị bắt gì.

### PR Q2 — Bộ đo, và đo nền trước mọi thay đổi

- 32 câu trong `eval::tasks` (nhóm `visual-quality`), 10 câu đánh dấu `held_out`. Báo cáo chỉ in
  điểm tổng của phần giữ lại.
- Người chấm và bảng tiêu chí; hiệu chỉnh trên 21 ảnh.
- Chạy nền trên luna × 3, ghi vào `eval-agent-2026-10-01.md` dưới dự đoán ở §5.

### PR Q3 — Bản thiết kế và kiểu giải thích

- `show_visual`: trường `brief` bắt buộc; thiếu hoặc `pattern` không nằm trong danh sách thì từ chối
  như `check.rs`.
- `visual_guide`: nhận `pattern`; `guides/patterns/*.md` mỗi kiểu một file với phần bắt buộc và hình
  mẫu viết tay.
- `core.md`: luật một mô hình nhiều cách hiện, thế ngược.
- Đo lại.

### PR Q4 — Vòng tự kiểm trước khi giữ

- Engine: `show_visual` chờ kết quả tự kiểm; lỗi trả về model trong số lần thử sẵn có (`MAX_TRIES`).
- App: iframe ẩn trên `synvisual`, cùng sandbox; hạn 3 giây.
- Eval: harness gọi `visual-trial.mjs` khi `SYN_EVAL_TRIAL=1`.
- `report` vào kết quả của `show_visual`.
- Đo lại, kèm thời gian thêm vào mỗi hình.

### PR Q5 — `chart`, `controls`, `stats`, `layout`

### PR Q6 — `stepper`, `graph`

### PR Q7 — `sim`, `solve`, `integrate`

Mỗi PR: thành phần, test trong `visualKit.spec.ts`, trang mẫu, guide và hình mẫu dùng thành phần đó,
đo lại.

---

## 4. Gate

| # | Điều kiện | Cách kiểm |
| --- | --- | --- |
| 1 | Tự kiểm qua ≥ 90% trên cả 32 câu | Bộ đo, sau Q4 |
| 2 | Đúng ≥ 95% trên câu có đáp số | Bộ đo |
| 3 | Tầng 1 trung bình ≥ 1,4 / 2; tầng 2 ≥ 1,4 / 2 | Người chấm đã hiệu chỉnh |
| 4 | Phần giữ lại không thấp hơn phần phát triển quá 0,2 điểm ở tầng 1 và 2 | Bộ đo |
| 5 | `safety` không tụt | Eval |
| 6 | Tự kiểm thêm dưới 1,5 giây cho mỗi hình trên máy dev | Đo tay |
| 7 | Bảy câu ở §1 hỏi lại trên luna: không câu nào hỏng ở tầng 3, và người dùng xếp Syn ít nhất ngang Gemini ở năm câu | Người dùng so tay |

Gate 4 là chỗ nguyên tắc "không vá theo ví dụ" được kiểm: phần phát triển tăng mà phần giữ lại
không tăng nghĩa là đã khớp theo ví dụ.

---

## 5. Dự đoán (ghi 2026-10-28, trước mọi lần đo; không sửa sau)

| Lúc | Tự kiểm qua | Đúng | Tầng 1 | Tầng 2 |
| --- | --- | --- | --- | --- |
| Nền (sau Q2) | 45% | 75% | 0,8 | 0,7 |
| Sau Q3 | 50% | 80% | 1,3 | 0,8 |
| Sau Q4 | 85% | 90% | 1,3 | 1,0 |
| Sau Q5–Q7 | 95% | 95% | 1,4 | 1,5 |

- Q3 đẩy tầng 1 nhiều nhất và gần như không đụng tầng 2. Nếu tầng 1 sau Q3 dưới 1,1 thì bản thiết
  kế không đủ, và cần cho model xem ảnh chụp hình của mình (bước chấm chạy trong vòng tự kiểm).
- Q4 đẩy "tự kiểm qua" và "đúng" nhiều nhất. Nếu "đúng" không tăng ít nhất 5 điểm thì `expect` do
  chính model viết không bắt được lỗi của chính model, và `proof` phải được tính độc lập hơn.
- Phần giữ lại thấp hơn phần phát triển khoảng 0,1 ở mỗi tầng.
- Nhóm câu yếu nhất ở nền: `step` và `zoom`, vì Syn chưa bao giờ tự chọn hai kiểu đó.

---

## 6. Rủi ro

| Rủi ro | Dấu hiệu | Xử lý |
| --- | --- | --- |
| Người chấm luna chấm chính luna, thiên vị hoặc nhiễu | Hiệu chỉnh dưới 5 / 7; điểm một hình dao động hơn 0,5 giữa hai lần chấm | Chấm 3 lần lấy trung vị; bảng tiêu chí cụ thể hơn; không dùng điểm tầng 1–2 làm gate cho tới khi qua hiệu chỉnh |
| Playwright WebKit khác WKWebView của macOS | Hình qua tự kiểm trong eval mà hỏng trong app | Bước 0 của Lát 1 đã chạy trong app thật; thêm một phần `lint` vào spike không người |
| Thành phần làm mọi hình giống nhau, hoặc bó tay model | Tầng 1 giảm sau Q5 | Thành phần là tuỳ chọn; d3 tay vẫn được; hình mẫu cho thấy khi nào không dùng |
| Vòng tự kiểm làm chậm câu trả lời | Gate 6 trượt | Chạy tự kiểm song song với việc model viết chữ; chỉ chờ khi có `expect` |
| Model viết `expect` để luôn qua | `expect` qua mà đáp số sai | Bộ đo chấm đúng sai từ đáp số biết trước, không từ `expect` |
| Bản thiết kế thành thủ tục điền cho có | `point` là tên chủ đề | `check` từ chối `point` trùng tiêu đề; người chấm chấm hình theo `point` |

---

## 7. Quan hệ với Lát 1 và Lát 2

- Các mục gate của Lát 1 còn mở (Windows, Linux, macOS 13.3, đo thời gian, Android, một tuần dùng
  thật) không phụ thuộc kế hoạch này và chạy song song được.
- Lát 2: **Kit mô phỏng** chuyển vào Q7. **Nhờ Syn sửa** được Q4 làm một phần — model sửa trước khi
  giữ; nút cho người dùng vẫn để Lát 2.
- Bản vá 2026-10-28 (`compute` khi đã vẽ) giữ cho tới Q4, rồi `report` thay nó; khi đó xem lại có
  còn cần không.
