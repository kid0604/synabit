//! Which tools exist, what they cost to undo, and who runs them.
//!
//! # Why a registry rather than a longer `match`
//!
//! `tools::execute_tool` is a `match` on a name over a fixed list, and for one
//! fixed list that is exactly right — it is fast, it is exhaustive, and the
//! compiler checks it. What it cannot do is answer two questions that are about
//! to be asked constantly:
//!
//! * **Which tools does *this* run get?** A run that only reads the vault has no
//!   business seeing `create_transaction`, and a run that has not been granted
//!   network access must not be shown a tool that uses it. Every tool in the
//!   list is charged for in tokens on every turn, and a longer list makes the
//!   model likelier to pick wrongly from it — the reason twenty tools became
//!   twelve. Tools that come and go cannot be a `match`.
//! * **What happens if it goes wrong?** Everything Syn can do today is undoable
//!   from inside the app, and that — not a permission prompt — is what makes it
//!   safe to let it write to a vault without asking. That property is currently
//!   true by inspection. It needs to be true by declaration before anything can
//!   send an email.
//!
//! So a tool now belongs to a *provider*, a provider answers what its tools are
//! and what they cost to undo, and the registry is the list of providers. Today
//! there is one provider and it wraps the existing `match` unchanged. That is
//! the point: the seam is cut before it is needed, while there is still only
//! one thing on either side of it.
//!
//! # What is deliberately not here yet
//!
//! No consent, no grants, no network capabilities. `Capability` lists the three
//! kinds of thing that exist today and nothing it cannot yet produce — an enum
//! arm with no producer is a claim the code cannot keep.

use serde::Serialize;
use serde_json::Value;

use crate::error::AppResult;
use crate::models::syn::ToolDefinition;

// ═══════════════════════════════════════════════════════════════
//  WHAT A TOOL COSTS TO UNDO
// ═══════════════════════════════════════════════════════════════

/// How a tool's effect can be taken back.
///
/// Recorded on every step of a run's transcript, so a person reading what
/// happened is told not only what Syn did but how to undo it. It is also the
/// field that a later phase asks before deciding whether to stop and ask
/// permission — the rule being that what reverses itself does not need to ask,
/// and what does not, does.
///
/// `how` is an owned `String` rather than a `&'static str` because a transcript
/// is read back from disk as well as written, and a borrowed lifetime cannot
/// survive that. One small allocation per tool call, against a database query
/// and a network round trip.
#[derive(Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reversal {
    /// Read something and changed nothing.
    Nothing,
    /// Undone from inside the app, by the means named.
    Automatic { how: String },
    /// Undone, but somebody has to go and do it somewhere else.
    Manual { how: String },
    /// Cannot be undone. Nothing returns this yet, and the day something does
    /// is the day this app needs a consent step in front of it.
    Irreversible,
}

/// The kind of thing a tool does.
///
/// Coarse on purpose. This is not an access-control list; it is the answer to
/// "what sort of power is this", which is the question a consent screen has to
/// put into a sentence. Three arms today because three kinds of thing exist.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Looks at the vault. Never asks.
    VaultRead,
    /// Changes one node. Never asks, because trash and version history put
    /// every one of these back.
    VaultWrite,
    /// Changes many files at once — a field renamed on every task, a whole
    /// kind removed. Already asks, in its own way: called without
    /// `confirm_nodes` these report the count and change nothing, and a wrong
    /// count is refused with the real one.
    VaultStructural,
}

impl Capability {
    /// What undoing a tool of this kind looks like.
    ///
    /// Uniform today, and that uniformity is the current safety model stated
    /// out loud rather than left to be noticed: everything Syn can reach is in
    /// the vault, and everything in the vault comes back.
    pub fn reversal(self) -> Reversal {
        match self {
            Capability::VaultRead => Reversal::Nothing,
            Capability::VaultWrite => Reversal::Automatic {
                how: "trash_node puts a new node away; restore_version undoes an edit".into(),
            },
            Capability::VaultStructural => Reversal::Automatic {
                how: "the nodes were trashed, not erased; list_trash and restore_node bring them back".into(),
            },
        }
    }
}

/// What a tool returned, and what it would take to undo.
pub struct ToolOutcome {
    /// The JSON string handed back to the model as a `tool` message.
    pub content: String,
    pub reversal: Reversal,
}

// ═══════════════════════════════════════════════════════════════
//  WHAT A TOOL IS RUN WITH
// ═══════════════════════════════════════════════════════════════

/// Everything a tool call happens inside.
///
/// `ToolContext` in `tools.rs` is what an individual tool gets — the database,
/// the vault, the app handle — and it stays exactly as it is. This is the layer
/// above: which run this is part of, so that a provider can attribute, meter or
/// refuse a call. Keeping them separate is what let this land without touching
/// twenty-three tool functions.
pub struct RunContext<'a, R: tauri::Runtime> {
    pub run_id: &'a str,
    pub db: &'a crate::db::DbState,
    pub vault_path: &'a str,
    pub app: &'a tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> RunContext<'_, R> {
    /// The context an individual tool expects.
    fn tools(&self) -> crate::syn::tools::ToolContext<'_, R> {
        crate::syn::tools::ToolContext {
            db: self.db,
            vault_path: self.vault_path,
            app: self.app,
        }
    }
}

/// A group of tools with something in common — where they live, what they
/// reach, what it takes to be allowed to call them.
pub trait ToolProvider<R: tauri::Runtime>: Send + Sync {
    /// Short, stable, and used in logs. Not shown to the model.
    fn name(&self) -> &'static str;

    /// The tools this provider offers for the run described by `ctx`.
    ///
    /// Takes the context so the list can differ per run. Nothing varies it yet.
    fn definitions(&self, ctx: &RunContext<R>) -> Vec<ToolDefinition>;

    /// What kind of power a tool of this name has, or `None` if this provider
    /// does not offer it.
    fn capability(&self, tool: &str) -> Option<Capability>;

    fn execute(&self, ctx: &RunContext<R>, tool: &str, args: &Value) -> AppResult<ToolOutcome>;
}

// ═══════════════════════════════════════════════════════════════
//  THE VAULT TOOLS
// ═══════════════════════════════════════════════════════════════

/// The twenty-three tools that reach the vault.
///
/// A wrapper, and no more than one. `get_tool_definitions` and `execute_tool`
/// are untouched; what is added is the table below, which is the part that did
/// not exist anywhere.
pub struct VaultTools;

impl VaultTools {
    /// What kind of thing each tool does.
    ///
    /// Every name in `get_tool_definitions()` must appear here, and a test
    /// asserts it — so a tool added without deciding what sort of power it has
    /// fails to build rather than defaulting to "whatever the others get".
    fn table(tool: &str) -> Option<Capability> {
        use Capability::*;
        Some(match tool {
            "query_nodes" | "get_node" | "list_schemas" | "get_linked_nodes" | "list_trash"
            | "list_versions" | "search_feed_articles" | "search_files" | "read_file_text"
            | "get_finance_summary" | "search_finance" | "get_transactions" => VaultRead,

            "create_node" | "update_node" | "trash_node" | "restore_node" | "restore_version"
            | "update_feed_article" | "create_transaction" => VaultWrite,

            "rename_field" | "delete_field" | "rename_kind" | "delete_kind" => VaultStructural,

            _ => return None,
        })
    }
}

impl<R: tauri::Runtime> ToolProvider<R> for VaultTools {
    fn name(&self) -> &'static str {
        "vault"
    }

    fn definitions(&self, _ctx: &RunContext<R>) -> Vec<ToolDefinition> {
        crate::syn::tools::get_tool_definitions()
    }

    fn capability(&self, tool: &str) -> Option<Capability> {
        Self::table(tool)
    }

    fn execute(&self, ctx: &RunContext<R>, tool: &str, args: &Value) -> AppResult<ToolOutcome> {
        let capability = Self::table(tool)
            .ok_or_else(|| crate::error::AppError::General(format!("Unknown tool: {tool}")))?;

        let content = crate::syn::tools::execute_tool(&ctx.tools(), tool, args)?;
        Ok(ToolOutcome {
            content,
            reversal: capability.reversal(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════
//  THE REGISTRY
// ═══════════════════════════════════════════════════════════════

/// Every provider a run can reach, in the order they are offered to the model.
pub struct Registry<R: tauri::Runtime> {
    providers: Vec<Box<dyn ToolProvider<R>>>,
}

impl<R: tauri::Runtime> Registry<R> {
    /// The providers a chat gets. One, today.
    pub fn for_chat() -> Self {
        Self {
            providers: vec![Box::new(VaultTools)],
        }
    }

    /// The tool definitions to send with a completion request.
    pub fn definitions(&self, ctx: &RunContext<R>) -> Vec<ToolDefinition> {
        self.providers
            .iter()
            .flat_map(|p| p.definitions(ctx))
            .collect()
    }

    /// Run a tool, whoever owns it.
    ///
    /// A name no provider claims is an error rather than a silent no-op: the
    /// model invented it, and telling it so is what makes it try something
    /// else.
    pub fn execute(&self, ctx: &RunContext<R>, tool: &str, args: &Value) -> AppResult<ToolOutcome> {
        for provider in &self.providers {
            if provider.capability(tool).is_some() {
                return provider.execute(ctx, tool, args);
            }
        }
        Err(crate::error::AppError::General(format!(
            "Unknown tool: {tool}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard this module exists to provide: a tool that nobody decided the
    /// power of does not get to ship.
    ///
    /// Reads the real definition list, so adding a tool to `tools.rs` and
    /// forgetting the table here is a failing test rather than a tool that
    /// quietly inherits whatever the dispatcher happened to allow.
    #[test]
    fn every_tool_that_is_offered_has_a_declared_capability() {
        let missing: Vec<String> = crate::syn::tools::get_tool_definitions()
            .into_iter()
            .map(|d| d.function.name)
            .filter(|name| VaultTools::table(name).is_none())
            .collect();

        assert!(
            missing.is_empty(),
            "these tools are offered to the model with no capability declared in \
             VaultTools::table: {missing:?}"
        );
    }

    /// And the other direction: a name in the table that nothing offers is a
    /// tool that was removed, and the row should have gone with it.
    #[test]
    fn the_capability_table_names_no_tool_that_no_longer_exists() {
        let offered: Vec<String> = crate::syn::tools::get_tool_definitions()
            .into_iter()
            .map(|d| d.function.name)
            .collect();

        // The table is a `match`, so it cannot be iterated; this is the list it
        // is written from, kept beside it and checked against reality.
        let declared = [
            "query_nodes", "get_node", "list_schemas", "get_linked_nodes", "list_trash",
            "list_versions", "search_feed_articles", "search_files", "read_file_text",
            "get_finance_summary", "search_finance", "get_transactions", "create_node",
            "update_node", "trash_node", "restore_node", "restore_version",
            "update_feed_article", "create_transaction", "rename_field", "delete_field",
            "rename_kind", "delete_kind",
        ];

        for name in declared {
            assert!(
                offered.iter().any(|o| o == name),
                "`{name}` has a capability but is not offered to the model any more"
            );
            assert!(
                VaultTools::table(name).is_some(),
                "`{name}` is in this list but not in the table"
            );
        }
        assert_eq!(declared.len(), offered.len(), "the two lists are different lengths");
    }

    /// Reading changes nothing, and saying so is what lets the transcript tell
    /// a user which steps they might want to undo.
    #[test]
    fn reads_report_nothing_to_undo_and_writes_report_how() {
        assert_eq!(Capability::VaultRead.reversal(), Reversal::Nothing);
        assert!(matches!(
            Capability::VaultWrite.reversal(),
            Reversal::Automatic { .. }
        ));
        assert!(matches!(
            Capability::VaultStructural.reversal(),
            Reversal::Automatic { .. }
        ));
    }

    #[test]
    fn a_name_nothing_claims_is_not_a_capability() {
        assert_eq!(VaultTools::table("send_email"), None);
        assert_eq!(VaultTools::table(""), None);
    }
}
