# Eval — trích xuất ký ức từ văn bản, 2026-09-15

**Trạng thái:** dự đoán, ghi **trước khi đo**, và trước khi viết prompt trích xuất. Kết quả sẽ được thêm
vào cuối file. Phần dự đoán không được sửa lại sau đó, kể cả khi nó sai.
**Đo cho:** Nhát E của `docs/tua-lai-2026-09-14.md`.

## Hỏi gì

Gate của Nhát E có một điều kiện trước mọi thứ khác: **precision đo trên một tập nhãn tay trước khi
bật cho ai**. Nên trích xuất mặc định **tắt**. Bật lên là việc của người dùng, sau khi đọc được con số
dưới đây.

Chia làm hai phần, đo riêng.

1. **Phần không cần model** (offline, test thường):
   - quy `happened` tương đối về đúng khoảng theo ngày ghi (`recorded`);
   - nối tên người về đúng node người;
   - chọn đúng nguồn: bỏ lượt `assistant`, bỏ nội dung phong ấn, bỏ note có `timeline: false`.
2. **Phần cần model** (`#[ignore]`, chạy tay): trên một tập note dựng tay, mỗi note có nhãn là những
   ký ức một người đọc kỹ sẽ rút ra, gồm chuyện gì, khi nào và với ai.

## Tập nhãn

- 12 note tiếng Việt, dựng tay, không lấy từ vault thật:
  - 5 daily note;
  - 3 interaction;
  - 2 person note;
  - 2 quickcap.
- Trong đó 3 note **không có ký ức nào**: một danh sách việc cần làm, một ghi chép kiến thức, một kế
  hoạch cho tuần sau. Chúng đo xem model có bịa ra chuyện đã xảy ra từ một dự định hay không.
- Một mục trích ra được tính **đúng** khi khoảng thời gian của nó nằm trong khoảng của một nhãn, và tập
  người của nó trùng với nhãn đó. Tiêu đề không được so, vì cùng một chuyện có trăm cách gọi.

## Dự đoán

### Offline

- `happened` tương đối được quy đúng ở **mọi** trường hợp trong test. Đây là code tất định; sai là bug,
  không phải tỉ lệ.

### Model thật (provider mặc định của máy chạy test)

- **Precision ≥ 0.7.** Chỗ sai tao đoán nhiều nhất là **ngày**, không phải chuyện: model sẽ gán ngày
  ghi note cho một chuyện được kể lại là đã xảy ra từ trước ("hồi cấp 3…"), thay vì để khoảng mờ.
- **Recall ≥ 0.6.** Recall không phải gate, nhưng con số quá thấp nghĩa là tính năng không đáng bật.
- **Note không có ký ức:** model trích ra ít nhất một mục ở **1/3** note. Tao đoán là bản kế hoạch tuần
  sau, vì nó có ngày và có tên người.
- **Tên người:** ít nhất 90% tên model đưa ra nối được về một node người có thật trong vault mẫu. Tên
  không nối được thì giữ lại dạng chữ, không bị bỏ.

## Vì sao ghi ra trước

Giống `eval-timeline-questions-2026-09-15.md`: một dự đoán chỉ viết sau khi đã có con số thì không bao
giờ sai, nên cũng không cho biết điều gì. Ở đây còn thêm một lý do: con số này quyết định có bật tính
năng cho ai hay không.

---

## Kết quả

### Offline — đo 2026-09-15

```bash
cargo test --lib timeline::extract
```

| Dự đoán | Đo được |
| --- | --- |
| `happened` tương đối quy đúng ở mọi trường hợp trong test | **đúng**: hôm qua, tuần trước, sáng nay, không ghi thời gian, tháng này, cả năm (cắt ở ngày ghi) |

Còn hai điều test cho thấy mà dự đoán không nói tới:

- **Thời gian không đọc được bị bỏ.** Một mốc code không đọc được ("hồi cấp 3"), một thời gian sau ngày ghi (kế hoạch), hay một note về người không có thời gian nào, đều bị bỏ, và số lượng mỗi loại được đếm riêng.
- **Người không đoán bừa.** Khi vault có hai người tên Tuấn, "Tuấn" không được nối về ai, và tên được giữ nguyên dạng chữ.

Bộ note trong eval thật không bị sửa để khớp phần offline. Thứ duy nhất đổi so với lúc ghi dự đoán là prompt
trích xuất. Prompt được viết **sau** file này, trong lúc dựng tập nhãn: một note tổng kết năm viết "tháng 4" không kèm
năm, nên prompt cho phép model tự điền năm theo ngày ghi. Note đó sau cùng bị thay bằng một bản kế hoạch tuần sau,
để có đủ ba note không chứa ký ức như đã hứa ở trên.

### Model thật — chưa chạy

Test `timeline::extract::tests::live::precision_on_hand_labelled_notes` đã sẵn: 12 note, 12 nhãn,
3 note không có ký ức. Test in precision, recall, số note rỗng bị trích ra mục, và tỉ lệ tên nối được.
Test tốn tiền API (hoặc vài phút Ollama), nên chưa chạy khi chưa được đồng ý.

```bash
cargo test --lib timeline::extract::tests::live -- --ignored --nocapture
```

Trích xuất vẫn **tắt mặc định** cho tới khi có con số này.
