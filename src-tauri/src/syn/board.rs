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
const SCHEMA_VERSION: u32 = 1;

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
}

fn default_link() -> String {
    "default".to_string()
}

impl Item {
    pub fn label(&self) -> &str {
        self.data.get("label").and_then(Value::as_str).unwrap_or("")
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
    let boxes: Vec<&Item> = board.nodes.iter().filter(|n| n.kind == "shape").collect();
    let strokes = board.nodes.iter().filter(|n| n.kind == "stroke").count();
    let notes: Vec<&Item> = board
        .nodes
        .iter()
        .filter(|n| n.kind == "text" || n.kind == "note" || n.kind == "mindmap")
        .collect();

    let all: Vec<Item> = boxes.iter().map(|b| (*b).clone()).collect();
    let mut frames: Vec<(&Item, usize)> =
        boxes.iter().map(|b| (*b, b.holds(&all))).filter(|(_, n)| *n >= 2).collect();
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
        board.edges.len()
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
        out.push_str(&format!(
            "- {} [{}] at {},{}\n",
            naming(item.label()),
            item.shape(),
            item.position.x.round(),
            item.position.y.round()
        ));
    }
    if boxes.len() > NAMED {
        out.push_str(&format!("…and {} more.\n", boxes.len() - NAMED));
    }

    if !board.edges.is_empty() {
        let by_id: std::collections::HashMap<&str, &Item> =
            boxes.iter().map(|b| (b.id.as_str(), *b)).collect();
        out.push_str("\nLines:\n");
        for link in board.edges.iter().take(NAMED) {
            let name = |id: &str| {
                by_id
                    .get(id)
                    .map(|i| naming(i.label()).to_string())
                    .unwrap_or_else(|| "(something no longer here)".to_string())
            };
            let label = link.data.get("label").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!("- {} → {}", name(&link.source), name(&link.target)));
            if !label.is_empty() {
                out.push_str(&format!(" ({label})"));
            }
            out.push('\n');
        }
        if board.edges.len() > NAMED {
            out.push_str(&format!("…and {} more.\n", board.edges.len() - NAMED));
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
#[derive(Debug, Clone, Deserialize)]
pub struct Sketch {
    pub items: Vec<SketchItem>,
    #[serde(default)]
    pub links: Vec<SketchLink>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SketchItem {
    pub label: String,
    #[serde(default)]
    pub shape: Option<String>,
    /// Which group this belongs in, by name. Groups become frames.
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SketchLink {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
}

/// How much room a box gets, and how much is left between them.
const BOX_H: f64 = 56.0;
const GAP_Y: f64 = 72.0;
const GAP_X: f64 = 48.0;
const FRAME_PAD: f64 = 32.0;
const FRAME_TOP: f64 = 44.0;
const START: f64 = 80.0;

/// A box wide enough for what is written in it.
fn box_width(label: &str) -> f64 {
    let longest = label.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
    ((longest as f64) * 8.2 + 36.0).clamp(120.0, 300.0)
}

/// Lay a sketch out, and hand back a board.
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
pub fn draw(title: &str, sketch: &Sketch, now_ms: i64) -> Result<Board, String> {
    if sketch.items.is_empty() {
        return Err("A board with nothing on it is not worth drawing.".into());
    }
    let mut seen = std::collections::HashSet::new();
    for item in &sketch.items {
        if item.label.trim().is_empty() {
            return Err("Every item needs a label; a box with no name cannot be referred to.".into());
        }
        if !seen.insert(item.label.trim().to_lowercase()) {
            return Err(format!(
                "Two items are both called \"{}\"; a line could not say which it meant.",
                item.label
            ));
        }
    }
    for link in &sketch.links {
        for end in [&link.from, &link.to] {
            if !seen.contains(&end.trim().to_lowercase()) {
                return Err(format!("A line names \"{end}\", which is not one of the items."));
            }
        }
    }

    // ── Columns: one per group, in the order the groups first appear ──
    let mut columns: Vec<(Option<String>, Vec<&SketchItem>)> = Vec::new();
    for item in &sketch.items {
        let group = item.group.as_ref().map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
        match columns.iter_mut().find(|(name, _)| name == &group) {
            Some((_, members)) => members.push(item),
            None => columns.push((group, vec![item])),
        }
    }

    let rank_of = ranks(sketch);
    let mut nodes: Vec<Item> = Vec::new();
    let mut where_is: std::collections::HashMap<String, (f64, f64, f64, f64)> = Default::default();
    let mut n = 0usize;
    let mut id = |kind: &str| {
        n += 1;
        format!("{kind}-{now_ms:x}-{n}")
    };

    // The ungrouped go across the top; the groups go side by side below them.
    let loose: Vec<&SketchItem> =
        columns.iter().filter(|(g, _)| g.is_none()).flat_map(|(_, m)| m.clone()).collect();
    let grouped: Vec<&(Option<String>, Vec<&SketchItem>)> =
        columns.iter().filter(|(g, _)| g.is_some()).collect();

    let mut y = START;
    let mut x = START;
    let mut widest_top: f64 = 0.0;
    for item in &loose {
        let w = box_width(&item.label);
        let at_rank = rank_of.get(item.label.trim()).copied().unwrap_or(0);
        let row_y = START + (at_rank as f64) * (BOX_H + GAP_Y);
        where_is.insert(item.label.trim().to_lowercase(), (x, row_y, w, BOX_H));
        nodes.push(shape(id("box"), x, row_y, w, BOX_H, &item.label, item.shape.as_deref(), now_ms));
        x += w + GAP_X;
        widest_top = widest_top.max(row_y + BOX_H);
    }
    if !loose.is_empty() {
        y = widest_top + GAP_Y * 1.5;
    }

    // ── Each group a column, ranked top to bottom ──
    let mut frames: Vec<(String, f64, f64, f64, f64)> = Vec::new();
    let mut column_x = START;
    for (group, members) in grouped {
        let column_w = members.iter().map(|m| box_width(&m.label)).fold(160.0_f64, f64::max);
        let mut placed: Vec<(&SketchItem, f64)> = members
            .iter()
            .map(|m| (*m, rank_of.get(m.label.trim()).copied().unwrap_or(0) as f64))
            .collect();
        placed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let inner_x = column_x + FRAME_PAD;
        let mut inner_y = y + FRAME_TOP;
        for (item, _) in &placed {
            let w = box_width(&item.label);
            let at = inner_x + (column_w - w) / 2.0;
            where_is.insert(item.label.trim().to_lowercase(), (at, inner_y, w, BOX_H));
            nodes.push(shape(id("box"), at, inner_y, w, BOX_H, &item.label, item.shape.as_deref(), now_ms));
            inner_y += BOX_H + GAP_Y;
        }

        let frame_h = (inner_y - GAP_Y) - y + FRAME_PAD;
        frames.push((
            group.clone().unwrap_or_default(),
            column_x,
            y,
            column_w + FRAME_PAD * 2.0,
            frame_h,
        ));
        column_x += column_w + FRAME_PAD * 2.0 + GAP_X * 1.5;
    }

    // Frames first in the list, so they are behind what they hold.
    let mut all: Vec<Item> = frames
        .iter()
        .map(|(label, fx, fy, fw, fh)| frame(id("frame"), *fx, *fy, *fw, *fh, label, now_ms))
        .collect();
    all.append(&mut nodes);

    let by_label: std::collections::HashMap<String, &Item> = all
        .iter()
        .filter(|i| i.data.get("color").and_then(Value::as_str) != Some(FRAME_COLOUR))
        .map(|i| (i.label().trim().to_lowercase(), i))
        .collect();

    let mut edges = Vec::new();
    for link in &sketch.links {
        let (Some(from), Some(to)) = (
            by_label.get(&link.from.trim().to_lowercase()),
            by_label.get(&link.to.trim().to_lowercase()),
        ) else {
            continue;
        };
        let (out_side, in_side) = sides_between(from, to);
        edges.push(Link {
            id: format!("edge-{now_ms:x}-{}", edges.len()),
            source: from.id.clone(),
            source_handle: Some(out_side.into()),
            target: to.id.clone(),
            target_handle: Some(in_side.into()),
            kind: "default".into(),
            data: link
                .label
                .as_ref()
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    let mut m = Map::new();
                    m.insert("label".into(), json!(l));
                    m
                })
                .unwrap_or_default(),
            updated: Some(now_ms),
        });
    }

    let stamp = chrono::Utc::now().to_rfc3339();
    let mut metadata = Map::new();
    metadata.insert("updated_at".into(), json!(stamp));
    Ok(Board {
        schema_version: Some(SCHEMA_VERSION),
        title: title.to_string(),
        tags: Vec::new(),
        created_at: stamp,
        metadata: Some(metadata),
        viewport: origin(),
        nodes: all,
        edges,
        rest: Map::new(),
    })
}

/// How far along the chain each item sits.
///
/// The longest path to it, which is what every layered layout ranks by: a box
/// goes below everything that feeds it, however many ways round there are. The
/// pass is repeated rather than recursed so that a loop — and these drawings
/// have loops, a pair of switches that talk both ways — settles instead of
/// running forever.
fn ranks(sketch: &Sketch) -> std::collections::HashMap<String, usize> {
    let mut rank: std::collections::HashMap<String, usize> =
        sketch.items.iter().map(|i| (i.label.trim().to_string(), 0)).collect();

    for _ in 0..sketch.items.len().min(64) {
        let mut moved = false;
        for link in &sketch.links {
            let from = link.from.trim();
            let to = link.to.trim();
            let (Some(&a), Some(&b)) = (rank.get(from), rank.get(to)) else { continue };
            if b <= a && from != to {
                rank.insert(to.to_string(), a + 1);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    rank
}

/// The colour a frame is drawn in — and what tells one apart afterwards.
const FRAME_COLOUR: &str = "#94a3b8";
const BOX_COLOUR: &str = "#7c3aed";

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
    let mut data = Map::new();
    data.insert("shapeType".into(), json!(kind.unwrap_or("rectangle")));
    data.insert("label".into(), json!(label));
    data.insert("color".into(), json!(BOX_COLOUR));
    data.insert("width".into(), json!(w));
    data.insert("height".into(), json!(h));
    Item { id, kind: "shape".into(), position: Point { x, y }, data, updated: Some(now_ms) }
}

fn frame(id: String, x: f64, y: f64, w: f64, h: f64, label: &str, now_ms: i64) -> Item {
    let mut data = Map::new();
    data.insert("shapeType".into(), json!("rectangle"));
    data.insert("label".into(), json!(label));
    data.insert("color".into(), json!(FRAME_COLOUR));
    data.insert("width".into(), json!(w));
    data.insert("height".into(), json!(h));
    Item { id, kind: "shape".into(), position: Point { x, y }, data, updated: Some(now_ms) }
}

/// Which side of each box a line leaves from, decided by where they ended up.
fn sides_between(from: &Item, to: &Item) -> (&'static str, &'static str) {
    let dx = to.position.x + to.width() / 2.0 - (from.position.x + from.width() / 2.0);
    let dy = to.position.y + to.height() / 2.0 - (from.position.y + from.height() / 2.0);
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
    },
    /// Join two things that are already there.
    Connect {
        from: String,
        to: String,
        #[serde(default)]
        label: Option<String>,
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
            Change::Add { label, shape: kind, near } => {
                if label.trim().is_empty() {
                    return Err("An item needs a label.".into());
                }
                if find(board, label).is_some() {
                    return Err(format!("\"{label}\" is already on this board."));
                }
                let w = box_width(label);
                let (x, y) = match near {
                    Some(name) => {
                        let anchor = find(board, name)
                            .ok_or_else(|| format!("There is nothing called \"{name}\" here."))?;
                        let (ax, ay, aw) = (
                            board.nodes[anchor].position.x,
                            board.nodes[anchor].position.y,
                            board.nodes[anchor].width(),
                        );
                        free_spot(board, ax + aw + GAP_X, ay, w, BOX_H)
                    }
                    None => {
                        let below = board
                            .nodes
                            .iter()
                            .map(|n| n.position.y + n.height())
                            .fold(START, f64::max);
                        (START, below + GAP_Y)
                    }
                };
                let id = format!("box-{now_ms:x}-{}", board.nodes.len());
                board.nodes.push(shape(id, x, y, w, BOX_H, label, kind.as_deref(), now_ms));
                done.push(format!("added \"{label}\""));
            }

            Change::Connect { from, to, label } => {
                let a = find(board, from)
                    .ok_or_else(|| format!("There is nothing called \"{from}\" here."))?;
                let b = find(board, to)
                    .ok_or_else(|| format!("There is nothing called \"{to}\" here."))?;
                let (out_side, in_side) = sides_between(&board.nodes[a], &board.nodes[b]);
                let (source, target) = (board.nodes[a].id.clone(), board.nodes[b].id.clone());
                if board.edges.iter().any(|e| e.source == source && e.target == target) {
                    return Err(format!("\"{from}\" and \"{to}\" are already joined."));
                }
                let mut data = Map::new();
                if let Some(words) = label.as_ref().filter(|l| !l.trim().is_empty()) {
                    data.insert("label".into(), json!(words));
                }
                board.edges.push(Link {
                    id: format!("edge-{now_ms:x}-{}", board.edges.len()),
                    source,
                    source_handle: Some(out_side.into()),
                    target,
                    target_handle: Some(in_side.into()),
                    kind: "default".into(),
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
                board.edges.retain(|e| e.source != id && e.target != id);
                let lines = before - board.edges.len();
                done.push(format!("removed \"{item}\" and {lines} line(s) that ended on it"));
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
                done.push(format!("put \"{item}\" {side} of \"{of}\""));
            }
        }
    }

    board.metadata.get_or_insert_with(Map::new).insert(
        "updated_at".into(),
        json!(chrono::Utc::now().to_rfc3339()),
    );
    Ok(done)
}

/// The box somebody means when they say a name.
fn find(board: &Board, label: &str) -> Option<usize> {
    let wanted = label.trim().to_lowercase();
    board
        .nodes
        .iter()
        .position(|n| n.kind == "shape" && n.label().trim().to_lowercase() == wanted)
}

/// A place near where it was asked for, that nothing is already sitting on.
fn free_spot(board: &Board, x: f64, y: f64, w: f64, h: f64) -> (f64, f64) {
    let clashes = |x: f64, y: f64| {
        board.nodes.iter().any(|n| {
            n.kind == "shape"
                && n.holds(&[]) == 0
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
        assert_eq!(json["nodes"][0]["type"], "shape");
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
}
