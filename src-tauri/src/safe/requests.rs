//! Syn asking the user for a secret, without the secret passing through Syn.
//!
//! `safe_request` opens a request here and shows a card; the card sends the
//! value to `safe_request_submit`, which takes the request back by its id and
//! writes the item. The id is single-use and short-lived, so a card cannot be
//! replayed, and one Syn never opened cannot be submitted.
//!
//! What Syn suggested — the title, the handle, the connectors — is only a
//! suggestion: the card shows it, the user may change every part, and nothing
//! is shared with Syn beyond what they leave ticked.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

pub const EVENT: &str = "safe://request";

/// How long a card stays answerable.
const LIFETIME: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Destination {
    /// As an item stores it: `connector:<id>`.
    pub key: String,
    pub label: String,
    /// Whether Syn named it. The card ticks these to begin with.
    pub suggested: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Request {
    pub id: String,
    pub title: String,
    pub handle: String,
    pub why: String,
    /// Every connector in the vault, the suggested ones marked.
    pub destinations: Vec<Destination>,
}

struct Open {
    request: Request,
    vault: String,
    at: Instant,
}

static OPEN: Mutex<Option<HashMap<String, Open>>> = Mutex::new(None);

fn connectors(vault: &str) -> Vec<(String, String)> {
    crate::syn::connector::config::load(vault)
        .servers
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| (s.id, s.name))
        .collect()
}

/// Open a request and return what the card shows.
pub fn open(vault: &str, title: &str, handle: &str, why: &str, wanted: &[String]) -> Request {
    open_with(vault, title, handle, why, wanted, connectors(vault))
}

fn open_with(vault: &str, title: &str, handle: &str, why: &str, wanted: &[String], servers: Vec<(String, String)>) -> Request {
    let fold = |s: &str| crate::syn::connector::config::slug(s);
    let destinations = servers
        .into_iter()
        .map(|(id, name)| Destination {
            suggested: wanted.iter().any(|w| fold(w) == fold(&name) || w == &id),
            key: format!("connector:{id}"),
            label: name,
        })
        .collect();
    let id = hex::encode(super::crypto::random_bytes::<16>().expect("the system random number generator failed"));
    let request = Request { id: id.clone(), title: title.into(), handle: handle.into(), why: why.into(), destinations };
    let mut guard = OPEN.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    map.retain(|_, o| o.at.elapsed() < LIFETIME);
    map.insert(id, Open { request: request.clone(), vault: vault.into(), at: Instant::now() });
    request
}

/// Take a request back, once, for the vault it was opened in. `None` for an
/// id never issued, already used, expired, or from another vault.
pub fn take(vault: &str, id: &str) -> Option<Request> {
    let mut guard = OPEN.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.as_mut()?;
    let open = map.remove(id)?;
    (open.vault == vault && open.at.elapsed() < LIFETIME).then_some(open.request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn servers() -> Vec<(String, String)> {
        vec![("id-linear".into(), "Linear".into()), ("id-jira".into(), "Jira Cloud".into())]
    }

    #[test]
    fn suggested_connectors_are_marked_and_the_rest_offered() {
        let r = open_with("/v", "Linear API key", "linear-key", "file issues", &["linear".into()], servers());
        assert_eq!(r.destinations.len(), 2);
        assert!(r.destinations[0].suggested && r.destinations[0].key == "connector:id-linear");
        assert!(!r.destinations[1].suggested);
    }

    #[test]
    fn a_request_is_taken_once_and_only_in_its_vault() {
        let r = open_with("/v", "t", "h1", "w", &[], servers());
        assert!(take("/other", &r.id).is_none(), "another vault took it");
        let r = open_with("/v", "t", "h2", "w", &[], servers());
        assert!(take("/v", &r.id).is_some());
        assert!(take("/v", &r.id).is_none(), "taken twice");
        assert!(take("/v", "never-issued").is_none());
    }
}
