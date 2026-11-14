//! Whiteboards, as something Syn can read, draw and change.
//!
//! # Why a board needs its own module at all
//!
//! Because it is the one thing in this vault that is *spatial*. A note is
//! words and a task is fields, and the tools for those hand the model the file
//! and take the file back. A board is a hundred items each with a position,
//! and the two halves of that need opposite treatment:
//!
//! * **The model must never write a coordinate.** Asked where a box goes, a
//!   language model guesses, and a guessed coordinate is a box on top of
//!   another box. So `draw` takes what the model is good at — what exists and
//!   what connects to what — and works out the geometry here.
//! * **The model must never re-do a layout somebody has arranged.** The whole
//!   point of a board is that a person can drag it into the shape they meant;
//!   a tool that redraws the board to add one item throws that away. So `edit`
//!   moves nothing that is already placed.
//!
//! The file format is the Whiteboard app's, unchanged and not versioned here:
//! `src/mini-apps/whiteboard/boardFile.ts` is where it is decided, and a board
//! written by these tools has to open in that app like any other.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Where boards live, which is `folder_for_type("whiteboard")`.
pub const BOARDS_DIR: &str = "Whiteboards";

/// The format version the Whiteboard app writes and refuses to read past.
pub const SCHEMA_VERSION: u32 = 1;

/// The names of the three tools, which are offered together or not at all.
pub const TOOLS: [&str; 3] = ["read_board", "draw_board", "edit_board"];

/// Whether this vault has a board in it.
///
/// # Why the tools are not always offered
///
/// Three declarations cost about two thousand characters of every request, and
/// a vault with no whiteboard in it will never use one of them — the same
/// argument that kept `web_search` out of the payload when no endpoint was
/// configured. A description paid for on every turn, for something that cannot
/// apply, is a promise charged in advance.
///
/// A directory listing, deliberately, rather than a query: this runs while the
/// tool list is built, the answer is one bit, and the folder holds tens of
/// files at most.
pub fn any_board(vault_path: &str) -> bool {
    let dir = std::path::Path::new(vault_path).join(BOARDS_DIR);
    std::fs::read_dir(dir).map(|mut entries| {
        entries.any(|e| {
            e.map(|e| e.file_name().to_string_lossy().ends_with(".whiteboard.json"))
                .unwrap_or(false)
        })
    })
    .unwrap_or(false)
}

// ═══════════════════════════════════════════════════════════════
//  The file
// ═══════════════════════════════════════════════════════════════

/// A board file. Unknown keys are kept: this is somebody else's format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    #[serde(rename = "schemaVersion", skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
    #[serde(default = "origin")]
    pub viewport: Value,
    #[serde(default)]
    pub nodes: Vec<Item>,
    #[serde(default)]
    pub edges: Vec<Link>,
    /// Everything else the file carries — `type`, and whatever a newer build
    /// of the app writes. A tool that dropped these would quietly rewrite
    /// somebody's board into a smaller one.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

fn origin() -> Value {
    json!({ "x": 0, "y": 0, "zoom": 1 })
}

/// One thing on the board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub position: Point,
    #[serde(default)]
    pub data: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<i64>,
    /// Fields of the item this module does not use, kept as they were — the
    /// same reason as `Board::rest`, one level down.
    #[serde(flatten, default)]
    pub rest: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A line between two things.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    pub source: String,
    #[serde(rename = "sourceHandle", skip_serializing_if = "Option::is_none")]
    pub source_handle: Option<String>,
    pub target: String,
    #[serde(rename = "targetHandle", skip_serializing_if = "Option::is_none")]
    pub target_handle: Option<String>,
    #[serde(rename = "type", default = "default_link")]
    pub kind: String,
    #[serde(default)]
    pub data: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<i64>,
    /// As `Item::rest`.
    #[serde(flatten, default)]
    pub rest: Map<String, Value>,
}

fn default_link() -> String {
    "default".to_string()
}

impl Item {
    pub fn label(&self) -> &str {
        // A card shows a vault thing, and its name is that thing's title.
        self.data
            .get("label")
            .or_else(|| self.data.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("")
    }
    pub fn width(&self) -> f64 {
        self.data.get("width").and_then(Value::as_f64).unwrap_or(160.0)
    }
    pub fn height(&self) -> f64 {
        self.data.get("height").and_then(Value::as_f64).unwrap_or(80.0)
    }
    pub fn shape(&self) -> &str {
        self.data.get("shapeType").and_then(Value::as_str).unwrap_or("rectangle")
    }
    /// Whether this is a frame drawn behind other things — a group.
    ///
    /// Boards have no notion of a group; a subgraph is a big rectangle with
    /// the others sitting on it. What makes it recognisable is that it holds
    /// them: a frame is a box with at least two other boxes inside it.
    fn holds(&self, others: &[Item]) -> usize {
        others
            .iter()
            .filter(|o| {
                o.id != self.id
                    && o.position.x >= self.position.x
                    && o.position.y >= self.position.y
                    && o.position.x + o.width() <= self.position.x + self.width()
                    && o.position.y + o.height() <= self.position.y + self.height()
            })
            .count()
    }
}

// ═══════════════════════════════════════════════════════════════
//  Reading
// ═══════════════════════════════════════════════════════════════

/// How many items a summary will name before it starts counting instead.
const NAMED: usize = 80;

/// A board in words.
///
/// # Why not just hand over the file
///
/// Because `get_node` does, and on a real board that is four thousand
/// characters of the coordinates of one freehand stroke. The model reads it,
/// learns nothing, and has no way to tell that it learned nothing.
///
/// What a person sees when they look at a board is: what is on it, what is
/// joined to what, and which things sit inside which. That is what this says,
/// in that order. Freehand strokes are counted and not described — a thousand
/// points of somebody's pen is not a thing to put in a sentence.
pub fn describe(board: &Board) -> String {
    // A sequence's bars and the dashed pieces of its lifelines are how its
    // messages are held up, not things on the board: the messages are read as
    // between the participants, and the scaffolding is left out.
    let boxes: Vec<&Item> = board
        .nodes
        .iter()
        .filter(|n| n.kind == "shape" && !n.data.contains_key("lifelineOf"))
        .collect();
    let lines: Vec<&Link> = board
        .edges
        .iter()
        .filter(|e| e.data.get("lifeline").and_then(Value::as_bool) != Some(true))
        .collect();
    let strokes = board.nodes.iter().filter(|n| n.kind == "stroke").count();
    let notes: Vec<&Item> = board
        .nodes
        .iter()
        .filter(|n| matches!(n.kind.as_str(), "text" | "note" | "mindmap" | "sticky" | "card"))
        .collect();

    // Frames are of two kinds: the app's own `frame` items, and — from boards
    // drawn before Syn and diagrams made real ones — a box with two or more
    // others sitting on it.
    let all: Vec<Item> = boxes.iter().map(|b| (*b).clone()).collect();
    let mut frames: Vec<(&Item, usize)> =
        boxes.iter().map(|b| (*b, b.holds(&all))).filter(|(_, n)| *n >= 2).collect();
    let held_things: Vec<Item> = board
        .nodes
        .iter()
        .filter(|n| n.kind != "frame" && n.kind != "stroke")
        .cloned()
        .collect();
    frames.extend(
        board
            .nodes
            .iter()
            .filter(|n| n.kind == "frame")
            .map(|f| (f, f.holds(&held_things))),
    );
    frames.sort_by_key(|(_, n)| std::cmp::Reverse(*n));

    // A frame is a box, and counting it as one would say a drawing of eight
    // things has ten. They are listed in their own right below.
    let framed: std::collections::HashSet<&str> =
        frames.iter().map(|(f, _)| f.id.as_str()).collect();
    let boxes: Vec<&Item> = boxes.into_iter().filter(|b| !framed.contains(b.id.as_str())).collect();

    let mut out = String::new();
    out.push_str(&format!("Board \"{}\"\n", board.title));
    out.push_str(&format!(
        "{} boxes, {} lines between them",
        boxes.len(),
        lines.len()
    ));
    if !notes.is_empty() {
        out.push_str(&format!(", {} loose pieces of writing", notes.len()));
    }
    if strokes > 0 {
        out.push_str(&format!(", {strokes} freehand strokes (not described here)"));
    }
    out.push_str(".\n");

    if !frames.is_empty() {
        out.push_str("\nFrames, each with what sits inside it:\n");
        for (frame, held) in frames.iter().take(20) {
            let inside: Vec<&str> = boxes
                .iter()
                .chain(notes.iter())
                .filter(|b| {
                    b.id != frame.id
                        && b.position.x >= frame.position.x
                        && b.position.y >= frame.position.y
                        && b.position.x + b.width() <= frame.position.x + frame.width()
                        && b.position.y + b.height() <= frame.position.y + frame.height()
                })
                .map(|b| b.label())
                .filter(|l| !l.is_empty())
                .take(24)
                .collect();
            out.push_str(&format!(
                "- {} ({held} inside): {}\n",
                naming(frame.label()),
                inside.join(", ")
            ));
        }
    }

    out.push_str("\nBoxes:\n");
    for item in boxes.iter().take(NAMED) {
        // An icon is a picture of something: said as what it is a picture of.
        let icon = item.data.get("glyph").and_then(|g| g.get("name")).and_then(Value::as_str);
        let kind = match (item.shape(), icon) {
            ("glyph", Some(name)) => format!("icon: {name}"),
            (shape, _) => shape.to_string(),
        };
        out.push_str(&format!(
            "- {} [{}] at {},{}\n",
            naming(item.label()),
            kind,
            item.position.x.round(),
            item.position.y.round()
        ));
    }
    if boxes.len() > NAMED {
        out.push_str(&format!("…and {} more.\n", boxes.len() - NAMED));
    }

    if !lines.is_empty() {
        // Every item, not just the boxes: a line can end on a frame, a
        // sticky note or a piece of writing.
        let by_id: std::collections::HashMap<&str, &Item> =
            board.nodes.iter().map(|b| (b.id.as_str(), b)).collect();
        out.push_str("\nLines:\n");
        for link in lines.iter().take(NAMED) {
            let name = |id: &str| {
                let item = by_id.get(id).map(|i| {
                    // A bar on a lifeline answers to its participant.
                    i.data
                        .get("lifelineOf")
                        .and_then(Value::as_str)
                        .and_then(|head| by_id.get(head))
                        .unwrap_or(i)
                });
                item.map(|i| naming(i.label()).to_string())
                    .unwrap_or_else(|| "(something no longer here)".to_string())
            };
            let label = link.data.get("label").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!("- {} → {}", name(&link.source), name(&link.target)));
            if !label.is_empty() {
                out.push_str(&format!(" ({label})"));
            }
            // What the ends are, when they are not the plain arrow: a
            // crow's foot or a hollow triangle is half of what the line says.
            let end = |key: &str| link.data.get(key).and_then(Value::as_str).filter(|k| *k != "none");
            let (start, finish) = (end("markerStart"), end("markerEnd"));
            if start.is_some() || finish != Some("arrow") {
                out.push_str(&format!(" [{} – {}]", start.unwrap_or("none"), finish.unwrap_or("none")));
            }
            if link.data.get("dashStyle").and_then(Value::as_str) == Some("dashed") {
                out.push_str(" dashed");
            }
            out.push('\n');
        }
        if lines.len() > NAMED {
            out.push_str(&format!("…and {} more.\n", lines.len() - NAMED));
        }
    }

    // Comments: what the people working on the board said about it, and
    // about which thing — often the most useful thing on a board to read.
    let comments: Vec<&Item> = board.nodes.iter().filter(|n| n.kind == "comment").collect();
    if !comments.is_empty() {
        out.push_str("\nComments:\n");
        for c in comments.iter().take(NAMED) {
            let about = c
                .data
                .get("on")
                .and_then(Value::as_str)
                .and_then(|id| board.nodes.iter().find(|n| n.id == id))
                .map(|n| format!(" (on \"{}\")", n.label()))
                .unwrap_or_default();
            let done = if c.data.get("resolved").and_then(Value::as_bool).unwrap_or(false) { " [resolved]" } else { "" };
            out.push_str(&format!("- {}{about}{done}\n", c.label()));
        }
        if comments.len() > NAMED {
            out.push_str(&format!("…and {} more.\n", comments.len() - NAMED));
        }
    }

    out
}

/// What to call a box with nothing written on it.
fn naming(label: &str) -> &str {
    if label.trim().is_empty() {
        "(unnamed)"
    } else {
        label
    }
}

// ═══════════════════════════════════════════════════════════════
//  Drawing a new one
// ═══════════════════════════════════════════════════════════════

/// What the model says it wants drawn: things, and what joins them.
///
/// # One shape of request, read three ways
///
/// The same few words cover every drawing this lays out, and what is in the
/// request decides how it is read — never a `type` the model has to pick:
///
/// * **Layers**, the default: what feeds what, top to bottom, with `group`
///   making the frames that stand side by side as columns.
/// * **Lanes**, when items have a `lane`: one band across the board per lane,
///   and the flow left to right — a process that passes between teams.
/// * **A sequence**, when links have an `order`: the items are participants
///   side by side, and the links are messages between them, read top to
///   bottom in that order.
///
/// What composes with all of them: `fields` turn an item into a table or a
/// class, `start`/`end` say what each end of a line means, `dashed` marks a
/// reply or a dependency, and `row`/`col` say where something goes when the
/// model knows. None of it is a coordinate: a row is not a place a box can be
/// put on top of another one.
///
/// Why not a schema per kind of diagram, as other tools have: the kinds that
/// were not thought of. A swimlane with a table in it, or a sequence with the
/// participants in frames, is two of these put together, and needs nobody to
/// have written a third schema for it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Sketch {
    pub items: Vec<SketchItem>,
    #[serde(default)]
    pub links: Vec<SketchLink>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SketchItem {
    pub label: String,
    #[serde(default)]
    pub shape: Option<String>,
    /// Which group this belongs in, by name. Groups become frames.
    #[serde(default)]
    pub group: Option<String>,
    /// Which band across the board this belongs in. Lanes are rows where
    /// groups are columns, and a drawing has one or the other.
    #[serde(default)]
    pub lane: Option<String>,
    /// A table's columns or a class's members, one line each. An item with
    /// fields is drawn as a frame with a row per field, and a line can end on
    /// one of them: `"orders.user_id"`.
    #[serde(default)]
    pub fields: Vec<String>,
    /// Which row it sits in, counted from 0 — within its group, its lane, or
    /// the board.
    #[serde(default)]
    pub row: Option<usize>,
    /// Its place along that row, counted from 0. In lanes, which column; in a
    /// sequence, which participant from the left.
    #[serde(default)]
    pub col: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SketchLink {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
    /// When this happens, for a sequence: 1, 2, 3 … Links with an order make
    /// the drawing a sequence diagram.
    #[serde(default)]
    pub order: Option<f64>,
    /// What the line's ends are: see `END_NAMES`. The far end is an arrow
    /// unless it says otherwise, the near end nothing.
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub dashed: bool,
}

/// The words a line's end can be given, and the board's own name for each.
///
/// The board's names (`er-one-many`) and plain ones (`one-many`) both, since
/// a model reaches for the plain one and a person reading the tool's
/// description for the other.
const END_NAMES: &[(&str, &str)] = &[
    ("none", "none"),
    ("arrow", "arrow"),
    ("open", "arrow-open"),
    ("arrow-open", "arrow-open"),
    ("triangle", "triangle-open"),
    ("triangle-open", "triangle-open"),
    ("diamond", "diamond"),
    ("diamond-open", "diamond-open"),
    ("circle", "circle"),
    ("circle-open", "circle-open"),
    ("one", "er-one"),
    ("many", "er-many"),
    ("one-many", "er-one-many"),
    ("zero-one", "er-zero-one"),
    ("zero-many", "er-zero-many"),
    ("er-one", "er-one"),
    ("er-many", "er-many"),
    ("er-one-many", "er-one-many"),
    ("er-zero-one", "er-zero-one"),
    ("er-zero-many", "er-zero-many"),
];

/// The board's name for an end, or `None` for a word that is not one.
pub fn end_kind(word: &str) -> Option<&'static str> {
    let word = word.trim().to_lowercase().replace(['_', ' '], "-");
    END_NAMES.iter().find(|(name, _)| *name == word).map(|(_, kind)| *kind)
}

/// The plain names, for a refusal that says what would have worked.
fn end_words() -> String {
    END_NAMES
        .iter()
        .map(|(name, _)| *name)
        .filter(|n| !n.starts_with("er-") && *n != "arrow-open" && *n != "triangle-open")
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a field is called, for a line that ends on it: `user_id` of
/// `user_id: uuid FK`, `total` of `+ total(): Money`.
pub fn field_name(field: &str) -> String {
    field
        .trim()
        .trim_start_matches(['+', '-', '#', '~'])
        .trim_start()
        .split(|c: char| c == ':' || c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .to_lowercase()
}

/// How many fields one item may have: a table of forty columns is a list,
/// and the board is the wrong place for it.
const MAX_FIELDS: usize = 40;

/// How a sketch is laid out — decided by what is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrangement {
    Layers,
    Lanes,
    Sequence,
}

fn arrangement(sketch: &Sketch) -> Arrangement {
    if sketch.links.iter().any(|l| l.order.is_some()) {
        Arrangement::Sequence
    } else if sketch.items.iter().any(|i| i.lane.is_some()) {
        Arrangement::Lanes
    } else {
        Arrangement::Layers
    }
}

/// Whether a sketch can be drawn, and if not, why — in words the model can
/// act on, since it is the model that has to fix it.
fn check(sketch: &Sketch) -> Result<(), String> {
    if sketch.items.is_empty() {
        return Err("A board with nothing on it is not worth drawing.".into());
    }
    let mut ends = std::collections::HashSet::new();
    for item in &sketch.items {
        if item.label.trim().is_empty() {
            return Err("Every item needs a label; a box with no name cannot be referred to.".into());
        }
        if !ends.insert(item.label.trim().to_lowercase()) {
            return Err(format!(
                "Two items are both called \"{}\"; a line could not say which it meant.",
                item.label
            ));
        }
    }
    for item in &sketch.items {
        if item.fields.len() > MAX_FIELDS {
            return Err(format!(
                "\"{}\" has {} fields; {MAX_FIELDS} is the most a box on a board can show.",
                item.label,
                item.fields.len()
            ));
        }
        if item.fields.iter().any(|f| f.trim().is_empty()) {
            return Err(format!("\"{}\" has an empty field.", item.label));
        }
        for field in &item.fields {
            ends.insert(format!("{}.{}", item.label.trim().to_lowercase(), field_name(field)));
        }
    }
    for link in &sketch.links {
        for end in [&link.from, &link.to] {
            if !ends.contains(&end.trim().to_lowercase()) {
                return Err(format!("A line names \"{end}\", which is not one of the items."));
            }
        }
        for word in [&link.start, &link.end].into_iter().flatten() {
            if end_kind(word).is_none() {
                return Err(format!("\"{word}\" is not a line end; one of: {}.", end_words()));
            }
        }
    }

    match arrangement(sketch) {
        Arrangement::Sequence => {
            if let Some(item) = sketch.items.iter().find(|i| i.lane.is_some()) {
                return Err(format!(
                    "\"{}\" has a lane, but links with an `order` make this a sequence, which has participants, not lanes.",
                    item.label
                ));
            }
            if let Some(item) = sketch.items.iter().find(|i| !i.fields.is_empty()) {
                return Err(format!(
                    "\"{}\" has fields, but in a sequence it is a participant; draw its fields on a board of their own.",
                    item.label
                ));
            }
            if let Some(item) = sketch.items.iter().find(|i| i.row.is_some()) {
                return Err(format!(
                    "\"{}\" has a row; in a sequence the messages decide what is lower down. Use `col` to place a participant.",
                    item.label
                ));
            }
        }
        Arrangement::Lanes => {
            if let Some(item) = sketch.items.iter().find(|i| i.lane.is_none()) {
                return Err(format!(
                    "\"{}\" has no lane; once lanes are used every item needs one.",
                    item.label
                ));
            }
            if let Some(item) = sketch.items.iter().find(|i| i.group.is_some()) {
                return Err(format!(
                    "\"{}\" has both a lane and a group; lanes are rows and groups are columns, so use one or the other.",
                    item.label
                ));
            }
        }
        Arrangement::Layers => {}
    }
    Ok(())
}

/// How much room a box gets, and how much is left between them.
const BOX_H: f64 = 56.0;
const GAP_Y: f64 = 72.0;
const GAP_X: f64 = 48.0;
const FRAME_PAD: f64 = 32.0;
const FRAME_TOP: f64 = 44.0;
const START: f64 = 80.0;

/// A table's rows, and the margin between them and the frame around them.
const ROW_H: f64 = 30.0;
const TABLE_PAD: f64 = 12.0;

/// A box wide enough for what is written in it.
fn box_width(label: &str) -> f64 {
    let longest = label.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
    ((longest as f64) * 8.2 + 36.0).clamp(120.0, 300.0)
}

/// How much room an item takes: a box, a shape with less room inside it
/// than its outline, or a table with a row per field.
///
/// A diamond's words fit in the middle half of it, not across its corners; at
/// the size of a box, "Thông tin đầy đủ?" ran out past both points. An ellipse
/// loses its sides the same way, and a pill its round ends.
fn size_of(label: &str, shape: Option<&str>, fields: &[String]) -> (f64, f64) {
    if fields.is_empty() {
        let words = box_width(label);
        return match shape.and_then(crate::syn::board_shapes::board_shape) {
            Some("diamond" | "decision") => ((words * 1.6).clamp(160.0, 440.0), 88.0),
            Some("ellipse") => ((words * 1.3).clamp(140.0, 380.0), 64.0),
            Some("pill" | "hexagon") => (words + 28.0, BOX_H),
            _ => (words, BOX_H),
        };
    }
    let widest = fields
        .iter()
        .map(|f| (f.trim().chars().count() as f64) * 7.4 + TABLE_PAD * 2.0 + 20.0)
        .fold(box_width(label), f64::max);
    (widest.clamp(160.0, 380.0), table_height(fields.len()))
}

fn item_size(item: &SketchItem) -> (f64, f64) {
    size_of(&item.label, item.shape.as_deref(), &item.fields)
}

fn table_height(fields: usize) -> f64 {
    FRAME_TOP + ROW_H * fields as f64 + TABLE_PAD
}

/// The site an item belongs to: everything before the first `/`.
fn site_of(item: &SketchItem) -> Option<String> {
    item.group
        .as_ref()
        .map(|g| g.split('/').next().unwrap_or("").trim().to_string())
        .filter(|g| !g.is_empty())
}

/// The zone within that site: everything after the first `/`, if there is one.
fn zone_of(item: &SketchItem) -> Option<String> {
    let group = item.group.as_ref()?;
    let (_, zone) = group.split_once('/')?;
    let zone = zone.trim();
    (!zone.is_empty()).then(|| zone.to_string())
}

/// A column's members, split into bands — one per zone, in the order the zones
/// first appear — and each band laid out in rows by rank.
///
/// Items with no zone come first, as the part of the site that is not in one.
type Band<'a> = (Option<String>, Vec<Vec<&'a SketchItem>>);

fn bands_within<'a>(
    members: &[&'a SketchItem],
    rank_of: &std::collections::HashMap<String, usize>,
) -> Vec<Band<'a>> {
    let mut bands: Vec<(Option<String>, Vec<&SketchItem>)> = Vec::new();
    for item in members {
        let zone = zone_of(item);
        match bands.iter_mut().find(|(name, _)| name == &zone) {
            Some((_, held)) => held.push(item),
            None => bands.push((zone, vec![item])),
        }
    }
    bands.sort_by_key(|(zone, _)| zone.is_some());
    bands
        .into_iter()
        .map(|(zone, held)| (zone, rows_by_rank(&held, rank_of)))
        .collect()
}

/// The items of a column, gathered into a row per rank.
///
/// Ranks are compacted as they are read: a drawing with cycles in it can rank
/// its boxes 0, 1, 82, 124 and those numbers are an order, not a distance. Two
/// boxes on the same rank share a row; the rows come out in the order the
/// ranks do.
///
/// An item given a `row` goes in that row of the compacted order instead of
/// the one its rank would have put it in, and an item given a `col` comes
/// before the ones that were not, in that order: the model said where it
/// goes, and it is never a place another box is.
///
/// A rank with more in it than `PER_ROW` is split across rows rather than run
/// out sideways. Eighteen boxes on one line is the tower laid on its side, and
/// just as unreadable — a site drawn that way came out 2,510 pixels wide.
fn rows_by_rank<'a>(
    members: &[&'a SketchItem],
    rank_of: &std::collections::HashMap<String, usize>,
) -> Vec<Vec<&'a SketchItem>> {
    let rank = |item: &SketchItem| rank_of.get(&item.label.trim().to_lowercase()).copied().unwrap_or(0);
    let order: Vec<usize> = members
        .iter()
        .filter(|i| i.row.is_none())
        .map(|i| rank(i))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut by_row: std::collections::BTreeMap<usize, Vec<&SketchItem>> = Default::default();
    for item in members {
        let row = item.row.unwrap_or_else(|| order.binary_search(&rank(item)).unwrap_or(0));
        by_row.entry(row).or_default().push(item);
    }
    by_row
        .into_values()
        .flat_map(|mut row| {
            row.sort_by_key(|i| i.col.unwrap_or(usize::MAX));
            row.chunks(PER_ROW).map(<[&SketchItem]>::to_vec).collect::<Vec<_>>()
        })
        .collect()
}

/// How many boxes a row holds before the rest go on the next one.
///
/// Four. Wide enough that the things arriving at one gateway are visibly
/// together, narrow enough that a site stays something you can read across.
const PER_ROW: usize = 4;

/// How wide a row of boxes is, gaps included.
fn row_width(row: &[&SketchItem]) -> f64 {
    let boxes: f64 = row.iter().map(|i| item_size(i).0).sum();
    boxes + GAP_X * (row.len().saturating_sub(1)) as f64
}

/// How tall a row is: its tallest item, a table being taller than a box.
fn row_height(row: &[&SketchItem]) -> f64 {
    row.iter().map(|i| item_size(i).1).fold(BOX_H, f64::max)
}

/// Lay a sketch out, and hand back a board.
///
/// Which of the three layouts it gets is what the sketch holds (see
/// `Sketch`). All three put down the same things — boxes, tables, frames —
/// through `Canvas`, so a table in a lane is the same table as one in a group.
pub fn draw(title: &str, sketch: &Sketch, now_ms: i64) -> Result<Board, String> {
    check(sketch)?;
    let mut canvas = Canvas::new(now_ms);
    match arrangement(sketch) {
        Arrangement::Layers => layers(&mut canvas, sketch),
        Arrangement::Lanes => lanes(&mut canvas, sketch),
        Arrangement::Sequence => sequence(&mut canvas, sketch),
    }
    Ok(canvas.into_board(title))
}

/// A rectangle on the board: x, y, width, height.
type Rect = (f64, f64, f64, f64);

/// What a layout puts things down on.
///
/// Frames for groups and lanes are kept apart from everything else so they
/// can be listed first, and so drawn behind; a table's frame goes with its
/// rows, after the groups it may sit in.
struct Canvas {
    now: i64,
    n: usize,
    frames: Vec<Item>,
    nodes: Vec<Item>,
    edges: Vec<Link>,
    /// Every name a line can end on — an item's label, and `table.field` —
    /// in lower case, with the id and the rectangle it stands for.
    named: std::collections::HashMap<String, (String, Rect)>,
    /// The ids of tables' rows, whose lines leave from the side.
    fields: std::collections::HashSet<String>,
}

impl Canvas {
    fn new(now: i64) -> Self {
        Canvas {
            now,
            n: 0,
            frames: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            named: Default::default(),
            fields: Default::default(),
        }
    }

    fn id(&mut self, kind: &str) -> String {
        self.n += 1;
        format!("{kind}-{:x}-{}", self.now, self.n)
    }

    /// A group's or a lane's frame.
    fn group_frame(&mut self, label: &str, rect: Rect) {
        let id = self.id("frame");
        self.frames.push(frame(id, rect.0, rect.1, rect.2, rect.3, label, self.now));
    }

    /// Put an item down with its top-left corner here: a box, or a table.
    /// Returns where it ended up.
    fn place(&mut self, item: &SketchItem, x: f64, y: f64) -> Rect {
        let key = item.label.trim().to_lowercase();
        if item.fields.is_empty() {
            let id = self.id("box");
            let (w, h) = item_size(item);
            let node = shape(id, x, y, w, h, &item.label, item.shape.as_deref(), self.now);
            let rect = rect_of(&node);
            self.named.insert(key, (node.id.clone(), rect));
            self.nodes.push(node);
            return rect;
        }
        let (w, _) = item_size(item);
        let now = self.now;
        let mut ids = |kind: &str| self.id(kind);
        let parts = table(&mut ids, x, y, w, &item.label, &item.fields, now);
        let rect = rect_of(&parts[0]);
        self.named.insert(key.clone(), (parts[0].id.clone(), rect));
        for (field, row) in item.fields.iter().zip(&parts[1..]) {
            self.named.insert(format!("{key}.{}", field_name(field)), (row.id.clone(), rect_of(row)));
            self.fields.insert(row.id.clone());
        }
        self.nodes.extend(parts);
        rect
    }

    /// The lines of a sketch, between whatever its ends name.
    fn join(&mut self, links: &[SketchLink]) {
        for link in links {
            let (Some(from), Some(to)) = (
                self.named.get(&link.from.trim().to_lowercase()).cloned(),
                self.named.get(&link.to.trim().to_lowercase()).cloned(),
            ) else {
                continue;
            };
            let fields = self.fields.contains(&from.0) || self.fields.contains(&to.0);
            let (out_side, in_side) = if fields { sides_beside(from.1, to.1) } else { sides_facing(from.1, to.1) };
            let mut data = line_data(link.label.as_deref(), link.start.as_deref(), link.end.as_deref(), link.dashed);
            if fields {
                data.remove("sides");
            } else {
                route_later(&mut data);
            }
            let id = format!("edge-{:x}-{}", self.now, self.edges.len());
            self.edges.push(Link {
                id,
                rest: Map::new(),
                source: from.0,
                source_handle: Some(out_side.into()),
                target: to.0,
                target_handle: Some(in_side.into()),
                kind: line_kind(fields).into(),
                data,
                updated: Some(self.now),
            });
        }
    }

    fn into_board(mut self, title: &str) -> Board {
        let stamp = chrono::Utc::now().to_rfc3339();
        let mut metadata = Map::new();
        metadata.insert("updated_at".into(), json!(stamp));
        self.frames.append(&mut self.nodes);
        Board {
            schema_version: Some(SCHEMA_VERSION),
            title: title.to_string(),
            tags: Vec::new(),
            created_at: stamp,
            metadata: Some(metadata),
            viewport: origin(),
            nodes: self.frames,
            edges: self.edges,
            rest: Map::new(),
        }
    }
}

fn rect_of(item: &Item) -> Rect {
    (item.position.x, item.position.y, item.width(), item.height())
}

/// A table or a class: the app's frame with its name on it, and a row inside
/// for each field, one under the other with nothing between, as a table is.
///
/// A frame, rather than the board's `erTable` or `umlClass` shapes, because
/// those are drawings of a table — three rows, always — and have nowhere to
/// write a field. A frame holds real rows, which a person can edit, reorder
/// and add to, and which `read_board` reads as what is inside it.
fn table(
    ids: &mut dyn FnMut(&str) -> String,
    x: f64,
    y: f64,
    w: f64,
    label: &str,
    fields: &[String],
    now_ms: i64,
) -> Vec<Item> {
    let mut parts = vec![frame(ids("frame"), x, y, w, table_height(fields.len()), label, now_ms)];
    for (i, field) in fields.iter().enumerate() {
        let mut row = shape(
            ids("box"),
            x + TABLE_PAD,
            y + FRAME_TOP + ROW_H * i as f64,
            w - TABLE_PAD * 2.0,
            ROW_H,
            field.trim(),
            None,
            now_ms,
        );
        row.data.insert("textAlign".into(), json!("left"));
        row.data.insert("fontSize".into(), json!(13));
        row.data.insert("field".into(), json!(true));
        parts.push(row);
    }
    parts
}

/// The default layout: what feeds what, top to bottom.
///
/// # The shape this produces, and why that one
///
/// Groups become **columns side by side**, and what is not in a group goes in
/// a row across the top. That is not a general-purpose graph layout; it is the
/// shape these drawings keep turning out to be — two data centres that mirror
/// each other, fed from something upstream — and the complaint that started
/// all this was that a general-purpose layout would not put the two sites side
/// by side however the diagram was written.
///
/// Inside a column, an item sits below the ones that feed it: rank by the
/// longest path along the links, which is how every layered layout does it,
/// and which puts a chain in the order it is read.
fn layers(c: &mut Canvas, sketch: &Sketch) {
    // ── Columns: one per group, in the order the groups first appear ──
    let mut columns: Vec<(Option<String>, Vec<&SketchItem>)> = Vec::new();
    for item in &sketch.items {
        let group = site_of(item);
        match columns.iter_mut().find(|(name, _)| name == &group) {
            Some((_, members)) => members.push(item),
            None => columns.push((group, vec![item])),
        }
    }

    let rank_of = ranks(sketch);

    // The ungrouped go across the top; the groups go side by side below them.
    let loose: Vec<&SketchItem> =
        columns.iter().filter(|(g, _)| g.is_none()).flat_map(|(_, m)| m.clone()).collect();
    let grouped: Vec<&(Option<String>, Vec<&SketchItem>)> =
        columns.iter().filter(|(g, _)| g.is_some()).collect();

    let mut y = START;
    let mut lowest_top: f64 = START;
    // Each row centred under the widest, as the rows in a group are: a chain
    // of boxes of different widths, each lined up on the left, put a kink in
    // every line down it, the middle of one box being nowhere near the
    // middle of the next.
    let loose_rows = rows_by_rank(&loose, &rank_of);
    let loose_w = loose_rows.iter().map(|row| row_width(row)).fold(0.0_f64, f64::max);
    for row in loose_rows {
        let mut at_x = START + (loose_w - row_width(&row)) / 2.0;
        let row_y = lowest_top;
        let row_h = row_height(&row);
        for item in &row {
            let (w, h) = item_size(item);
            c.place(item, at_x, row_y + (row_h - h) / 2.0);
            at_x += w + GAP_X;
        }
        lowest_top = row_y + row_h + GAP_Y;
    }
    if !loose.is_empty() {
        y = lowest_top + GAP_Y / 2.0;
    }

    // ── Each group a column; inside it, a band per zone, a row per rank ──
    //
    // A row per rank, not a row per item. The first real drawing this was
    // asked for had twenty-one boxes in a site, four of them fed by the same
    // gateway — and stacking them one to a row made each site a tower 2,692
    // pixels tall and 354 wide. Things that arrive at the same point belong
    // beside each other; that is what a layer *is*.
    let mut column_x = START;
    for (group, members) in grouped {
        // A zone inside a site: `group: "DC 1/Network Hub"`. One level of
        // nesting, because that is what these drawings have — a site with
        // zones in it — and because the alternative was what happened without
        // it: three devices renamed `[Network Hub] SW WAN` to say in words
        // what a frame says by holding them.
        let bands = bands_within(members, &rank_of);
        let column_w = bands
            .iter()
            .flat_map(|(zone, rows)| {
                let pad = if zone.is_some() { FRAME_PAD * 2.0 } else { 0.0 };
                rows.iter().map(move |row| row_width(row) + pad)
            })
            .fold(160.0_f64, f64::max);

        let inner_x = column_x + FRAME_PAD;
        let mut inner_y = y + FRAME_TOP;
        let mut zones = Vec::new();
        for (zone, rows) in &bands {
            let band_top = inner_y;
            if zone.is_some() {
                inner_y += FRAME_TOP;
            }
            for row in rows {
                let mut at_x = inner_x + (column_w - row_width(row)) / 2.0;
                let row_h = row_height(row);
                for item in row {
                    // In the middle of the row's height, so a line between a
                    // box and the diamond beside it runs straight across.
                    let (w, h) = item_size(item);
                    c.place(item, at_x, inner_y + (row_h - h) / 2.0);
                    at_x += w + GAP_X;
                }
                inner_y += row_h + GAP_Y;
            }
            if let Some(name) = zone {
                // The zone's own frame, around the rows that belong to it.
                zones.push((
                    name.clone(),
                    (
                        inner_x - FRAME_PAD / 2.0,
                        band_top,
                        column_w + FRAME_PAD,
                        (inner_y - GAP_Y) - band_top + FRAME_PAD / 2.0,
                    ),
                ));
                inner_y += GAP_Y / 2.0;
            }
        }

        let frame_h = (inner_y - GAP_Y) - y + FRAME_PAD;
        // The site before its zones: frames are drawn in the order they are
        // listed, and a site listed after its zones would be laid over them.
        c.group_frame(group.as_deref().unwrap_or_default(), (column_x, y, column_w + FRAME_PAD * 2.0, frame_h));
        for (name, rect) in zones {
            c.group_frame(&name, rect);
        }
        column_x += column_w + FRAME_PAD * 2.0 + GAP_X * 1.5;
    }

    c.join(&sketch.links);
}

/// Lanes: one band across the board for each, and the flow left to right.
///
/// # Why the flow turns sideways
///
/// Because the lanes take the other direction. A lane is who does a step —
/// a team, a system, a person — and the steps go from one lane to the next
/// and back as the work passes between them. Read down, that is the lane; read
/// across, that is the order. So an item's rank along the links is its
/// column, the same in every lane, and a step two lanes down that follows one
/// up here is one column to the right of it.
///
/// Two items of the same lane in the same column share it, one below the
/// other, and the lane is as tall as its fullest column.
fn lanes(c: &mut Canvas, sketch: &Sketch) {
    let mut names: Vec<String> = Vec::new();
    for item in &sketch.items {
        let lane = item.lane.as_deref().unwrap_or("").trim().to_string();
        if !names.contains(&lane) {
            names.push(lane);
        }
    }

    let rank_of = ranks(sketch);
    let rank = |item: &SketchItem| rank_of.get(&item.label.trim().to_lowercase()).copied().unwrap_or(0);
    let order: Vec<usize> = sketch
        .items
        .iter()
        .filter(|i| i.col.is_none())
        .map(rank)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let col_of = |item: &SketchItem| item.col.unwrap_or_else(|| order.binary_search(&rank(item)).unwrap_or(0));
    let lane_of = |item: &SketchItem| {
        let lane = item.lane.as_deref().unwrap_or("").trim();
        names.iter().position(|n| n == lane).unwrap_or(0)
    };

    let columns = sketch.items.iter().map(col_of).max().unwrap_or(0) + 1;
    let mut cells: std::collections::BTreeMap<(usize, usize), Vec<&SketchItem>> = Default::default();
    for item in &sketch.items {
        cells.entry((lane_of(item), col_of(item))).or_default().push(item);
    }
    for held in cells.values_mut() {
        held.sort_by_key(|i| i.row.unwrap_or(usize::MAX));
    }

    let mut col_w = vec![120.0_f64; columns];
    for item in &sketch.items {
        let w = item_size(item).0;
        let k = col_of(item);
        col_w[k] = col_w[k].max(w);
    }
    let mut col_x = Vec::with_capacity(columns);
    let mut x = START + FRAME_PAD;
    for w in &col_w {
        col_x.push(x);
        x += w + GAP_X * 1.5;
    }
    let width = x - GAP_X * 1.5 + FRAME_PAD - START;

    const STACK_GAP: f64 = GAP_Y / 2.0;
    const LANE_GAP: f64 = 16.0;
    let mut lane_y = START;
    for (li, name) in names.iter().enumerate() {
        // The first item of every column shares a middle line, so a lane's
        // steps read across in a straight line whatever their shapes.
        let first_h = (0..columns)
            .filter_map(|k| cells.get(&(li, k)).and_then(|held| held.first()))
            .map(|i| item_size(i).1)
            .fold(BOX_H, f64::max);
        let tallest = (0..columns)
            .map(|k| {
                cells.get(&(li, k)).map_or(0.0, |held| {
                    first_h
                        + held.iter().skip(1).map(|i| item_size(i).1 + STACK_GAP).sum::<f64>()
                })
            })
            .fold(BOX_H, f64::max);
        let lane_h = FRAME_TOP + tallest + FRAME_PAD;
        c.group_frame(name, (START, lane_y, width, lane_h));

        for k in 0..columns {
            let Some(held) = cells.get(&(li, k)) else { continue };
            let mut y = lane_y + FRAME_TOP;
            for (at, item) in held.iter().enumerate() {
                let (w, h) = item_size(item);
                let top = if at == 0 { y + (first_h - h) / 2.0 } else { y };
                c.place(item, col_x[k] + (col_w[k] - w) / 2.0, top);
                y = if at == 0 { y + first_h } else { y + h } + STACK_GAP;
            }
        }
        lane_y += lane_h + LANE_GAP;
    }

    c.join(&sketch.links);
}

/// A sequence: participants side by side, and messages between them from the
/// top down in the order they happen.
///
/// # What it is made of, and why those things
///
/// A message is a line, and a line on this board runs from one item to
/// another. So each end of a message is an item: a small activation bar on
/// the participant's lifeline at that message's height, and the lifeline is
/// the dashed line from the participant down through its bars, one segment
/// between each. A person can drag a bar up or down, and the message and the
/// lifeline go with it; nothing here is a picture of a line that is really
/// nowhere.
///
/// The board's own `umlLifeline` shape was the obvious choice and the wrong
/// one: it is a drawing on a fixed 100×100 grid, so a lifeline tall enough for
/// twenty messages has a head two hundred pixels tall, and no message can be
/// joined to it anywhere but its middle.
///
/// The bars and segments say which participant they belong to (`lifelineOf`,
/// `lifeline`), so `read_board` reads the messages by the participants' names
/// and leaves the scaffolding out.
fn sequence(c: &mut Canvas, sketch: &Sketch) {
    // ── Who, in which order ──────────────────────────────────
    // A participant given a `col` comes first, in that order; the rest in the
    // order they were written. A group's participants stand together, so a
    // frame can go round them.
    let mut placed: Vec<&SketchItem> = sketch.items.iter().collect();
    placed.sort_by_key(|i| i.col.unwrap_or(usize::MAX));
    let mut blocks: Vec<(Option<String>, Vec<&SketchItem>)> = Vec::new();
    for item in placed {
        match site_of(item) {
            Some(group) => match blocks.iter_mut().find(|(g, _)| g.as_ref() == Some(&group)) {
                Some((_, held)) => held.push(item),
                None => blocks.push((Some(group), vec![item])),
            },
            None => blocks.push((None, vec![item])),
        }
    }
    let people: Vec<&SketchItem> = blocks.iter().flat_map(|(_, held)| held.iter().copied()).collect();
    let index: std::collections::HashMap<String, usize> =
        people.iter().enumerate().map(|(i, p)| (p.label.trim().to_lowercase(), i)).collect();

    let mut messages: Vec<&SketchLink> = sketch.links.iter().collect();
    messages.sort_by(|a, b| match (a.order, b.order) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    // ── Where each lifeline stands ───────────────────────────
    // Far enough apart for the heads, and then for what is written on the
    // messages between them: a label wider than the gap it spans runs over
    // the lifelines on either side.
    let widths: Vec<f64> = people.iter().map(|p| box_width(&p.label)).collect();
    let mut cx: Vec<f64> = Vec::with_capacity(people.len());
    for (i, w) in widths.iter().enumerate() {
        let x = if i == 0 { START + w / 2.0 } else { cx[i - 1] + widths[i - 1] / 2.0 + GAP_X * 2.0 + w / 2.0 };
        cx.push(x);
    }
    let ends = |m: &SketchLink| {
        let a = index.get(&m.from.trim().to_lowercase()).copied()?;
        let b = index.get(&m.to.trim().to_lowercase()).copied()?;
        Some((a, b))
    };
    let mut spans: Vec<(usize, usize, f64)> = messages
        .iter()
        .filter_map(|m| {
            let (a, b) = ends(m)?;
            let words = m.label.as_deref().unwrap_or("").trim().chars().count();
            (a != b && words > 0).then(|| (a.min(b), a.max(b), words as f64 * 7.4 + 56.0))
        })
        .collect();
    spans.sort_by_key(|(lo, hi, _)| hi - lo);
    for (lo, hi, need) in spans {
        let have = cx[hi] - cx[lo];
        if have < need {
            let more = (need - have) / (hi - lo) as f64;
            for (k, x) in cx.iter_mut().enumerate().skip(lo + 1) {
                *x += more * (k.min(hi) - lo) as f64;
            }
        }
    }

    // ── Heads, then the messages from the top down ───────────
    let grouped = blocks.iter().any(|(g, _)| g.is_some());
    let top = if grouped { START + FRAME_TOP } else { START };
    let mut last: Vec<(String, Rect)> = Vec::with_capacity(people.len());
    let mut heads: Vec<String> = Vec::with_capacity(people.len());
    for (i, person) in people.iter().enumerate() {
        let rect = c.place(person, cx[i] - widths[i] / 2.0, top);
        let id = c.named[&person.label.trim().to_lowercase()].0.clone();
        heads.push(id.clone());
        last.push((id, rect));
    }

    const STEP: f64 = 56.0;
    const BAR_W: f64 = 12.0;
    const BAR_H: f64 = 24.0;
    let mut y = top + BOX_H + 40.0;
    for message in &messages {
        let Some((a, b)) = ends(message) else { continue };
        let data = line_data(message.label.as_deref(), message.start.as_deref(), message.end.as_deref(), message.dashed);
        if a == b {
            // To itself: out of one bar and back into the one below it.
            let first = bar(c, &heads[a], cx[a], y, BAR_W, BAR_H);
            lifeline(c, &mut last[a], first.clone());
            let second = bar(c, &heads[a], cx[a], y + STEP * 0.6, BAR_W, BAR_H);
            lifeline(c, &mut last[a], second.clone());
            let mut data = data;
            data.remove("sides");
            message_line(c, first.0, "right", second.0, "right", data);
            y += STEP * 1.6;
        } else {
            let from = bar(c, &heads[a], cx[a], y, BAR_W, BAR_H);
            lifeline(c, &mut last[a], from.clone());
            let to = bar(c, &heads[b], cx[b], y, BAR_W, BAR_H);
            lifeline(c, &mut last[b], to.clone());
            let (out_side, in_side) = if a < b { ("right", "left") } else { ("left", "right") };
            message_line(c, from.0, out_side, to.0, in_side, data);
            y += STEP;
        }
    }

    // Every lifeline runs to the same depth, and ends there.
    for (i, held) in last.iter_mut().enumerate() {
        let end = bar(c, &heads[i], cx[i], y, 24.0, 4.0);
        lifeline(c, held, end);
    }

    // ── Frames round the groups' lifelines ───────────────────
    let mut at = 0;
    for (group, held) in &blocks {
        let (first, past) = (at, at + held.len());
        at = past;
        let Some(name) = group else { continue };
        let left = cx[first] - widths[first] / 2.0 - FRAME_PAD;
        let right = cx[past - 1] + widths[past - 1] / 2.0 + FRAME_PAD;
        c.group_frame(name, (left, START, right - left, y + FRAME_PAD - START));
    }
}

/// A small bar on a lifeline, centred on it at this height.
fn bar(c: &mut Canvas, head: &str, cx: f64, y: f64, w: f64, h: f64) -> (String, Rect) {
    let id = c.id("box");
    let mut node = shape(id, cx - w / 2.0, y - h / 2.0, w, h, "", Some("umlActivation"), c.now);
    node.data.insert("lifelineOf".into(), json!(head));
    let rect = rect_of(&node);
    let out = (node.id.clone(), rect);
    c.nodes.push(node);
    out
}

/// The dashed piece of a lifeline from the last thing on it down to this.
fn lifeline(c: &mut Canvas, last: &mut (String, Rect), next: (String, Rect)) {
    let mut data = Map::new();
    data.insert("lifeline".into(), json!(true));
    data.insert("dashStyle".into(), json!("dashed"));
    data.insert("color".into(), json!("#94a3b8"));
    data.insert("markerEnd".into(), json!("none"));
    let id = format!("edge-{:x}-{}", c.now, c.edges.len());
    c.edges.push(Link {
        id,
        rest: Map::new(),
        source: last.0.clone(),
        source_handle: Some("bottom".into()),
        target: next.0.clone(),
        target_handle: Some("top".into()),
        kind: "straight".into(),
        data,
        updated: Some(c.now),
    });
    *last = next;
}

fn message_line(c: &mut Canvas, from: String, out_side: &str, to: String, in_side: &str, data: Map<String, Value>) {
    let id = format!("edge-{:x}-{}", c.now, c.edges.len());
    c.edges.push(Link {
        id,
        rest: Map::new(),
        source: from,
        source_handle: Some(out_side.into()),
        target: to,
        target_handle: Some(in_side.into()),
        kind: "straight".into(),
        data,
        updated: Some(c.now),
    });
}

/// How far along the chain each item sits.
///
/// The longest path to it: a box goes below everything that feeds it — once
/// the lines that go back have been set aside.
///
/// Keyed by the label in lower case, as everything else here compares them: a
/// line to "kong" is a line to "Kong", and was drawn as one while being left
/// out of the ranking. A line to a field ranks the table it is in.
///
/// # The lines that go back
///
/// A flowchart loops: *information complete? — no — add it — check again*.
/// The longest path around a loop has no end, and the first version of this
/// relaxed ranks pass after pass and kept whatever the last pass left — so
/// the steps of a loop came out in an order nobody wrote. A Change Request
/// drawn with one such loop put "complete?" before the step that checks, and
/// the line from "submit" ran through a box to reach it.
///
/// So the loop is broken first, where a person reading the drawing would
/// break it: walking from where the drawing starts (the items nothing points
/// at, in the order they were written, then anything left), a line to a step
/// that is still being walked is the way back. It is drawn like any other, and
/// does not count towards anything's rank.
///
/// Where to start the walk is what makes this agree with the reader. A walk
/// started from an arbitrary item was tried before, and put a database on the
/// row of a gateway three steps above it: which line counts as the way back
/// depends on where the walk began, and only the start of the drawing is a
/// place the drawing itself says to begin.
fn ranks(sketch: &Sketch) -> std::collections::HashMap<String, usize> {
    let names: Vec<String> = sketch.items.iter().map(|i| i.label.trim().to_lowercase()).collect();
    let index: std::collections::HashMap<&str, usize> =
        names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    // `orders.user_id` ranks as `orders`, unless an item is called that.
    let item_of = |end: &str| -> Option<usize> {
        let end = end.trim().to_lowercase();
        if let Some(&i) = index.get(end.as_str()) {
            return Some(i);
        }
        let (table, _) = end.rsplit_once('.')?;
        index.get(table).copied()
    };

    let n = names.len();
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut fed = vec![false; n];
    for link in &sketch.links {
        if let (Some(a), Some(b)) = (item_of(&link.from), item_of(&link.to)) {
            if a != b {
                out[a].push(b);
                fed[b] = true;
            }
        }
    }

    // ── Which lines go back ──────────────────────────────────
    const NEW: u8 = 0;
    const WALKING: u8 = 1;
    const DONE: u8 = 2;
    let mut state = vec![NEW; n];
    let mut back: std::collections::HashSet<(usize, usize)> = Default::default();
    let starts = (0..n).filter(|&i| !fed[i]).chain(0..n);
    for start in starts {
        if state[start] != NEW {
            continue;
        }
        // Each frame: an item, and how far through its lines the walk is.
        let mut stack = vec![(start, 0usize)];
        state[start] = WALKING;
        while let Some(&mut (at, ref mut next)) = stack.last_mut() {
            if let Some(&to) = out[at].get(*next) {
                *next += 1;
                match state[to] {
                    NEW => {
                        state[to] = WALKING;
                        stack.push((to, 0));
                    }
                    WALKING => {
                        back.insert((at, to));
                    }
                    _ => {}
                }
            } else {
                state[at] = DONE;
                stack.pop();
            }
        }
    }

    // ── The longest path along what is left ──────────────────
    // What is left has no loop in it, so every item is reached once all that
    // feeds it has been: Kahn's order, ranking as it goes.
    let mut waiting = vec![0usize; n];
    for (a, outs) in out.iter().enumerate() {
        for &b in outs {
            if !back.contains(&(a, b)) {
                waiting[b] += 1;
            }
        }
    }
    let mut rank = vec![0usize; n];
    let mut ready: std::collections::VecDeque<usize> = (0..n).filter(|&i| waiting[i] == 0).collect();
    while let Some(a) = ready.pop_front() {
        for &b in &out[a] {
            if back.contains(&(a, b)) {
                continue;
            }
            rank[b] = rank[b].max(rank[a] + 1);
            waiting[b] -= 1;
            if waiting[b] == 0 {
                ready.push_back(b);
            }
        }
    }
    names.into_iter().zip(rank).collect()
}

const BOX_COLOUR: &str = "#7c3aed";

#[allow(clippy::too_many_arguments)]
fn shape(
    id: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    label: &str,
    kind: Option<&str>,
    now_ms: i64,
) -> Item {
    // A figure (a person, a server, a phone) is drawn square, in the middle of
    // the room its label was given, with the label under it: stretched to a
    // label's width it is no longer the thing, and the label lands across it.
    // The board's own name for the shape: a model's "database" is a cylinder,
    // and a name the board does not have is a plain box (see `board_shapes`).
    let kind = kind.and_then(crate::syn::board_shapes::board_shape);
    let (x, w, h) = if kind.is_some_and(crate::syn::board_shapes::is_figure) {
        let side = h.min(56.0);
        (x + (w - side) / 2.0, side, side)
    } else {
        (x, w, h)
    };
    let mut data = Map::new();
    data.insert("shapeType".into(), json!(kind.unwrap_or("rectangle")));
    data.insert("label".into(), json!(label));
    data.insert("color".into(), json!(BOX_COLOUR));
    data.insert("width".into(), json!(w));
    data.insert("height".into(), json!(h));
    Item { id, kind: "shape".into(), position: Point { x, y }, data, updated: Some(now_ms), rest: Map::new() }
}

/// An id nothing on the board has. Counting what is on the board is not
/// enough: a removal in the same call shrinks the count, and the next thing
/// added would take the id of the one added before it.
fn fresh_id(board: &Board, prefix: &str, now_ms: i64) -> String {
    let taken = |id: &str| board.nodes.iter().any(|n| n.id == id) || board.edges.iter().any(|e| e.id == id);
    let mut n = board.nodes.len() + board.edges.len();
    loop {
        let id = format!("{prefix}-{now_ms:x}-{n}");
        if !taken(&id) {
            return id;
        }
        n += 1;
    }
}

/// A group, as the app's own frame.
///
/// It used to be a grey rectangle, which looked like a frame and was not one:
/// dragging it left its contents behind, present mode had no slide for it, and
/// the app laid it out as a box among boxes. A `frame` is all of those things.
fn frame(id: String, x: f64, y: f64, w: f64, h: f64, label: &str, now_ms: i64) -> Item {
    let mut data = Map::new();
    data.insert("label".into(), json!(label));
    data.insert("width".into(), json!(w));
    data.insert("height".into(), json!(h));
    Item { id, kind: "frame".into(), position: Point { x, y }, data, updated: Some(now_ms), rest: Map::new() }
}

/// What a line Syn draws carries: its words, its ends, and sides that follow
/// the boxes.
///
/// An arrowhead at the far end unless told otherwise, because a link is
/// written `from` → `to` and was drawn with no way to see which end was which.
/// `sides: 'auto'` because the sides were fixed where the boxes first landed,
/// so a box dragged round to the other side had its line leave from the far
/// edge and wrap back across it — the app gives a line a person draws the same
/// (routing.ts). The ends are checked before anything is drawn (`check`), so a
/// word that is not one is not reached here; it would draw nothing.
fn line_data(label: Option<&str>, start: Option<&str>, end: Option<&str>, dashed: bool) -> Map<String, Value> {
    let mut data = Map::new();
    if let Some(words) = label.filter(|l| !l.trim().is_empty()) {
        data.insert("label".into(), json!(words));
    }
    if let Some(kind) = start.and_then(end_kind).filter(|k| *k != "none") {
        data.insert("markerStart".into(), json!(kind));
    }
    data.insert("markerEnd".into(), json!(end.and_then(end_kind).unwrap_or("arrow")));
    if dashed {
        data.insert("dashStyle".into(), json!("dashed"));
    }
    data.insert("sides".into(), json!("auto"));
    data
}

/// Ask the app to route this line around the boxes the next time it opens
/// the board (`routeLines`, autoRoute.ts), which then takes the flag off.
///
/// Routed there rather than here because the router is MSAGL's, which is
/// JavaScript: it spreads the lines that leave one box along its side and
/// keeps parallel runs apart, where the app's own stepped line (`sides:
/// 'auto'`) takes the middle of the side, so every line out of a busy box
/// left from one point. Until it is routed — a board opened only in a note,
/// or on a WebView the router does not run on — the line is that stepped
/// line, as before.
///
/// On the line, not the board: an edit that adds one line asks for that line
/// alone, and two devices' copies merge line by line (`board_merge`).
fn route_later(data: &mut Map<String, Value>) {
    data.insert("routing".into(), json!("pending"));
}

/// A moved box's stepped lines, routed again. Their bends went around the
/// box where it was, and are not a way to where it is now.
fn reroute_lines_of(board: &mut Board, id: &str, now_ms: i64) {
    for line in board.edges.iter_mut().filter(|e| (e.source == id || e.target == id) && e.kind == "step") {
        for key in ["waypoints", "sourceAt", "targetAt", "labelAt", "routed"] {
            line.data.remove(key);
        }
        route_later(&mut line.data);
        line.updated = Some(now_ms);
    }
}

/// How a line Syn draws between boxes runs: in steps, at right angles.
///
/// Because a stepped line whose sides follow the boxes is the one the app
/// routes *around* what is in its way (`orthogonalRoute`, routing.ts); a curve
/// goes straight through. A Change Request flow had "rejected → end" cross
/// five boxes on its way down the page. Right angles are also what a flowchart
/// or a swimlane is drawn in.
///
/// A line to a table's row keeps its curve: it leaves from the side of the
/// row, often both ends on the right, and bows out beside the tables.
fn line_kind(to_a_field: bool) -> &'static str {
    if to_a_field {
        "default"
    } else {
        "step"
    }
}

/// Which side of each box a line leaves from, decided by where they ended up.
fn sides_between(from: &Item, to: &Item) -> (&'static str, &'static str) {
    sides_facing(rect_of(from), rect_of(to))
}

/// The sides for a line that ends on a table's row: the left or the right,
/// never the top or bottom, which are the rows above and below it. Two tables
/// one above the other are joined right side to right side, the line bowing
/// out beside them, as a schema is drawn.
///
/// Fixed rather than `sides: 'auto'`: the app's choice of facing sides would
/// put the line back through the rows as soon as a table was dragged.
fn sides_beside(from: Rect, to: Rect) -> (&'static str, &'static str) {
    let (a, b) = (from.0 + from.2 / 2.0, to.0 + to.2 / 2.0);
    if (b - a).abs() < (from.2 + to.2) / 4.0 {
        ("right", "right")
    } else if b > a {
        ("right", "left")
    } else {
        ("left", "right")
    }
}

fn sides_facing(from: Rect, to: Rect) -> (&'static str, &'static str) {
    let dx = to.0 + to.2 / 2.0 - (from.0 + from.2 / 2.0);
    let dy = to.1 + to.3 / 2.0 - (from.1 + from.3 / 2.0);
    if dx.abs() > dy.abs() {
        if dx >= 0.0 {
            ("right", "left")
        } else {
            ("left", "right")
        }
    } else if dy >= 0.0 {
        ("bottom", "top")
    } else {
        ("top", "bottom")
    }
}

// ═══════════════════════════════════════════════════════════════
//  Changing one that exists
// ═══════════════════════════════════════════════════════════════

/// A change to a board that is already arranged.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Change {
    /// Put something new on the board, beside something already there.
    Add {
        label: String,
        #[serde(default)]
        shape: Option<String>,
        /// Which existing box to put it next to. Without one it goes below
        /// everything, where it is visible and in nobody's way.
        #[serde(default)]
        near: Option<String>,
        /// Which frame it belongs in — a zone, a site, a rack.
        ///
        /// Without this there was no way to say "inside", and it showed: asked
        /// to put three devices in a Network Hub, the assistant put them
        /// underneath it, was told that was wrong, and settled for renaming
        /// them `[Network Hub] SW WAN`. The frame is what says where something
        /// lives; a label saying so is a caption pretending to be a place.
        #[serde(default)]
        inside: Option<String>,
        /// A table's or a class's fields: drawn as `draw` draws them.
        #[serde(default)]
        fields: Vec<String>,
    },
    /// Join two things that are already there. Either end may be a table's
    /// field: `"orders.user_id"`.
    Connect {
        from: String,
        to: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        start: Option<String>,
        #[serde(default)]
        end: Option<String>,
        #[serde(default)]
        dashed: bool,
    },
    Rename {
        item: String,
        label: String,
    },
    /// Take something off, and the lines that ended on it.
    Remove {
        item: String,
    },
    /// Move one box to a named side of another. The only op that moves
    /// anything, and it moves exactly what it was told to.
    Place {
        item: String,
        side: String,
        of: String,
    },
    /// A new, empty frame — a zone, a site, a stage — for things to be put
    /// in with `inside` or `move_into`.
    AddFrame {
        label: String,
        #[serde(default)]
        near: Option<String>,
        #[serde(default)]
        inside: Option<String>,
    },
    /// Move something that is already on the board into a frame.
    MoveInto {
        item: String,
        frame: String,
    },
}

/// Apply changes, and say what happened.
///
/// # The rule this keeps
///
/// **Nothing already on the board moves unless a `place` says so.** A board is
/// arranged by hand; adding an item is not permission to re-draw the drawing.
/// It is the difference between a tool somebody can use on their own work and
/// one they have to check after every call.
pub fn apply(board: &mut Board, changes: &[Change], now_ms: i64) -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    for change in changes {
        match change {
            Change::Add { label, shape: kind, near, inside, fields } => {
                if label.trim().is_empty() {
                    return Err("An item needs a label.".into());
                }
                if find(board, label).is_some() {
                    return Err(format!("\"{label}\" is already on this board."));
                }
                if fields.len() > MAX_FIELDS || fields.iter().any(|f| f.trim().is_empty()) {
                    return Err(format!("\"{label}\" needs between one and {MAX_FIELDS} fields, none of them empty."));
                }
                let (w, h) = size_of(label, kind.as_deref(), fields);
                let (x, y, holder) = spot_for(board, inside.as_deref(), near.as_deref(), w, h)?;
                if fields.is_empty() {
                    let id = fresh_id(board, "box", now_ms);
                    board.nodes.push(shape(id, x, y, w, h, label, kind.as_deref(), now_ms));
                } else {
                    // One base id for the whole table, and a part number
                    // after it: `fresh_id` is only fresh until something is
                    // added, and nothing is added until the table is built.
                    let base = fresh_id(board, "table", now_ms);
                    let mut part = 0;
                    let mut ids = |kind: &str| {
                        part += 1;
                        format!("{base}-{kind}-{part}")
                    };
                    let parts = table(&mut ids, x, y, w, label, fields, now_ms);
                    board.nodes.extend(parts);
                }
                // A frame told to hold something holds it from this moment,
                // not from whenever geometry happens to agree: the new box is
                // placed under the last thing in the frame, which is below the
                // frame's own edge until the frame is grown for it.
                if let Some(frame) = holder {
                    grow_to_hold(board, frame, x, y, w, h, now_ms);
                }
                done.push(format!("added \"{label}\""));
            }

            Change::AddFrame { label, near, inside } => {
                if label.trim().is_empty() {
                    return Err("A frame needs a label.".into());
                }
                if find(board, label).is_some() {
                    return Err(format!("\"{label}\" is already on this board."));
                }
                let (w, h) = (box_width(label).max(320.0), 200.0);
                let (x, y, holder) = spot_for(board, inside.as_deref(), near.as_deref(), w, h)?;
                let id = fresh_id(board, "frame", now_ms);
                board.nodes.push(frame(id, x, y, w, h, label, now_ms));
                if let Some(outer) = holder {
                    grow_to_hold(board, outer, x, y, w, h, now_ms);
                }
                done.push(format!("added the frame \"{label}\""));
            }

            Change::Connect { from, to, label, start, end, dashed } => {
                for word in [start, end].into_iter().flatten() {
                    if end_kind(word).is_none() {
                        return Err(format!("\"{word}\" is not a line end; one of: {}.", end_words()));
                    }
                }
                let a = find_end(board, from)
                    .ok_or_else(|| format!("There is nothing called \"{from}\" here."))?;
                let b = find_end(board, to)
                    .ok_or_else(|| format!("There is nothing called \"{to}\" here."))?;
                let fields = [a, b].iter().any(|&i| board.nodes[i].data.get("field").and_then(Value::as_bool) == Some(true));
                let (out_side, in_side) = if fields {
                    sides_beside(rect_of(&board.nodes[a]), rect_of(&board.nodes[b]))
                } else {
                    sides_between(&board.nodes[a], &board.nodes[b])
                };
                let (source, target) = (board.nodes[a].id.clone(), board.nodes[b].id.clone());
                if board.edges.iter().any(|e| e.source == source && e.target == target) {
                    return Err(format!("\"{from}\" and \"{to}\" are already joined."));
                }
                let mut data = line_data(label.as_deref(), start.as_deref(), end.as_deref(), *dashed);
                if fields {
                    data.remove("sides");
                } else {
                    route_later(&mut data);
                }
                board.edges.push(Link {
                    id: fresh_id(board, "edge", now_ms),
                    rest: Map::new(),
                    source,
                    source_handle: Some(out_side.into()),
                    target,
                    target_handle: Some(in_side.into()),
                    kind: line_kind(fields).into(),
                    data,
                    updated: Some(now_ms),
                });
                done.push(format!("joined \"{from}\" to \"{to}\""));
            }

            Change::Rename { item, label } => {
                let at = find(board, item)
                    .ok_or_else(|| format!("There is nothing called \"{item}\" here."))?;
                board.nodes[at].data.insert("label".into(), json!(label));
                board.nodes[at].updated = Some(now_ms);
                done.push(format!("renamed \"{item}\" to \"{label}\""));
            }

            Change::Remove { item } => {
                let at = find(board, item)
                    .ok_or_else(|| format!("There is nothing called \"{item}\" here."))?;
                let id = board.nodes[at].id.clone();
                board.nodes.remove(at);
                let before = board.edges.len();
                let mut gone = vec![id.clone()];
                board.edges.retain(|e| {
                    let keep = e.source != id && e.target != id;
                    if !keep {
                        gone.push(e.id.clone());
                    }
                    keep
                });
                let lines = before - board.edges.len();
                // Recorded as deleted, so a sync with another device's copy
                // does not bring them back (see `sync::core::board_merge`).
                let deleted = board
                    .metadata
                    .get_or_insert_with(Map::new)
                    .entry("deleted")
                    .or_insert_with(|| json!({}));
                if let Some(map) = deleted.as_object_mut() {
                    for id in gone {
                        map.insert(id, json!(now_ms));
                    }
                }
                done.push(format!("removed \"{item}\" and {lines} line(s) that ended on it"));
            }

            Change::MoveInto { item, frame: zone } => {
                let moving = find(board, item)
                    .ok_or_else(|| format!("There is nothing called \"{item}\" here."))?;
                let frame = find_frame(board, zone)?;
                if moving == frame {
                    return Err("A frame cannot be put inside itself.".into());
                }
                let (w, h) = (board.nodes[moving].width(), board.nodes[moving].height());
                let (x, y) = under_the_last_thing_in(board, frame, w);
                board.nodes[moving].position = Point { x, y };
                board.nodes[moving].updated = Some(now_ms);
                let id = board.nodes[moving].id.clone();
                reroute_lines_of(board, &id, now_ms);
                grow_to_hold(board, frame, x, y, w, h, now_ms);
                done.push(format!("put \"{item}\" inside \"{zone}\""));
            }

            Change::Place { item, side, of } => {
                let moving = find(board, item)
                    .ok_or_else(|| format!("There is nothing called \"{item}\" here."))?;
                let anchor = find(board, of)
                    .ok_or_else(|| format!("There is nothing called \"{of}\" here."))?;
                if moving == anchor {
                    return Err("A box cannot be placed beside itself.".into());
                }
                let (ax, ay, aw, ah) = (
                    board.nodes[anchor].position.x,
                    board.nodes[anchor].position.y,
                    board.nodes[anchor].width(),
                    board.nodes[anchor].height(),
                );
                let (w, h) = (board.nodes[moving].width(), board.nodes[moving].height());
                let to = match side.trim().to_lowercase().as_str() {
                    "right" => (ax + aw + GAP_X, ay),
                    "left" => (ax - w - GAP_X, ay),
                    "above" | "up" => (ax, ay - h - GAP_Y),
                    "below" | "under" | "down" => (ax, ay + ah + GAP_Y),
                    other => return Err(format!("\"{other}\" is not a side: left, right, above, below.")),
                };
                board.nodes[moving].position = Point { x: to.0, y: to.1 };
                board.nodes[moving].updated = Some(now_ms);
                let id = board.nodes[moving].id.clone();
                reroute_lines_of(board, &id, now_ms);
                done.push(format!("put \"{item}\" {side} of \"{of}\""));
            }
        }
    }

    // A frame has to hold what it holds.
    //
    // It did not: three devices added to a site landed below it, the frame
    // stayed the size it was drawn, and the picture showed an empty box with
    // its contents dangling underneath. A frame is the one thing on a board
    // whose size is not a choice somebody made — it is a consequence of what
    // is inside it.
    refit_frames(board, now_ms);

    board.metadata.get_or_insert_with(Map::new).insert(
        "updated_at".into(),
        json!(chrono::Utc::now().to_rfc3339()),
    );
    Ok(done)
}

/// Where something new goes: inside a frame, beside a thing, or below
/// everything — and the frame that is to hold it, if one was named.
fn spot_for(
    board: &Board,
    inside: Option<&str>,
    near: Option<&str>,
    w: f64,
    h: f64,
) -> Result<(f64, f64, Option<usize>), String> {
    if let Some(zone) = inside {
        let frame = find_frame(board, zone)?;
        let (x, y) = under_the_last_thing_in(board, frame, w);
        return Ok((x, y, Some(frame)));
    }
    if let Some(name) = near {
        let anchor = find(board, name).ok_or_else(|| format!("There is nothing called \"{name}\" here."))?;
        let n = &board.nodes[anchor];
        let (x, y) = free_spot(board, n.position.x + n.width() + GAP_X, n.position.y, w, h);
        return Ok((x, y, None));
    }
    let below = board.nodes.iter().map(|n| n.position.y + n.height()).fold(START, f64::max);
    Ok((START, below + GAP_Y, None))
}

/// The frame called this — and only a frame.
///
/// Any named box used to do: asked for swimlanes on a board that had none,
/// Syn added four rectangles called "Swimlane: …" and moved every step into
/// them, each rectangle grew around what it was given, and the four ended up
/// nested inside each other on top of the whole drawing. A box is not a place.
fn find_frame(board: &Board, name: &str) -> Result<usize, String> {
    let wanted = name.trim().to_lowercase();
    let named = |n: &Item| n.label().trim().to_lowercase() == wanted;
    if let Some(at) = (0..board.nodes.len()).find(|&i| named(&board.nodes[i]) && is_frame(board, i)) {
        return Ok(at);
    }
    if find(board, name).is_some() {
        return Err(format!(
            "\"{name}\" is a box, not a frame, and cannot hold anything. Make one with add_frame; \
             for swimlanes, redraw with draw_board and a `lane` on every item."
        ));
    }
    Err(format!("There is no frame called \"{name}\" here."))
}

/// A free row inside a frame, under whatever is already in it.
///
/// Inside, not beside: the caller has said which zone this belongs to, and a
/// zone is a place on the board rather than a word in a label.
fn under_the_last_thing_in(board: &Board, frame: usize, w: f64) -> (f64, f64) {
    let (fx, fy, fw) = (
        board.nodes[frame].position.x,
        board.nodes[frame].position.y,
        board.nodes[frame].width(),
    );
    let bottom = held_by(board, frame)
        .into_iter()
        .map(|i| board.nodes[i].position.y + board.nodes[i].height())
        .fold(fy + FRAME_TOP - GAP_Y, f64::max);

    let x = fx + (fw - w).max(FRAME_PAD * 2.0) / 2.0;
    (x, bottom + GAP_Y)
}

/// Stretch a frame so that one more box is inside it.
///
/// And every frame that held this one grows with it, innermost first. A zone
/// sits inside its site; grown at the bottom, it would otherwise hang out of
/// the site with the box that was put in it. Which frames those are is settled
/// before anything grows, so a frame that grows past its own site cannot be
/// taken for the site's site.
#[allow(clippy::too_many_arguments)]
fn grow_to_hold(board: &mut Board, frame: usize, x: f64, y: f64, w: f64, h: f64, now_ms: i64) {
    let mut chain: Vec<usize> = (0..board.nodes.len())
        .filter(|&i| {
            i != frame
                && is_frame(board, i)
                && board.nodes[i].data.get("query").is_none()
                && encloses(&board.nodes[i], &board.nodes[frame])
        })
        .collect();
    chain.sort_by(|&a, &b| area(&board.nodes[a]).total_cmp(&area(&board.nodes[b])));

    let mut want = (x, y, w, h);
    for at in std::iter::once(frame).chain(chain) {
        let node = &mut board.nodes[at];
        let left = node.position.x.min(want.0 - FRAME_PAD);
        let top = node.position.y.min(want.1 - FRAME_TOP);
        let right = (node.position.x + node.width()).max(want.0 + want.2 + FRAME_PAD);
        let bottom = (node.position.y + node.height()).max(want.1 + want.3 + FRAME_PAD);

        node.position = Point { x: left, y: top };
        node.data.insert("width".into(), json!(right - left));
        node.data.insert("height".into(), json!(bottom - top));
        node.updated = Some(now_ms);
        want = (left, top, right - left, bottom - top);
    }
}

fn area(n: &Item) -> f64 {
    n.width() * n.height()
}

/// Whether `outer` holds `inner` wholly and is the larger of the two — larger,
/// so that two frames drawn the same size do not each hold the other and grow
/// each other for ever.
fn encloses(outer: &Item, inner: &Item) -> bool {
    area(outer) > area(inner)
        && inner.position.x >= outer.position.x
        && inner.position.y >= outer.position.y
        && inner.position.x + inner.width() <= outer.position.x + outer.width()
        && inner.position.y + inner.height() <= outer.position.y + outer.height()
}

/// Which items sit inside this one.
fn held_by(board: &Board, frame: usize) -> Vec<usize> {
    let outer = &board.nodes[frame];
    board
        .nodes
        .iter()
        .enumerate()
        .filter(|(i, n)| {
            *i != frame
                && nameable(n)
                && n.kind != "frame"
                && n.position.x >= outer.position.x
                && n.position.y >= outer.position.y
                && n.position.x + n.width() <= outer.position.x + outer.width()
                && n.position.y + n.height() <= outer.position.y + outer.height()
        })
        .map(|(i, _)| i)
        .collect()
}

/// Grow every frame around what it holds.
///
/// Membership is decided before anything moves, because a frame that has been
/// outgrown no longer contains its own contents — which is exactly the state
/// this repairs. Anything that was inside it, or was put there by name, counts.
///
/// Only frames change size. Nothing inside one moves: the arrangement is the
/// person's, and this is the board catching up with it.
fn refit_frames(board: &mut Board, now_ms: i64) {
    // The app's own frames hold what is in them however few; a live frame
    // lays out its own cards and is left to do so.
    let own_frame = |n: &Item| n.kind == "frame" && n.data.get("query").is_none();
    let frames: Vec<(usize, usize)> = (0..board.nodes.len())
        .filter(|i| own_frame(&board.nodes[*i]) || (board.nodes[*i].kind == "shape" && held_by(board, *i).len() >= 2))
        .map(|i| (i, if own_frame(&board.nodes[i]) { 1 } else { 2 }))
        .collect();

    for (frame, least) in frames {
        let held = held_by(board, frame);
        if held.len() < least {
            continue;
        }
        let (mut left, mut top, mut right, mut bottom) =
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for i in held {
            let n = &board.nodes[i];
            left = left.min(n.position.x);
            top = top.min(n.position.y);
            right = right.max(n.position.x + n.width());
            bottom = bottom.max(n.position.y + n.height());
        }

        let x = (left - FRAME_PAD).min(board.nodes[frame].position.x);
        let y = (top - FRAME_TOP).min(board.nodes[frame].position.y);
        let w = (right + FRAME_PAD - x).max(board.nodes[frame].width());
        let h = (bottom + FRAME_PAD - y).max(board.nodes[frame].height());

        let node = &mut board.nodes[frame];
        if node.position.x != x || node.position.y != y || node.width() != w || node.height() != h {
            node.position = Point { x, y };
            node.data.insert("width".into(), json!(w));
            node.data.insert("height".into(), json!(h));
            node.updated = Some(now_ms);
        }
    }
}

/// What can be named on a board: anything with words on it, except a comment
/// (which is about something, not a thing) and freehand ink.
fn nameable(n: &Item) -> bool {
    !matches!(n.kind.as_str(), "comment" | "stroke")
}

/// The thing somebody means when they say a name: a box first, then a frame,
/// a sticky note, a card or a piece of writing. Only boxes used to answer, so
/// a frame Syn had made itself, by grouping, could not be put anything into.
fn find(board: &Board, label: &str) -> Option<usize> {
    let wanted = label.trim().to_lowercase();
    let named = |n: &Item| nameable(n) && n.label().trim().to_lowercase() == wanted;
    board
        .nodes
        .iter()
        .position(|n| n.kind == "shape" && named(n))
        .or_else(|| board.nodes.iter().position(named))
}

/// What a line's end names: a thing by its label, or a table's field —
/// `orders.user_id`, the row in the `orders` frame whose field is `user_id`.
fn find_end(board: &Board, name: &str) -> Option<usize> {
    if let Some(at) = find(board, name) {
        return Some(at);
    }
    let (table, field) = name.trim().rsplit_once('.')?;
    let frame = find(board, table).filter(|&f| board.nodes[f].kind == "frame")?;
    let field = field.trim().to_lowercase();
    held_by(board, frame).into_iter().find(|&i| field_name(board.nodes[i].label()) == field)
}

/// Whether this item is a frame: the app's own, or a box that holds others.
fn is_frame(board: &Board, at: usize) -> bool {
    let n = &board.nodes[at];
    n.kind == "frame" || (n.kind == "shape" && held_by(board, at).len() >= 2)
}

/// A place near where it was asked for, that nothing is already sitting on.
fn free_spot(board: &Board, x: f64, y: f64, w: f64, h: f64) -> (f64, f64) {
    // A frame is somewhere to be, not something in the way: what sits in one
    // is what is in the way. (Every frame used to count as in the way, so a
    // box put next to something in a frame was pushed out below it.)
    let frames: Vec<bool> = (0..board.nodes.len()).map(|i| is_frame(board, i)).collect();
    let clashes = |x: f64, y: f64| {
        board.nodes.iter().enumerate().any(|(i, n)| {
            nameable(n)
                && !frames[i]
                && x < n.position.x + n.width()
                && n.position.x < x + w
                && y < n.position.y + n.height()
                && n.position.y < y + h
        })
    };
    let mut at_y = y;
    for _ in 0..40 {
        if !clashes(x, at_y) {
            return (x, at_y);
        }
        at_y += h + GAP_Y / 2.0;
    }
    (x, at_y)
}


#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_757_000_000_000;

    /// The app's own frames and sticky notes are read as what they are: a
    /// frame names what sits in it, and a line to a sticky note says where it
    /// goes rather than that its end has gone.
    #[test]
    fn frames_and_sticky_notes_are_described() {
        let board: Board = serde_json::from_value(json!({
            "title": "Plan",
            "nodes": [
                { "id": "f", "type": "frame", "position": { "x": 0.0, "y": 0.0 },
                  "data": { "label": "Sprint", "width": 600.0, "height": 400.0 } },
                { "id": "a", "type": "shape", "position": { "x": 40.0, "y": 40.0 },
                  "data": { "label": "Design" } },
                { "id": "s", "type": "sticky", "position": { "x": 300.0, "y": 60.0 },
                  "data": { "label": "Ask legal", "width": 200.0, "height": 200.0 } }
            ],
            "edges": [ { "id": "e", "source": "a", "target": "s" } ]
        }))
        .expect("a board");

        let said = describe(&board);
        assert!(said.contains("- Sprint (2 inside): Design, Ask legal"), "{said}");
        assert!(said.contains("1 loose pieces of writing"), "{said}");
        assert!(said.contains("- Design → Ask legal"), "{said}");
    }

    /// Comments are read, with what they are about.
    #[test]
    fn comments_are_read_with_what_they_are_about() {
        let board: Board = serde_json::from_value(json!({
            "title": "Plan",
            "nodes": [
                { "id": "a", "type": "shape", "position": { "x": 0.0, "y": 0.0 }, "data": { "label": "Launch" } },
                { "id": "c", "type": "comment", "position": { "x": 120.0, "y": 0.0 }, "data": { "label": "Move to May?", "on": "a" } }
            ]
        }))
        .expect("a board");
        let said = describe(&board);
        assert!(said.contains("- Move to May? (on \"Launch\")"), "{said}");
    }

    /// An edit keeps what it does not understand, on items and lines too.
    #[test]
    fn an_edit_keeps_fields_it_does_not_know() {
        let mut board: Board = serde_json::from_value(json!({
            "title": "Plan",
            "nodes": [
                { "id": "a", "type": "shape", "position": { "x": 0.0, "y": 0.0 }, "data": { "label": "A" }, "locked": true },
                { "id": "b", "type": "shape", "position": { "x": 300.0, "y": 0.0 }, "data": { "label": "B" } }
            ],
            "edges": [ { "id": "e", "source": "a", "target": "b", "zIndex": 1000000 } ]
        }))
        .expect("a board");
        apply(&mut board, &[Change::Rename { item: "A".into(), label: "Alpha".into() }], NOW).expect("renamed");
        let back = serde_json::to_value(&board).unwrap();
        assert_eq!(back["nodes"][0]["locked"], json!(true));
        assert_eq!(back["edges"][0]["zIndex"], json!(1000000));
    }

    /// Adding, removing and adding again in one call gives two different ids.
    #[test]
    fn ids_stay_unique_within_one_call() {
        let mut board: Board = serde_json::from_value(json!({
            "title": "Plan",
            "nodes": [ { "id": "x", "type": "shape", "position": { "x": 0.0, "y": 0.0 }, "data": { "label": "X" } } ]
        }))
        .expect("a board");
        let changes = vec![
            Change::Add { label: "One".into(), shape: None, near: None, inside: None, fields: Vec::new() },
            Change::Remove { item: "X".into() },
            Change::Add { label: "Two".into(), shape: None, near: None, inside: None, fields: Vec::new() },
        ];
        apply(&mut board, &changes, NOW).expect("applied");
        let ids: std::collections::HashSet<_> = board.nodes.iter().map(|n| n.id.clone()).collect();
        assert_eq!(ids.len(), board.nodes.len(), "{:?}", board.nodes.iter().map(|n| &n.id).collect::<Vec<_>>());
    }

    fn sketch(json: Value) -> Sketch {
        serde_json::from_value(json).expect("a sketch")
    }

    /// The drawing this was built for: something upstream, feeding two sites
    /// that mirror each other.
    fn two_sites() -> Sketch {
        sketch(json!({
            "items": [
                { "label": "TCTV" },
                { "label": "FW Checkpoint" },
                { "label": "SW Core 1", "group": "DC1" },
                { "label": "F5 TPZ 1", "group": "DC1" },
                { "label": "Kong 1", "group": "DC1" },
                { "label": "SW Core 2", "group": "DC2" },
                { "label": "F5 TPZ 2", "group": "DC2" },
                { "label": "Kong 2", "group": "DC2" }
            ],
            "links": [
                { "from": "TCTV", "to": "FW Checkpoint", "label": "MPLS" },
                { "from": "FW Checkpoint", "to": "SW Core 1" },
                { "from": "FW Checkpoint", "to": "SW Core 2" },
                { "from": "SW Core 1", "to": "F5 TPZ 1" },
                { "from": "F5 TPZ 1", "to": "Kong 1" },
                { "from": "SW Core 2", "to": "F5 TPZ 2" },
                { "from": "F5 TPZ 2", "to": "Kong 2" }
            ]
        }))
    }

    fn at(board: &Board, label: &str) -> Item {
        board
            .nodes
            .iter()
            .find(|n| n.label() == label)
            .unwrap_or_else(|| panic!("`{label}` is on the board"))
            .clone()
    }

    /// Sites side by side, and each chain in the order it is read.
    ///
    /// This is the whole argument for laying boards out here rather than
    /// reusing the diagram renderer: asked for two data centres, Mermaid
    /// stacked them — one 129 pixels above the other — and there was no way to
    /// say otherwise. A group is a column, and columns stand next to each
    /// other.
    #[test]
    fn the_two_sites_come_out_side_by_side() {
        let board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");

        let dc1 = at(&board, "DC1");
        let dc2 = at(&board, "DC2");
        assert!(dc2.position.x > dc1.position.x + dc1.width(), "the frames overlap");
        assert_eq!(dc1.position.y, dc2.position.y, "and they start at the same height");

        // Inside a column, below what feeds it.
        let core = at(&board, "SW Core 1");
        let f5 = at(&board, "F5 TPZ 1");
        let kong = at(&board, "Kong 1");
        assert!(core.position.y < f5.position.y, "the core is above the F5");
        assert!(f5.position.y < kong.position.y, "and the F5 above Kong");

        // The upstream chain is above both sites.
        assert!(at(&board, "TCTV").position.y < dc1.position.y);
        assert!(at(&board, "FW Checkpoint").position.y < dc1.position.y);
    }

    /// The drawing Syn was actually asked for, and what it came out as.
    ///
    /// Forty-four boxes, two sites, fifty-two lines — kept as a fixture
    /// because it is the shape that showed both defects at once. Ranked by a
    /// loop of relaxations, its cycles pushed ranks to 82, 124, 127; stacked a
    /// box to a row, each site came out 2,692 pixels tall and 354 wide. Two
    /// towers with a wire between them, and the person who asked for it said
    /// it was ugly, which it was.
    #[test]
    fn a_real_drawing_comes_out_in_layers_rather_than_a_tower() {
        let sketch: Sketch =
            serde_json::from_str(include_str!("testdata/board-sketch.json")).expect("the sketch");
        let board = draw("Kiến trúc DC1 - DC2", &sketch, NOW).expect("drawn");

        let frames: Vec<&Item> = board
            .nodes
            .iter()
            .filter(|n| n.kind == "frame")
            .collect();
        assert_eq!(frames.len(), 2, "a frame per site");

        // Twenty-one boxes come out as a block somebody can read across:
        // 1,205 × 1,284, eleven rows, never more than four to a row. Before
        // this it was 354 × 2,692 — a tower — and the first attempt at fixing
        // it went the other way, to 2,510 wide with eighteen boxes on one line.
        for frame in &frames {
            let (w, h) = (frame.width(), frame.height());
            assert!(h < w * 2.0, "`{}` is {w}×{h} — a tower again", frame.label());
            assert!(w < h * 2.0, "`{}` is {w}×{h} — a tower on its side", frame.label());
        }

        // Boxes fed by the same thing share a row.
        let by_y = |y: f64| {
            board
                .nodes
                .iter()
                .filter(move |n| {
                    n.data.get("color").and_then(Value::as_str) == Some(BOX_COLOUR)
                        && (n.position.y - y).abs() < 1.0
                })
                .count()
        };
        let rows: std::collections::BTreeSet<i64> = board
            .nodes
            .iter()
            .filter(|n| n.data.get("color").and_then(Value::as_str) == Some(BOX_COLOUR))
            .map(|n| n.position.y as i64)
            .collect();
        assert!(
            rows.iter().any(|y| by_y(*y as f64) >= 4),
            "no row holds more than three boxes, so nothing was laid side by side"
        );
        // Both sites share every row, so a row holds at most PER_ROW from each.
        for y in &rows {
            let across = by_y(*y as f64);
            assert!(across <= PER_ROW * 2, "a row of {across} boxes is a line nobody reads");
        }

        // And still nothing on top of anything else.
        let boxes: Vec<&Item> = board
            .nodes
            .iter()
            .filter(|n| n.data.get("color").and_then(Value::as_str) == Some(BOX_COLOUR))
            .collect();
        for (i, a) in boxes.iter().enumerate() {
            for b in boxes.iter().skip(i + 1) {
                let apart = a.position.x + a.width() <= b.position.x
                    || b.position.x + b.width() <= a.position.x
                    || a.position.y + a.height() <= b.position.y
                    || b.position.y + b.height() <= a.position.y;
                assert!(apart, "`{}` sits on `{}`", a.label(), b.label());
            }
        }
    }

    /// A zone inside a site, which is what these drawings are made of.
    ///
    /// Asked to put three devices in a Network Hub, the assistant put them
    /// *under* it, was told that was wrong, and settled for renaming them
    /// `[Network Hub] SW WAN` — a caption pretending to be a place, because
    /// there was no way to say "inside".
    #[test]
    fn a_zone_inside_a_site_is_a_frame_inside_a_frame() {
        let nested = sketch(json!({
            "items": [
                { "label": "SW WAN 1", "group": "DC 1/Network Hub" },
                { "label": "FW CheckPoint 1", "group": "DC 1/Network Hub" },
                { "label": "TPZ 1", "group": "DC 1" },
                { "label": "SW WAN 2", "group": "DC 2/Network Hub" },
                { "label": "FW CheckPoint 2", "group": "DC 2/Network Hub" },
                { "label": "TPZ 2", "group": "DC 2" }
            ],
            "links": [
                { "from": "SW WAN 1", "to": "FW CheckPoint 1" },
                { "from": "FW CheckPoint 1", "to": "TPZ 1" },
                { "from": "SW WAN 2", "to": "FW CheckPoint 2" },
                { "from": "FW CheckPoint 2", "to": "TPZ 2" }
            ]
        }));
        let board = draw("Hai DC", &nested, NOW).expect("drawn");

        let inside = |outer: &Item, inner: &Item| {
            inner.position.x >= outer.position.x
                && inner.position.y >= outer.position.y
                && inner.position.x + inner.width() <= outer.position.x + outer.width()
                && inner.position.y + inner.height() <= outer.position.y + outer.height()
        };

        let site = at(&board, "DC 1");
        let hubs: Vec<Item> = board
            .nodes
            .iter()
            .filter(|n| n.label() == "Network Hub")
            .cloned()
            .collect();
        assert_eq!(hubs.len(), 2, "a hub per site");

        let hub = hubs.iter().find(|h| inside(&site, h)).expect("the hub is inside its site");
        for label in ["SW WAN 1", "FW CheckPoint 1"] {
            assert!(inside(hub, &at(&board, label)), "`{label}` is not in the hub");
        }
        assert!(!inside(hub, &at(&board, "TPZ 1")), "the TPZ is not part of the hub");
        assert!(inside(&site, &at(&board, "TPZ 1")), "but it is part of the site");
    }

    // ── Loops, lines, shapes: what the first real drawings got wrong ─

    fn real(name: &str) -> Sketch {
        let text = match name {
            "flowchart" => include_str!("testdata/board-flowchart-loop.json"),
            _ => include_str!("testdata/board-swimlane.json"),
        };
        serde_json::from_str(text).expect("the sketch")
    }

    fn centre(item: &Item) -> (f64, f64) {
        (item.position.x + item.width() / 2.0, item.position.y + item.height() / 2.0)
    }

    /// A Change Request flow with a loop — information incomplete, add it,
    /// check again — as Syn drew it on 2026-11-13. The loop used to scramble
    /// the order: "add information" landed between "submit" and the check it
    /// returns to, and the line from "submit" ran through it.
    #[test]
    fn a_loop_back_does_not_reorder_the_steps() {
        let board = draw("Change Request", &real("flowchart"), NOW).expect("drawn");
        let y = |label: &str| centre(&at(&board, label)).1;
        let order = [
            "Bắt đầu",
            "Gửi Change Request",
            "Thông tin đầy đủ?",
            "Ghi nhận và kiểm tra thông tin",
            "Đánh giá tác động và rủi ro",
            "Phê duyệt Change?",
            "Lập kế hoạch và lịch triển khai",
            "Triển khai Change",
            "Triển khai thành công?",
            "Kiểm tra và xác nhận kết quả",
            "Kết thúc",
        ];
        for pair in order.windows(2) {
            assert!(y(pair[0]) < y(pair[1]), "`{}` is not above `{}`", pair[0], pair[1]);
        }
        // The step the loop goes through comes after the question that sends
        // it there, not before it.
        assert!(y("Bổ sung thông tin") > y("Thông tin đầy đủ?"));
        // The two answers to a question sit side by side.
        assert_eq!(y("Bổ sung thông tin"), y("Ghi nhận và kiểm tra thông tin"));
    }

    /// The same flow in lanes, as Syn drew it: the Change Manager's steps read
    /// left to right in the order they happen.
    #[test]
    fn a_loop_back_does_not_reorder_a_lane() {
        let board = draw("Change Request", &real("swimlane"), NOW).expect("drawn");
        let x = |label: &str| centre(&at(&board, label)).0;
        let order = [
            "Gửi Change Request",
            "Ghi nhận và kiểm tra thông tin",
            "Thông tin đầy đủ?",
            "Đánh giá tác động và rủi ro",
            "Phê duyệt Change?",
            "Lập kế hoạch và lịch triển khai",
            "Triển khai Change",
            "Kiểm tra và xác nhận kết quả",
            "Kết thúc - hoàn tất",
        ];
        for pair in order.windows(2) {
            assert!(x(pair[0]) < x(pair[1]), "`{}` is not left of `{}`", pair[0], pair[1]);
        }
        // Each lane's steps share a line, whatever their shapes.
        let y = |label: &str| centre(&at(&board, label)).1;
        assert_eq!(y("Ghi nhận và kiểm tra thông tin"), y("Thông tin đầy đủ?"));
        assert_eq!(y("Lập kế hoạch và lịch triển khai"), y("Triển khai thành công?"));
    }

    /// Lines between boxes are stepped and follow the boxes, which is what the
    /// app routes around whatever is in the way.
    #[test]
    fn lines_between_boxes_go_around_what_is_in_the_way() {
        let board = draw("Change Request", &real("flowchart"), NOW).expect("drawn");
        for line in &board.edges {
            assert_eq!(line.kind, "step");
            assert_eq!(line.data["sides"], "auto");
            assert_eq!(line.data["routing"], "pending", "the app routes it on open");
        }
    }

    /// Lines in time order, and lines to a field, are where they belong
    /// already: nothing asks the app to route them.
    #[test]
    fn only_lines_between_boxes_wait_to_be_routed() {
        let talk = sketch(json!({
            "items": [{ "label": "App" }, { "label": "Kong" }],
            "links": [{ "from": "App", "to": "Kong", "order": 1 }]
        }));
        let schema = sketch(json!({
            "items": [{ "label": "users", "fields": ["id"] }, { "label": "orders", "fields": ["user_id"] }],
            "links": [{ "from": "orders.user_id", "to": "users.id" }]
        }));
        for (name, sketch) in [("sequence", talk), ("tables", schema)] {
            let board = draw("t", &sketch, NOW).expect("drawn");
            assert!(board.edges.iter().all(|e| e.data.get("routing").is_none()), "{name}");
        }
    }

    /// A line added by an edit waits to be routed; a box moved by one has its
    /// lines' old bends taken off and routed again, and no other line changes.
    #[test]
    fn an_edit_routes_the_lines_it_touches() {
        let mut board = draw("Change Request", &real("flowchart"), NOW).expect("drawn");
        for line in &mut board.edges {
            line.data.remove("routing");
            line.data.insert("waypoints".into(), json!([{ "x": 1, "y": 2 }]));
            line.data.insert("sourceAt".into(), json!({ "side": "bottom", "t": 0.3 }));
            line.data.insert("routed".into(), json!(true));
        }
        let moved = at(&board, "Kết thúc").id;
        apply(
            &mut board,
            &changes(json!([
                { "op": "add", "label": "Lưu hồ sơ" },
                { "op": "connect", "from": "Kết thúc", "to": "Lưu hồ sơ" },
                { "op": "place", "item": "Kết thúc", "side": "right", "of": "Bắt đầu" }
            ])),
            NOW,
        )
        .expect("applied");
        for line in &board.edges {
            let touches = line.source == moved || line.target == moved;
            assert_eq!(line.data.get("routing").is_some(), touches, "{} → {}", line.source, line.target);
            assert_eq!(line.data.get("waypoints").is_none(), touches);
            assert_eq!(line.data.get("sourceAt").is_none(), touches);
            assert_eq!(line.data.get("routed").is_none(), touches);
        }
    }

    /// A chain of boxes of different widths, one under the other, is lined
    /// up on their middles, so the lines down it run straight.
    #[test]
    fn a_chain_is_lined_up_on_its_middles() {
        let board = draw("Change Request", &real("flowchart"), NOW).expect("drawn");
        let x = |label: &str| centre(&at(&board, label)).0;
        for pair in [["Bắt đầu", "Gửi Change Request"], ["Gửi Change Request", "Thông tin đầy đủ?"], ["Kiểm tra và xác nhận kết quả", "Kết thúc"]] {
            assert!((x(pair[0]) - x(pair[1])).abs() < 0.5, "`{}` and `{}` are not in line", pair[0], pair[1]);
        }
    }

    /// A diamond's words fit in its middle, so it is drawn wider and taller
    /// than a box with the same words; a line beside it still runs level.
    #[test]
    fn a_decision_has_room_for_its_question() {
        let board = draw(
            "t",
            &sketch(json!({
                "items": [{ "label": "Thông tin đầy đủ?", "shape": "diamond" }, { "label": "Thông tin đầy đủ!" }],
                "links": []
            })),
            NOW,
        )
        .expect("drawn");
        let (diamond, plain) = (at(&board, "Thông tin đầy đủ?"), at(&board, "Thông tin đầy đủ!"));
        assert!(diamond.width() >= plain.width() * 1.5 && diamond.height() > plain.height());
        assert_eq!(centre(&diamond).1, centre(&plain).1, "side by side, on one line");
    }

    /// What Syn did on 2026-11-13 when asked for swimlanes on a board that had
    /// none: four boxes called "Swimlane: …", every step moved into them. A box
    /// cannot hold anything, and saying so points at what can.
    #[test]
    fn only_a_frame_can_hold_things() {
        let mut board = draw("Change Request", &real("flowchart"), NOW).expect("drawn");
        let err = apply(
            &mut board,
            &changes(json!([
                { "op": "add", "label": "Swimlane: Người yêu cầu", "shape": "rectangle" },
                { "op": "move_into", "item": "Bắt đầu", "frame": "Swimlane: Người yêu cầu" }
            ])),
            NOW,
        )
        .unwrap_err();
        assert!(err.contains("not a frame") && err.contains("lane"), "{err}");

        apply(
            &mut board,
            &changes(json!([
                { "op": "add_frame", "label": "CAB", "near": "Phê duyệt Change?" },
                { "op": "move_into", "item": "Phê duyệt Change?", "frame": "CAB" }
            ])),
            NOW,
        )
        .expect("a frame holds it");
        let frame = at(&board, "CAB");
        assert_eq!(frame.kind, "frame");
        assert!(inside(&frame, &at(&board, "Phê duyệt Change?")));
    }

    // ── Lanes, sequences, tables ─────────────────────────────

    fn inside(outer: &Item, inner: &Item) -> bool {
        inner.position.x >= outer.position.x
            && inner.position.y >= outer.position.y
            && inner.position.x + inner.width() <= outer.position.x + outer.width()
            && inner.position.y + inner.height() <= outer.position.y + outer.height()
    }

    fn messages(board: &Board) -> Vec<&Link> {
        board.edges.iter().filter(|e| e.data.get("lifeline").is_none()).collect()
    }

    /// A process between two teams: a band per team, and the steps read left
    /// to right in the order they happen, whichever band they are in.
    #[test]
    fn lanes_are_bands_and_the_flow_goes_across() {
        let flow = sketch(json!({
            "items": [
                { "label": "Gửi đề nghị", "lane": "Nhân viên" },
                { "label": "Duyệt", "lane": "Kế toán" },
                { "label": "Chi tiền", "lane": "Kế toán" },
                { "label": "Nhận tiền", "lane": "Nhân viên" }
            ],
            "links": [
                { "from": "Gửi đề nghị", "to": "Duyệt" },
                { "from": "Duyệt", "to": "Chi tiền" },
                { "from": "Chi tiền", "to": "Nhận tiền" }
            ]
        }));
        let board = draw("Tạm ứng", &flow, NOW).expect("drawn");

        let staff = at(&board, "Nhân viên");
        let accounts = at(&board, "Kế toán");
        assert_eq!(staff.kind, "frame");
        assert!(staff.position.y + staff.height() <= accounts.position.y, "the lanes overlap");
        for (lane, label) in [(&staff, "Gửi đề nghị"), (&staff, "Nhận tiền"), (&accounts, "Duyệt"), (&accounts, "Chi tiền")] {
            assert!(inside(lane, &at(&board, label)), "`{label}` is not in its lane");
        }
        let x = |label: &str| at(&board, label).position.x;
        assert!(x("Gửi đề nghị") < x("Duyệt") && x("Duyệt") < x("Chi tiền") && x("Chi tiền") < x("Nhận tiền"));
    }

    #[test]
    fn lanes_are_all_or_nothing_and_not_with_groups() {
        let some = sketch(json!({ "items": [{ "label": "A", "lane": "X" }, { "label": "B" }] }));
        let err = draw("t", &some, NOW).unwrap_err();
        assert!(err.contains("\"B\" has no lane"), "{err}");

        let both = sketch(json!({ "items": [{ "label": "A", "lane": "X", "group": "G" }] }));
        let err = draw("t", &both, NOW).unwrap_err();
        assert!(err.contains("both a lane and a group"), "{err}");
    }

    /// Messages in time: participants side by side, messages read from the
    /// top in their `order` — not the order they were written in.
    #[test]
    fn a_sequence_reads_down_in_order() {
        let talk = sketch(json!({
            "items": [{ "label": "App" }, { "label": "Kong" }, { "label": "DB" }],
            "links": [
                { "from": "Kong", "to": "DB", "label": "INSERT", "order": 2 },
                { "from": "App", "to": "Kong", "label": "POST /order", "order": 1 },
                { "from": "DB", "to": "App", "label": "OK", "order": 3, "dashed": true, "end": "open" }
            ]
        }));
        let board = draw("Đặt hàng", &talk, NOW).expect("drawn");

        let (app, kong, db) = (at(&board, "App"), at(&board, "Kong"), at(&board, "DB"));
        assert!(app.position.x < kong.position.x && kong.position.x < db.position.x);
        assert_eq!(app.position.y, db.position.y, "the heads stand in a row");

        let said = messages(&board);
        let words: Vec<&str> = said.iter().map(|e| e.data["label"].as_str().unwrap()).collect();
        assert_eq!(words, ["POST /order", "INSERT", "OK"]);

        // Each message lower than the one before, and on the lifelines of
        // the two it is between.
        let by_id = |id: &str| board.nodes.iter().find(|n| n.id == id).unwrap();
        let centre = |n: &Item| n.position.x + n.width() / 2.0;
        let mut last_y = f64::MIN;
        for (e, (from, to)) in said.iter().zip([(&app, &kong), (&kong, &db), (&db, &app)]) {
            let (a, b) = (by_id(&e.source), by_id(&e.target));
            assert!((centre(a) - centre(from)).abs() < 0.5 && (centre(b) - centre(to)).abs() < 0.5);
            assert!(a.position.y > last_y, "`{}` is not below the message before it", e.data["label"]);
            last_y = a.position.y;
        }
        assert_eq!(said[2].data["dashStyle"], "dashed");
        assert_eq!(said[2].data["markerEnd"], "arrow-open");

        let read = describe(&board);
        assert!(read.contains("3 boxes, 3 lines"), "the scaffolding is not counted: {read}");
        assert!(read.contains("- App → Kong (POST /order)\n"), "{read}");
        assert!(read.contains("- DB → App (OK) [none – arrow-open] dashed"), "{read}");
        assert!(!read.contains("(unnamed)"), "{read}");
    }

    #[test]
    fn a_participant_can_answer_itself() {
        let talk = sketch(json!({
            "items": [{ "label": "Worker" }],
            "links": [{ "from": "Worker", "to": "Worker", "label": "retry", "order": 1 }]
        }));
        let board = draw("Lặp", &talk, NOW).expect("drawn");
        let said = messages(&board);
        assert_eq!(said.len(), 1);
        assert_eq!((said[0].source_handle.as_deref(), said[0].target_handle.as_deref()), (Some("right"), Some("right")));
        assert_ne!(said[0].source, said[0].target, "out of one bar, into the one below");
    }

    #[test]
    fn a_sequence_says_what_it_cannot_hold() {
        let tabled = sketch(json!({
            "items": [{ "label": "App", "fields": ["id"] }, { "label": "DB" }],
            "links": [{ "from": "App", "to": "DB", "order": 1 }]
        }));
        assert!(draw("t", &tabled, NOW).unwrap_err().contains("participant"));
    }

    /// Tables are frames with a row per field, and a line between fields
    /// carries its cardinality on both ends.
    #[test]
    fn tables_have_rows_and_lines_end_on_fields() {
        let schema = sketch(json!({
            "items": [
                { "label": "users", "fields": ["id: uuid PK", "email: text"] },
                { "label": "orders", "fields": ["id: uuid PK", "user_id: uuid FK", "total: numeric"] }
            ],
            "links": [{ "from": "orders.user_id", "to": "users.id", "start": "many", "end": "one" }]
        }));
        let board = draw("CSDL", &schema, NOW).expect("drawn");

        let users = at(&board, "users");
        assert_eq!(users.kind, "frame");
        for row in ["id: uuid PK", "email: text"] {
            assert!(board.nodes.iter().any(|n| n.label() == row && inside(&users, n)), "`{row}` is not in users");
        }
        let line = &board.edges[0];
        let row = |id: &str| board.nodes.iter().find(|n| n.id == id).unwrap().label().to_string();
        assert_eq!(row(&line.source), "user_id: uuid FK");
        assert_eq!(row(&line.target), "id: uuid PK");
        assert_eq!((line.data["markerStart"].as_str(), line.data["markerEnd"].as_str()), (Some("er-many"), Some("er-one")));
        // Out of the side of the row, not through the rows below it.
        assert_eq!((line.source_handle.as_deref(), line.target_handle.as_deref()), (Some("right"), Some("right")));
        assert!(line.data.get("sides").is_none(), "the app would turn it back to top and bottom");

        // A table that points at another is above it, as a box would be.
        assert!(at(&board, "orders").position.y < users.position.y);
        // And nothing sits on anything: the tables are taller than boxes.
        let orders = at(&board, "orders");
        assert!(orders.position.y + orders.height() <= users.position.y, "the tables overlap");

        let read = describe(&board);
        assert!(read.contains("users (2 inside): id: uuid PK, email: text"), "{read}");
        assert!(read.contains("[er-many – er-one]"), "{read}");
    }

    #[test]
    fn an_end_that_is_not_one_is_refused_with_the_ones_that_are() {
        let bad = sketch(json!({
            "items": [{ "label": "A" }, { "label": "B" }],
            "links": [{ "from": "A", "to": "B", "end": "crowfoot" }]
        }));
        let err = draw("t", &bad, NOW).unwrap_err();
        assert!(err.contains("crowfoot") && err.contains("one-many"), "{err}");
    }

    /// A `row` and a `col` put a box where the model said, in place of where
    /// its rank would have.
    #[test]
    fn row_and_col_say_where_a_box_goes() {
        let chain = sketch(json!({
            "items": [{ "label": "A" }, { "label": "B" }, { "label": "C", "row": 0, "col": 0 }],
            "links": [{ "from": "A", "to": "B" }, { "from": "B", "to": "C" }]
        }));
        let board = draw("t", &chain, NOW).expect("drawn");
        let (a, c) = (at(&board, "A"), at(&board, "C"));
        assert_eq!(a.position.y, c.position.y, "C was put in the first row");
        assert!(c.position.x < a.position.x, "and first along it");
    }

    #[test]
    fn an_edit_can_add_a_table_and_join_its_fields() {
        let mut board = draw("CSDL", &sketch(json!({ "items": [{ "label": "users", "fields": ["id: uuid PK"] }] })), NOW)
            .expect("drawn");
        apply(
            &mut board,
            &changes(json!([
                { "op": "add", "label": "orders", "fields": ["id: uuid PK", "user_id: uuid FK"], "near": "users" },
                { "op": "connect", "from": "orders.user_id", "to": "users.id", "start": "zero-many", "end": "one" }
            ])),
            NOW,
        )
        .expect("applied");

        let orders = at(&board, "orders");
        assert_eq!(orders.kind, "frame");
        assert!(board.nodes.iter().any(|n| n.label() == "user_id: uuid FK" && inside(&orders, n)));
        let line = board.edges.last().unwrap();
        assert_eq!(line.data["markerStart"], "er-zero-many");
        let ids: std::collections::HashSet<&str> = board.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), board.nodes.len(), "every part of the table has an id of its own");

        let err = apply(&mut board, &changes(json!([{ "op": "connect", "from": "orders", "to": "users", "end": "x" }])), NOW)
            .unwrap_err();
        assert!(err.contains("not a line end"), "{err}");
    }

    /// A group is the app's own frame, the site drawn before its zones, and
    /// a line says which way it goes and follows the boxes it joins.
    #[test]
    fn groups_are_frames_and_lines_have_a_direction() {
        let nested = sketch(json!({
            "items": [
                { "label": "SW WAN 1", "group": "DC 1/Network Hub" },
                { "label": "TPZ 1", "group": "DC 1" }
            ],
            "links": [{ "from": "SW WAN 1", "to": "TPZ 1", "label": "LAN" }]
        }));
        let board = draw("Một DC", &nested, NOW).expect("drawn");

        let site = board.nodes.iter().position(|n| n.label() == "DC 1").expect("site");
        let hub = board.nodes.iter().position(|n| n.label() == "Network Hub").expect("hub");
        assert_eq!(board.nodes[site].kind, "frame");
        assert_eq!(board.nodes[hub].kind, "frame");
        assert!(!board.nodes[site].data.contains_key("shapeType"), "a frame is not a shape");
        assert!(site < hub, "a site listed after its zone is laid over it");

        let line = &board.edges[0];
        assert_eq!(line.data["markerEnd"], "arrow");
        assert_eq!(line.data["sides"], "auto");
        assert_eq!(line.data["label"], "LAN");
    }

    /// A line that spells a label in another case is the same line, for the
    /// layout as much as for the drawing.
    #[test]
    fn a_link_in_another_case_still_ranks() {
        let chain = sketch(json!({
            "items": [{ "label": "Kong" }, { "label": "App PSS" }],
            "links": [{ "from": "kong", "to": "app pss" }]
        }));
        let board = draw("Chuỗi", &chain, NOW).expect("drawn");
        assert_eq!(board.edges.len(), 1);
        assert!(
            at(&board, "Kong").position.y < at(&board, "App PSS").position.y,
            "what Kong feeds sits below it"
        );
    }

    /// Put into a zone, a box grows the zone — and the site the zone is in,
    /// which used to stay the size it was drawn with the zone hanging out of it.
    #[test]
    fn adding_to_a_zone_grows_its_site_as_well() {
        let nested = sketch(json!({
            "items": [
                { "label": "SW WAN 1", "group": "DC 1/Network Hub" },
                { "label": "FW 1", "group": "DC 1/Network Hub" },
                { "label": "TPZ 1", "group": "DC 1" }
            ],
            "links": [{ "from": "SW WAN 1", "to": "FW 1" }]
        }));
        let mut board = draw("Một DC", &nested, NOW).expect("drawn");
        apply(
            &mut board,
            &changes(json!([{ "op": "add", "label": "IPS 1", "inside": "Network Hub" }])),
            NOW,
        )
        .expect("added");

        let contains = |outer: &Item, inner: &Item| {
            inner.position.x >= outer.position.x
                && inner.position.y >= outer.position.y
                && inner.position.x + inner.width() <= outer.position.x + outer.width()
                && inner.position.y + inner.height() <= outer.position.y + outer.height()
        };
        let (site, hub, ips) = (at(&board, "DC 1"), at(&board, "Network Hub"), at(&board, "IPS 1"));
        assert!(contains(&hub, &ips), "the new box is in the zone");
        assert!(contains(&site, &hub), "and the zone is still in its site");
    }

    /// A frame is the one box whose size is not somebody's choice: it is what
    /// is inside it. Adding to a site used to leave the site the size it was
    /// drawn, with its new contents hanging below the line.
    #[test]
    fn a_frame_grows_around_what_is_put_in_it() {
        let mut board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        let was = at(&board, "DC1").height();

        apply(
            &mut board,
            &changes(json!([
                { "op": "add", "label": "Keycloak", "inside": "DC1" },
                { "op": "add", "label": "Redis", "inside": "DC1" }
            ])),
            NOW,
        )
        .expect("applied");

        let frame = at(&board, "DC1");
        assert!(frame.height() > was, "the frame did not grow");
        for label in ["Keycloak", "Redis", "Kong 1"] {
            let item = at(&board, label);
            assert!(
                item.position.x >= frame.position.x
                    && item.position.y >= frame.position.y
                    && item.position.x + item.width() <= frame.position.x + frame.width()
                    && item.position.y + item.height() <= frame.position.y + frame.height(),
                "`{label}` is outside the frame that is supposed to hold it"
            );
        }
        // And the other site is where it was.
        assert_eq!(at(&board, "DC2").height(), was);
    }

    /// Something already on the board, moved into a zone.
    #[test]
    fn a_box_can_be_moved_into_a_frame() {
        let mut board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        apply(&mut board, &changes(json!([{ "op": "add", "label": "Keycloak" }])), NOW)
            .expect("added");
        assert!(
            at(&board, "Keycloak").position.y > at(&board, "DC1").position.y + at(&board, "DC1").height(),
            "it starts outside, below everything"
        );

        apply(
            &mut board,
            &changes(json!([{ "op": "move_into", "item": "Keycloak", "frame": "DC1" }])),
            NOW,
        )
        .expect("moved");

        let frame = at(&board, "DC1");
        let moved = at(&board, "Keycloak");
        assert!(
            moved.position.x >= frame.position.x
                && moved.position.y >= frame.position.y
                && moved.position.x + moved.width() <= frame.position.x + frame.width()
                && moved.position.y + moved.height() <= frame.position.y + frame.height(),
            "it is still not inside"
        );
    }

    /// A frame is a box drawn behind its members, so it has to contain them.
    #[test]
    fn a_frame_holds_what_belongs_to_it() {
        let board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        let dc1 = at(&board, "DC1");

        for label in ["SW Core 1", "F5 TPZ 1", "Kong 1"] {
            let item = at(&board, label);
            assert!(
                item.position.x >= dc1.position.x
                    && item.position.y >= dc1.position.y
                    && item.position.x + item.width() <= dc1.position.x + dc1.width()
                    && item.position.y + item.height() <= dc1.position.y + dc1.height(),
                "`{label}` is outside its frame"
            );
        }

        // And the frames come first, so they are drawn behind.
        let frame_at = board.nodes.iter().position(|n| n.label() == "DC1").expect("DC1");
        let box_at = board.nodes.iter().position(|n| n.label() == "SW Core 1").expect("core");
        assert!(frame_at < box_at, "a frame drawn last hides what it holds");
    }

    /// No box may sit on another one. The failure it guards is what a model
    /// writing its own coordinates produces every time.
    #[test]
    fn nothing_lands_on_top_of_anything_else() {
        let board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        let boxes: Vec<&Item> = board
            .nodes
            .iter()
            .filter(|n| n.data.get("color").and_then(Value::as_str) == Some(BOX_COLOUR))
            .collect();

        for (i, a) in boxes.iter().enumerate() {
            for b in boxes.iter().skip(i + 1) {
                let apart = a.position.x + a.width() <= b.position.x
                    || b.position.x + b.width() <= a.position.x
                    || a.position.y + a.height() <= b.position.y
                    || b.position.y + b.height() <= a.position.y;
                assert!(apart, "`{}` sits on `{}`", a.label(), b.label());
            }
        }
    }

    #[test]
    fn the_lines_join_the_boxes_and_keep_their_words() {
        let board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        assert_eq!(board.edges.len(), 7);

        let tctv = at(&board, "TCTV");
        let labelled = board
            .edges
            .iter()
            .find(|e| e.data.get("label").and_then(Value::as_str) == Some("MPLS"))
            .expect("the MPLS line");
        assert_eq!(labelled.source, tctv.id);
        assert!(labelled.source_handle.is_some() && labelled.target_handle.is_some());
    }

    /// A loop is a pair of switches that talk both ways, which these drawings
    /// have. Ranking must settle rather than run forever.
    #[test]
    fn a_drawing_that_loops_still_settles() {
        let looped = sketch(json!({
            "items": [{ "label": "A" }, { "label": "B" }],
            "links": [{ "from": "A", "to": "B" }, { "from": "B", "to": "A" }]
        }));
        let board = draw("Vòng", &looped, NOW).expect("drawn");
        assert_eq!(board.nodes.len(), 2);
    }

    #[test]
    fn a_drawing_that_cannot_be_read_is_refused_with_the_reason() {
        let twice = sketch(json!({ "items": [{ "label": "A" }, { "label": "a" }] }));
        assert!(draw("x", &twice, NOW).unwrap_err().contains("both called"));

        let missing = sketch(json!({
            "items": [{ "label": "A" }],
            "links": [{ "from": "A", "to": "B" }]
        }));
        assert!(draw("x", &missing, NOW).unwrap_err().contains("\"B\""));

        let empty = sketch(json!({ "items": [] }));
        assert!(draw("x", &empty, NOW).is_err());
    }

    // ── Changing one that exists ──────────────────────────────

    fn changes(json: Value) -> Vec<Change> {
        serde_json::from_value(json).expect("changes")
    }

    /// The rule the whole editing design rests on.
    ///
    /// A board has been dragged into the shape somebody meant. Adding an item
    /// is not permission to redraw it, and a tool that did would make every
    /// call something to check afterwards.
    #[test]
    fn adding_to_a_board_moves_nothing_that_was_there() {
        let mut board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        // As if somebody had dragged it about.
        board.nodes[3].position = Point { x: 1234.0, y: 99.0 };
        let before: Vec<(String, f64, f64)> = board
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.position.x, n.position.y))
            .collect();

        let done = apply(
            &mut board,
            &changes(json!([
                { "op": "add", "label": "Keycloak", "near": "Kong 1" },
                { "op": "connect", "from": "Kong 1", "to": "Keycloak", "label": "Validate" }
            ])),
            NOW,
        )
        .expect("applied");

        assert_eq!(done.len(), 2);
        for (id, x, y) in before {
            let now = board.nodes.iter().find(|n| n.id == id).expect("still there");
            assert_eq!((now.position.x, now.position.y), (x, y), "`{}` moved", now.label());
        }
        let added = at(&board, "Keycloak");
        let kong = at(&board, "Kong 1");
        assert!(added.position.x >= kong.position.x + kong.width(), "it landed on Kong");
    }

    #[test]
    fn a_line_can_be_drawn_renamed_and_taken_away() {
        let mut board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");

        apply(&mut board, &changes(json!([{ "op": "rename", "item": "Kong 1", "label": "Kong DC1" }])), NOW)
            .expect("renamed");
        assert_eq!(at(&board, "Kong DC1").label(), "Kong DC1");

        let lines = board.edges.len();
        apply(&mut board, &changes(json!([{ "op": "remove", "item": "F5 TPZ 1" }])), NOW)
            .expect("removed");
        assert!(board.nodes.iter().all(|n| n.label() != "F5 TPZ 1"));
        assert_eq!(board.edges.len(), lines - 2, "the lines that ended on it went too");
    }

    #[test]
    fn one_box_can_be_put_beside_another() {
        let mut board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        apply(
            &mut board,
            &changes(json!([{ "op": "place", "item": "SW Core 2", "side": "right", "of": "SW Core 1" }])),
            NOW,
        )
        .expect("placed");

        let one = at(&board, "SW Core 1");
        let two = at(&board, "SW Core 2");
        assert!(two.position.x > one.position.x + one.width());
        assert_eq!(two.position.y, one.position.y, "side by side means level");

        let nonsense = apply(
            &mut board,
            &changes(json!([{ "op": "place", "item": "SW Core 2", "side": "sideways", "of": "SW Core 1" }])),
            NOW,
        );
        assert!(nonsense.unwrap_err().contains("not a side"));
    }

    #[test]
    fn a_change_to_something_that_is_not_there_says_so() {
        let mut board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        let err = apply(&mut board, &changes(json!([{ "op": "remove", "item": "Splunk" }])), NOW)
            .unwrap_err();
        assert!(err.contains("Splunk"), "{err}");
    }

    // ── Reading ───────────────────────────────────────────────

    /// What a person sees when they look at a board — not the file.
    #[test]
    fn a_board_reads_as_what_is_on_it() {
        let mut board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        // A board in real use has a page of somebody's handwriting on it.
        board.nodes.push(Item {
            id: "stroke-1".into(),
            kind: "stroke".into(),
            position: Point { x: 0.0, y: 0.0 },
            data: Map::new(),
            updated: None,
            rest: Map::new(),
        });

        let said = describe(&board);
        assert!(said.contains("Board \"Luồng PSS\""), "{said}");
        assert!(said.contains("8 boxes"), "the frames are not counted as boxes: {said}");
        assert!(said.contains("1 freehand strokes (not described here)"), "{said}");
        assert!(said.contains("DC1"), "{said}");
        assert!(said.contains("SW Core 1, F5 TPZ 1, Kong 1"), "what is inside DC1: {said}");
        assert!(said.contains("TCTV → FW Checkpoint (MPLS)"), "{said}");
    }

    /// The file this writes is the Whiteboard app's, and has to be read back by
    /// the same parser the app's own boards go through.
    #[test]
    fn what_is_written_is_a_board_file() {
        let board = draw("Luồng PSS", &two_sites(), NOW).expect("drawn");
        let written = serde_json::to_string(&board).expect("serialised");
        let json: Value = serde_json::from_str(&written).expect("json");

        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["viewport"]["zoom"], 1);
        assert_eq!(json["nodes"][0]["type"], "frame", "a group is the app's own frame");
        assert!(json["nodes"][0]["data"]["width"].is_number());
        assert!(json["edges"][0]["sourceHandle"].is_string());

        let back: Board = serde_json::from_str(&written).expect("read back");
        assert_eq!(back.nodes.len(), board.nodes.len());
    }

    /// And a board written by the app, with keys this does not know, comes back
    /// with those keys still on it.
    #[test]
    fn a_board_from_the_app_keeps_what_this_does_not_understand() {
        let theirs = json!({
            "schemaVersion": 1, "title": "board 2", "tags": [], "type": "whiteboard",
            "created_at": "2026-08-18T00:00:00Z",
            "viewport": { "x": 0, "y": 0, "zoom": 1 },
            "nodes": [{ "id": "n1", "type": "shape", "position": { "x": 10.0, "y": 20.0 },
                        "data": { "label": "A", "width": 160, "height": 80, "rotation": 45 } }],
            "edges": []
        });
        let mut board: Board = serde_json::from_value(theirs).expect("read");
        apply(&mut board, &changes(json!([{ "op": "add", "label": "B" }])), NOW).expect("applied");

        let out = serde_json::to_value(&board).expect("written");
        assert_eq!(out["type"], "whiteboard", "an unknown key was dropped");
        assert_eq!(out["nodes"][0]["data"]["rotation"], 45, "so was one inside a node");
    }

    /// The app's own frames, sticky notes and writing can be named, as boxes can.
    #[test]
    fn what_the_app_draws_can_be_named_and_added_to() {
        let mut board: Board = serde_json::from_value(json!({
            "title": "T",
            "nodes": [
                { "id": "f", "type": "frame", "position": { "x": 0, "y": 0 }, "data": { "label": "Hub", "width": 400, "height": 300 } },
                { "id": "s", "type": "sticky", "position": { "x": 40, "y": 60 }, "data": { "label": "Note", "width": 160, "height": 160 } },
                { "id": "i", "type": "shape", "position": { "x": 600, "y": 0 }, "data": { "shapeType": "glyph", "label": "", "glyph": { "name": "server" } } }
            ],
            "edges": []
        }))
        .unwrap();
        assert!(describe(&board).contains("[icon: server]"));
        apply(&mut board, &changes(json!([
            { "op": "add", "label": "Switch", "inside": "Hub" },
            { "op": "connect", "from": "Note", "to": "Switch" },
            { "op": "add", "label": "Router", "near": "Note" }
        ])), NOW).expect("applied");
        let frame = board.nodes.iter().find(|n| n.id == "f").unwrap().clone();
        let inside = |label: &str| {
            let n = at(&board, label);
            n.position.x >= frame.position.x && n.position.y >= frame.position.y
                && n.position.x + n.width() <= frame.position.x + frame.width()
                && n.position.y + n.height() <= frame.position.y + frame.height()
        };
        assert!(inside("Switch"), "added inside the app's frame, which grew for it");
        assert_eq!(board.edges.len(), 1);
        // Beside the note, not pushed out of the frame because the frame was "in the way".
        let router = at(&board, "Router");
        let note = at(&board, "Note");
        assert!((router.position.y - note.position.y).abs() < 1.0, "{:?} vs {:?}", router.position, note.position);
    }

    /// A shape a model names in its own words is the board's shape.
    #[test]
    fn a_shape_named_in_other_words_is_the_boards_own() {
        let mut board: Board = serde_json::from_value(json!({ "title": "T", "nodes": [], "edges": [] })).unwrap();
        apply(&mut board, &changes(json!([
            { "op": "add", "label": "Orders", "shape": "database" },
            { "op": "add", "label": "Thing", "shape": "netNonsense" },
            { "op": "add", "label": "Idea", "shape": "infoLightbulb" }
        ])), NOW).expect("applied");
        assert_eq!(at(&board, "Orders").shape(), "cylinder");
        assert_eq!(at(&board, "Thing").shape(), "rectangle");
        let idea = at(&board, "Idea");
        assert_eq!(idea.width(), idea.height(), "a figure is drawn square");
    }
}
