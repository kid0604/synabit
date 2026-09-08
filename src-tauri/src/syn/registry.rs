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
/// One definition, in `consent.rs`, re-exported here because the registry is
/// where tools declare theirs. Two enums for one idea is where drift starts —
/// and the drift that matters would be a tool declaring a power the consent
/// ledger has never heard of.
pub use crate::syn::consent::Capability;

/// What undoing a capability looks like.
///
/// Uniform across the vault arms, and that uniformity is the current safety
/// model stated out loud rather than left to be noticed: everything Syn can
/// reach today is in the vault, and everything in the vault comes back.
///
/// The arms that reach outside are where it stops holding, which is the whole
/// reason consent exists in front of them. A message sent is not un-sent by
/// this app, and saying `Manual` about it is more honest than saying nothing.
pub fn reversal_of(capability: &Capability) -> Reversal {
    match capability {
        Capability::VaultRead => Reversal::Nothing,
        Capability::VaultWrite => Reversal::Automatic {
            how: "trash_node puts a new node away; restore_version undoes an edit".into(),
        },
        Capability::VaultStructural => Reversal::Automatic {
            how: "the nodes were trashed, not erased; list_trash and restore_node bring them back"
                .into(),
        },
        Capability::Browse | Capability::NetRead { .. } => Reversal::Nothing,
        Capability::NetWrite { domain, .. } => Reversal::Manual {
            how: format!("whatever was sent is at {domain} now; undoing it happens there"),
        },
        Capability::Spend { .. } => Reversal::Manual {
            how: "a refund is asked for wherever the money went".into(),
        },
        Capability::Execute => Reversal::Irreversible,
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
            run_id: Some(self.run_id),
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

    /// What kind of power this call has, or `None` if this provider does not
    /// offer the tool.
    ///
    /// Takes the arguments as well as the name, because for anything reaching
    /// outside the machine the *scope* is in them. `NetRead` promises to ask
    /// once per host and remember; a capability computed from the name alone
    /// could only say "the internet", and one yes would have granted every
    /// site there is — which would make that promise false while every doc
    /// comment still claimed it.
    fn capability(&self, tool: &str, args: &Value) -> Option<Capability>;

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
    /// `_args` because nothing needs them today: `browse` used to read the host
    /// out of them and no longer does, since one capability covers the tool.
    /// The parameter stays because the question *"what power is this call
    /// about to use"* is one a future tool may well answer differently for
    /// different arguments — a `run_recipe` that could declare its steps, say.
    fn table(tool: &str, _args: &Value) -> Option<Capability> {
        use Capability::*;
        Some(match tool {
            "query_nodes" | "get_node" | "list_schemas" | "get_linked_nodes" | "list_trash"
            | "list_versions" | "search_feed_articles" | "search_files" | "read_file_text"
            | "get_finance_summary" | "search_finance" | "get_transactions" | "recall"
            | "load_skill" | crate::syn::tools::LOOK_BACK_TOOL => {
                VaultRead
            }

            // `run_recipe` is the union of whatever its steps do, which cannot
            // be declared statically — so the format refuses the structural
            // tools instead (`recipe::NOT_IN_A_RECIPE`), and what is left tops
            // out here. Under-declaring would be the dangerous direction; this
            // errs the other way and stays true.
            "create_node" | "update_node" | "trash_node" | "restore_node" | "restore_version"
            | "update_feed_article" | "create_transaction" | "remember" | "run_recipe" => {
                VaultWrite
            }

            "rename_field" | "delete_field" | "rename_kind" | "delete_kind" => VaultStructural,

            // The one thing here that leaves the machine.
            //
            // One capability, whatever the argument is. It used to be scoped to
            // the host when the call named one, which asked a question nobody
            // could answer: given a question rather than an address, `browse`
            // searches and then opens what the results point at, so the host is
            // unknown when the card is drawn and already read by the time it is
            // known. See `Capability::Browse`.
            name if name == crate::syn::tools::BROWSE_TOOL => Browse,

            _ => return None,
        })
    }
}

impl<R: tauri::Runtime> ToolProvider<R> for VaultTools {
    fn name(&self) -> &'static str {
        "vault"
    }

    /// What this vault's settings say Syn can actually do.
    ///
    /// A settings read per run, to leave `web_search` out when no endpoint is
    /// configured. Describing a tool that cannot work costs tokens on every
    /// turn and ends in a model reaching for it and failing, which is worse
    /// than never having offered it.
    fn definitions(&self, ctx: &RunContext<R>) -> Vec<ToolDefinition> {
        let settings = crate::syn::settings::load_settings(ctx.vault_path).unwrap_or_default();
        crate::syn::tools::get_tool_definitions_for(&settings)
    }

    fn capability(&self, tool: &str, args: &Value) -> Option<Capability> {
        Self::table(tool, args)
    }

    fn execute(&self, ctx: &RunContext<R>, tool: &str, args: &Value) -> AppResult<ToolOutcome> {
        let capability = Self::table(tool, args)
            .ok_or_else(|| crate::error::AppError::General(format!("Unknown tool: {tool}")))?;

        let content = crate::syn::tools::execute_tool(&ctx.tools(), tool, args)?;
        Ok(ToolOutcome {
            content,
            reversal: reversal_of(&capability),
        })
    }
}

// ═══════════════════════════════════════════════════════════════
//  THE REGISTRY
// ═══════════════════════════════════════════════════════════════

/// Every provider a run can reach, in the order they are offered to the model.
/// A tool that reaches outside and does nothing.
///
/// The roadmap's P4 gate asks for exactly this: a run driven against a
/// simulated capability, so the consent path can be proved without an account
/// anywhere, a network, or anything that could actually be sent. It is behind
/// `cfg(test)` and absent from `Registry::for_chat`, because a tool in the
/// prompt costs tokens on every turn of every conversation and this one has
/// nothing to offer a real user.
#[cfg(test)]
pub struct SendTest;

#[cfg(test)]
impl<R: tauri::Runtime> ToolProvider<R> for SendTest {
    fn name(&self) -> &'static str {
        "test"
    }

    fn definitions(&self, _ctx: &RunContext<R>) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            tool_type: "function".to_string(),
            function: crate::models::syn::FunctionDefinition {
                name: "send_test".to_string(),
                description: "Send a message nowhere. Exists to exercise consent.".to_string(),
                parameters: serde_json::json!({ "type": "object", "properties": {} }),
            },
        }]
    }

    fn capability(&self, tool: &str, _args: &Value) -> Option<Capability> {
        (tool == "send_test").then(|| Capability::NetWrite {
            domain: "example.test".to_string(),
            tool: "send_test".to_string(),
        })
    }

    fn execute(&self, _ctx: &RunContext<R>, _tool: &str, _args: &Value) -> AppResult<ToolOutcome> {
        Ok(ToolOutcome {
            content: serde_json::json!({ "sent": true }).to_string(),
            reversal: Reversal::Manual {
                how: "nothing was really sent; this tool exists to be asked about".into(),
            },
        })
    }
}

/// One tool, as somebody deciding whether to trust this thing would read it.
///
/// # Why this is a screen and not just a list in the source
///
/// The run inspector could answer *what did Syn do* (the transcript), *what was
/// it told* (the prompt), *what has it been allowed* (the ledger) — and not
/// **what can it reach at all**. The catalogue existed the whole time and was
/// read in exactly one place in the codebase, by `syn_recipe_problems`, which
/// took the names to validate a recipe and threw the rest away.
///
/// That is the question people ask *before* they decide to trust something,
/// not after. `table` already knows what power each tool needs and
/// `reversal_of` already knows what puts it back; neither had ever reached a
/// person.
#[derive(serde::Serialize, Debug, Clone)]
pub struct ToolCard {
    pub name: String,
    /// The description the model is given, verbatim.
    ///
    /// Not a friendlier paraphrase written for this screen. This panel's whole
    /// job is to show what Syn is actually told, and a second, kinder wording
    /// would be a second thing to keep in step — and the one people read would
    /// be the one that was not sent.
    pub description: String,
    /// `None` would mean a tool the registry cannot classify.
    ///
    /// `every_tool_that_is_offered_has_a_declared_capability` makes that
    /// impossible today, and it
    /// stays an `Option` so that if it ever became possible the screen would
    /// say so in amber rather than quietly pick a default.
    pub capability: Option<Capability>,
    /// What puts it back, derived from the capability rather than declared
    /// twice.
    pub reversal: Option<Reversal>,
}

/// Every tool a chat can reach, with what it needs and what undoes it.
///
/// Built from `get_tool_definitions()` paired with the registry's own
/// classification, rather than from `ToolProvider::definitions` — that takes a
/// `RunContext`, which needs a live database and an app handle, and this is
/// asked for by a panel with neither.
///
/// The shortcut holds only while `for_chat` has one provider whose definitions
/// are exactly that list. `the_catalogue_covers_what_a_chat_can_reach` is the
/// guard: add a second provider and it fails here rather than silently showing
/// a screen that is missing half of what Syn can do.
pub fn catalogue() -> Vec<ToolCard> {
    let registry = Registry::<tauri::Wry>::for_chat();
    crate::syn::tools::get_tool_definitions()
        .into_iter()
        .map(|definition| {
            // No arguments: this is the catalogue describing what a tool
            // *is*, not a call about to be made. A scoped capability answers
            // with an empty scope and the screen says so.
            let capability = registry.capability_of(&definition.function.name, &Value::Null);
            ToolCard {
                name: definition.function.name,
                description: definition.function.description,
                reversal: capability.as_ref().map(reversal_of),
                capability,
            }
        })
        .collect()
}

pub struct Registry<R: tauri::Runtime> {
    providers: Vec<Box<dyn ToolProvider<R>>>,
}

impl<R: tauri::Runtime> Registry<R> {
    /// No providers at all — a turn that answers from what it was given.
    ///
    /// The instant tempo: the question was recognised as a count, the count was
    /// run before the model was asked, and there is nothing left to look up. A
    /// turn with tools would spend a round deciding not to use them.
    pub fn none() -> Self {
        Self { providers: Vec::new() }
    }

    /// The providers a chat gets. One, today.
    pub fn for_chat() -> Self {
        Self {
            providers: vec![Box::new(VaultTools)],
        }
    }

    /// The providers a run gets when the point is to exercise consent.
    #[cfg(test)]
    pub fn for_consent_test() -> Self {
        Self {
            providers: vec![Box::new(VaultTools), Box::new(SendTest)],
        }
    }

    /// The tool definitions to send with a completion request.
    pub fn definitions(&self, ctx: &RunContext<R>) -> Vec<ToolDefinition> {
        self.providers
            .iter()
            .flat_map(|p| p.definitions(ctx))
            .collect()
    }

    /// What sort of power a tool has, whoever owns it.
    ///
    /// `None` for a name nothing claims. The engine treats that as "no consent
    /// question to ask" and lets `execute` produce the real error, so an
    /// invented tool name fails as an unknown tool rather than as a permission
    /// problem — two different things to be told.
    pub fn capability_of(&self, tool: &str, args: &Value) -> Option<Capability> {
        self.providers.iter().find_map(|p| p.capability(tool, args))
    }

    /// Run a tool, whoever owns it.
    ///
    /// A name no provider claims is an error rather than a silent no-op: the
    /// model invented it, and telling it so is what makes it try something
    /// else.
    pub fn execute(&self, ctx: &RunContext<R>, tool: &str, args: &Value) -> AppResult<ToolOutcome> {
        for provider in &self.providers {
            if provider.capability(tool, args).is_some() {
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
            .filter(|name| VaultTools::table(name, &Value::Null).is_none())
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
            "rename_kind", "delete_kind", "remember", "recall", "load_skill", "run_recipe",
            crate::syn::tools::LOOK_BACK_TOOL,
            crate::syn::tools::BROWSE_TOOL,
        ];

        for name in declared {
            assert!(
                offered.iter().any(|o| o == name),
                "`{name}` has a capability but is not offered to the model any more"
            );
            assert!(
                VaultTools::table(name, &Value::Null).is_some(),
                "`{name}` is in this list but not in the table"
            );
        }
        assert_eq!(declared.len(), offered.len(), "the two lists are different lengths");
    }

    /// Reading changes nothing, and saying so is what lets the transcript tell
    /// a user which steps they might want to undo.
    #[test]
    fn reads_report_nothing_to_undo_and_writes_report_how() {
        assert_eq!(reversal_of(&Capability::VaultRead), Reversal::Nothing);
        assert!(matches!(
            reversal_of(&Capability::VaultWrite),
            Reversal::Automatic { .. }
        ));
        assert!(matches!(
            reversal_of(&Capability::VaultStructural),
            Reversal::Automatic { .. }
        ));
    }

    /// Nothing that leaves this machine claims to be undoable from inside it.
    ///
    /// The vault arms are `Automatic` because trash and version history really
    /// do put things back. A message that has been sent is at somebody else's
    /// server, and the app saying it can undo that would be the transcript
    /// telling the user something false at the moment they most need it true.
    #[test]
    fn nothing_that_leaves_the_machine_claims_to_be_undoable_here() {
        assert!(matches!(
            reversal_of(&Capability::NetWrite {
                domain: "example.com".into(),
                tool: "post".into()
            }),
            Reversal::Manual { .. }
        ));
        assert!(matches!(
            reversal_of(&Capability::Spend { cents_estimate: 100 }),
            Reversal::Manual { .. }
        ));
        assert_eq!(reversal_of(&Capability::Execute), Reversal::Irreversible);
        assert_eq!(
            reversal_of(&Capability::NetRead { domain: "example.com".into() }),
            Reversal::Nothing,
            "reading changes nothing, wherever it reads from"
        );
    }

    /// The catalogue is the whole of what a chat can reach, not a sample.
    ///
    /// `catalogue` pairs the static definition list with the registry's
    /// classification rather than asking the providers, because
    /// `ToolProvider::definitions` needs a live `RunContext` and the panel
    /// asking has none. That shortcut is exact while `for_chat` holds one
    /// provider offering exactly that list — and this is where it stops being
    /// exact, loudly, rather than in a screen quietly missing half of what Syn
    /// can do.
    #[test]
    fn the_catalogue_covers_what_a_chat_can_reach() {
        let cards = catalogue();
        let offered: Vec<String> = crate::syn::tools::get_tool_definitions()
            .into_iter()
            .map(|t| t.function.name)
            .collect();

        assert_eq!(cards.len(), offered.len(), "the catalogue lost or invented a tool");
        for name in &offered {
            assert!(cards.iter().any(|c| &c.name == name), "`{name}` is missing");
        }

        // The registry must claim every one of them. A card with no capability
        // renders as unclassified, which is the honest failure — but it should
        // never happen, and this says so here as well as at the table.
        let unclassified: Vec<&str> = cards
            .iter()
            .filter(|c| c.capability.is_none())
            .map(|c| c.name.as_str())
            .collect();
        assert!(unclassified.is_empty(), "no declared power: {unclassified:?}");
    }

    /// The screen can name every power the catalogue can carry.
    ///
    /// `capabilityLabel` maps a capability to an i18n key by lowercasing the
    /// variant name. A capability with no matching key renders as the raw key
    /// string — no crash, no red anything, just `syn.cap_execute` sitting in
    /// the list — which is exactly the kind of failure nobody reports.
    #[test]
    fn every_power_a_tool_can_need_has_words_in_both_languages() {
        let variants = [
            Capability::VaultRead,
            Capability::VaultWrite,
            Capability::VaultStructural,
            Capability::NetRead { domain: "x".into() },
            Capability::NetWrite { domain: "x".into(), tool: "y".into() },
            Capability::Spend { cents_estimate: 1 },
            Capability::Execute,
        ];

        for locale in ["en", "vi"] {
            let raw = std::fs::read_to_string(format!("../src/i18n/locales/{locale}.json"))
                .expect("locale file");
            let json: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
            let syn = json.get("syn").expect("a syn namespace");

            for capability in &variants {
                // The name the frontend lowercases, which is the outer key for
                // the struct-like arms and the string itself for the unit ones.
                let value = serde_json::to_value(capability).expect("serialises");
                let name = value
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| value.as_object().and_then(|o| o.keys().next().cloned()))
                    .expect("a variant name");
                let key = format!("cap_{}", name.to_lowercase());
                assert!(
                    syn.get(&key).is_some(),
                    "{locale}.json has no `syn.{key}` for {capability:?}"
                );
            }
        }
    }

    /// A read says there is nothing to undo; a write says what puts it back.
    /// That pairing is the reason the screen shows both columns, and deriving
    /// the second from the first is what stops them ever disagreeing.
    #[test]
    fn the_catalogue_says_what_undoes_each_tool() {
        let cards = catalogue();

        let read = cards.iter().find(|c| c.name == "query_nodes").expect("query_nodes");
        assert_eq!(read.reversal, Some(Reversal::Nothing));

        let write = cards.iter().find(|c| c.name == "create_node").expect("create_node");
        assert!(
            matches!(write.reversal, Some(Reversal::Automatic { .. })),
            "{:?}",
            write.reversal
        );

        let structural = cards.iter().find(|c| c.name == "delete_kind").expect("delete_kind");
        assert!(matches!(structural.reversal, Some(Reversal::Automatic { .. })));
    }

    /// The description is the model's, verbatim. A friendlier paraphrase
    /// written for the screen would be a second wording to keep in step, and
    /// the one people read would be the one that was never sent.
    #[test]
    fn the_description_is_the_one_the_model_is_given() {
        let cards = catalogue();
        for definition in crate::syn::tools::get_tool_definitions() {
            let card = cards
                .iter()
                .find(|c| c.name == definition.function.name)
                .expect("every tool has a card");
            assert_eq!(card.description, definition.function.description);
        }
    }

    /// A tool nothing in the table claims has no capability, and that is how the
    /// engine knows to leave it alone rather than inventing a permission for it.
    ///
    /// This had never run. Its `#[test]` had drifted onto the function above —
    /// which therefore carried two, and this one none — so it sat here as dead
    /// code through every rewrite of `table`, including the one that replaced
    /// the browsing arm. `cargo` did say so, in a warning, in a file with
    /// twenty-five others.
    #[test]
    fn a_name_nothing_claims_is_not_a_capability() {
        assert_eq!(VaultTools::table("send_email", &Value::Null), None);
        assert_eq!(VaultTools::table("", &Value::Null), None);
    }
}
