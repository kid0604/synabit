use loro::LoroDoc;
use similar::{DiffOp, TextDiff};

/// Áp dụng nội dung text mới vào LoroDoc thông qua thuật toán Myers Diff.
/// Việc này giúp duy trì lịch sử CRDT thay vì xóa trắng và viết lại toàn bộ.
pub fn apply_text_update(doc: &LoroDoc, new_text: &str) -> Result<Vec<u8>, String> {
    let text_handler = doc.get_text("content");
    let old_text = text_handler.to_string();

    if old_text == new_text {
        // Nếu không có thay đổi, trả về rỗng.
        return Ok(vec![]);
    }

    let old_vv = doc.oplog_vv();

    let diff = TextDiff::from_chars(old_text.as_str(), new_text);
    
    // Cực kỳ quan trọng: Áp dụng các thay đổi từ cuối lên đầu để các index không bị xô lệch
    let mut ops: Vec<_> = diff.ops().iter().collect();
    ops.reverse();

    // Collect chars to slice safely by character indices
    let new_chars: Vec<char> = new_text.chars().collect();

    for op in ops {
        match op {
            DiffOp::Delete { old_index, old_len, .. } => {
                text_handler.delete(*old_index, *old_len).map_err(|e| format!("Loro Delete Error: {:?}", e))?;
            },
            DiffOp::Insert { old_index, new_index, new_len, .. } => {
                let insert_str: String = new_chars[*new_index..(*new_index + *new_len)].iter().collect();
                text_handler.insert(*old_index, &insert_str).map_err(|e| format!("Loro Insert Error: {:?}", e))?;
            },
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                let insert_str: String = new_chars[*new_index..(*new_index + *new_len)].iter().collect();
                // Thực hiện Replace = Delete sau đó Insert
                text_handler.delete(*old_index, *old_len).map_err(|e| format!("Loro Replace Delete Error: {:?}", e))?;
                text_handler.insert(*old_index, &insert_str).map_err(|e| format!("Loro Replace Insert Error: {:?}", e))?;
            },
            DiffOp::Equal { .. } => {}
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
