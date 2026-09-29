//! Keeping secrets out of what Syn sends and keeps — the leak guard of
//! section 8.10.
//!
//! People paste API keys into notes, and passwords into chat. The first reaches
//! a cloud provider the moment Syn reads the note; the second the moment it is
//! sent. This pass runs over text about to leave or be kept, and replaces what
//! looks like a secret with a marker. The note or message itself is not
//! changed; only what goes out.
//!
//! # Two layers
//!
//! * **The user's own secrets**, while the Safe is open. Each concealed value
//!   of eight characters or more is remembered as a keyed BLAKE3 fingerprint —
//!   under a key derived from the Safe Key, never the value itself — and every
//!   word of the outgoing text is fingerprinted the same way and looked up. This
//!   is the layer that catches `correct-horse-battery` pasted into a note, which
//!   no pattern could.
//! * **Shapes**, always: the prefixes providers put on their keys, private-key
//!   blocks, JWTs, `otpauth://` links.
//!
//! It over-redacts rather than under-redacts on purpose — the cost of a false
//! alarm is a word Syn cannot read, the cost of a miss is a key in someone's
//! logs.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::crypto::Key;

/// Shorter than this and a value is too likely to be an ordinary word.
pub const MIN_FINGERPRINTED: usize = 8;

pub const MARK: &str = "‹secret hidden›";

type Print = [u8; 16];

/// The fingerprints of the open Safe's values. Empty while it is locked.
#[derive(Default)]
pub struct Fingerprints {
    key: Option<[u8; 32]>,
    prints: HashSet<Print>,
}

impl Drop for Fingerprints {
    fn drop(&mut self) {
        if let Some(k) = self.key.as_mut() {
            zeroize::Zeroize::zeroize(k);
        }
    }
}

impl Fingerprints {
    /// Fingerprints of `values`, under a key derived from `safe_key`.
    pub fn new<'a>(safe_key: &Key, values: impl IntoIterator<Item = &'a str>) -> Fingerprints {
        let key = *super::crypto::subkey(super::crypto::ctx::FINGERPRINT, safe_key).as_bytes();
        let mut f = Fingerprints { key: Some(key), prints: HashSet::new() };
        for v in values {
            f.add(v);
        }
        f
    }

    pub fn add(&mut self, value: &str) {
        let value = value.trim();
        if value.chars().count() >= MIN_FINGERPRINTED {
            if let Some(p) = self.print(value) {
                self.prints.insert(p);
            }
        }
    }

    fn print(&self, word: &str) -> Option<Print> {
        let key = self.key.as_ref()?;
        let hash = blake3::keyed_hash(key, word.as_bytes());
        Some(hash.as_bytes()[..16].try_into().expect("16 of 32"))
    }

    pub fn len(&self) -> usize {
        self.prints.len()
    }

    pub fn is_empty(&self) -> bool {
        self.prints.is_empty()
    }

    fn matches(&self, word: &str) -> bool {
        !self.prints.is_empty() && self.print(word).is_some_and(|p| self.prints.contains(&p))
    }
}

fn shapes() -> &'static regex::Regex {
    static SHAPES: OnceLock<regex::Regex> = OnceLock::new();
    SHAPES.get_or_init(|| {
        regex::Regex::new(concat!(
            r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----",
            r"|\bsk-ant-[A-Za-z0-9_\-]{20,}",
            r"|\bsk-(?:proj-)?[A-Za-z0-9_\-]{20,}",
            r"|\bgh[pousr]_[A-Za-z0-9]{30,}",
            r"|\bgithub_pat_[A-Za-z0-9_]{30,}",
            r"|\bglpat-[A-Za-z0-9_\-]{20,}",
            r"|\bxox[abprs]-[A-Za-z0-9\-]{10,}",
            r"|\bAKIA[0-9A-Z]{16}\b",
            r"|\bAIza[0-9A-Za-z_\-]{35}",
            r"|\b[0-9]{8,10}:AA[0-9A-Za-z_\-]{33}\b",
            r"|\beyJ[A-Za-z0-9_\-]{10,}\.eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
            r"|otpauth://[^\s)]+",
        ))
        .expect("the shapes compile")
    })
}

/// Words split the way a pasted secret is bounded: whitespace, and the
/// punctuation that wraps or labels one — quotes, brackets, `=`, `:`, commas.
fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let bound = |c: char| c.is_whitespace() || "\"'`()[]{}<>,;=|".contains(c);
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (bound(c), start) {
            (true, Some(s)) => {
                out.push((s, &text[s..i]));
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, &text[s..]));
    }
    out.into_iter().flat_map(|(s, w)| {
        // A trailing full stop or colon ends a sentence, not a password. Try
        // the word both with and without it.
        let trimmed = w.trim_end_matches(['.', ':', '!', '?']);
        let mut both = vec![(s, w)];
        if trimmed.len() != w.len() && !trimmed.is_empty() {
            both.push((s, trimmed));
        }
        both
    })
}

/// `text` with every secret it holds replaced by [`MARK`], and how many there
/// were. `prints` may be empty (the Safe is locked); shapes are always checked.
pub fn redact(text: &str, prints: &Fingerprints) -> (String, usize) {
    let mut spans: Vec<std::ops::Range<usize>> = shapes().find_iter(text).map(|m| m.range()).collect();
    for (start, word) in words(text) {
        if word.chars().count() >= MIN_FINGERPRINTED && prints.matches(word) {
            spans.push(start..start + word.len());
        }
    }
    if spans.is_empty() {
        return (text.to_string(), 0);
    }
    spans.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for r in spans {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for r in &merged {
        out.push_str(&text[at..r.start]);
        out.push_str(MARK);
        at = r.end;
    }
    out.push_str(&text[at..]);
    (out, merged.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prints(values: &[&str]) -> Fingerprints {
        Fingerprints::new(&Key::from_bytes([7; 32]), values.iter().copied())
    }

    #[test]
    fn the_users_own_password_is_caught_wherever_it_sits() {
        let p = prints(&["correct-horse-battery", "short"]);
        for text in [
            "my wifi is correct-horse-battery",
            "password=\"correct-horse-battery\".",
            "(correct-horse-battery)",
            "login: anh / correct-horse-battery:",
        ] {
            let (out, n) = redact(text, &p);
            assert_eq!(n, 1, "{text} → {out}");
            assert!(!out.contains("correct-horse-battery"));
        }
        let (out, n) = redact("this is short and fine", &p);
        assert_eq!((out.as_str(), n), ("this is short and fine", 0), "values under 8 characters are not fingerprinted");
    }

    #[test]
    fn a_locked_safe_still_catches_shapes() {
        let none = Fingerprints::default();
        let text = "key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123 and ghp_abcdefghijklmnopqrstuvwxyz0123456789 \
                    and AKIAABCDEFGHIJKLMNOP and otpauth://totp/x?secret=JBSWY3DPEHPK3PXP";
        let (out, n) = redact(text, &none);
        assert_eq!(n, 4, "{out}");
        for leaked in ["sk-ant-api03", "ghp_", "AKIA", "JBSWY3DP"] {
            assert!(!out.contains(leaked), "{leaked} survived: {out}");
        }
    }

    #[test]
    fn a_private_key_block_goes_whole() {
        let text = "here:\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\nxyz\n-----END OPENSSH PRIVATE KEY-----\nthanks";
        let (out, n) = redact(text, &Fingerprints::default());
        assert_eq!(n, 1);
        assert_eq!(out, format!("here:\n{MARK}\nthanks"));
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        let p = prints(&["correct-horse-battery"]);
        let text = "Meeting at 10:30 with Minh about the Q3 budget; see https://example.com/plan.";
        assert_eq!(redact(text, &p), (text.to_string(), 0));
    }

    #[test]
    fn fingerprints_hold_no_value() {
        let p = prints(&["correct-horse-battery"]);
        assert_eq!(p.len(), 1);
        assert!(!p.prints.iter().any(|print| print.windows(7).any(|w| w == b"correct")));
    }
}
