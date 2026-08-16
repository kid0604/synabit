use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The kinds of node the app knows what to do with.
///
/// A node's type comes from the `type:` line of a Markdown file's frontmatter,
/// or the `type` field of a JSON one — which is to say, from something the user
/// can edit. Nothing validated it, so `type: taks` produced a perfectly valid
/// node of a type no mini-app queries: it vanished from the Tasks list with no
/// error anywhere, and the file looked fine.
///
/// `Other` exists so that stays a warning rather than a correction. A vault may
/// legitimately carry types this app has never heard of — someone else's tool
/// wrote them, or the user invented one — and rewriting those to something
/// "known" would corrupt a file to satisfy a list. The type round-trips
/// untouched; only the log notices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeType {
    Note,
    Task,
    Project,
    Event,
    Person,
    QuickCap,
    Whiteboard,
    File,
    FinanceMonth,
    FinanceConfig,
    FinanceDebts,
    PdfHighlight,
    PdfDrawing,
    /// A JSON or canvas file that declared no type of its own.
    Json,
    Canvas,
    /// Anything else, preserved exactly as written.
    Other(String),
}

impl NodeType {
    /// Every type the app handles, for callers that need the whole list.
    pub const KNOWN: &'static [&'static str] = &[
        "note",
        "task",
        "project",
        "event",
        "person",
        "quickcap",
        "whiteboard",
        "file",
        "finance_month",
        "finance_config",
        "finance_debts",
        "pdf_highlight",
        "pdf_drawing",
        "json",
        "canvas",
    ];

    /// The string form — what is stored, queried and sent to the frontend.
    pub fn as_str(&self) -> &str {
        match self {
            NodeType::Note => "note",
            NodeType::Task => "task",
            NodeType::Project => "project",
            NodeType::Event => "event",
            NodeType::Person => "person",
            NodeType::QuickCap => "quickcap",
            NodeType::Whiteboard => "whiteboard",
            NodeType::File => "file",
            NodeType::FinanceMonth => "finance_month",
            NodeType::FinanceConfig => "finance_config",
            NodeType::FinanceDebts => "finance_debts",
            NodeType::PdfHighlight => "pdf_highlight",
            NodeType::PdfDrawing => "pdf_drawing",
            NodeType::Json => "json",
            NodeType::Canvas => "canvas",
            NodeType::Other(raw) => raw,
        }
    }

    /// Whether the app has a mini-app that will ever ask for this type.
    pub fn is_known(&self) -> bool {
        !matches!(self, NodeType::Other(_))
    }
}

impl From<&str> for NodeType {
    fn from(raw: &str) -> Self {
        match raw {
            "note" => NodeType::Note,
            "task" => NodeType::Task,
            "project" => NodeType::Project,
            "event" => NodeType::Event,
            "person" => NodeType::Person,
            "quickcap" => NodeType::QuickCap,
            "whiteboard" => NodeType::Whiteboard,
            "file" => NodeType::File,
            "finance_month" => NodeType::FinanceMonth,
            "finance_config" => NodeType::FinanceConfig,
            "finance_debts" => NodeType::FinanceDebts,
            "pdf_highlight" => NodeType::PdfHighlight,
            "pdf_drawing" => NodeType::PdfDrawing,
            "json" => NodeType::Json,
            "canvas" => NodeType::Canvas,
            other => NodeType::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A node as a list needs it: everything except the body.
///
/// A list screen shows a title, a date, a few properties and the opening of the
/// text. Sending the whole body as well is the single largest cost of opening
/// one — measured at 476ms of JSON serialisation for five thousand notes of
/// ordinary length, of which the bodies were 93%, none of it displayed.
///
/// `preview` is cut in SQL, so the body is never read out of the database, let
/// alone serialised.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeSummary {
    pub id: String,
    pub node_type: String,
    pub title: String,
    /// The opening of the body — not the body.
    pub preview: String,
    pub properties: Value,
    pub created_at: String,
    pub updated_at: String,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeMetadata {
    pub id: String,        // Relative path in Vault (e.g., "Projects/Synabit.md")
    pub node_type: String, // e.g., "note", "task", "project", "habit", "contact"
    pub title: String,     // Extracted from Frontmatter or filename
    pub content: String,   // Raw content (Markdown or JSON/Canvas string)
    pub properties: Value, // Parsed YAML Frontmatter or JSON metadata
    pub created_at: String,
    pub updated_at: String,
    pub timestamp: i64, // Used for cache invalidation
    #[serde(skip)]
    pub blocks: Option<Vec<(String, String)>>, // Block-level contents
}

impl NodeMetadata {
    /// The identity that stays with this node when its file moves.
    ///
    /// A node's `id` is where its file currently sits, which makes it a poor
    /// name for the node itself: move the file and every reference to it breaks,
    /// even though the note is plainly the same note. Archiving a task does
    /// exactly this — the file moves to `archived/`, and every backlink to it
    /// dangles.
    ///
    /// The identity that does survive already exists. Sync writes a `node_id`
    /// into each file's frontmatter and keeps it there across moves, renames
    /// and machines; the parser reads it into `properties` like any other field.
    ///
    /// Falls back to the path for a file that has not been given one yet — a
    /// file the CRDT bridge has not reached, or one written by hand. Those
    /// behave exactly as before rather than having no identity at all.
    pub fn stable_id(&self) -> &str {
        self.properties
            .get("node_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&self.id)
    }
}

#[cfg(test)]
mod node_type_tests {
    use super::NodeType;

    /// The property that keeps a user's vault safe from this list: whatever a
    /// file says its type is, that is what comes back out.
    #[test]
    fn an_unknown_type_survives_the_round_trip_unchanged() {
        for raw in ["taks", "recipe", "Note", "note ", "", "réunion"] {
            let parsed = NodeType::from(raw);
            assert_eq!(parsed.as_str(), raw, "'{raw}' was altered on the way through");
            assert!(!parsed.is_known(), "'{raw}' should not count as known");
        }
    }

    #[test]
    fn every_known_type_round_trips_and_knows_itself() {
        for raw in NodeType::KNOWN {
            let parsed = NodeType::from(*raw);
            assert_eq!(parsed.as_str(), *raw);
            assert!(parsed.is_known(), "'{raw}' is on the list but reads as unknown");
        }
    }

    /// Matching is exact. A near miss is the case this whole enum exists for,
    /// so it must not be quietly forgiven.
    #[test]
    fn a_near_miss_is_not_treated_as_the_type_it_resembles() {
        assert!(!NodeType::from("Task").is_known());
        assert!(!NodeType::from("tasks").is_known());
        assert!(!NodeType::from("finance-month").is_known());
        assert_eq!(NodeType::from("task"), NodeType::Task);
    }

    /// The frontend has its own list of the types it may write. If it can name
    /// a type this enum does not, the two have drifted and the warning becomes
    /// noise on a type that is in fact handled — so the drift is caught here
    /// rather than discovered in a log.
    #[test]
    fn the_types_the_frontend_can_write_are_all_types_the_backend_knows() {
        let source = std::fs::read_to_string("../src/composables/useNodeService.ts")
            .expect("the frontend node service should be readable from src-tauri");

        let union = source
            .split("export type NodeType =")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .expect("useNodeService.ts should still declare a NodeType union");

        let declared: Vec<&str> = union
            .split('|')
            .map(|part| part.trim().trim_matches(|c| c == '\'' || c == '"'))
            .filter(|part| !part.is_empty())
            .collect();

        assert!(
            declared.len() >= 10,
            "parsed too few types from the frontend union — the parsing broke, not the code: {declared:?}"
        );

        for name in declared {
            assert!(
                NodeType::from(name).is_known(),
                "the frontend can write '{name}', which NodeType does not list"
            );
        }
    }
}
