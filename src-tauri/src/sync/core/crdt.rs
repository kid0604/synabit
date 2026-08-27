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
    let old_vv = doc.oplog_vv();
    apply_text_ops(doc, new_text)?;
    Ok(doc.export_from(&old_vv))
}

/// The character edits themselves, without exporting anything.
///
/// Split out so a caller changing both containers exports one delta covering
/// the lot rather than two that have to be stitched together.
fn apply_text_ops(doc: &LoroDoc, new_text: &str) -> Result<(), String> {
    let text_handler = doc.get_text("content");
    let old_text = text_handler.to_string();

    if old_text == new_text {
        return Ok(());
    }

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
    Ok(())
}

// ---------------------------------------------------------------------------
// The document as two containers
// ---------------------------------------------------------------------------
//
// `content` holds the body. `frontmatter` holds each field under its own key.
//
// The split exists because the two need different merge rules. Prose wants
// character-level merging — two people adding a paragraph each should end up
// with both. A field does not: merging `done` against `in_progress` character
// by character produced `in_pronegress`, which is valid YAML, means nothing,
// and moved the task to a column nobody put it in.
//
// # Why the frontmatter leaves the text rather than being repaired in it
//
// Repairing would mean each device noticing the damage and writing the correct
// value back. Two devices doing that independently produce two insertions of
// the same string, and a text CRDT keeps both: measured, `body` repaired to
// `body FIXED` on two devices merges to `body FIXED FIXED`. The bug again,
// wearing a hat.
//
// Taking the frontmatter out of the text is a *deletion*, and concurrent
// deletion of the same characters is idempotent — measured, two devices
// stripping the same frontmatter block independently both arrive at exactly
// the body. Setting a map key twice to the same value is likewise harmless.
// So the migration is safe to perform on any device, at any time, without
// coordination, which is what lets it happen without a protocol version.

use super::node_document::{self, NodeParts};

const FRONTMATTER: &str = "frontmatter";

/// Where the frontmatter's key order is kept.
///
/// Not decoration. The device that wrote a file keeps it on disk in its own
/// key order and never rewrites it; a device that receives the file rebuilds
/// the frontmatter from the map. If the rebuild chose its own order the two
/// would hold different bytes for the same document, each would see a change,
/// each would publish it, and they would sync each other's reordering back and
/// forth with no user involved. Measured — the two devices settled on
/// different bytes and stayed there.
///
/// Carrying the order makes the rebuild reproduce the original exactly. Under
/// concurrency it is one more map key, so two devices that reordered
/// differently resolve to one of the two orders rather than to a blend.
///
/// The leading underscores keep it out of the way of a real frontmatter key.
/// A file that genuinely had a field of this name would lose it, which is a
/// trade worth making for a name nobody writes.
const ORDER_KEY: &str = "__key_order__";

/// The whole file this document represents.
pub fn node_text(doc: &LoroDoc) -> String {
    let body = doc.get_text("content").to_string();

    // A document that has not been migrated yet still keeps everything in the
    // text. Its map is empty, and the text already is the file.
    let fields = read_fields(doc);
    if fields.is_empty() {
        return body;
    }

    node_document::rebuild(&NodeParts { order: read_order(doc), fields, body })
}

/// Every frontmatter field, as JSON-encoded values.
fn read_fields(doc: &LoroDoc) -> std::collections::BTreeMap<String, String> {
    let mut fields = std::collections::BTreeMap::new();
    let map = doc.get_map(FRONTMATTER);
    for (key, value) in map.get_value().into_map().unwrap_or_default().iter() {
        if key == ORDER_KEY {
            continue;
        }
        if let loro::LoroValue::String(encoded) = value {
            fields.insert(key.to_string(), encoded.to_string());
        }
    }
    fields
}

/// The key order the file was written in, if the document remembers one.
fn read_order(doc: &LoroDoc) -> Vec<String> {
    let map = doc.get_map(FRONTMATTER);
    let Some(loro::LoroValue::String(encoded)) =
        map.get_value().into_map().unwrap_or_default().get(ORDER_KEY).cloned()
    else {
        return Vec::new();
    };
    serde_json::from_str(&encoded).unwrap_or_default()
}

/// Bring both containers in line with `new_file`, returning the operations.
///
/// The body diff is the same character-level work `apply_text_update` does. On
/// a document whose text still holds its frontmatter that diff also strips it,
/// which is the migration — a deletion, and therefore safe to arrive from two
/// devices at once.
pub fn apply_node_update(doc: &LoroDoc, new_file: &str) -> Result<Vec<u8>, String> {
    let parts = node_document::split(new_file);
    let old_vv = doc.oplog_vv();

    apply_text_ops(doc, &parts.body)?;

    let map = doc.get_map(FRONTMATTER);
    let existing = read_fields(doc);
    for (key, encoded) in &parts.fields {
        if existing.get(key) != Some(encoded) {
            map.insert(key.as_str(), encoded.as_str())
                .map_err(|e| format!("Loro map insert failed for {key}: {e:?}"))?;
        }
    }
    // A key the file no longer has is a key the user removed. Leaving it would
    // bring the field back the next time the document was read.
    for key in existing.keys() {
        if !parts.fields.contains_key(key) {
            map.delete(key)
                .map_err(|e| format!("Loro map delete failed for {key}: {e:?}"))?;
        }
    }

    // The order this file was written in, so a device rebuilding from the map
    // reproduces these bytes rather than its own idea of them. See `ORDER_KEY`.
    let order = serde_json::to_string(&parts.order).unwrap_or_else(|_| "[]".into());
    if read_order(doc) != parts.order {
        map.insert(ORDER_KEY, order.as_str())
            .map_err(|e| format!("Loro map insert failed for the key order: {e:?}"))?;
    }

    doc.commit();
    Ok(doc.export_from(&old_vv))
}

// ---------------------------------------------------------------------------
// Finance documents
// ---------------------------------------------------------------------------

use super::finance_document::{
    self, FinanceParts, HEAD, KEY_BODY, KEY_ROWS_KEY, KEY_TITLE, KEY_TYPE, META, ROWS,
};

/// Every entry of one Loro map, as the JSON text it holds.
fn read_map(doc: &LoroDoc, container: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let map = doc.get_map(container);
    for (key, value) in map.get_value().into_map().unwrap_or_default().iter() {
        if let loro::LoroValue::String(encoded) = value {
            out.insert(key.to_string(), encoded.to_string());
        }
    }
    out
}

/// Bring one map in line with `wanted`, inserting what changed and removing
/// what the file no longer has.
fn sync_map(
    doc: &LoroDoc,
    container: &str,
    wanted: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    let map = doc.get_map(container);
    let existing = read_map(doc, container);

    for (key, encoded) in wanted {
        if existing.get(key) != Some(encoded) {
            map.insert(key.as_str(), encoded.as_str())
                .map_err(|e| format!("Loro map insert failed for {container}/{key}: {e:?}"))?;
        }
    }
    // A key the file no longer has is a row somebody deleted. Leaving it would
    // bring the transaction back the next time the document was read.
    for key in existing.keys() {
        if !wanted.contains_key(key) {
            map.delete(key)
                .map_err(|e| format!("Loro map delete failed for {container}/{key}: {e:?}"))?;
        }
    }
    Ok(())
}

/// Bring every container in line with `new_file`, returning the operations.
///
/// The text container is kept current too, holding the whole file exactly as it
/// always did. Nothing here reads it — but a device running an older build
/// does, and leaving it empty would hand that device an empty ledger.
pub fn apply_finance_update(doc: &LoroDoc, new_file: &str) -> Result<Vec<u8>, String> {
    let parts = finance_document::split(new_file)
        .ok_or_else(|| "not a Finance document this can take apart".to_string())?;
    let old_vv = doc.oplog_vv();

    apply_text_ops(doc, new_file)?;

    let mut head = std::collections::BTreeMap::new();
    head.insert(KEY_TITLE.to_string(), parts.title.clone());
    head.insert(KEY_TYPE.to_string(), parts.node_type.clone());
    head.insert(KEY_BODY.to_string(), parts.body.clone());
    if let Some(rows_key) = &parts.rows_key {
        head.insert(KEY_ROWS_KEY.to_string(), rows_key.clone());
    }

    sync_map(doc, HEAD, &head)?;
    sync_map(doc, META, &parts.meta)?;
    sync_map(doc, ROWS, &parts.rows)?;

    doc.commit();
    Ok(doc.export_from(&old_vv))
}

/// The Finance file these containers describe, or `None` if there are none.
///
/// `None` is the answer for a document that has only ever been written by an
/// older build, and it is what tells the caller to fall back to resolving the
/// file whole.
pub fn finance_text(doc: &LoroDoc) -> Option<String> {
    let head = read_map(doc, HEAD);
    let node_type = head.get(KEY_TYPE)?.clone();
    if node_type.is_empty() {
        return None;
    }

    let parts = FinanceParts {
        title: head.get(KEY_TITLE).cloned().unwrap_or_default(),
        node_type,
        body: head.get(KEY_BODY).cloned().unwrap_or_default(),
        rows_key: head.get(KEY_ROWS_KEY).cloned(),
        meta: read_map(doc, META),
        rows: read_map(doc, ROWS),
    };

    Some(finance_document::rebuild(&parts))
}

/// Merge a remote snapshot into a Finance document.
///
/// Returns the operations to store and the file both devices now agree on, or
/// `None` for that file if the merged document carries no Finance containers —
/// which means the other device has not been taught about them yet.
///
/// The text container is brought back in line with the rebuilt file rather than
/// left as whatever the character merge made of two versions. Nothing here
/// reads it, but an older device does, and what it reads should at least be
/// the ledger rather than two ledgers spliced together.
pub fn merge_finance_snapshot(
    doc: &LoroDoc,
    remote_bytes: &[u8],
) -> Result<(Vec<u8>, Option<String>), String> {
    let old_vv = doc.oplog_vv();
    doc.import(remote_bytes)
        .map_err(|e| format!("Failed to merge remote Finance snapshot: {:?}", e))?;

    let merged = finance_text(doc);
    if let Some(file) = &merged {
        apply_text_ops(doc, file)?;
        doc.commit();
    }

    Ok((doc.export_from(&old_vv), merged))
}

/// Merge a remote snapshot and report the file it produces.
///
/// Nothing is written back into the document to tidy the result up: see the
/// note above about concurrent repairs. What comes out is what both containers
/// already agree on.
pub fn merge_node_snapshot(doc: &LoroDoc, remote_bytes: &[u8]) -> Result<(Vec<u8>, String), String> {
    let old_vv = doc.oplog_vv();
    doc.import(remote_bytes)
        .map_err(|e| format!("Failed to merge remote snapshot: {:?}", e))?;
    let delta = doc.export_from(&old_vv);
    Ok((delta, node_text(doc)))
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
