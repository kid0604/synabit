//! Which one Telegram account may talk to this bot.
//!
//! # Why the app makes the code and the phone brings it
//!
//! Hermes pairs the other way round: a stranger messages the bot, the bot
//! answers with a code, and the owner approves it at a terminal. That is the
//! right shape for something with no screen, and it means the bot answers
//! strangers.
//!
//! Synabit has a screen, and the person who owns the vault is sitting at it
//! when they pair. So the app makes a code, the phone sends it back through a
//! `t.me` link, and the bot never has to say a word to anybody it does not
//! know.
//!
//! A code is 128 random bits, lasts ten minutes and works once. Pairing again
//! replaces whoever was paired before; unpairing is available from both ends.

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::db::DbBridge;
use crate::error::AppResult;

const PENDING_KEY: &str = "telegram:pairing";
const PAIRED_KEY: &str = "telegram:paired";

/// How long a pairing link works.
pub const LASTS_MINUTES: i64 = 10;

/// A code the app has shown and nobody has used yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub code: String,
    pub expires_at: String,
}

/// The account this bot answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paired {
    pub user_id: i64,
    pub chat_id: i64,
    pub name: String,
    pub paired_at: String,
}

/// Start pairing: a fresh code, replacing any unused one.
pub fn begin(db: &DbBridge, now: DateTime<Utc>) -> AppResult<Pending> {
    let bytes: [u8; 16] = rand::random();
    let pending = Pending {
        // URL-safe without padding: 22 characters, all of them allowed in a
        // deep link's start parameter.
        code: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
        expires_at: (now + Duration::minutes(LASTS_MINUTES)).to_rfc3339(),
    };
    db.set_kv(PENDING_KEY, &serde_json::to_string(&pending)?)?;
    Ok(pending)
}

/// Accept a code, if it is the one shown and still good.
///
/// A wrong code leaves the right one in place: a stranger guessing cannot use
/// up somebody else's link. An expired one is removed.
pub fn redeem(
    db: &DbBridge,
    code: &str,
    user_id: i64,
    chat_id: i64,
    name: &str,
    now: DateTime<Utc>,
) -> AppResult<bool> {
    let Some(pending) = db
        .get_kv(PENDING_KEY)?
        .and_then(|raw| serde_json::from_str::<Pending>(&raw).ok())
    else {
        return Ok(false);
    };

    let expired = DateTime::parse_from_rfc3339(&pending.expires_at)
        .map(|at| now >= at)
        .unwrap_or(true);
    if expired {
        db.delete_kv(PENDING_KEY)?;
        return Ok(false);
    }
    if !same(code.as_bytes(), pending.code.as_bytes()) {
        return Ok(false);
    }

    db.delete_kv(PENDING_KEY)?;
    let paired = Paired {
        user_id,
        chat_id,
        name: name.to_string(),
        paired_at: now.to_rfc3339(),
    };
    db.set_kv(PAIRED_KEY, &serde_json::to_string(&paired)?)?;
    Ok(true)
}

pub fn paired(db: &DbBridge) -> Option<Paired> {
    db.get_kv(PAIRED_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

/// Forget the paired account, and any code still waiting.
pub fn unpair(db: &DbBridge) -> AppResult<()> {
    db.delete_kv(PAIRED_KEY)?;
    db.delete_kv(PENDING_KEY)
}

/// The link that pairs whoever opens it.
pub fn link(bot_username: &str, code: &str) -> String {
    format!("https://t.me/{}?start={code}", bot_username.trim_start_matches('@'))
}

/// Compared without stopping at the first difference.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-13T08:00:00Z").expect("a date").with_timezone(&Utc)
            + Duration::minutes(minutes)
    }

    /// A code is long, unguessable, and fits in a deep link.
    #[test]
    fn a_code_fits_a_link_and_is_not_repeated() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        let first = begin(&db, at(0)).expect("a code").code;
        let second = begin(&db, at(0)).expect("a code").code;
        assert_eq!(first.len(), 22);
        assert!(first.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(first, second);
        assert_eq!(link("@synabit_bot", &second), format!("https://t.me/synabit_bot?start={second}"));
    }

    /// It works once.
    #[test]
    fn a_code_pairs_once() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        let code = begin(&db, at(0)).expect("a code").code;

        assert!(redeem(&db, &code, 42, 42, "An", at(1)).expect("checked"));
        assert_eq!(paired(&db).map(|p| p.user_id), Some(42));
        assert!(!redeem(&db, &code, 99, 99, "Someone", at(2)).expect("checked"), "used up");
        assert_eq!(paired(&db).map(|p| p.user_id), Some(42), "and the pairing stands");
    }

    /// A wrong guess does not use up the right link.
    #[test]
    fn a_wrong_code_does_not_spend_the_right_one() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        let code = begin(&db, at(0)).expect("a code").code;

        assert!(!redeem(&db, "not-the-code-at-all-xx", 99, 99, "Someone", at(1)).expect("checked"));
        assert!(paired(&db).is_none());
        assert!(redeem(&db, &code, 42, 42, "An", at(2)).expect("checked"));
    }

    /// Ten minutes.
    #[test]
    fn a_code_expires() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        let code = begin(&db, at(0)).expect("a code").code;
        assert!(!redeem(&db, &code, 42, 42, "An", at(LASTS_MINUTES)).expect("checked"));
        assert!(paired(&db).is_none());
    }

    #[test]
    fn unpairing_forgets_the_account() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        let code = begin(&db, at(0)).expect("a code").code;
        redeem(&db, &code, 42, 42, "An", at(1)).expect("checked");
        unpair(&db).expect("unpaired");
        assert!(paired(&db).is_none());
    }
}
