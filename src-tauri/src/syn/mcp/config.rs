//! Which MCP servers a vault uses, where their secrets are, and which of them
//! this computer has agreed to start.
//!
//! # Three places, on purpose
//!
//! * **`{vault}/Syn/mcp.json`** — the servers: a name, an address or a
//!   command, and the *names* of the headers and environment variables that
//!   carry secrets. It lives in the vault beside `settings.json` and syncs with
//!   it, so a server set up on one computer is known on the other.
//! * **The OS keychain** — the secret values, one slot per header or variable,
//!   through `SecretManager`'s slot map like every API key. Never in the JSON:
//!   that file syncs, opens in any editor, and on a vault kept in git gets
//!   committed. See `slot`.
//! * **`{vault}/.synabit/mcp-trusted.json`** — which servers *this computer*
//!   has agreed to connect to, as they were when it agreed. A dotfile, which
//!   sync skips, for the reason `consent.json` is one.
//!
//! # Why the third one exists
//!
//! Because a stdio server is a program this computer will run. A config file
//! that syncs is a file another device — or anything that can write into the
//! vault folder — can change, and "start whatever command `mcp.json` names when
//! the app opens" would make that file a way to run code here without anybody
//! here choosing to. So a server is connected only while its transport is
//! exactly what was saved *on this computer*: a server added elsewhere, or
//! edited elsewhere, shows up in settings as needing to be turned on here, and
//! saving it here is the turning on. The same rule covers HTTP servers, which
//! costs nothing and means an address cannot be quietly swapped either.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

/// How a server is reached. Names of secrets only; values are in the keychain.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportConfig {
    Http {
        url: String,
        /// Headers whose values are secrets — `Authorization`, `X-Api-Key`.
        #[serde(default)]
        secret_headers: Vec<String>,
    },
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        /// Environment variables whose values are secrets.
        #[serde(default)]
        env_keys: Vec<String>,
    },
}

/// One server, as `mcp.json` holds it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub id: String,
    /// What the person calls it. It is what consent cards and the audit log
    /// name, and the model is told it before every tool from this server.
    pub name: String,
    pub transport: TransportConfig,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Server {
    /// The part of the tool names that says which server: `mcp__<slug>__…`.
    pub fn slug(&self) -> String {
        slug(&self.name)
    }

    pub fn is_stdio(&self) -> bool {
        matches!(self.transport, TransportConfig::Stdio { .. })
    }

    /// Every secret this server needs, as `(kind, key)`.
    pub fn secret_keys(&self) -> Vec<(&'static str, String)> {
        match &self.transport {
            TransportConfig::Http { secret_headers, .. } => {
                secret_headers.iter().map(|h| ("header", h.clone())).collect()
            }
            TransportConfig::Stdio { env_keys, .. } => env_keys.iter().map(|k| ("env", k.clone())).collect(),
        }
    }

    /// What this computer agreed to, written so that any change reads as a
    /// different string. The whole transport, including argument order.
    pub fn fingerprint(&self) -> String {
        serde_json::to_string(&self.transport).unwrap_or_default()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<Server>,
}

/// The keychain slot for one secret.
///
/// Keyed by the server's id rather than its name, so renaming a server does
/// not orphan its secret; and prefixed so nothing here can collide with a
/// provider's key slot.
pub fn slot(server_id: &str, kind: &str, key: &str) -> String {
    format!("mcp:{server_id}:{kind}:{key}")
}

/// `Jira Cloud` → `jira_cloud`, `Tệp của tôi` → `tep_cua_toi`.
///
/// Folded first, so a Vietnamese name keeps its letters rather than becoming a
/// row of underscores. Then letters and digits, lowercased; anything else
/// becomes one underscore, and never two in a row, because `__` is what
/// separates the server from the tool in `mcp__<slug>__<tool>`. Short, because
/// a provider allows sixty-four characters for the whole name.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in crate::search_fold::fold(name.trim()).chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out: String = out.trim_end_matches('_').chars().take(20).collect();
    let out = out.trim_end_matches('_').to_string();
    if out.is_empty() {
        "server".to_string()
    } else {
        out
    }
}

fn path(vault_path: &str) -> PathBuf {
    std::path::Path::new(vault_path).join("Syn").join("mcp.json")
}

/// The servers. An unreadable file is no servers, logged — and nothing is
/// started from a file this code cannot read.
pub fn load(vault_path: &str) -> McpConfig {
    let Ok(content) = std::fs::read_to_string(path(vault_path)) else {
        return McpConfig::default();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        log::warn!("[Syn] Syn/mcp.json is unreadable, using no MCP servers: {e}");
        McpConfig::default()
    })
}

pub fn save(vault_path: &str, config: &McpConfig) -> AppResult<()> {
    // Whole-file last writer wins across devices: the list is edited by hand,
    // rarely, on one screen at a time. `vault_json` stamps
    // `metadata.updated_at` so the newer edit is the one that survives.
    crate::syn::vault_json::write(&path(vault_path), config)
}

/// What is wrong with a server as entered, in a sentence for the screen.
///
/// `others` are the vault's other servers, for the one rule that is about
/// them: two servers cannot share a slug, because the slug is the only thing
/// in a tool's name that says which server it belongs to.
pub fn problem(server: &Server, others: &[Server]) -> Option<String> {
    let name = server.name.trim();
    if name.is_empty() {
        return Some("Give the server a name.".into());
    }
    if name.chars().count() > 40 {
        return Some("A name of forty characters or fewer.".into());
    }
    // A consent scope is `net_write:<name>:<tool>`; a colon in the name would
    // let two different servers' scopes read as one.
    if name.contains(':') {
        return Some("A name without a colon in it.".into());
    }
    let slug = server.slug();
    if others.iter().any(|o| o.id != server.id && o.slug() == slug) {
        return Some(format!("Another server already goes by “{name}”."));
    }

    match &server.transport {
        TransportConfig::Http { url, secret_headers } => {
            let Ok(parsed) = url::Url::parse(url.trim()) else {
                return Some("The address is not a URL.".into());
            };
            if !matches!(parsed.scheme(), "http" | "https") {
                return Some("The address has to start with http:// or https://.".into());
            }
            // This file syncs and is plain text. A password in the address
            // would be a password in the vault.
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Some("Put credentials in a secret header, not in the address — the address is saved in the vault.".into());
            }
            for header in secret_headers {
                let lower = header.trim().to_ascii_lowercase();
                if reqwest::header::HeaderName::from_bytes(lower.as_bytes()).is_err() || lower.is_empty() {
                    return Some(format!("“{header}” is not a header name."));
                }
                if super::transport_http::RESERVED_HEADERS.contains(&lower.as_str()) {
                    return Some(format!("“{header}” is set by Syn itself."));
                }
            }
        }
        TransportConfig::Stdio { command, args, env_keys } => {
            if command.trim().is_empty() {
                return Some("Say which program to run.".into());
            }
            if command.contains(['\n', '\r', '\0']) || args.iter().any(|a| a.contains('\0')) {
                return Some("The command has a character in it that cannot be run.".into());
            }
            for key in env_keys {
                let ok = key.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !ok {
                    return Some(format!("“{key}” is not an environment variable name."));
                }
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════
//  WHAT THIS COMPUTER AGREED TO
// ═══════════════════════════════════════════════════════════════

fn trust_path(vault_path: &str) -> AppResult<PathBuf> {
    let dir = std::path::Path::new(vault_path).join(".synabit");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("mcp-trusted.json"))
}

fn trust_map(vault_path: &str) -> HashMap<String, String> {
    trust_path(vault_path)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn save_trust(vault_path: &str, map: &HashMap<String, String>) -> AppResult<()> {
    let path = trust_path(vault_path)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(map)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Whether this computer agreed to this server exactly as it now is.
pub fn trusted_here(vault_path: &str, server: &Server) -> bool {
    trust_map(vault_path).get(&server.id).is_some_and(|f| f == &server.fingerprint())
}

/// Agree to it, as it now is. Called when it is saved from this computer's
/// settings screen, which is the person choosing it.
pub fn trust_here(vault_path: &str, server: &Server) -> AppResult<()> {
    let mut map = trust_map(vault_path);
    map.insert(server.id.clone(), server.fingerprint());
    save_trust(vault_path, &map)
}

pub fn forget_here(vault_path: &str, server_id: &str) -> AppResult<()> {
    let mut map = trust_map(vault_path);
    if map.remove(server_id).is_some() {
        save_trust(vault_path, &map)?;
    }
    Ok(())
}

/// Take back every permission granted for a server by this name.
///
/// Called when a server is removed, renamed or pointed somewhere else. A
/// grant says *read from Jira*; it was given about the Jira that was
/// configured then, and must not carry over to a different address that
/// happens to wear the same name.
pub fn revoke_grants(vault_path: &str, server_name: &str) {
    let name = server_name.to_lowercase();
    let read = format!("net_read:{name}");
    let write = format!("net_write:{name}:");
    for grant in crate::syn::consent::load(vault_path).grants {
        if grant.scope == read || grant.scope.starts_with(&write) {
            if let Err(e) = crate::syn::consent::revoke(vault_path, &grant.scope) {
                log::warn!("[Syn] Could not take back `{}`: {e}", grant.scope);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(name: &str, url: &str) -> Server {
        Server {
            id: format!("id-{name}"),
            name: name.into(),
            transport: TransportConfig::Http { url: url.into(), secret_headers: vec!["Authorization".into()] },
            enabled: true,
        }
    }

    #[test]
    fn a_slug_never_holds_the_separator() {
        assert_eq!(slug("Jira Cloud"), "jira_cloud");
        assert_eq!(slug("  a__b--c "), "a_b_c");
        assert_eq!(slug("Tệp của tôi"), "tep_cua_toi");
        assert_eq!(slug("!!!"), "server");
        assert!(!slug("x _ _ y").contains("__"));
        assert!(slug("a very long server name indeed, longer than twenty").len() <= 20);
    }

    #[test]
    fn the_config_round_trips_and_no_secret_is_in_it() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        let config = McpConfig {
            servers: vec![
                http("Jira", "https://mcp.example/jira"),
                Server {
                    id: "id-files".into(),
                    name: "Files".into(),
                    transport: TransportConfig::Stdio {
                        command: "/usr/local/bin/files-mcp".into(),
                        args: vec!["--root".into(), "/tmp".into()],
                        env_keys: vec!["FILES_TOKEN".into()],
                    },
                    enabled: false,
                },
            ],
        };
        save(vault, &config).expect("saved");
        let back = load(vault);
        assert_eq!(back.servers, config.servers);

        let raw = std::fs::read_to_string(dir.path().join("Syn/mcp.json")).expect("the file");
        assert!(raw.contains("\"Authorization\""), "the header's name is kept: {raw}");
        assert!(raw.contains("\"FILES_TOKEN\""), "and the variable's name");
        for field in ["value", "secret\":", "token\":", "password"] {
            assert!(!raw.to_lowercase().contains(field), "no field for a secret value: {field} in {raw}");
        }
    }

    #[test]
    fn an_unreadable_config_starts_nothing() {
        let dir = tempfile::tempdir().expect("temp");
        std::fs::create_dir_all(dir.path().join("Syn")).expect("dir");
        std::fs::write(dir.path().join("Syn/mcp.json"), "{ not json").expect("written");
        assert!(load(dir.path().to_str().expect("utf8")).servers.is_empty());
    }

    /// The file syncs. A server changed anywhere but here is not the server
    /// this computer agreed to start.
    #[test]
    fn a_server_changed_elsewhere_is_not_trusted_here() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        let mut server = http("Jira", "https://mcp.example/jira");

        assert!(!trusted_here(vault, &server), "added elsewhere: not yet");
        trust_here(vault, &server).expect("trusted");
        assert!(trusted_here(vault, &server));

        server.transport = TransportConfig::Http { url: "https://evil.example/".into(), secret_headers: vec![] };
        assert!(!trusted_here(vault, &server), "pointed somewhere else: asks again");

        server.transport = TransportConfig::Stdio { command: "/bin/sh".into(), args: vec![], env_keys: vec![] };
        assert!(!trusted_here(vault, &server), "turned into a program: asks again");

        assert!(
            !dir.path().join("Syn/mcp-trusted.json").exists() && dir.path().join(".synabit/mcp-trusted.json").exists(),
            "and the agreement is in the folder that does not sync"
        );
    }

    #[test]
    fn what_is_entered_is_checked_in_words() {
        let jira = http("Jira", "https://mcp.example/jira");
        assert_eq!(problem(&jira, &[]), None);

        let mut twin = http("JIRA", "https://other.example/");
        twin.id = "id-2".into();
        assert!(problem(&twin, std::slice::from_ref(&jira)).is_some(), "same slug");
        assert!(problem(&http("a:b", "https://x.example/"), &[]).is_some(), "a colon");
        assert!(problem(&http("x", "https://user:pw@x.example/"), &[]).is_some(), "a password in the address");
        assert!(problem(&http("x", "ftp://x.example/"), &[]).is_some());

        let mut header = http("x", "https://x.example/");
        header.transport = TransportConfig::Http { url: "https://x.example/".into(), secret_headers: vec!["Mcp-Session-Id".into()] };
        assert!(problem(&header, &[]).is_some(), "a header Syn sets");

        let program = |env: &str| Server {
            id: "p".into(),
            name: "p".into(),
            transport: TransportConfig::Stdio { command: "node".into(), args: vec![], env_keys: vec![env.into()] },
            enabled: true,
        };
        assert_eq!(problem(&program("API_KEY"), &[]), None);
        assert!(problem(&program("1BAD"), &[]).is_some());
        assert!(problem(&program("A=B"), &[]).is_some());
    }

    #[test]
    fn a_renamed_or_removed_server_leaves_no_permission_behind() {
        use crate::syn::consent::{record, Answer, Capability};
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        let now = chrono::Utc::now();
        record(vault, &Capability::NetRead { domain: "Jira".into() }, Answer::Always, now).expect("granted");
        record(vault, &Capability::NetWrite { domain: "Jira".into(), tool: "create_issue".into() }, Answer::Always, now)
            .expect("granted");
        record(vault, &Capability::NetRead { domain: "Jira Two".into() }, Answer::Always, now).expect("granted");

        revoke_grants(vault, "Jira");
        let left: Vec<String> = crate::syn::consent::load(vault).grants.into_iter().map(|g| g.scope).collect();
        assert_eq!(left, vec!["net_read:jira two".to_string()]);
    }
}
