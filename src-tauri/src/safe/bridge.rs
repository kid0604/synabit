//! Where Syn meets the Safe: the few questions it may ask, and the words it is
//! told when the answer is no.
//!
//! Everything here goes through the process's open Safe
//! (`session::global()`), for the vault the run is in. Nothing here returns a
//! value to Syn — [`fill`] returns one only to the connector call that is about
//! to send it.
//!
//! The refusals are written for the model: what happened, and what it may do
//! next. "Do not ask the user for the value" is in them on purpose — the
//! obvious workaround for a locked Safe is to ask for the password in chat,
//! which is the one thing the whole design exists to make unnecessary.

use std::path::Path;

use serde_json::Value;

use super::egress::{self, Destination, EgressError, Injected};
use super::session::{global, AiItem, SafeError};

/// The server a connector tool belongs to, as the destination an item lists.
fn destination_of(tool: &str) -> Option<(Destination, String)> {
    let offered = crate::syn::connector::provider::find_anywhere(tool)?;
    Some((Destination::Connector(offered.server_id), offered.server_name))
}

fn said(e: &EgressError, server: &str) -> String {
    match e {
        EgressError::Locked => "The Safe is locked. Tell the user it needs to be unlocked in Synabit for this, and \
             stop here. Do not ask them for the value instead."
            .into(),
        EgressError::Unknown(h) => format!(
            "There is no Safe item named `{h}` that you may use. Call safe_list to see the names you may use; if \
             what you need is not there, call safe_request so the user can add it. Never ask for the value in chat."
        ),
        EgressError::NotAllowed { handle, .. } => format!(
            "The Safe item `{handle}` may not be sent to {server}. Only the user can allow that, in Safe. Do not \
             try another way."
        ),
        EgressError::NoSuchField(h) => format!(
            "The Safe item `{h}` has no hidden field by that name. Use `{{{{safe:{h}}}}}` for its main secret."
        ),
    }
}

fn locked(e: SafeError) -> EgressError {
    match e {
        SafeError::Locked => EgressError::Locked,
        other => EgressError::Unknown(other.to_string()),
    }
}

/// Whether the item Syn calls `handle` may go to the server behind `tool`.
/// `Ok` carries the destination as a consent scope names it, and the server's
/// name for the card.
pub fn may_send(vault: &str, tool: &str, handle: &str) -> Result<(String, String), String> {
    let (destination, server) =
        destination_of(tool).ok_or_else(|| format!("`{tool}` is not connected right now."))?;
    global()
        .peek(Path::new(vault), |u| Ok(u.permits(handle, &destination)))
        .map_err(locked)
        .and_then(|r| r)
        .map(|()| (destination.key(), server.clone()))
        .map_err(|e| said(&e, &server))
}

/// A copy of `args` with its placeholders filled, for the server `server_id`.
pub fn fill(vault: &str, server_id: &str, server_name: &str, args: &Value) -> Result<(Value, Injected), String> {
    let destination = Destination::Connector(server_id.to_string());
    global()
        .peek(Path::new(vault), |u| Ok(egress::fill(args, &destination, u)))
        .map_err(locked)
        .and_then(|r| r)
        .map_err(|e| said(&e, server_name))
}

/// A connector's header or variable whose stored value is a placeholder —
/// `Bearer {{safe:linear-key}}` — filled from the open Safe for that server.
/// `None` when it cannot be: the Safe is locked, or the item is not the
/// server's to have. The connector then connects without it and the server
/// says so, which the settings screen shows.
pub fn fill_setting(server_id: &str, server_name: &str, stored: &str) -> Option<String> {
    let destination = Destination::Connector(server_id.to_string());
    let filled = global().peek_any(|open| {
        let u = open.ok_or(EgressError::Locked)?;
        egress::fill(&Value::String(stored.to_string()), &destination, u)
    });
    match filled {
        Ok((Value::String(s), _)) => Some(s),
        Ok(_) => None,
        Err(e) => {
            log::warn!("[Safe] {server_name}: a setting names a Safe item it could not have: {}", said(&e, server_name));
            None
        }
    }
}

/// `text` with anything that looks like a secret hidden — every value of any
/// open Safe, and the shapes of known keys. For what leaves or is kept:
/// prompts, logs, messages. See `safe::guard`.
pub fn redact(text: &str) -> (String, usize) {
    global().peek_any(|open| match open {
        Some(u) => super::guard::redact(text, u.fingerprints()),
        None => super::guard::redact(text, &super::guard::Fingerprints::default()),
    })
}

/// How healthy the Safe is, for Syn: counts, and flags on the items Syn may
/// already know of by name. Nothing about a hidden item but that it counts.
pub fn health(vault: &str) -> Result<serde_json::Value, String> {
    global().peek(Path::new(vault), |u| Ok(u.health_for_syn())).map_err(|e| match e {
        SafeError::Locked => "The Safe is locked, so its health cannot be read. Say so.".into(),
        other => other.to_string(),
    })
}

/// The items Syn may know of, or why it cannot be told.
pub fn list(vault: &str) -> Result<Vec<AiItem>, String> {
    global().peek(Path::new(vault), |u| Ok(u.ai_items())).map_err(|e| match e {
        SafeError::Locked => "The Safe is locked, so its names cannot be listed. Say so; do not ask for values.".into(),
        other => other.to_string(),
    })
}
