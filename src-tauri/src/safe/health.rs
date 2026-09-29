//! How healthy each password is: section 9.8 of the design.
//!
//! Five judgements made on this device from what the Safe holds, and one that
//! asks the internet — only when the user turned it on:
//!
//! | Flag | When |
//! | --- | --- |
//! | `Weak` | a password scores under [`STRONG_ENOUGH`] |
//! | `Reused` | the same password is in another live item |
//! | `Old` | a password was last changed more than [`OLD_DAYS`] ago |
//! | `Expiring` / `Expired` | the item's own expiry is within [`EXPIRING_DAYS`], or past |
//! | `Breached` | Have I Been Pwned has seen it (see [`breach`]) |
//!
//! Flags are not secrets and travel with an item's summary. What they are made
//! from — the values — never leaves this module except as a hash.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::item::{FieldKind, ItemBody};
use super::store::ItemId;

/// zxcvbn's scale: 0 and 1 are guessed in minutes, 2 in hours offline, 3 and
/// 4 not in any time that matters. Below this is weak.
pub const STRONG_ENOUGH: u8 = 3;
pub const OLD_DAYS: i64 = 365;
pub const EXPIRING_DAYS: i64 = 14;
const DAY: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    Breached,
    Reused,
    Weak,
    Expired,
    Expiring,
    Old,
}

/// How hard a password is to guess.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Strength {
    /// 0–4, zxcvbn's scale.
    pub score: u8,
    /// log2 of the guesses it takes.
    pub bits: f64,
}

/// zxcvbn's estimate of the guesses a password takes — it knows dictionaries,
/// names, dates, keyboard walks and the substitutions people think are clever
/// — told `context` too (the item's title, the username, the email), so
/// `github2024` for GitHub scores as what it is.
///
/// Its numbers run low beside a naive entropy count, on purpose: a random
/// ten-character string is about 33 bits to zxcvbn, which assumes an attacker
/// who knows how people choose. That is why the flag uses its score, not a
/// bit count borrowed from another scale.
///
/// zxcvbn is a desktop dependency (about 1.3 MB, stripped, on arm64 — measured
/// when it was added). A phone falls back to `generator::estimate_bits`, which
/// knows only runs and character classes, mapped onto the same five steps.
pub fn strength(password: &str, context: &[&str]) -> Strength {
    #[cfg(desktop)]
    {
        let e = zxcvbn::zxcvbn(password, context);
        Strength { score: u8::from(e.score()), bits: e.guesses_log10() * std::f64::consts::LOG2_10 }
    }
    #[cfg(not(desktop))]
    {
        let _ = context;
        let bits = super::generator::estimate_bits(password);
        let score = match bits {
            b if b < 28.0 => 0,
            b if b < 40.0 => 1,
            b if b < 55.0 => 2,
            b if b < 75.0 => 3,
            _ => 4,
        };
        Strength { score, bits }
    }
}

/// What one item says about itself, without comparing it to any other.
fn own_flags(body: &ItemBody, now: i64) -> Vec<Flag> {
    let mut flags = Vec::new();
    let context: Vec<&str> = std::iter::once(body.title.as_str())
        .chain(body.fields.iter().filter(|f| matches!(f.kind, FieldKind::Username | FieldKind::Email)).map(|f| f.value.expose()))
        .collect();
    for f in body.fields.iter().filter(|f| f.kind == FieldKind::Password && !f.value.is_empty()) {
        if strength(f.value.expose(), &context).score < STRONG_ENOUGH && !flags.contains(&Flag::Weak) {
            flags.push(Flag::Weak);
        }
        // The value was set when the last one was pushed into history, or when
        // the item was made.
        let set_at = body.history.iter().filter(|h| h.field == f.id).map(|h| h.replaced_at).max().unwrap_or(body.created_at);
        if now - set_at > OLD_DAYS * DAY && !flags.contains(&Flag::Old) {
            flags.push(Flag::Old);
        }
    }
    match body.expires_at {
        Some(at) if at <= now => flags.push(Flag::Expired),
        Some(at) if at - now <= EXPIRING_DAYS * DAY => flags.push(Flag::Expiring),
        _ => {}
    }
    flags
}

/// One item, judged on its own, kept so that a change to one item does not
/// mean judging every other again — zxcvbn is fast, but not ten thousand
/// times over on every save.
#[derive(Debug, Clone, Default)]
pub struct Assessed {
    own: Vec<Flag>,
    /// Keyed fingerprints of its passwords, for the reuse check: equal
    /// passwords are found without holding any of them side by side.
    prints: Vec<[u8; 32]>,
}

/// `None` for an item in the trash, which is not judged.
pub fn assess_one(body: &ItemBody, now: i64, key: &[u8; 32]) -> Option<Assessed> {
    if body.trashed_at.is_some() {
        return None;
    }
    let prints = body
        .fields
        .iter()
        .filter(|f| f.kind == FieldKind::Password && !f.value.is_empty())
        .map(|f| *blake3::keyed_hash(key, f.value.expose().as_bytes()).as_bytes())
        .collect();
    Some(Assessed { own: own_flags(body, now), prints })
}

/// Every item's flags, from what each was judged on its own plus what they
/// share and what the last breach check found.
pub fn combine(assessed: &HashMap<ItemId, Assessed>, breached: &HashSet<ItemId>) -> HashMap<ItemId, Vec<Flag>> {
    let mut flags: HashMap<ItemId, Vec<Flag>> = assessed.iter().map(|(id, a)| (*id, a.own.clone())).collect();
    let mut by_password: HashMap<[u8; 32], HashSet<ItemId>> = HashMap::new();
    for (id, a) in assessed {
        for p in &a.prints {
            by_password.entry(*p).or_default().insert(*id);
        }
    }
    for ids in by_password.values().filter(|ids| ids.len() > 1) {
        for id in ids {
            let f = flags.entry(*id).or_default();
            if !f.contains(&Flag::Reused) {
                f.push(Flag::Reused);
            }
        }
    }
    for id in breached {
        if let Some(f) = flags.get_mut(id) {
            f.push(Flag::Breached);
        }
    }
    for f in flags.values_mut() {
        f.sort();
    }
    flags
}

/// Every live item's flags at once. What the session does in two steps.
pub fn assess<'a>(
    items: impl IntoIterator<Item = (ItemId, &'a ItemBody)>,
    now: i64,
    key: &[u8; 32],
    breached: &HashSet<ItemId>,
) -> HashMap<ItemId, Vec<Flag>> {
    let assessed = items.into_iter().filter_map(|(id, b)| assess_one(b, now, key).map(|a| (id, a))).collect();
    combine(&assessed, breached)
}

/// Checking passwords against Have I Been Pwned without sending them.
///
/// k-anonymity: SHA-1 of the password, the first five hex characters sent,
/// every suffix sharing that prefix sent back — a few hundred — and the match
/// made here. With `Add-Padding: true` the response is padded with fake
/// entries, so its size does not reveal how many real ones there are. SHA-1 is
/// the index HIBP uses, not a protection anybody relies on.
pub mod breach {
    use sha1::Digest as _;

    pub const RANGE_URL: &str = "https://api.pwnedpasswords.com/range/";

    /// Upper-case hex SHA-1, split into the five characters sent and the rest.
    pub fn split(password: &str) -> (String, String) {
        let hex = hex::encode_upper(sha1::Sha1::digest(password.as_bytes()));
        (hex[..5].to_string(), hex[5..].to_string())
    }

    /// Whether a range response lists `suffix`, and how often it was seen.
    /// Padding lines carry a count of 0 and are not a match.
    pub fn seen_in(response: &str, suffix: &str) -> Option<u64> {
        response.lines().find_map(|line| {
            let (s, count) = line.trim().split_once(':')?;
            let count: u64 = count.trim().parse().ok()?;
            (s.eq_ignore_ascii_case(suffix) && count > 0).then_some(count)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safe::item::{EditValue, FieldEdit, ItemEdit, ItemKind, SecretString};

    const KEY: [u8; 32] = [4; 32];

    fn login(title: &str, password: &str, created: i64) -> ItemBody {
        ItemBody::new_from(
            ItemEdit {
                kind: ItemKind::Login,
                title: title.into(),
                fields: vec![
                    FieldEdit { id: None, label: "username".into(), kind: FieldKind::Username, value: EditValue::Set { v: SecretString::new("anh".into()) } },
                    FieldEdit { id: None, label: "password".into(), kind: FieldKind::Password, value: EditValue::Set { v: SecretString::new(password.into()) } },
                ],
                urls: vec![],
                tags: vec![],
                favorite: false,
                notes: String::new(),
                totp: Default::default(),
                expires_at: None,
            },
            created,
        )
        .unwrap()
    }

    const NOW: i64 = 1_790_000_000;
    const STRONG: &str = "Vx7#qL9!mR2@tP5$wZ8&";

    #[test]
    fn a_strong_new_password_has_no_flags() {
        let body = login("Bank", STRONG, NOW);
        let f = assess([([1; 16], &body)], NOW, &KEY, &HashSet::new());
        assert_eq!(f[&[1; 16]], Vec::<Flag>::new());
    }

    #[test]
    fn weak_passwords_are_flagged_including_ones_built_from_the_title() {
        for weak in ["password1", "123456789012", "github2024", "anh12345"] {
            let body = login("GitHub", weak, NOW);
            let f = assess([([1; 16], &body)], NOW, &KEY, &HashSet::new());
            assert!(f[&[1; 16]].contains(&Flag::Weak), "{weak} was not weak");
        }
    }

    #[test]
    fn reuse_is_found_across_items_and_not_in_the_trash() {
        let a = login("A", STRONG, NOW);
        let b = login("B", STRONG, NOW);
        let mut c = login("C", STRONG, NOW);
        c.trashed_at = Some(NOW);
        let d = login("D", "Another-Str0ng!pass#word", NOW);
        let f = assess([([1; 16], &a), ([2; 16], &b), ([3; 16], &c), ([4; 16], &d)], NOW, &KEY, &HashSet::new());
        assert!(f[&[1; 16]].contains(&Flag::Reused) && f[&[2; 16]].contains(&Flag::Reused));
        assert!(!f.contains_key(&[3; 16]), "a trashed item is not assessed");
        assert!(!f[&[4; 16]].contains(&Flag::Reused));
    }

    #[test]
    fn age_counts_from_the_last_change_not_from_creation() {
        let old = login("Old", STRONG, NOW - 400 * DAY);
        let f = assess([([1; 16], &old)], NOW, &KEY, &HashSet::new());
        assert!(f[&[1; 16]].contains(&Flag::Old));

        let mut changed = login("Changed", "Zz9!first-Strong#pw", NOW - 400 * DAY);
        let ids: Vec<String> = changed.fields.iter().map(|x| x.id.clone()).collect();
        let edit = ItemEdit {
            kind: ItemKind::Login,
            title: "Changed".into(),
            fields: vec![
                FieldEdit { id: Some(ids[0].clone()), label: "username".into(), kind: FieldKind::Username, value: EditValue::Unchanged },
                FieldEdit { id: Some(ids[1].clone()), label: "password".into(), kind: FieldKind::Password, value: EditValue::Set { v: SecretString::new(STRONG.into()) } },
            ],
            urls: vec![],
            tags: vec![],
            favorite: false,
            notes: String::new(),
            totp: Default::default(),
            expires_at: None,
        };
        changed.apply(edit, NOW - 10 * DAY).unwrap();
        let f = assess([([2; 16], &changed)], NOW, &KEY, &HashSet::new());
        assert!(!f[&[2; 16]].contains(&Flag::Old), "{:?}", f[&[2; 16]]);
    }

    #[test]
    fn expiry_is_flagged_before_and_after() {
        let mut soon = login("Soon", "Vx7#qL9!mR2@tP5$wZ8&1", NOW);
        soon.expires_at = Some(NOW + 3 * DAY);
        let mut gone = login("Gone", "Vx7#qL9!mR2@tP5$wZ8&2", NOW);
        gone.expires_at = Some(NOW - DAY);
        let mut later = login("Later", "Vx7#qL9!mR2@tP5$wZ8&3", NOW);
        later.expires_at = Some(NOW + 60 * DAY);
        let f = assess([([1; 16], &soon), ([2; 16], &gone), ([3; 16], &later)], NOW, &KEY, &HashSet::new());
        assert_eq!(f[&[1; 16]], vec![Flag::Expiring]);
        assert_eq!(f[&[2; 16]], vec![Flag::Expired]);
        assert!(f[&[3; 16]].is_empty());
    }

    #[test]
    fn the_scale_runs_from_guessable_to_not() {
        assert!(strength("password", &[]).score <= 1);
        assert!(strength("correct horse battery staple", &[]).score >= 3);
        assert!(strength(STRONG, &[]).score == 4);
        assert!(strength("anh-github", &["GitHub", "anh"]).score < strength("anh-github", &[]).score, "context counts");
    }

    #[test]
    fn a_breach_found_is_carried_on_the_item() {
        let body = login("Bank", STRONG, NOW);
        let f = assess([([1; 16], &body)], NOW, &KEY, &HashSet::from([[1; 16]]));
        assert_eq!(f[&[1; 16]], vec![Flag::Breached]);
    }

    /// The HIBP example: "password" is SHA-1 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8.
    #[test]
    fn the_breach_check_sends_five_characters_and_matches_locally() {
        let (prefix, suffix) = breach::split("password");
        assert_eq!(prefix, "5BAA6");
        assert_eq!(suffix, "1E4C9B93F3F0682250B6CF8331B7EE68FD8");
        let response = "0018A45C4D1DEF81644B54AB7F969B88D65:1\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:9659365\r\n00D4F6E8FA6EECAD2A3AA415EEC418D38EC:0\r\n";
        assert_eq!(breach::seen_in(response, &suffix), Some(9_659_365));
        assert_eq!(breach::seen_in(response, "00D4F6E8FA6EECAD2A3AA415EEC418D38EC"), None, "padding is not a match");
        assert_eq!(breach::seen_in(response, "FFFF"), None);
    }
}
