//! How good Syn's visuals are, measured on questions rather than examples.
//!
//! # Why this exists
//!
//! Seven questions asked of Syn, ChatGPT and Gemini (2026-10-27/28) showed
//! Syn's visuals failing at three levels: they draw the topic rather than what
//! the question wants seen, they are poorly presented, and they are wrong or
//! contradict themselves. Fixing each example as it broke would never end.
//! `docs/syn-visuals-quality-plan-2026-10-28.md` changes what is common to
//! every question instead, and this is how a change is kept or dropped: by the
//! score on all of these questions, ten of which no fix may look at.
//!
//! # What each trial is scored on
//!
//! - **Lint** — whether every visual it drew passes `synabit.lint`, drawn in
//!   WebKit by `e2e/visualTrial.mjs`. A trial that drew nothing does not pass.
//! - **Correct** — whether every number the question has an answer for
//!   appears in what the visual shows, in any state its controls reach, in
//!   what it reports, or in the answer under it.
//! - **Judged** — `point` (does it show what the question wants seen) and
//!   `presentation`, 0 to 2, by a model looking at a screenshot with a fixed
//!   rubric, the median of three. Trusted only once it has passed
//!   [`calibrated`] on screenshots a person ranked.
//!
//! # Running it
//!
//! ```bash
//! SYN_EVAL_PROVIDER=openai SYN_EVAL_MODEL=gpt-6-luna SYN_EVAL_KEY=keychain SYN_EVAL_OUT=/tmp/vq \
//!   cargo test --lib syn::eval::tests::judge_calibration -- --ignored --nocapture
//! SYN_EVAL_PROVIDER=… SYN_EVAL_OUT=/tmp/vq cargo test --lib syn::eval::tests::visual_quality -- --ignored --nocapture
//! SYN_EVAL_OUT=/tmp/vq cargo test --lib syn::eval::tests::visual_quality_report -- --ignored --nocapture
//! ```
//!
//! `SYN_EVAL_QUALITY` picks `dev`, `held` or `all` (the default);
//! `SYN_EVAL_ONLY` a comma-separated list of question ids. Several runs may
//! share one `SYN_EVAL_OUT`; the report reads every `trials-*.json` there.

use serde::{Deserialize, Serialize};

/// How a question is best explained (plan §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pattern {
    Step,
    Compare,
    Decompose,
    Zoom,
    Areas,
    Explore,
}

impl Pattern {
    pub const ALL: [Pattern; 6] = [Pattern::Step, Pattern::Compare, Pattern::Decompose, Pattern::Zoom, Pattern::Areas, Pattern::Explore];

    pub fn name(self) -> &'static str {
        match self {
            Pattern::Step => "step",
            Pattern::Compare => "compare",
            Pattern::Decompose => "decompose",
            Pattern::Zoom => "zoom",
            Pattern::Areas => "areas",
            Pattern::Explore => "explore",
        }
    }
}

/// A number the answer must contain: any of these values, each within its
/// tolerance — the same figure in the units a visual might write it in.
#[derive(Debug, Clone, Copy)]
pub struct Expected {
    pub what: &'static str,
    pub any_of: &'static [(f64, f64)],
}

#[derive(Debug, Clone, Copy)]
pub struct Question {
    pub id: &'static str,
    pub ask: &'static str,
    pub pattern: Pattern,
    pub field: &'static str,
    /// Never looked at one by one: only its share of the totals is read.
    pub held_out: bool,
    /// Worked out by hand, 2026-10-28 (the plan's §1 and PR Q2). Empty where
    /// the answer is a picture, or random.
    pub expected: &'static [Expected],
}

const fn e(what: &'static str, any_of: &'static [(f64, f64)]) -> Expected {
    Expected { what, any_of }
}

const fn q(id: &'static str, pattern: Pattern, field: &'static str, ask: &'static str, expected: &'static [Expected]) -> Question {
    Question { id, ask, pattern, field, held_out: false, expected }
}

const fn held(id: &'static str, pattern: Pattern, field: &'static str, ask: &'static str, expected: &'static [Expected]) -> Question {
    Question { id, ask, pattern, field, held_out: true, expected }
}

use Pattern::*;

/// The 32 questions: 22 to look at and improve against, 10 held out.
pub fn questions() -> Vec<Question> {
    QUESTIONS.to_vec()
}

const QUESTIONS: &[Question] = &[
        // ── The seven of §1, with what they left open fixed ──
        q("dijkstra", Step, "thuật toán", "Minh hoạ thuật toán Dijkstra từ A trên đồ thị này, có nút bước tiếp và lùi lại, và hiện bảng khoảng cách sau mỗi bước: A–B 4, A–C 2, C–B 1, B–D 5, C–D 8, C–E 10, D–E 2, D–F 6, E–F 3, E–G 1, F–G 4.",
          &[e("khoảng cách tới F", &[(13.0, 0.0)]), e("khoảng cách tới G", &[(11.0, 0.0)])]),
        q("pendulum", Explore, "vật lý", "Mô phỏng con lắc đơn, cho tao chỉnh chiều dài dây và góc thả, hiện chu kỳ. Bắt đầu ở dây 1 m, góc thả 30°.",
          &[e("chu kỳ thật ở 30°", &[(2.041, 0.01)])]),
        q("gibbs", Zoom, "toán", "Mô phỏng chuỗi Fourier của sóng vuông và hiện tượng Gibbs, bắt đầu với 5 số hạng (các hài 1, 3, 5, 7, 9).",
          &[e("đỉnh vọt với 5 số hạng", &[(1.182, 0.006), (18.2, 0.6), (0.182, 0.006), (0.091, 0.003)])]),
        q("bayes", Decompose, "xác suất y khoa", "Một xét nghiệm có độ nhạy 99% và độ đặc hiệu 95%. Bệnh có tỷ lệ mắc 1%. Một người xét nghiệm dương tính thì xác suất họ thật sự mắc bệnh là bao nhiêu? Minh hoạ trực quan cho tao hiểu vì sao, và cho tao chỉnh tỷ lệ mắc, độ nhạy, độ đặc hiệu.",
          &[e("xác suất sau dương tính", &[(16.67, 0.1), (0.1667, 0.001)]), e("dương tính giả trên 10.000", &[(495.0, 0.5)])]),
        q("tax", Areas, "kinh tế", "Thị trường có cầu Qd = 100 − 2P và cung Qs = 3P − 20 (P tính bằng nghìn đồng). Nhà nước đánh thuế 2 nghìn đồng mỗi đơn vị vào người bán. Minh hoạ cho tao thấy người mua và người bán mỗi bên chịu bao nhiêu thuế, thu ngân sách bao nhiêu, mất mát xã hội bao nhiêu, và cho tao kéo mức thuế.",
          &[e("giá người mua trả", &[(25.2, 0.05)]), e("giá người bán nhận", &[(23.2, 0.05)]), e("thu ngân sách", &[(99.2, 0.1)]), e("mất mát xã hội", &[(2.4, 0.05)])]),
        q("drift", Compare, "sinh học", "Mô phỏng trôi dạt di truyền theo mô hình Wright–Fisher: chạy 20 quần thể song song qua 200 thế hệ, cho tao chỉnh kích thước quần thể N và tần số alen ban đầu p₀. Cho tao thấy vì sao quần thể nhỏ mất đa dạng nhanh hơn.",
          &[]),
        q("loan", Compare, "tài chính", "Giải thích khoản vay 500 triệu, 20 năm, lãi 9%/năm: so sánh trả góp đều với trả theo dư nợ giảm dần. Cho tao kéo lãi suất và thời hạn, vẽ dư nợ và tiền trả mỗi tháng theo thời gian, hiện tổng tiền lãi của mỗi cách.",
          &[e("tổng lãi trả góp đều", &[(579.67, 0.2), (579_671_147.0, 200_000.0)]), e("tổng lãi dư nợ giảm dần", &[(451.88, 0.2), (451_875_000.0, 200_000.0)])]),
        // ── Fifteen more to look at ──
        q("euclid", Step, "số học", "Minh hoạ từng bước thuật toán Euclid tìm ước chung lớn nhất của 1071 và 462, có nút bước tiếp và lùi lại.",
          &[e("số dư đầu tiên", &[(147.0, 0.0)]), e("ước chung lớn nhất", &[(21.0, 0.0)])]),
        q("schedule", Step, "tài chính", "Minh hoạ từng tháng lịch trả nợ của khoản vay 120 triệu trong 12 tháng, lãi 12%/năm, trả gốc đều: mỗi bước một tháng, hiện gốc, lãi và dư nợ.",
          &[e("tiền trả tháng đầu", &[(11.2, 0.05), (11_200_000.0, 50_000.0)])]),
        q("binary-search", Step, "thuật toán", "Minh hoạ từng bước tìm kiếm nhị phân số 37 trong mảng [3, 8, 12, 17, 21, 26, 30, 37, 41, 45, 50], có nút bước tiếp.",
          &[]),
        q("compound", Compare, "tài chính", "So sánh lãi kép với lãi đơn khi gửi 100 triệu, lãi 8%/năm trong 30 năm: đặt hai cách cạnh nhau và cho tao kéo lãi suất.",
          &[e("lãi kép sau 30 năm", &[(1006.27, 0.5), (1_006_265_689.0, 500_000.0), (1.00627, 0.0005)]), e("lãi đơn sau 30 năm", &[(340.0, 0.5), (340_000_000.0, 500_000.0)])]),
        q("sorting", Compare, "khoa học máy tính", "So sánh số phép so sánh của insertion sort trong trường hợp xấu nhất với merge sort khi n tăng từ 1 tới 1000. Đặt hai cách cạnh nhau để thấy vì sao O(n log n) thắng O(n²).",
          &[e("insertion sort xấu nhất với n = 1000", &[(499_500.0, 1.0)])]),
        q("braking", Compare, "vật lý", "Vì sao xe chạy 120 km/h cần quãng phanh gấp bốn lần lúc 60 km/h? Minh hoạ quãng phanh theo tốc độ với hệ số ma sát 0,7.",
          &[e("quãng phanh ở 60 km/h", &[(20.2, 0.4)]), e("quãng phanh ở 120 km/h", &[(80.9, 1.0)])]),
        q("sir", Compare, "dịch tễ học", "So sánh hai trận dịch theo mô hình SIR với R0 = 1,5 và R0 = 3 (γ = 0,1/ngày, dân số 10.000, 10 ca ban đầu): đặt cạnh nhau và cho thấy tổng số người từng nhiễm của mỗi trận.",
          &[e("tổng nhiễm với R0 = 3", &[(94.0, 1.5), (9_405.0, 150.0), (0.94, 0.015)]), e("tổng nhiễm với R0 = 1,5", &[(58.3, 1.5), (5_828.0, 150.0), (0.583, 0.015)])]),
        q("punnett", Decompose, "di truyền học", "Minh hoạ phép lai AaBb × AaBb bằng bảng Punnett và cho tao thấy tỉ lệ kiểu hình mang cả hai tính trạng trội.",
          &[e("tỉ lệ kiểu hình trội cả hai", &[(56.25, 0.1), (0.5625, 0.001)])]),
        q("electricity", Decompose, "kinh tế hộ gia đình", "Phân tích hoá đơn tiền điện 450 kWh theo giá bậc thang: bậc 1 (0–50 kWh) 1.893 đ, bậc 2 (51–100) 1.956 đ, bậc 3 (101–200) 2.271 đ, bậc 4 (201–300) 2.860 đ, bậc 5 (301–400) 3.197 đ, bậc 6 (từ 401) 3.302 đ mỗi kWh, chưa tính thuế. Minh hoạ mỗi bậc đóng góp bao nhiêu.",
          &[e("tổng tiền chưa thuế", &[(1_190_350.0, 1.0)])]),
        q("glucose", Decompose, "hoá học", "Phân tích thành phần phần trăm khối lượng các nguyên tố trong glucose C6H12O6, minh hoạ trực quan.",
          &[e("phần trăm cacbon", &[(40.0, 0.2)]), e("phần trăm oxy", &[(53.3, 0.2)])]),
        q("resonance", Zoom, "vật lý", "Minh hoạ hiện tượng cộng hưởng của dao động cưỡng bức có cản với tần số riêng ω0 = 1 và hệ số cản ζ = 0,1: cho thấy đỉnh biên độ ở đâu và cao bao nhiêu.",
          &[e("tần số đỉnh", &[(0.99, 0.005)]), e("biên độ đỉnh so với độ lệch tĩnh", &[(5.03, 0.05)])]),
        q("euler-e", Zoom, "giải tích", "Minh hoạ vì sao (1 + 1/n)^n tiến tới e khi n tăng, và phóng to để thấy nó hội tụ chậm thế nào.",
          &[e("e", &[(2.718, 0.001)])]),
        q("break-even", Areas, "kinh doanh", "Một cửa hàng có chi phí cố định 50 triệu, chi phí biến đổi 30.000 đ mỗi sản phẩm và giá bán 50.000 đ. Minh hoạ điểm hoà vốn, vùng lỗ và vùng lãi.",
          &[e("sản lượng hoà vốn", &[(2_500.0, 1.0)]), e("doanh thu hoà vốn", &[(125.0, 0.5), (125_000_000.0, 500_000.0)])]),
        q("projectile", Explore, "vật lý", "Mô phỏng chuyển động ném xiên, cho tao chỉnh vận tốc đầu và góc ném, bắt đầu ở 20 m/s và 45°, bỏ qua sức cản không khí.",
          &[e("tầm xa", &[(40.8, 0.3)]), e("độ cao cực đại", &[(10.2, 0.15)])]),
        q("logistic", Explore, "sinh thái học", "Mô phỏng tăng trưởng quần thể logistic với sức chứa K = 1000, tốc độ r = 0,5/ngày và 10 cá thể ban đầu; cho tao chỉnh r và K, và cho biết sau bao lâu quần thể đạt nửa sức chứa.",
          &[e("thời điểm đạt K/2", &[(9.19, 0.1)])]),
        // ── Held out: only their share of the totals is ever read ──
        held("kruskal", Step, "lý thuyết đồ thị", "Minh hoạ từng bước thuật toán Kruskal tìm cây khung nhỏ nhất của đồ thị: A–B 7, A–D 5, B–C 8, B–D 9, B–E 7, C–E 5, D–E 15, D–F 6, E–F 8, E–G 9, F–G 11.",
             &[e("tổng trọng số cây khung", &[(39.0, 0.0)])]),
        held("horner", Step, "đại số", "Minh hoạ từng bước chia đa thức x³ − 6x² + 11x − 6 cho x − 1 bằng sơ đồ Horner.",
             &[]),
        held("savings", Compare, "tài chính", "So sánh gửi tiết kiệm 200 triệu lãi 6%/năm trong 10 năm khi ghép lãi hằng tháng và khi ghép lãi hằng năm, đặt hai cách cạnh nhau.",
             &[e("ghép lãi hằng tháng", &[(363.88, 0.3), (363_879_347.0, 300_000.0)]), e("ghép lãi hằng năm", &[(358.17, 0.3), (358_169_539.0, 300_000.0)])]),
        held("rc", Compare, "điện tử", "So sánh mạch RC nạp điện với R = 10 kΩ, C = 100 µF và với R = 20 kΩ, C = 100 µF: hiện hằng số thời gian và thời gian tụ đạt 99% điện áp nguồn.",
             &[e("99% với 10 kΩ", &[(4.61, 0.05)]), e("99% với 20 kΩ", &[(9.21, 0.05)])]),
        held("diet", Decompose, "dinh dưỡng", "Phân tích khẩu phần 2.000 kcal theo tỉ lệ năng lượng 50% tinh bột, 30% chất béo và 20% đạm thành số gam mỗi chất, minh hoạ trực quan.",
             &[e("gam tinh bột", &[(250.0, 1.0)]), e("gam chất béo", &[(66.7, 0.5)])]),
        held("venn", Decompose, "tập hợp", "Một lớp 40 học sinh có 25 em học tiếng Anh, 18 em học tiếng Nhật, 7 em học cả hai. Minh hoạ bằng biểu đồ Venn: bao nhiêu em học ít nhất một ngoại ngữ, bao nhiêu em không học?",
             &[e("học ít nhất một ngoại ngữ", &[(36.0, 0.0)])]),
        held("overshoot", Zoom, "điều khiển", "Minh hoạ đáp ứng bậc thang của hệ bậc hai với hệ số cản ζ = 0,2 và phóng to quanh đỉnh để thấy độ quá điều chỉnh.",
             &[e("độ quá điều chỉnh", &[(52.7, 0.5), (1.527, 0.005), (0.527, 0.005)])]),
        held("surplus", Areas, "kinh tế", "Minh hoạ thặng dư người tiêu dùng và thặng dư nhà sản xuất của thị trường có cầu P = 100 − Q và cung P = 20 + Q.",
             &[e("thặng dư người tiêu dùng", &[(800.0, 1.0)]), e("sản lượng cân bằng", &[(40.0, 0.0)])]),
        held("spring", Explore, "vật lý", "Mô phỏng con lắc lò xo với k = 50 N/m, m = 0,5 kg, biên độ 0,1 m; cho tao chỉnh k và m, hiện chu kỳ và cơ năng.",
             &[e("chu kỳ", &[(0.628, 0.005)]), e("cơ năng", &[(0.25, 0.005)])]),
        held("gas", Explore, "hoá học", "Mô phỏng định luật khí lý tưởng với 1 mol khí ở 300 K, cho tao chỉnh thể tích và nhiệt độ, hiện áp suất; bắt đầu ở thể tích 24,6 lít.",
             &[e("áp suất ban đầu", &[(101.4, 0.5), (101_390.0, 500.0)])]),
];

// ── Numbers as a visual or an answer writes them ──

/// Every number in `text`, each in every reading it could have: "1,500" is
/// one thousand five hundred in English and one and a half in Vietnamese, and
/// which one a visual meant is not knowable from the text.
pub fn numbers_in(text: &str) -> Vec<f64> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || (matches!(chars[i], '.' | ',') && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()))) {
            i += 1;
        }
        out.extend(readings(&chars[start..i].iter().collect::<String>()));
    }
    out
}

fn readings(raw: &str) -> Vec<f64> {
    let dots = raw.matches('.').count();
    let commas = raw.matches(',').count();
    let parse = |s: String| s.parse::<f64>().ok();
    let grouped = |sep: char| raw.split(sep).skip(1).all(|g| g.len() == 3);
    let found = match (dots, commas) {
        (0, 0) => vec![parse(raw.to_string())],
        (d, c) if d > 0 && c > 0 => {
            // The last mark is the decimal one: 1.234,5 or 1,234.5.
            let decimal = if raw.rfind(',') > raw.rfind('.') { ',' } else { '.' };
            let thousands = if decimal == ',' { '.' } else { ',' };
            vec![parse(raw.replace(thousands, "").replace(decimal, "."))]
        }
        (1, 0) | (0, 1) => {
            let sep = if dots == 1 { '.' } else { ',' };
            let mut both = vec![parse(raw.replace(sep, "."))];
            if grouped(sep) {
                both.push(parse(raw.replace(sep, "")));
            }
            both
        }
        _ => {
            let sep = if dots > 1 { '.' } else { ',' };
            if grouped(sep) { vec![parse(raw.replace(sep, ""))] } else { Vec::new() }
        }
    };
    found.into_iter().flatten().collect()
}

/// Whether every expected number is among `numbers`. `None` when the question
/// has none to check.
pub fn correct(question: &Question, numbers: &[f64]) -> Option<bool> {
    if question.expected.is_empty() {
        return None;
    }
    Some(question.expected.iter().all(|e| e.any_of.iter().any(|(v, t)| numbers.iter().any(|n| (n - v).abs() <= t + 1e-9))))
}

// ── The judge ──

/// What the judge is told. Fixed: changed only before a measurement, never
/// after one (the plan's §2.5).
pub const RUBRIC: &str = "You grade an interactive visual that an assistant drew under its answer to a question. \
The screenshot may show the chat around the visual, overlays such as a scroll button, or only part of the visual where \
the screenshot was cut: ignore all of that, and grade only the part of the visual that is shown. Score three things from 0 to 2.\n\
point: first name, to yourself, what the question wants the reader to see — why one case differs from another, how a \
process unfolds step by step, where a phenomenon happens and how big it is, what a whole is made of, who bears how much. \
2: the visual is built around exactly that, and a reader who never touches a control sees it: cases side by side on one \
scale, steps that mark what is done, current and still to come, the phenomenon magnified or measured, the parts shown as \
parts of the whole with their numbers. 1: the topic is drawn, but the reader must work for the point — move a slider and \
remember, read it off a single number, or infer it. 0: the point is missing, or the visual is about something else.\n\
presentation: count these faults — an axis without a label or unit; colours that mean something without a legend; text \
overlapping, cut off inside the visual, or crossed by a line; a control without its current value or its range; a large \
number that only repeats a control's value; plain unstyled controls or large empty areas; sentences of explanation inside \
the visual. 2: none; 1: one or two; 0: three or more, or hard to read.\n\
correct: is what is visible right — numbers, shapes, labels, terms? Check the numbers you can check from the question. \
0: something visible is wrong; 1: cannot tell, or a small slip; 2: right as far as can be seen.\n\
Do not reward size, colour or polish for their own sake. Reply with JSON only: \
{\"point\":n,\"presentation\":n,\"correct\":n,\"why\":\"one sentence\"}.";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Judged {
    pub point: f64,
    pub presentation: f64,
    pub correct: f64,
}

impl Judged {
    pub fn total(&self) -> f64 {
        self.point + self.presentation + self.correct
    }
}

fn schema() -> serde_json::Value {
    let score = serde_json::json!({ "type": "integer", "minimum": 0, "maximum": 2 });
    serde_json::json!({
        "type": "object",
        "properties": { "point": score, "presentation": score, "correct": score, "why": { "type": "string" } },
        "required": ["point", "presentation", "correct", "why"],
        "additionalProperties": false,
    })
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if v.is_empty() {
        0.0
    } else {
        v[v.len() / 2]
    }
}

/// One screenshot, judged three times; the median of each score. `image` is a
/// `data:` URL. `None` if no call came back as JSON.
pub async fn judge(provider: &dyn crate::syn::provider::ChatProvider, model: &str, question: &str, image: &str) -> Option<(Judged, Vec<String>)> {
    let schema = schema();
    let mut scores: Vec<Judged> = Vec::new();
    let mut why = Vec::new();
    for _ in 0..3 {
        let mut user = crate::syn::provider::ChatMessage::new("user", format!("Question: {question}"));
        user.images = Some(vec![image.to_string()]);
        let messages = [crate::syn::provider::ChatMessage::new("system", RUBRIC), user];
        let request = |json_schema| crate::syn::provider::ChatRequest {
            model,
            messages: &messages,
            temperature: None,
            num_ctx: 32_768,
            tools: None,
            json_schema,
        };
        // A call that never came back held a calibration run for an hour and a
        // half on 2026-10-28; three minutes is ten times a slow one.
        let within = |call| tokio::time::timeout(std::time::Duration::from_secs(180), call);
        let reply = match within(provider.chat(request(Some(&schema)))).await {
            Ok(Ok(r)) => Ok(r),
            Ok(Err(_)) => within(provider.chat(request(None))).await.map_err(|_| ()).and_then(|r| r.map_err(|_| ())),
            Err(_) => Err(()),
        };
        let Ok(reply) = reply else { continue };
        let text = reply.content.trim();
        let body = text.find('{').and_then(|a| text.rfind('}').map(|b| &text[a..=b])).unwrap_or(text);
        let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { continue };
        let n = |k: &str| v.get(k).and_then(|x| x.as_f64()).map(|x| x.clamp(0.0, 2.0));
        if let (Some(point), Some(presentation), Some(correct)) = (n("point"), n("presentation"), n("correct")) {
            scores.push(Judged { point, presentation, correct });
            why.push(v.get("why").and_then(|w| w.as_str()).unwrap_or_default().to_string());
        }
    }
    if scores.is_empty() {
        return None;
    }
    Some((
        Judged {
            point: median(scores.iter().map(|s| s.point).collect()),
            presentation: median(scores.iter().map(|s| s.presentation).collect()),
            correct: median(scores.iter().map(|s| s.correct).collect()),
        },
        why,
    ))
}

/// An image file as a `data:` URL.
pub fn data_url(path: &std::path::Path) -> Option<String> {
    use base64::Engine as _;
    let bytes = std::fs::read(path).ok()?;
    let kind = match path.extension().and_then(|e| e.to_str()) {
        Some("jpg" | "jpeg") => "jpeg",
        _ => "png",
    };
    Some(format!("data:image/{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

// ── Calibration ──

/// Screenshots a person compared and ranked (`calibration/calibration.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct Calibration {
    pub questions: Vec<Ranked>,
    /// A screenshot the judge must score low on `correct`, however it looks.
    pub wrong: Vec<String>,
    /// How many of the ranked questions the judge must agree with.
    pub agree_at_least: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ranked {
    pub id: String,
    pub ask: String,
    /// `who` → file.
    pub shots: Vec<Shot>,
    /// Who the person put first.
    pub best: String,
    /// Who the person put last; two when they could not tell them apart.
    pub worst: Vec<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Shot {
    pub who: String,
    pub file: String,
}

/// Whether the judge's totals agree with a person's ranking of one question:
/// the person's first strictly above the rest, and their last (when there is
/// one) not above anything in the middle.
pub fn agrees(ranked: &Ranked, total: &dyn Fn(&str) -> f64) -> bool {
    let best = total(&ranked.best);
    let others: Vec<&Shot> = ranked.shots.iter().filter(|s| s.who != ranked.best).collect();
    if !others.iter().all(|s| total(&s.who) < best) {
        return false;
    }
    if ranked.worst.len() != 1 {
        return true;
    }
    let worst = total(&ranked.worst[0]);
    others.iter().filter(|s| s.who != ranked.worst[0]).all(|s| worst <= total(&s.who))
}

/// The judge passes when it agrees with the person on enough questions and
/// scores every visual known to be wrong at most 1 on `correct`.
pub fn calibrated(calibration: &Calibration, scored: &std::collections::HashMap<String, Judged>) -> (bool, usize) {
    let agreeing = calibration
        .questions
        .iter()
        .filter(|q| {
            let total = |who: &str| {
                q.shots.iter().find(|s| s.who == who).and_then(|s| scored.get(&s.file)).map(Judged::total).unwrap_or(0.0)
            };
            agrees(q, &total)
        })
        .count();
    let wrong_seen = calibration.wrong.iter().all(|file| scored.get(file).is_some_and(|j| j.correct <= 1.0));
    (agreeing >= calibration.agree_at_least && wrong_seen, agreeing)
}

// ── What a trial came to, and the totals ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
    /// Empty for a held-out question: its trials are counted, not read.
    pub id: String,
    pub pattern: Pattern,
    pub held_out: bool,
    pub drew: bool,
    pub lint_clean: bool,
    /// The checks that failed, for a question that is looked at.
    pub problems: Vec<String>,
    pub correct: Option<bool>,
    pub judged: Option<Judged>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Totals {
    pub trials: usize,
    pub drew: f64,
    pub lint_clean: f64,
    pub correct: f64,
    /// Trials with a number to check.
    pub correct_of: usize,
    pub point: f64,
    pub presentation: f64,
    pub judged_of: usize,
}

/// The totals of `trials`. A trial that drew nothing scores 0 on the judge's
/// two and does not pass the lint.
pub fn totals<'a>(trials: impl IntoIterator<Item = &'a Trial>) -> Totals {
    let trials: Vec<&Trial> = trials.into_iter().collect();
    let n = trials.len();
    let share = |count: usize, of: usize| if of == 0 { 0.0 } else { count as f64 / of as f64 };
    let checked: Vec<&&Trial> = trials.iter().filter(|t| t.correct.is_some()).collect();
    let judged: Vec<Judged> = trials.iter().map(|t| if t.drew { t.judged.unwrap_or_default() } else { Judged::default() }).collect();
    let judged_of = trials.iter().filter(|t| !t.drew || t.judged.is_some()).count();
    let mean = |f: fn(&Judged) -> f64| {
        let scored: Vec<f64> = trials.iter().zip(&judged).filter(|(t, _)| !t.drew || t.judged.is_some()).map(|(_, j)| f(j)).collect();
        if scored.is_empty() { 0.0 } else { scored.iter().sum::<f64>() / scored.len() as f64 }
    };
    Totals {
        trials: n,
        drew: share(trials.iter().filter(|t| t.drew).count(), n),
        lint_clean: share(trials.iter().filter(|t| t.lint_clean).count(), n),
        correct: share(checked.iter().filter(|t| t.correct == Some(true)).count(), checked.len()),
        correct_of: checked.len(),
        point: mean(|j| j.point),
        presentation: mean(|j| j.presentation),
        judged_of,
    }
}

/// The report: totals for the questions looked at and for the held-out ones,
/// the looked-at ones by pattern and by question. Nothing of a held-out
/// question beyond its share of the totals.
pub fn report(trials: &[Trial], judge_trusted: Option<bool>) -> String {
    let row = |name: &str, t: &Totals| {
        format!(
            "| {name} | {} | {:.0}% | {:.0}% | {} | {:.2} | {:.2} |\n",
            t.trials,
            t.drew * 100.0,
            t.lint_clean * 100.0,
            // A question with no number to find is not 0% right.
            if t.correct_of == 0 { "—".to_string() } else { format!("{:.0}% ({})", t.correct * 100.0, t.correct_of) },
            t.point,
            t.presentation
        )
    };
    let head = "| | Lần thử | Vẽ | Tự kiểm qua | Đúng (số lần có đáp số) | Tầng 1 | Tầng 2 |\n| --- | --- | --- | --- | --- | --- | --- |\n";
    let mut out = String::new();
    out.push_str(&format!(
        "Người chấm: {}\n\n",
        match judge_trusted {
            Some(true) => "đã qua hiệu chỉnh",
            Some(false) => "CHƯA qua hiệu chỉnh — điểm tầng 1 và 2 chưa dùng được",
            None => "chưa hiệu chỉnh",
        }
    ));
    out.push_str(head);
    let dev: Vec<&Trial> = trials.iter().filter(|t| !t.held_out).collect();
    let kept: Vec<&Trial> = trials.iter().filter(|t| t.held_out).collect();
    out.push_str(&row("Phát triển", &totals(dev.iter().copied())));
    out.push_str(&row("Giữ lại", &totals(kept.iter().copied())));
    out.push_str(&row("Tất cả", &totals(trials.iter())));
    out.push_str("\nPhần phát triển, theo kiểu giải thích:\n\n");
    out.push_str(head);
    for p in Pattern::ALL {
        let these: Vec<&Trial> = dev.iter().copied().filter(|t| t.pattern == p).collect();
        if !these.is_empty() {
            out.push_str(&row(p.name(), &totals(these)));
        }
    }
    out.push_str("\nPhần phát triển, theo câu:\n\n");
    out.push_str(head);
    let mut ids: Vec<&str> = dev.iter().map(|t| t.id.as_str()).collect();
    ids.dedup();
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            continue;
        }
        out.push_str(&row(id, &totals(dev.iter().copied().filter(|t| t.id == id))));
    }
    let mut problems: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for t in &dev {
        for p in &t.problems {
            *problems.entry(p.as_str()).or_default() += 1;
        }
    }
    if !problems.is_empty() {
        out.push_str("\nLỗi tự kiểm, phần phát triển (số lần thử):\n\n");
        for (p, n) in problems {
            out.push_str(&format!("- {p}: {n}\n"));
        }
    }
    out
}
