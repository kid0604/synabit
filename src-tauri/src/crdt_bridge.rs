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

    let old_vv = doc.oplog_vv();

    // Use line-level diff first for better CRDT operation grouping.
    // This produces far fewer operations than character-level diff while
    // still capturing all changes accurately.
    let diff = TextDiff::from_lines(old_text.as_str(), new_text);

    // Collect chars to slice safely by character indices
    let old_chars: Vec<char> = old_text.chars().collect();
    let new_chars: Vec<char> = new_text.chars().collect();

    // Map line indices to character positions
    let old_line_offsets: Vec<usize> = {
        let mut offsets = vec![0];
        for (i, ch) in old_chars.iter().enumerate() {
            if *ch == '\n' && i + 1 < old_chars.len() {
                offsets.push(i + 1);
            }
        }
        offsets
    };
    let new_line_offsets: Vec<usize> = {
        let mut offsets = vec![0];
        for (i, ch) in new_chars.iter().enumerate() {
            if *ch == '\n' && i + 1 < new_chars.len() {
                offsets.push(i + 1);
            }
        }
        offsets
    };

    // Collect operations with character positions, apply in reverse
    let mut char_ops: Vec<(usize, usize, String)> = Vec::new(); // (old_char_pos, old_char_len, new_text)

    for op in diff.ops() {
        match op {
            DiffOp::Delete { old_index, old_len, .. } => {
                let start = old_line_offsets.get(*old_index).copied().unwrap_or(old_chars.len());
                let end = old_line_offsets.get(*old_index + *old_len).copied().unwrap_or(old_chars.len());
                char_ops.push((start, end - start, String::new()));
            },
            DiffOp::Insert { old_index, new_index, new_len, .. } => {
                let pos = old_line_offsets.get(*old_index).copied().unwrap_or(old_chars.len());
                let new_start = new_line_offsets.get(*new_index).copied().unwrap_or(new_chars.len());
                let new_end = new_line_offsets.get(*new_index + *new_len).copied().unwrap_or(new_chars.len());
                let insert_str: String = new_chars[new_start..new_end].iter().collect();
                char_ops.push((pos, 0, insert_str));
            },
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                let start = old_line_offsets.get(*old_index).copied().unwrap_or(old_chars.len());
                let end = old_line_offsets.get(*old_index + *old_len).copied().unwrap_or(old_chars.len());
                let new_start = new_line_offsets.get(*new_index).copied().unwrap_or(new_chars.len());
                let new_end = new_line_offsets.get(*new_index + *new_len).copied().unwrap_or(new_chars.len());
                let insert_str: String = new_chars[new_start..new_end].iter().collect();
                char_ops.push((start, end - start, insert_str));
            },
            DiffOp::Equal { .. } => {}
        }
    }

    // Cực kỳ quan trọng: Áp dụng các thay đổi từ cuối lên đầu để các index không bị xô lệch
    char_ops.reverse();
    for (pos, del_len, insert_text) in char_ops {
        if del_len > 0 {
            text_handler.delete(pos, del_len).map_err(|e| format!("Loro Delete Error: {:?}", e))?;
        }
        if !insert_text.is_empty() {
            text_handler.insert(pos, &insert_text).map_err(|e| format!("Loro Insert Error: {:?}", e))?;
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
