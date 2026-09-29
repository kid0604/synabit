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
//!   under a key derived from the Safe Key, never the value itself — with its
//!   length. The outgoing text is searched for every stretch of those lengths
//!   that starts where a secret could: at the beginning, or after anything that
//!   is not a letter or a digit. Each is fingerprinted the same way and looked
//!   up. This is the layer that catches `correct-horse-battery` pasted into a
//!   note, which no pattern could — and `k(9]x;Q=2…` or a passphrase with
//!   spaces, which no split into words would.
//! * **Shapes**, always: the prefixes providers put on their keys, private-key
//!   blocks, JWTs, `otpauth://` links.
//!
//! It over-redacts rather than under-redacts on purpose — the cost of a false
//! alarm is a word Syn cannot read, the cost of a miss is a key in someone's
//! logs.

use std::collections::{BTreeSet, HashSet};
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
    /// The byte lengths of the values printed: what to look for.
    lengths: BTreeSet<usize>,
    /// Each value's length and first byte, so that almost every stretch of
    /// text is ruled out without hashing it.
    openings: HashSet<(usize, u8)>,
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
        let mut f = Fingerprints { key: Some(key), prints: HashSet::new(), lengths: BTreeSet::new(), openings: HashSet::new() };
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
                self.lengths.insert(value.len());
                self.openings.insert((value.len(), value.as_bytes()[0]));
            }
        }
    }

    fn print(&self, word: &str) -> Option<Print> {
        let key = self.key.as_ref()?;
        let hash = blake3::keyed_hash(key, word.as_bytes());
        Some(hash.as_bytes()[..16].try_into().expect("16 of 32"))
    }

    /// Take in `other`'s fingerprints. Both must be under the same key —
    /// made from the same Safe Key — or `other`'s mean nothing here.
    pub fn merge(&mut self, other: &Fingerprints) {
        if other.key.is_some() && other.key == self.key {
            self.prints.extend(other.prints.iter().copied());
            self.lengths.extend(other.lengths.iter().copied());
            self.openings.extend(other.openings.iter().copied());
        }
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

/// Where a secret may start in `text`: the beginning, and after every
/// character that is not a letter or a digit — a space, a quote, `=`, `:`, a
/// bracket, a slash.
fn starts(text: &str) -> impl Iterator<Item = usize> + '_ {
    std::iter::once(0).chain(text.char_indices().filter(|(_, c)| !c.is_alphanumeric()).map(|(i, c)| i + c.len_utf8()))
}

/// `text` with every secret it holds replaced by [`MARK`], and how many there
/// were. `prints` may be empty (the Safe is locked); shapes are always checked.
pub fn redact(text: &str, prints: &Fingerprints) -> (String, usize) {
    let mut spans: Vec<std::ops::Range<usize>> = shapes().find_iter(text).map(|m| m.range()).collect();
    if !prints.is_empty() {
        for start in starts(text) {
            for &len in &prints.lengths {
                let end = start + len;
                if end > text.len() {
                    break;
                }
                if prints.openings.contains(&(len, text.as_bytes()[start]))
                    && text.is_char_boundary(end)
                    && prints.matches(&text[start..end])
                {
                    spans.push(start..end);
                }
            }
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

    /// What Safe's own generator makes — symbols that quote, bracket and
    /// separate — and passphrases with spaces, glued to what labels them.
    #[test]
    fn generated_passwords_and_passphrases_are_caught_too() {
        let p = prints(&["k(9]x;Q=2{a,\"b|<z>", "correct horse battery staple", "dGVzdDp0ZXN0MTIz=="]);
        for (text, n) in [
            ("the new one is k(9]x;Q=2{a,\"b|<z> ok", 1),
            ("pw=k(9]x;Q=2{a,\"b|<z>", 1),
            ("{\"password\":\"k(9]x;Q=2{a,\"b|<z>\"}", 1),
            ("phrase: correct horse battery staple.", 1),
            ("token dGVzdDp0ZXN0MTIz==, thanks", 1),
            ("correct horse battery is not the whole phrase", 0),
        ] {
            let (out, got) = redact(text, &p);
            assert_eq!(got, n, "{text} → {out}");
            if n > 0 {
                assert!(!out.contains("k(9]x") && !out.contains("battery staple") && !out.contains("dGVzdDp0"), "{out}");
            }
        }
    }

    /// It runs over whole conversations before every request.
    #[test]
    fn a_long_text_against_many_secrets_is_quick() {
        let values: Vec<String> = (0..300).map(|i| format!("S3cret-{i}-{}", "x".repeat(i % 40))).collect();
        let p = prints(&values.iter().map(String::as_str).collect::<Vec<_>>());
        let text = "Lorem ipsum, dolor sit amet; (consectetur) \"adipiscing\" elit = 42. ".repeat(3_000);
        let started = std::time::Instant::now();
        let (_, n) = redact(&text, &p);
        assert_eq!(n, 0);
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "{:?} for {} bytes", started.elapsed(), text.len());
    }

    #[test]
    fn non_ascii_text_around_a_secret_is_cut_on_character_boundaries() {
        let p = prints(&["mật-khẩu-ünïcode"]);
        let (out, n) = redact("mật khẩu là «mật-khẩu-ünïcode» nhé", &p);
        assert_eq!(n, 1, "{out}");
        assert_eq!(out, format!("mật khẩu là «{MARK}» nhé"));
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
