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

/// Text folded for comparing in Rust: lowercase, tone marks gone, `đ` as `d`.
///
/// # Why a second folding, when the module above argues for one
///
/// Everything above is about text going into SQLite, where the tokenizer
/// strips the marks and this module only has to add the one letter it will
/// not. Some comparisons never reach SQLite: a memory's subject checked
/// against another's, `recall` scanning a few dozen memories, a question
/// matched against the skills the user enabled. Those ran on
/// `eq_ignore_ascii_case` or a bare `to_lowercase`, so `Đức` missed `đức` and
/// `ca phe` missed `cà phê` — the same miss the tokenizer was configured to
/// prevent, reintroduced one layer up. This is the whole job, tokenizer and
/// all, for the places that have no tokenizer.
///
/// A table rather than Unicode decomposition. `unicode-normalization` would do
/// it in one line, and would be a new crate in the Android build, which
/// `timeline::media`'s size gate rightly refuses without a review. The table
/// is every precomposed Vietnamese vowel — which covers the French and Spanish
/// accents on the same letters too — and a mark typed as a separate combining
/// character is dropped as well, so text that arrived decomposed folds the
/// same. Letters outside the table (`ñ`, `ü`) pass through unchanged, which
/// errs towards a miss, never a false match.
///
/// Folding loses distinctions on purpose — `má` and `ma` compare equal. Use it
/// where a false match costs a question or a less precise list; where two
/// strings differing only by tone must stay two things, as `proposal`'s
/// duplicate check argues, this is the wrong function.
pub fn fold(text: &str) -> String {
    const MARKED: &[(char, &str)] = &[
        ('a', "àáảãạăằắẳẵặâầấẩẫậ"),
        ('e', "èéẻẽẹêềếểễệ"),
        ('i', "ìíỉĩị"),
        ('o', "òóỏõọôồốổỗộơờớởỡợ"),
        ('u', "ùúủũụưừứửữự"),
        ('y', "ỳýỷỹỵ"),
        ('d', "đ"),
    ];
    text.to_lowercase()
        .chars()
        // Combining diacritical marks, for text that arrived decomposed.
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .map(|c| {
            MARKED
                .iter()
                .find(|(_, marked)| marked.contains(c))
                .map_or(c, |(base, _)| *base)
        })
        .collect()
}

/// Two strings equal once folded, ignoring the space around them.
pub fn same_folded(a: &str, b: &str) -> bool {
    fold(a.trim()) == fold(b.trim())
}

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

    /// Case, tone marks, the horn on `ư`/`ơ` and the stroke on `đ`, all at once.
    #[test]
    fn a_full_fold_matches_what_a_person_types_without_marks() {
        assert_eq!(fold("Cà Phê Sữa Đá"), "ca phe sua da");
        assert_eq!(fold("Đức"), fold("đức"));
        assert_eq!(fold("Đức"), "duc");
        assert_eq!(fold("Trường Nguyễn"), "truong nguyen");
        assert_eq!(fold("weekly-review"), "weekly-review", "ASCII passes through");
        assert!(same_folded("  Đà Nẵng ", "da nang"));
        assert!(!same_folded("Hà Nội", "Hải Phòng"));
        // Typed decomposed — a base letter and a separate combining mark, as
        // some keyboards and every macOS filename send it.
        assert_eq!(fold("Ca\u{0300} phe\u{0302}\u{0301}"), "ca phe");
    }

    /// Every vowel with every mark, in both cases, folds to one of the six.
    #[test]
    fn every_vietnamese_vowel_folds_to_its_base() {
        let all = "àáảãạăằắẳẵặâầấẩẫậèéẻẽẹêềếểễệìíỉĩịòóỏõọôồốổỗộơờớởỡợùúủũụưừứửữựỳýỷỹỵđ";
        for c in all.chars().chain(all.to_uppercase().chars()) {
            let folded = fold(&c.to_string());
            assert!(
                ["a", "e", "i", "o", "u", "y", "d"].contains(&folded.as_str()),
                "`{c}` folded to `{folded}`"
            );
        }
    }

    #[test]
    fn recognises_where_folding_would_change_something() {
        assert!(has_d_stroke("đơn"));
        assert!(has_d_stroke("Đà Nẵng"));
        assert!(!has_d_stroke("don"));
    }
}
