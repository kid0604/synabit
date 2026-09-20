//! The text functions SQL needs and SQLite does not have.
//!
//! # `lower()` only speaks English
//!
//! SQLite's built-in `lower()` folds ASCII and nothing else. Measured:
//!
//! ```text
//! lower("Ăn tối")    = "Ăn tối"      unchanged
//! lower("Gặp Khánh") = "gặp khánh"   the G, because G is ASCII
//! lower("ĂN")        = "Ăn"          only the N
//! ```
//!
//! Every Vietnamese letter that is not also an English one — `Ă Â Đ Ê Ô Ơ Ư`
//! and their accented forms — comes back exactly as it went in. So every
//! comparison in this app that lowercased both sides was really comparing
//! *the Rust-lowercased text* against *the ASCII-lowercased text*, and they
//! differ for precisely the words this vault is written in.
//!
//! What that cost, measured before the fix: a node tagged `Gia-Đình` could not
//! be found by `#Gia-Đình` **or** by `#gia-đình` — it was unreachable by any
//! spelling. On the timeline, every word that starts a sentence is capitalised,
//! so `ăn` never found «Ăn tối với Minh» and `đi` never found «Đi chơi».
//!
//! # Why a function and not a stored column
//!
//! A normalised column beside `title` would fix the timeline and leave the
//! node side — tags, statuses, every frontmatter value — still broken, because
//! those are compared in place and there is nowhere to put a second copy of
//! each. One function fixes both, is used the same way `lower()` was, and can
//! never drift from the Rust side because it **is** the Rust side.
//!
//! The price is that it cannot be indexed. Nothing indexed `lower(title)`
//! before this, so nothing is lost today; a vault large enough to want one
//! would want a stored column, and this is the thing that would tell us so.

use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;

/// Every character that stands between two words.
///
/// Used to be sixteen nested `replace()` calls built into the SQL of every
/// timeline query carrying a word. Now it is a list, read once per row.
const BETWEEN_WORDS: &[char] = &[
    ',', '.', ';', ':', '!', '?', '(', ')', '"', '\'', '/', '\u{b7}', '\u{2014}', '\u{2013}',
    '\u{ab}', '\u{bb}',
];

/// A title as one space-padded, punctuation-flattened, lower-case string.
///
/// So that `LIKE '% word %'` matches a **word** rather than a run of letters.
/// The real vault taught this the hard way: `ăn` found fifteen events and the
/// first three were «công **văn**» and «Bùi **Văn** Phương». Vietnamese is
/// written in syllables separated by spaces, so a substring test turns every
/// short word into a wildcard.
///
/// Not a tokenizer and not pretending to be one — a word glued to a character
/// outside [`BETWEEN_WORDS`] is still missed. That is a far smaller wrong.
pub fn words_in(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    for c in text.chars() {
        if BETWEEN_WORDS.contains(&c) {
            out.push(' ');
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out.push(' ');
    out
}

/// Teach a connection the two functions the app's SQL uses.
///
/// Called wherever a connection is opened. A connection that has not been
/// taught them fails loudly on the first query that uses one, which is the
/// behaviour worth having: the alternative is a query that quietly matches
/// nothing, and that is the failure this module exists to end.
pub fn teach(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;

    // `vlower(x)` — lower case that knows about more than twenty-six letters.
    conn.create_scalar_function("vlower", 1, flags, |ctx| {
        Ok(ctx
            .get::<Option<String>>(0)?
            .map(|text| text.to_lowercase()))
    })?;

    // `vwords(x)` — the same, padded and with punctuation turned to spaces.
    conn.create_scalar_function("vwords", 1, flags, |ctx| {
        Ok(ctx.get::<Option<String>>(0)?.map(|text| words_in(&text)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The measurement this module exists because of, kept as a test so it
    /// cannot quietly stop being true.
    #[test]
    fn sqlites_own_lower_leaves_vietnamese_alone_and_ours_does_not() {
        let conn = Connection::open_in_memory().unwrap();
        teach(&conn).unwrap();
        let ask = |sql: &str, word: &str| -> String {
            conn.query_row(&format!("SELECT {sql}(?1)"), [word], |r| r.get(0))
                .unwrap()
        };

        assert_eq!(ask("lower", "Ăn tối"), "Ăn tối", "SQLite's, unchanged");
        assert_eq!(ask("vlower", "Ăn tối"), "ăn tối");
        assert_eq!(ask("vlower", "ĐI CHƠI"), "đi chơi");
        assert_eq!(ask("vlower", "Gặp Khánh"), "gặp khánh");
    }

    #[test]
    fn a_word_is_padded_and_punctuation_becomes_space() {
        assert_eq!(words_in("Ăn tối với Minh."), " ăn tối với minh  ");
        assert_eq!(words_in("Gặp Khánh (ở quán)"), " gặp khánh  ở quán  ");
    }

    /// The property the padding is for: a short word must not match inside a
    /// longer one. `ăn` is not `văn`.
    #[test]
    fn a_short_word_does_not_match_inside_a_longer_one() {
        let padded = words_in("Làm công văn");
        assert!(padded.contains(" văn "));
        assert!(!padded.contains(" ăn "), "{padded:?}");
        assert!(words_in("Ăn tối với Minh").contains(" ăn "));
    }

    #[test]
    fn nothing_is_still_nothing() {
        let conn = Connection::open_in_memory().unwrap();
        teach(&conn).unwrap();
        let empty: Option<String> = conn
            .query_row("SELECT vlower(NULL)", [], |r| r.get(0))
            .unwrap();
        assert!(empty.is_none(), "a missing value stays missing, not \"\"");
    }
}
