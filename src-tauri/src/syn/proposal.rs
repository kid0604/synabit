//! Things Syn would like to remember, waiting to be allowed to.
//!
//! # Why a queue rather than just writing it down
//!
//! The cheap version of memory is to let the assistant call `remember` whenever
//! it feels like it. That version fills a vault with a model's guesses about
//! somebody, and the person it is about finds out by scrolling a list of forty
//! claims, half of them wrong, none of them asked for. The vault is the
//! product; filling it with unreviewed inference is the fastest way to make it
//! worth less than it was.
//!
//! So reflection proposes and the user disposes. `remember` still exists and is
//! still called directly — when somebody says "nhớ giùm tao là…", that is not a
//! guess and it does not need reviewing. What goes in this queue is the other
//! thing: what Syn *worked out* from a conversation without being asked to.
//!
//! # Why not in the vault as nodes
//!
//! Because a proposal is not something the user keeps. Writing it as a node
//! would put the guess in the vault in order to ask permission to put the guess
//! in the vault, and a declined proposal would leave a file to clean up. It
//! lives in `{vault}/Syn/proposals.json`, beside the conversations and the run
//! transcripts, which is where this app already keeps its own working papers.
//!
//! Accepting one writes a real memory node. Declining one leaves nothing.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// How many proposals are kept before the oldest are dropped.
///
/// A tray nobody has emptied in a month is not a tray, it is a list — and a
/// list of stale guesses is exactly the noise this queue exists to keep out of
/// the vault. Small enough that it stays reviewable.
const KEEP_PROPOSALS: usize = 40;

/// How many declines are remembered.
///
/// Larger than the queue, and deliberately so: a decline has to outlive the
/// proposal it killed, and the queue is truncated at forty. Mixing the two
/// lifetimes in one list is how a queue quietly becomes a graveyard.
const KEEP_DECLINED: usize = 200;

/// Something Syn would remember, if allowed.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Proposal {
    pub id: String,
    /// The memory as it would be written.
    pub body: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub confidence: f64,
    /// Why Syn thinks this is worth keeping.
    ///
    /// Shown to the user, and the reason the tray is reviewable at all: "they
    /// moved three afternoon meetings in August" is checkable, and "I think
    /// they prefer mornings" is not.
    pub because: String,
    /// The run that proposed it, so its transcript can be read.
    pub source_run: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// The exact text of the memory this one replaces, when it replaces one.
    ///
    /// Named by text rather than by id because the reflector is shown bodies,
    /// not identifiers, and asking a model to invent an id it was never given
    /// is asking it to make one up. Resolved to an id when the user accepts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// Whether this came out of the user correcting Syn.
    ///
    /// The rarest and strongest signal the app gets, and worth showing
    /// differently in the tray: "you told me I had this wrong" is a better
    /// reason to keep something than "you mentioned it once".
    #[serde(default)]
    pub from_correction: bool,
    pub proposed_at: String,
}

fn syn_dir(vault_path: &str) -> AppResult<PathBuf> {
    let dir = Path::new(vault_path).join("Syn");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::General(format!("Failed to create Syn directory: {e}")))?;
    Ok(dir)
}

fn queue_path(vault_path: &str) -> AppResult<PathBuf> {
    Ok(syn_dir(vault_path)?.join("proposals.json"))
}

/// Where the answers "no" are kept.
///
/// Its own file rather than a section of `proposals.json`, for two reasons. The
/// queue is truncated to its newest forty and this list must not be; and adding
/// a second shape to a file already in people's vaults means a migration, for
/// no gain over a second file beside it.
///
/// Not a dotfile. Consent grants are per-device and must not sync; a decline is
/// the user's judgement about themselves, and should hold on every machine they
/// use.
fn declined_path(vault_path: &str) -> AppResult<PathBuf> {
    Ok(syn_dir(vault_path)?.join("declined.json"))
}

/// One suggestion the user has already turned down.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Declined {
    /// The body as it was proposed, kept readable so a person can audit the
    /// file and delete an entry to let a suggestion come back.
    pub body: String,
    pub at: String,
}

/// What the user has said no to. Unreadable or absent means nothing yet.
pub fn declined(vault_path: &str) -> Vec<Declined> {
    let Ok(path) = declined_path(vault_path) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<Declined>>(&content).unwrap_or_else(|e| {
        log::warn!("[Syn] Declined list is unreadable, treating it as empty: {e}");
        Vec::new()
    })
}

/// Record that this suggestion was turned down, so it stops coming back.
pub fn decline(vault_path: &str, body: &str) -> AppResult<()> {
    let body = body.trim();
    if body.is_empty() {
        return Ok(());
    }
    let mut list = declined(vault_path);
    if list.iter().any(|d| folded(&d.body) == folded(body)) {
        return Ok(());
    }
    list.insert(
        0,
        Declined {
            body: body.to_string(),
            at: chrono::Utc::now().to_rfc3339(),
        },
    );
    list.truncate(KEEP_DECLINED);
    let path = declined_path(vault_path)?;
    atomic_write(&path, &serde_json::to_string_pretty(&list)?)
}

/// Unicode case folding, not `eq_ignore_ascii_case`.
///
/// This app's users write Vietnamese, `Ọ` and `ọ` are not ASCII, and ASCII
/// folding leaves them different. Diacritics are deliberately *not* folded:
/// `má` and `ma` are different words, and two memories that differ only by tone
/// are two memories.
fn folded(text: &str) -> String {
    text.trim().to_lowercase()
}

/// Write through a temp file, so a crash leaves the old queue rather than half
/// the new one.
fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::General(format!("Failed to rename temp proposal file: {e}"))
    })?;
    Ok(())
}

/// Everything waiting to be reviewed, newest first.
///
/// A corrupt or absent queue is an empty one. This is a tray of suggestions;
/// refusing to answer a message because it could not be read would be a
/// spectacular over-reaction.
pub fn list(vault_path: &str) -> Vec<Proposal> {
    let Ok(path) = queue_path(vault_path) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<Proposal>>(&content) {
        Ok(mut queue) => {
            queue.sort_by(|a, b| b.proposed_at.cmp(&a.proposed_at));
            queue
        }
        Err(e) => {
            log::warn!("[Syn] Proposal queue is unreadable, treating it as empty: {e}");
            Vec::new()
        }
    }
}

fn save(vault_path: &str, queue: &[Proposal]) -> AppResult<()> {
    let path = queue_path(vault_path)?;
    atomic_write(&path, &serde_json::to_string_pretty(queue)?)
}

/// Add proposals, dropping any that repeat something already queued.
///
/// Matched on the body rather than on an id, because the same conversation
/// happening twice produces the same suggestion twice and the user should not
/// have to decline it twice.
pub fn add(vault_path: &str, new: Vec<Proposal>) -> AppResult<usize> {
    if new.is_empty() {
        return Ok(0);
    }
    let mut queue = list(vault_path);
    let refused = declined(vault_path);
    let mut added = 0;

    let same = |a: &str, b: &str| folded(a) == folded(b);

    for proposal in new {
        // Two ways a suggestion is old news: it is already waiting, or it was
        // already turned down. Only the first used to be checked, and because
        // declining removes a proposal from the queue, the check that was meant
        // to spare the user a second decline stopped applying at exactly the
        // moment it was needed. Reflection runs after every message, so the
        // same suggestion could return within the hour.
        let already = queue.iter().any(|q| same(&q.body, &proposal.body))
            || refused.iter().any(|d| same(&d.body, &proposal.body));
        if already {
            continue;
        }
        queue.push(proposal);
        added += 1;
    }

    queue.sort_by(|a, b| b.proposed_at.cmp(&a.proposed_at));
    queue.truncate(KEEP_PROPOSALS);
    save(vault_path, &queue)?;
    Ok(added)
}

/// Take one out of the queue and hand it back.
///
/// Used by both answers — accepting one needs its contents to write the memory,
/// and declining one needs it gone. Returns `None` for an id that is not there,
/// which is what a second click on the same button looks like.
pub fn take(vault_path: &str, id: &str) -> AppResult<Option<Proposal>> {
    let mut queue = list(vault_path);
    let Some(index) = queue.iter().position(|p| p.id == id) else {
        return Ok(None);
    };
    let proposal = queue.remove(index);
    save(vault_path, &queue)?;
    Ok(Some(proposal))
}

/// Turn one down: out of the queue, and into the record of refusals.
///
/// One function rather than two calls at the call site, so the rule cannot be
/// half-applied. Accepting uses `take` alone — an accepted suggestion becomes a
/// memory, and a memory the user later forgets is a different decision from a
/// suggestion they refused.
pub fn dismiss(vault_path: &str, id: &str) -> AppResult<Option<Proposal>> {
    let Some(proposal) = take(vault_path, id)? else {
        return Ok(None);
    };
    decline(vault_path, &proposal.body)?;
    Ok(Some(proposal))
}

/// Empty the tray without accepting anything.
pub fn clear(vault_path: &str) -> AppResult<()> {
    save(vault_path, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Declining is a decision, and it has to outlast the thing declined.
    ///
    /// `add` dedups against the queue, and declining removes the proposal from
    /// the queue — so the guard against a second decline stopped applying at
    /// exactly the moment it was needed. Reflection runs after every completed
    /// message, so the same suggestion could be back within the hour.
    #[test]
    fn a_declined_suggestion_does_not_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();

        let first = proposal("Người dùng thích cà phê đen.", "2026-09-04T10:00:00Z");
        let id = first.id.clone();
        assert_eq!(add(vault, vec![first]).unwrap(), 1, "queued once");

        dismiss(vault, &id).unwrap().expect("it was there to decline");
        assert!(list(vault).is_empty(), "gone from the tray");

        let again = proposal("Người dùng thích cà phê đen.", "2026-09-05T10:00:00Z");
        assert_eq!(
            add(vault, vec![again]).unwrap(),
            0,
            "a suggestion already turned down is not queued again"
        );
    }

    /// The same refusal in different case is the same refusal.
    #[test]
    fn a_decline_folds_case_the_way_vietnamese_needs() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();

        decline(vault, "Ọc ọc, người dùng thích trà.").unwrap();
        assert_eq!(
            add(
                vault,
                vec![proposal("ọc ọc, NGƯỜI DÙNG thích trà.", "2026-09-05T10:00:00Z")]
            )
            .unwrap(),
            0,
            "`Ọ` and `ọ` are not ASCII, and ASCII folding leaves them different"
        );
    }

    /// Accepting is not refusing.
    ///
    /// Both empty the tray, and it would be easy to route them through the same
    /// function. Then a memory the user accepted and later forgot could never be
    /// suggested again, which is not what forgetting one means.
    #[test]
    fn accepting_does_not_record_a_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();

        let p = proposal("Người dùng ăn chay.", "2026-09-04T10:00:00Z");
        let id = p.id.clone();
        add(vault, vec![p]).unwrap();
        take(vault, &id).unwrap().expect("accepted");

        assert!(declined(vault).is_empty(), "accepting records no refusal");
    }

    fn proposal(body: &str, at: &str) -> Proposal {
        Proposal {
            id: uuid::Uuid::new_v4().to_string(),
            body: body.to_string(),
            kind: "preference".into(),
            subject: None,
            confidence: 0.6,
            because: "they said so twice".into(),
            source_run: "run-1".into(),
            conversation_id: None,
            supersedes: None,
            from_correction: false,
            proposed_at: at.to_string(),
        }
    }

    #[test]
    fn a_vault_with_no_queue_has_an_empty_one() {
        let dir = tempfile::tempdir().expect("temp");
        assert!(list(dir.path().to_str().expect("utf8")).is_empty());
    }

    #[test]
    fn proposals_survive_a_round_trip_and_come_back_newest_first() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");

        add(
            vault,
            vec![
                proposal("họp buổi sáng", "2026-09-01T00:00:00Z"),
                proposal("thích trà", "2026-09-03T00:00:00Z"),
            ],
        )
        .expect("adds");

        let queue = list(vault);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].body, "thích trà", "newest first");
        assert_eq!(queue[0].because, "they said so twice");
    }

    /// The same conversation twice suggests the same thing twice, and nobody
    /// should have to decline it twice.
    #[test]
    fn the_same_suggestion_is_not_queued_again() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");

        assert_eq!(add(vault, vec![proposal("họp sáng", "2026-09-01T00:00:00Z")]).expect("adds"), 1);
        assert_eq!(
            add(vault, vec![proposal("  HỌP SÁNG  ", "2026-09-02T00:00:00Z")]).expect("adds"),
            0,
            "same suggestion, differently spaced and cased"
        );
        assert_eq!(list(vault).len(), 1);
    }

    #[test]
    fn taking_one_hands_it_back_and_leaves_the_rest() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        add(
            vault,
            vec![
                proposal("a", "2026-09-01T00:00:00Z"),
                proposal("b", "2026-09-02T00:00:00Z"),
            ],
        )
        .expect("adds");

        let id = list(vault)[0].id.clone();
        let taken = take(vault, &id).expect("takes").expect("was there");
        assert_eq!(taken.body, "b");
        assert_eq!(list(vault).len(), 1);

        // A second click on the same button is not an error.
        assert!(take(vault, &id).expect("takes").is_none());
    }

    #[test]
    fn a_tray_nobody_empties_stops_growing() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");

        let many: Vec<Proposal> = (0..KEEP_PROPOSALS + 15)
            .map(|i| proposal(&format!("thing {i}"), &format!("2026-09-{:02}T00:00:00Z", i % 28 + 1)))
            .collect();
        add(vault, many).expect("adds");

        assert_eq!(list(vault).len(), KEEP_PROPOSALS);
    }

    /// A tray of suggestions that cannot be read is not a reason to refuse the
    /// message the user is waiting on.
    #[test]
    fn an_unreadable_queue_is_an_empty_one_rather_than_an_error() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        std::fs::create_dir_all(dir.path().join("Syn")).expect("mkdir");
        std::fs::write(dir.path().join("Syn/proposals.json"), "{ not json").expect("write");

        assert!(list(vault).is_empty());
        // And it recovers: the next write replaces the rubbish.
        add(vault, vec![proposal("a", "2026-09-01T00:00:00Z")]).expect("adds");
        assert_eq!(list(vault).len(), 1);
    }
}
