use loro::LoroDoc;
use similar::{DiffOp, TextDiff};

/// Work out the smallest set of character edits that turns `old_text` into
/// `new_text`, as `(position, chars_to_delete, text_to_insert)` in `old_text`
/// coordinates.
///
/// Edits stay at character granularity on purpose. These become CRDT
/// operations, and typing one letter should be one operation: a word-level diff
/// would rewrite the whole surrounding word, touching text the user never
/// changed and giving concurrent edits inside that word nothing to merge.
///
/// The matching head and tail are stripped before diffing. That is purely a
/// speed measure: on a 228k-character note, changing one word took 13.9ms
/// diffing the whole document against 1.1ms diffing only the part that moved.
/// The edits produced were identical either way — three operations touching
/// seven characters — so trimming buys time, not quality. Every save of every
/// changed file pays this cost, which is why it is worth buying.
///
/// The 200ms guard below is a separate matter, and it is not defused by
/// trimming. It does fire on documents that differ throughout, and the diff
/// then abandons minimality: measured on two unrelated 40k-character texts it
/// comes back as one edit rewriting everything. There is no common head or tail
/// to strip in that case. The result stays correct, just coarse, which is a
/// defensible answer for a document that genuinely changed everywhere.
fn compute_char_ops(old_text: &str, new_text: &str) -> Vec<(usize, usize, String)> {
    let old_chars: Vec<char> = old_text.chars().collect();
    let new_chars: Vec<char> = new_text.chars().collect();

    let prefix = old_chars
        .iter()
        .zip(new_chars.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let remaining = old_chars.len().min(new_chars.len()) - prefix;
    let suffix = old_chars[prefix..]
        .iter()
        .rev()
        .zip(new_chars[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(remaining);

    let old_mid: String = old_chars[prefix..old_chars.len() - suffix].iter().collect();
    let new_mid: String = new_chars[prefix..new_chars.len() - suffix].iter().collect();

    if old_mid.is_empty() && new_mid.is_empty() {
        return Vec::new();
    }

    let mid_chars: Vec<char> = new_mid.chars().collect();
    let diff = TextDiff::configure()
        .timeout(std::time::Duration::from_millis(200))
        .diff_chars(old_mid.as_str(), new_mid.as_str());

    let mut char_ops: Vec<(usize, usize, String)> = Vec::new();
    for op in diff.ops() {
        match op {
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                char_ops.push((prefix + *old_index, *old_len, String::new()));
            }
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
                ..
            } => {
                let insert_str: String =
                    mid_chars[*new_index..*new_index + *new_len].iter().collect();
                char_ops.push((prefix + *old_index, 0, insert_str));
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let insert_str: String =
                    mid_chars[*new_index..*new_index + *new_len].iter().collect();
                char_ops.push((prefix + *old_index, *old_len, insert_str));
            }
            DiffOp::Equal { .. } => {}
        }
    }

    char_ops
}

/// Bring a `LoroDoc`'s `content` text in line with `new_text`, returning the
/// operations this produced so they can be persisted.
pub fn apply_text_update(doc: &LoroDoc, new_text: &str) -> Result<Vec<u8>, String> {
    let text_handler = doc.get_text("content");
    let old_text = text_handler.to_string();

    if old_text == new_text {
        return Ok(vec![]);
    }

    // Capture the version vector before applying, so `export_from` returns
    // exactly the operations added below.
    let old_vv = doc.oplog_vv();

    let mut char_ops = compute_char_ops(&old_text, new_text);

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
pub fn merge_remote_snapshot(
    doc: &LoroDoc,
    remote_bytes: &[u8],
) -> Result<(Vec<u8>, String), String> {
    let old_vv = doc.oplog_vv();

    // Import snapshot hoặc delta từ mạng
    doc.import(remote_bytes)
        .map_err(|e| format!("Failed to merge remote snapshot: {:?}", e))?;

    // Xuất ra Delta chứa sự khác biệt để lưu vào crdt_updates dưới Local DB
    let delta = doc.export_from(&old_vv);

    // Trích xuất văn bản đã được hợp nhất hoàn hảo
    let text = doc.get_text("content").to_string();

    Ok((delta, text))
}

#[cfg(test)]
mod char_ops_tests {
    use super::*;

    /// Replay the computed edits the way `apply_text_update` does, so a test
    /// failure means the document would genuinely have come out wrong.
    fn apply(old: &str, ops: &[(usize, usize, String)]) -> String {
        let mut chars: Vec<char> = old.chars().collect();
        let mut ordered = ops.to_vec();
        ordered.sort_by(|a, b| b.0.cmp(&a.0));
        for (pos, del_len, insert) in ordered {
            chars.splice(pos..pos + del_len, insert.chars());
        }
        chars.into_iter().collect()
    }

    fn chars_touched(ops: &[(usize, usize, String)]) -> usize {
        ops.iter()
            .map(|(_, del, ins)| del + ins.chars().count())
            .sum()
    }

    fn roundtrip(old: &str, new: &str) {
        let ops = compute_char_ops(old, new);
        assert_eq!(
            apply(old, &ops),
            new,
            "edits did not reproduce the new text\n  old: {old:?}\n  new: {new:?}\n  ops: {ops:?}"
        );
    }

    #[test]
    fn identical_text_produces_no_edits() {
        assert!(compute_char_ops("same", "same").is_empty());
    }

    #[test]
    fn edits_reproduce_the_new_text() {
        for (old, new) in [
            ("", "hello"),
            ("hello", ""),
            ("hello", "hello world"),
            ("hello world", "hello"),
            ("abc", "axc"),
            ("the quick brown fox", "the quick red fox"),
            ("one\ntwo\nthree", "one\ntwo and a half\nthree"),
        ] {
            roundtrip(old, new);
        }
    }

    #[test]
    fn multibyte_text_is_edited_by_character_not_byte() {
        // Vietnamese and emoji are multiple bytes per character; Loro indexes by
        // character, so the edits must too.
        roundtrip("Ghi chú của tôi", "Ghi chú mới của tôi");
        roundtrip("plan 🚀 ship", "plan 🚀🚀 ship");
        let ops = compute_char_ops("xin chào", "xin chảo");
        assert_eq!(chars_touched(&ops), 2, "expected a single character swap: {ops:?}");
    }

    #[test]
    fn a_small_edit_in_a_large_document_stays_small() {
        // The case that used to trip the diff guard and come back as a single
        // replace of the whole document.
        let filler = "Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n".repeat(2_000);
        let old = format!("{filler}TARGET\n{filler}");
        let new = format!("{filler}CHANGED\n{filler}");

        let ops = compute_char_ops(&old, &new);
        assert_eq!(apply(&old, &ops), new);

        assert!(
            chars_touched(&ops) < 40,
            "a one-word change touched {} characters of a {}-character document: {:?}",
            chars_touched(&ops),
            old.chars().count(),
            ops
        );
    }

    #[test]
    fn appending_only_touches_the_end() {
        let old = "a".repeat(50_000);
        let new = format!("{old}tail");

        let ops = compute_char_ops(&old, &new);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].0, 50_000, "the append should land at the end");
        assert_eq!(ops[0].1, 0, "nothing should be deleted");
        assert_eq!(ops[0].2, "tail");
    }

    #[test]
    fn prepending_only_touches_the_start() {
        let old = "b".repeat(50_000);
        let new = format!("head{old}");

        let ops = compute_char_ops(&old, &new);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].0, 0);
        assert_eq!(ops[0].1, 0);
        assert_eq!(ops[0].2, "head");
    }

    #[test]
    fn a_document_that_differs_throughout_still_applies_correctly() {
        // Nothing to trim, and the diff exceeds its time budget and gives up on
        // minimality: measured, it returns a single edit rewriting the whole
        // document. That is an acceptable answer for a document that really did
        // change everywhere, but it must still be a correct one.
        let old: String = (0..40_000).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let new: String = (0..40_000)
            .map(|i| char::from(b'a' + ((i * 7 + 3) % 26) as u8))
            .collect();

        let ops = compute_char_ops(&old, &new);
        assert_eq!(apply(&old, &ops), new);
    }

    #[test]
    fn a_typed_character_is_one_small_edit_not_a_word_rewrite() {
        // Character granularity is the point: two devices typing into different
        // halves of the same word still have something to merge.
        let ops = compute_char_ops("synchronise", "synchronize");
        assert_eq!(chars_touched(&ops), 2, "expected one character swapped: {ops:?}");
    }

    #[test]
    fn loro_applies_the_edits_at_the_positions_we_computed() {
        // compute_char_ops speaks in characters; this checks Loro agrees, which
        // matters most where characters and bytes diverge.
        for (old, new) in [
            ("plan 🚀 ship", "plan 🚀🚀 ship"),
            ("Ghi chú", "Ghi chú mới"),
            ("a🎉b🎉c", "a🎉B🎉c"),
            ("hello world", "hello brave world"),
        ] {
            let doc = LoroDoc::new();
            doc.get_text("content").insert(0, old).unwrap();
            doc.commit();

            apply_text_update(&doc, new).expect("apply");

            assert_eq!(
                doc.get_text("content").to_string(),
                new,
                "Loro landed the edits somewhere else\n  old: {old:?}\n  new: {new:?}"
            );
        }
    }
}
