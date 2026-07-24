use loro::LoroDoc;
use similar::{DiffOp, TextDiff};

/// Áp dụng nội dung text mới vào LoroDoc thông qua thuật toán diff.
/// Sử dụng word-level diff để giảm số lượng CRDT operations, tối ưu performance.
pub fn apply_text_update(doc: &LoroDoc, new_text: &str) -> Result<Vec<u8>, String> {
    let text_handler = doc.get_text("content");
    let old_text = text_handler.to_string();

    if old_text == new_text {
        // Nếu không có thay đổi, trả về rỗng.
        return Ok(vec![]);
    }

    // Lưu version vector TRƯỚC khi apply changes để export_from trả về đúng delta
    let old_vv = doc.oplog_vv();

    let diff = TextDiff::configure()
        .timeout(std::time::Duration::from_millis(200))
        .diff_chars(old_text.as_str(), new_text);
        
    let mut char_ops: Vec<(usize, usize, String)> = Vec::new();

    let new_chars: Vec<char> = new_text.chars().collect();

    for op in diff.ops() {
        match op {
            DiffOp::Delete { old_index, old_len, .. } => {
                char_ops.push((*old_index, *old_len, String::new()));
            },
            DiffOp::Insert { old_index, new_index, new_len, .. } => {
                let insert_str: String = new_chars[*new_index..*new_index + *new_len].iter().collect();
                char_ops.push((*old_index, 0, insert_str));
            },
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                let insert_str: String = new_chars[*new_index..*new_index + *new_len].iter().collect();
                char_ops.push((*old_index, *old_len, insert_str));
            },
            DiffOp::Equal { .. } => {}
        }
    }

    // Apply in reverse order to keep positions valid
    char_ops.sort_by(|a, b| b.0.cmp(&a.0));
    let text_handler = doc.get_text("content");
    
    for (pos, del_len, insert_str) in char_ops {
        if del_len > 0 {
            if let Err(e) = text_handler.delete(pos, del_len) {
                return Err(format!("Loro delete failed at pos {}: {:?}", pos, e));
            }
        }
        if !insert_str.is_empty() {
            if let Err(e) = text_handler.insert(pos, &insert_str) {
                return Err(format!("Loro insert failed at pos {}: {:?}", pos, e));
            }
        }
    }

    doc.commit();
    let delta = doc.export_from(&old_vv);
    Ok(delta)
}

/// Hợp nhất Snapshot CRDT từ mạng (Remote) vào tài liệu cục bộ.
/// Loro sẽ tự động tính toán để giữ lại mọi chỉnh sửa (Conflict-free).
/// Trả về (Delta chứa các thao tác remote để lưu DB, Text đã được gộp hoàn chỉnh).
pub fn merge_remote_snapshot(doc: &LoroDoc, remote_bytes: &[u8]) -> Result<(Vec<u8>, String), String> {
    let old_vv = doc.oplog_vv();

    // Import snapshot hoặc delta từ mạng
    doc.import(remote_bytes).map_err(|e| format!("Failed to merge remote snapshot: {:?}", e))?;
    
    // Xuất ra Delta chứa sự khác biệt để lưu vào crdt_updates dưới Local DB
    let delta = doc.export_from(&old_vv);
    
    // Trích xuất văn bản đã được hợp nhất hoàn hảo
    let text = doc.get_text("content").to_string();
    
    Ok((delta, text))
}
