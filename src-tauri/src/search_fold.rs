//! Folding `đ` into `d`, which the tokenizer will not do.
//!
//! SQLite's `unicode61 remove_diacritics 2` folds Vietnamese tone marks, so
//! `cong` finds `công`. It leaves `đ` alone, and correctly so by its own
//! rules: `đ` is a letter in its own right, not a `d` wearing a mark, and no
//! amount of Unicode decomposition turns one into the other.
//!
//! A person typing quickly does not make that distinction. They type `dong`
//! and expect `đông`, exactly as they type `cong` and expect `công`.
//!
//! # Why only some words are indexed
//!
//! The obvious fix — a second copy of every note with `đ` folded — makes every
//! ordinary search match twice, once in the real columns and once in the copy,
//! which doubles the term frequencies BM25 ranks on and quietly reorders
//! results that had nothing to do with `đ`.
//!
//! So the shadow column carries only the words that actually contain a `đ`.
//! A search for `cong` never touches it; a search for `dong` finds `đông`
//! there and nowhere else. The ranking of everything else is left exactly as
//! it was.

/// Replace every `đ`/`Đ` with `d`/`D`, leaving the rest of the text alone.
pub fn fold_d_stroke(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'đ' => 'd',
            'Đ' => 'D',
            other => other,
        })
        .collect()
}

/// Whether a string holds anything this module would change.
pub fn has_d_stroke(text: &str) -> bool {
    text.chars().any(|c| c == 'đ' || c == 'Đ')
}

/// The folded form of just the words containing `đ`, space-separated.
///
/// This is what goes in the shadow column. Empty for the great majority of
/// notes, which is the point: an index nobody's search touches costs nothing
/// to carry and nothing to rank against.
pub fn fold_d_stroke_words(text: &str) -> String {
    let mut out = String::new();
    for word in text.split_whitespace() {
        if !has_d_stroke(word) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&fold_d_stroke(word));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_the_stroked_d_in_both_cases() {
        // Only the letter changes. `ô` and `à` are the tokenizer's business.
        assert_eq!(fold_d_stroke("đông Dương Đà Nẵng"), "dông Dương Dà Nẵng");
    }

    #[test]
    fn leaves_everything_else_exactly_as_it_was() {
        // Tone marks are the tokenizer's job, not this module's. Stripping
        // them here as well would mean two different foldings to keep in step.
        assert_eq!(fold_d_stroke("công ty cổ phần"), "công ty cổ phần");
        assert_eq!(fold_d_stroke("splunk query"), "splunk query");
    }

    #[test]
    fn keeps_only_the_words_that_needed_folding() {
        // The shadow column exists to be small. A note with one `đ` word in a
        // thousand should add one word to the index, not a thousand.
        //
        // Note the tone marks survive: `đơn` becomes `dơn`, not `don`. This
        // module folds exactly one letter and leaves the rest to the
        // tokenizer, which strips the marks on its way into the index. Two
        // foldings doing half the job each is one fewer thing to keep in step
        // than two doing the same job differently.
        assert_eq!(fold_d_stroke_words("báo cáo đơn hàng tháng này"), "dơn");
        assert_eq!(fold_d_stroke_words("đông đủ mọi người"), "dông dủ");
    }

    #[test]
    fn is_empty_for_text_with_no_stroked_d() {
        assert_eq!(fold_d_stroke_words("công ty cổ phần abc"), "");
        assert_eq!(fold_d_stroke_words(""), "");
    }

    #[test]
    fn recognises_where_folding_would_change_something() {
        assert!(has_d_stroke("đơn"));
        assert!(has_d_stroke("Đà Nẵng"));
        assert!(!has_d_stroke("don"));
    }
}
