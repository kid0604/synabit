# Eval — câu hỏi về thời gian, 2026-09-15

**Trạng thái:** dự đoán, ghi **trước khi đo**. Kết quả sẽ được thêm vào cuối file sau khi chạy.
Phần dự đoán không được sửa lại sau đó, kể cả khi nó sai.
**Đo cho:** §6 (cách Timeline nói) của `docs/timeline-2026-09-17.md`.

## Hỏi gì

Gate của Nhát D tách hai câu hỏi, và mỗi câu được đo riêng:

1. **Có tra timeline không.** Harness phải tự nhận ra câu hỏi là về thời gian, quy nó về đúng
   khoảng thời gian, rồi tra timeline trước khi hỏi model. Không chờ model nhớ ra mà gọi tool,
   vì `recall` đã cho thấy chờ như vậy là 0 lần trong 15 run. Phần này không cần model, nên đo
   bằng một test thường, chạy offline.
2. **Có trả lời đúng không.** Khi khối timeline đã nằm trong prompt, model có nêu đúng những gì
   vault ghi và không thêm gì vault không ghi. Phần này cần model thật, là test `#[ignore]`
   chạy tay vì tốn tiền API.

## Dự đoán

### 1. Nhận ra câu hỏi

- **Câu hỏi về thời gian:** nhận ra và quy về đúng khoảng ở **ít nhất 90%** số câu, tức đạt đúng
  ngưỡng 9/10 của gate.
  - Dự đoán chỗ trượt: một câu mô tả mùa hoặc giai đoạn trong năm, như "mùa hè năm ngoái".
    Harness sẽ quy nó về cả năm ngoái, tức là đúng năm nhưng rộng hơn. Tao tính đó là **đúng
    một phần** và ghi riêng ra, không gộp vào "đúng".
  - Những câu không có mốc nào ("hồi đó", "dạo ấy") **không** được tính là câu về thời gian. Không
    có khoảng nào để tra, và đoán ra một khoảng thì còn tệ hơn không tra.
- **Câu hỏi không về thời gian:** nhận nhầm **nhiều nhất 1 câu**. Dự đoán câu nhầm là một câu có
  năm nằm trong ngữ cảnh không phải hỏi về đời mình, kiểu "iPhone ra năm 2025 giá bao nhiêu".

### 2. Trả lời đúng (chạy tay, model thật)

- **Nêu đủ những gì vault ghi** cho khoảng được hỏi: **ít nhất 8/10** câu.
- **Bịa** (nêu một mục vault không có trong khoảng đó): **nhiều nhất 1/10**.
- **Khoảng không có gì:** model nói rõ là vault không ghi gì, thay vì kể chung chung, ở **ít nhất
  2/3** câu kiểu này. Đây là dự đoán tao kém chắc nhất: model có thói quen lấp chỗ trống.

## Vì sao ghi ra trước

Hai ADR trước đều có một dự đoán sai vì đo mới lộ ra. Một dự đoán chỉ viết sau khi đã có con
số thì không bao giờ sai, nên cũng không cho biết điều gì.

---

## Kết quả

### 1. Nhận ra câu hỏi — đo 2026-09-15, offline

```bash
cargo test --lib timeline::asked::tests::which_questions_are_about_a_time -- --nocapture
```

Hôm nay tính là 2026-09-15. Bộ câu hỏi nằm trong test: 24 câu về thời gian (tiếng Việt có dấu và
không dấu, tiếng Anh, mốc viết ra và mốc tương đối) và 12 câu không. Bộ câu hỏi được viết cùng lúc
với phần nhận diện, sau khi đã ghi dự đoán. Nên hiểu con số là: phần nhận diện đọc được đúng những
kiểu câu mà người viết nó nghĩ ra. Chưa phải là đọc được câu người dùng thật hỏi.

| | Dự đoán | Đo được |
| --- | --- | --- |
| Tra timeline ở câu về thời gian | ≥90% | **24/24**: 23 đúng khoảng, 1 đúng một phần |
| Chỗ đúng một phần | "mùa hè năm ngoái" → cả năm ngoái | **đúng như dự đoán** |
| Báo nhầm ở câu không về thời gian | ≤1, là câu kiểu "iPhone ra năm 2025" | **2: dự đoán sai** |

**Chỗ dự đoán sai.** Câu "iPhone 17 ra năm 2025 giá bao nhiêu" bị báo nhầm như dự đoán. Nhưng còn
một câu tao không lường tới: "đọc hết feed **tuần này** rồi tổng hợp". Đó là một thời gian thật,
chỉ không phải thời gian trong đời người dùng. Regex không phân biệt được hai thứ đó, và tao không
thêm luật riêng cho chữ "feed", vì làm vậy là vá cho vừa bộ đo.

Cái giá của một lần báo nhầm là prompt có thêm một section liệt kê những gì vault ghi cho tuần đó,
chứ không phải một câu trả lời sai. Assert trong test được nới từ ≤1 lên ≤2 **sau khi đo**, để chặn
tình hình tệ đi, và comment trong test nói rõ điều đó. Gate thật (≥9/10) không đổi.

### 2. Trả lời đúng — chưa chạy

Test `timeline::asked::tests::live::answers_about_a_time_from_the_timeline_block` đã sẵn: 10 câu
trên một vault dựng trong bộ nhớ, 3 trong số đó hỏi về khoảng không có gì. Test tốn tiền API nên
chưa chạy khi chưa được đồng ý.

```bash
cargo test --lib timeline::asked::tests::live -- --ignored --nocapture
```

### Đo lại sau review — 2026-09-16

Phần đọc thời gian được sửa sau review: thêm số đếm tiếng Việt, tuần trước và ngày trước kèm số, bỏ qua câu có
"mấy/vài năm trước", và năm trần chỉ được đọc khi có từ chỉ thời gian đứng trước hoặc khi nằm cuối câu. Chạy lại
đúng bộ câu hỏi cũ, không thêm hay bớt câu nào:

| | Lần đầu | Sau review |
| --- | --- | --- |
| Câu về thời gian | 23 đúng khoảng, 1 đúng một phần | 23 đúng khoảng, 1 đúng một phần |
| Báo nhầm | 2 | 2 (vẫn là hai câu cũ) |

Những lỗi review tìm ra ("hai năm trước" bị đọc thành năm ngoái, "ngân sách 2000 đô" bị đọc thành năm 2000) không
có trong bộ câu hỏi này. Đó là giới hạn của bộ đo, không phải bằng chứng là phần đọc đã đủ. Các trường hợp đó giờ có
test riêng: `a_count_in_words_or_a_vague_one_is_read_as_said` và `an_amount_is_not_a_year`.
