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
/// A rank with more in it than `PER_ROW` is split across rows rather than run
/// out sideways. Eighteen boxes on one line is the tower laid on its side, and
/// just as unreadable — a site drawn that way came out 2,510 pixels wide.
fn rows_by_rank<'a>(
    members: &[&'a SketchItem],
    rank_of: &std::collections::HashMap<String, usize>,
) -> Vec<Vec<&'a SketchItem>> {
    let mut by_rank: std::collections::BTreeMap<usize, Vec<&SketchItem>> = Default::default();
    for item in members {
        let rank = rank_of.get(item.label.trim()).copied().unwrap_or(0);
        by_rank.entry(rank).or_default().push(item);
    }
    by_rank
        .into_values()
        .flat_map(|rank| {
            rank.chunks(PER_ROW).map(<[&SketchItem]>::to_vec).collect::<Vec<_>>()
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
    let boxes: f64 = row.iter().map(|i| box_width(&i.label)).sum();
    boxes + GAP_X * (row.len().saturating_sub(1)) as f64
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
        let group = site_of(item);
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
    let mut lowest_top: f64 = START;
    for row in rows_by_rank(&loose, &rank_of) {
        let mut at_x = START;
        let row_y = lowest_top;
        for item in &row {
            let w = box_width(&item.label);
            where_is.insert(item.label.trim().to_lowercase(), (at_x, row_y, w, BOX_H));
            nodes.push(shape(id("box"), at_x, row_y, w, BOX_H, &item.label, item.shape.as_deref(), now_ms));
            at_x += w + GAP_X;
        }
        lowest_top = row_y + BOX_H + GAP_Y;
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
    let mut frames: Vec<(String, f64, f64, f64, f64)> = Vec::new();
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
        for (zone, rows) in &bands {
            let band_top = inner_y;
            if zone.is_some() {
                inner_y += FRAME_TOP;
            }
            for row in rows {
                let mut at_x = inner_x + (column_w - row_width(row)) / 2.0;
                for item in row {
                    let w = box_width(&item.label);
                    where_is.insert(item.label.trim().to_lowercase(), (at_x, inner_y, w, BOX_H));
                    nodes.push(shape(id("box"), at_x, inner_y, w, BOX_H, &item.label, item.shape.as_deref(), now_ms));
                    at_x += w + GAP_X;
                }
                inner_y += BOX_H + GAP_Y;
            }
            if let Some(name) = zone {
                // The zone's own frame, around the rows that belong to it.
                frames.push((
                    name.clone(),
                    inner_x - FRAME_PAD / 2.0,
                    band_top,
                    column_w + FRAME_PAD,
                    (inner_y - GAP_Y) - band_top + FRAME_PAD / 2.0,
                ));
                inner_y += GAP_Y / 2.0;
            }
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
/// The longest path to it: a box goes below everything that feeds it. The pass
/// is repeated rather than recursed so that a loop — and these drawings have
/// them, a pair of firewalls that answer each other — settles instead of
/// running forever.
///
/// # Why the numbers are allowed to be absurd
///
/// Around a cycle every pass adds one to every node on it, so a real drawing
/// of twenty-one boxes came out ranked 1, 2, 82, 83, 84, 84, 85 … 127. Those
/// numbers are nonsense as distances and exactly right as an **order**, which
/// is all that is asked of them: `rows_by_rank` groups by rank and lays the
/// groups out in order, so 82 and 127 become the third row and the tenth.
///
/// A depth-first version that dropped the return legs was tried, and ranked
/// worse: it put a database on the same row as the gateway three steps above
/// it, because which edge counts as the way back depends on where the walk
/// happened to start.
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
        /// Which frame it belongs in — a zone, a site, a rack.
        ///
        /// Without this there was no way to say "inside", and it showed: asked
        /// to put three devices in a Network Hub, the assistant put them
        /// underneath it, was told that was wrong, and settled for renaming
        /// them `[Network Hub] SW WAN`. The frame is what says where something
        /// lives; a label saying so is a caption pretending to be a place.
        #[serde(default)]
        inside: Option<String>,
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
            Change::Add { label, shape: kind, near, inside } => {
                if label.trim().is_empty() {
                    return Err("An item needs a label.".into());
                }
                if find(board, label).is_some() {
                    return Err(format!("\"{label}\" is already on this board."));
                }
                let w = box_width(label);
                let (x, y) = match (inside, near) {
                    (Some(zone), _) => {
                        let frame = find(board, zone)
                            .ok_or_else(|| format!("There is no frame called \"{zone}\" here."))?;
                        under_the_last_thing_in(board, frame, w)
                    }
                    (None, Some(name)) => {
                        let anchor = find(board, name)
                            .ok_or_else(|| format!("There is nothing called \"{name}\" here."))?;
                        let (ax, ay, aw) = (
                            board.nodes[anchor].position.x,
                            board.nodes[anchor].position.y,
                            board.nodes[anchor].width(),
                        );
                        free_spot(board, ax + aw + GAP_X, ay, w, BOX_H)
                    }
                    (None, None) => {
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
                // A frame told to hold something holds it from this moment,
                // not from whenever geometry happens to agree: the new box is
                // placed under the last thing in the frame, which is below the
                // frame's own edge until the frame is grown for it.
                if let Some(zone) = inside {
                    if let Some(frame) = find(board, zone) {
                        grow_to_hold(board, frame, x, y, w, BOX_H, now_ms);
                    }
                }
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

            Change::MoveInto { item, frame: zone } => {
                let moving = find(board, item)
                    .ok_or_else(|| format!("There is nothing called \"{item}\" here."))?;
                let frame = find(board, zone)
                    .ok_or_else(|| format!("There is no frame called \"{zone}\" here."))?;
                if moving == frame {
                    return Err("A frame cannot be put inside itself.".into());
                }
                let (w, h) = (board.nodes[moving].width(), board.nodes[moving].height());
                let (x, y) = under_the_last_thing_in(board, frame, w);
                board.nodes[moving].position = Point { x, y };
                board.nodes[moving].updated = Some(now_ms);
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
#[allow(clippy::too_many_arguments)]
fn grow_to_hold(board: &mut Board, frame: usize, x: f64, y: f64, w: f64, h: f64, now_ms: i64) {
    let node = &mut board.nodes[frame];
    let left = node.position.x.min(x - FRAME_PAD);
    let top = node.position.y.min(y - FRAME_TOP);
    let right = (node.position.x + node.width()).max(x + w + FRAME_PAD);
    let bottom = (node.position.y + node.height()).max(y + h + FRAME_PAD);

    node.position = Point { x: left, y: top };
    node.data.insert("width".into(), json!(right - left));
    node.data.insert("height".into(), json!(bottom - top));
    node.updated = Some(now_ms);
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
                && n.kind == "shape"
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
    let frames: Vec<usize> = (0..board.nodes.len())
        .filter(|i| board.nodes[*i].kind == "shape" && held_by(board, *i).len() >= 2)
        .collect();

    for frame in frames {
        let held = held_by(board, frame);
        if held.len() < 2 {
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
            .filter(|n| n.data.get("color").and_then(Value::as_str) == Some(FRAME_COLOUR))
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
