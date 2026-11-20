//! Drawing on a whiteboard by hand: from SVG, and from a list of notes.
//!
//! # Why this is not `draw_board`
//!
//! `draw_board` is told what is on a diagram and what joins what, and never
//! where: the layout, the routing and the rule that lines do not cross all
//! depend on Syn giving no coordinates. Everything else a person does on a
//! whiteboard is the opposite — where each thing is, its colour and its outline
//! are the whole of it: a picture, a sketch that explains something, a
//! wireframe, sticky notes scattered for a brainstorm, a ring drawn round one
//! box of a diagram with a note beside it. Asked for a landscape with only the
//! diagram tool, Syn drew a sky, a horizon and a foreground as three frames
//! side by side, joined by arrows labelled "lights up" and "reflects".
//!
//! This began as `draw_picture`, a new board with a picture on it. A picture
//! is one of the things a hand does on a board, and a tool named for it kept
//! the rest out: asked for a sketch of a handshake, a brainstorm on sticky
//! notes or a wireframe, the model reached for `draw_board` every time
//! (2026-11-19). So it draws onto a board that exists as well as a new one,
//! and makes the board's own notes, boxes and text as well as ink.
//!
//! # Why SVG, and why ink
//!
//! A model writes SVG well, and nothing else it could be asked to write says as
//! much about a drawing in as few words. On the board, ink is already a filled
//! outline (`svgPath`, painted with `color` — see the app's `inkLayer.ts`), so
//! every filled shape of the drawing is one stroke as it stands, and every
//! outlined one is its outline, worked out here. Each is then an item of its
//! own: the person can move the boat, recolour the lake, erase a cloud. A
//! drawing kept as one image would be no more than the drawing in the chat.
//!
//! The SVG is read by `usvg`, which resolves what a hand-written SVG leaves to
//! the reader — styles, inheritance, `<use>`, transforms, every shape as a
//! path. Built without fonts, so `<text>` is read here instead and becomes the
//! board's own text, which the person can edit.
//!
//! # Names
//!
//! A stroke has no words on it, so nothing could name it: `read_board` counted
//! ink and said no more, and "add a dog, make the background yellow" could
//! only be a new board, with whatever the person had moved by hand lost. An
//! `id` in the SVG is now the name of what it marks — `<g id="boat">` — kept
//! on every stroke and piece of text under it (`data.name`, `boat/sail` for a
//! part of a part). The outermost named group is also one of the board's own
//! groups (`data.groupId`), so the person drags the boat whole. `read_board`
//! lists drawings by name, and `edit_board` recolours, places, renames and
//! removes them by it.
//!
//! Nothing outside the string is read: an `<image>` that names a file or a
//! URL is never loaded.

use std::str::FromStr;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Map, Value};
use tiny_skia_path::{PathSegment, PathStroker, StrokeDash, Transform};
use usvg::{Node, Paint};

use super::board::{Item, Point};

/// The longest SVG accepted. A rich picture is a few tens of kilobytes.
pub const MAX_SVG: usize = 300_000;
/// The most items one drawing may put on a board. The ink layer draws
/// thousands of strokes at ease, but a drawing of that many parts is a
/// texture, not something anyone will edit by hand.
const MAX_ITEMS: usize = 4_000;
/// The most notes, boxes and pieces of text one call may add.
const MAX_PIECES: usize = 300;
/// A drawing for a new board outside these sizes on its longer side is scaled
/// to `FIT`: one drawn in a 24-unit icon box is unreadable at that size, one in
/// a 10,000 unit box needs zooming out to see at all. Otherwise a new board's
/// coordinates are the drawing's own, not moved to a corner: asked to add a
/// dog beside the cat it had drawn, Syn drew it in the same viewBox, and a
/// picture moved 80 to the right and down put the dog 80 away from where it
/// meant (2026-11-19). Onto a board that exists nothing is scaled either.
const SMALLEST: f32 = 240.0;
const LARGEST: f32 = 2400.0;
const FIT: f32 = 1000.0;
/// Board text is never drawn smaller than this, whatever the drawing asked.
const LEAST_TEXT: f32 = 12.0;
/// The text item's padding (`.wb-text-node`), so its words land where the
/// drawing put them.
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 8.0;
/// A sticky note's side when none is given: the app's `STICKY_SIZE`.
const STICKY_SIZE: f32 = 200.0;
/// The app's sticky papers (`sticky.ts`), by name and fill. A note is stored
/// by name; a colour given as hex goes to the nearest paper.
const PAPERS: [(&str, (u8, u8, u8)); 7] = [
    ("yellow", (0xfe, 0xf0, 0x8a)),
    ("orange", (0xfe, 0xd7, 0xaa)),
    ("pink", (0xfb, 0xcf, 0xe8)),
    ("purple", (0xdd, 0xd6, 0xfe)),
    ("blue", (0xbf, 0xdb, 0xfe)),
    ("green", (0xbb, 0xf7, 0xd0)),
    ("gray", (0xe5, 0xe7, 0xeb)),
];
/// A box drawn by hand is paper and pencil, not one of a diagram's roles.
const BOX_FILL: &str = "#ffffff";
const BOX_LINE: &str = "#475569";
const TEXT_COLOUR: &str = "#1f2937";

/// A sticky note, a box or a piece of text, where the drawing puts it: one of
/// the board's own items, which the person types into.
#[derive(Debug, Clone, Deserialize)]
pub struct Piece {
    pub kind: String,
    #[serde(default)]
    pub text: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub size: Option<f32>,
    #[serde(default)]
    pub rotate: Option<f32>,
}

/// What `draw` made: items to put on a board.
pub struct Drawing {
    pub nodes: Vec<Item>,
    pub strokes: usize,
    pub words: usize,
    pub pieces: usize,
    /// The drawing's named parts, outermost first, each once.
    pub named: Vec<String>,
    /// Kinds of thing in the SVG that are not on the board, each said once.
    pub left_out: Vec<String>,
    /// How much a new board's drawing was scaled to be readable: 1 unless it
    /// was very small or very large.
    pub scale: f32,
}

/// What a part of the drawing answers to: its `id`s from the outside in
/// (`boat/sail`), and which outermost named group it is in.
#[derive(Clone)]
struct Name {
    path: String,
    group: usize,
}

/// One filled outline, in the drawing's coordinates.
struct Ink {
    path: tiny_skia_path::Path,
    color: String,
    opacity: f32,
    even_odd: bool,
    name: Option<Name>,
}

/// A run of text: where its baseline starts and how it is set.
struct Words {
    text: String,
    x: f32,
    y: f32,
    size: f32,
    color: String,
    opacity: f32,
    anchor: Anchor,
    bold: bool,
    name: Option<Name>,
}

#[derive(Clone, Copy, PartialEq)]
enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Default)]
struct Notes(Vec<String>);

impl Notes {
    fn add(&mut self, what: &str) {
        if !self.0.iter().any(|n| n == what) {
            self.0.push(what.to_string());
        }
    }
}

/// The outermost names, in the order met: a name's place is its group.
#[derive(Default)]
struct Groups(Vec<String>);

impl Groups {
    /// The name of `id` under `outer`: a new group when there is none outside it.
    fn under(&mut self, outer: Option<&Name>, id: &str) -> Name {
        match outer {
            Some(o) => Name { path: format!("{}/{id}", o.path), group: o.group },
            None => {
                let group = self.0.iter().position(|g| g == id).unwrap_or_else(|| {
                    self.0.push(id.to_string());
                    self.0.len() - 1
                });
                Name { path: id.to_string(), group }
            }
        }
    }
}

/// Items for a board from `svg` and `pieces`.
///
/// `onto` is a board that exists: the drawing is in its coordinates, as they
/// are. Without one it is a new board's, in its own coordinates, brought to a
/// readable size if it is far from one.
pub fn draw(svg: Option<&str>, pieces: &[Piece], onto: bool, now_ms: i64) -> Result<Drawing, String> {
    let svg = svg.map(str::trim).filter(|s| !s.is_empty());
    if svg.is_none() && pieces.is_empty() {
        return Err("Nothing to draw: give `svg`, `items`, or both.".into());
    }
    if pieces.len() > MAX_PIECES {
        return Err(format!("{} items; one call adds at most {MAX_PIECES}.", pieces.len()));
    }
    for p in pieces {
        match p.kind.as_str() {
            "sticky" | "text" if p.text.trim().is_empty() => {
                return Err(format!("A {} needs its `text`.", p.kind));
            }
            "sticky" | "box" | "text" => {}
            other => return Err(format!("\"{other}\" is not an item here: sticky, box or text.")),
        }
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err("An item needs a place: `x` and `y`.".into());
        }
    }

    let mut notes = Notes::default();
    let mut groups = Groups::default();
    let mut inks = Vec::new();
    let mut words = Vec::new();
    if let Some(svg) = svg {
        if svg.len() > MAX_SVG {
            return Err(format!(
                "That SVG is {} characters; a drawing here is at most {MAX_SVG}. Draw it with fewer, larger shapes.",
                svg.len()
            ));
        }
        if svg.contains("var(") {
            notes.add("CSS variables (var(--…)): a board has no theme to read them from, so those parts were painted black or not at all; write colours as hex");
        }
        let options = usvg::Options {
            // Never a file, never the network: only what is in the string.
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: Box::new(|_, _: Arc<Vec<u8>>, _| None),
                resolve_string: Box::new(|_, _| None),
            },
            ..usvg::Options::default()
        };
        let tree = usvg::Tree::from_str(svg, &options).map_err(|e| format!("That is not an SVG this can read: {e}"))?;
        walk(tree.root(), 1.0, None, &mut groups, &mut inks, &mut notes);
        // `usvg` gives everything in the size the SVG asked to be drawn at;
        // the drawing's own units are its viewBox's, which are what a board
        // that exists is drawn in.
        if let Some(back) = view_of(svg, tree.size()).invert() {
            inks = inks
                .into_iter()
                .filter_map(|mut ink| {
                    ink.path = ink.path.transform(back)?;
                    Some(ink)
                })
                .collect();
        }
        words = read_words(svg, &mut groups, &mut notes);
        if inks.is_empty() && words.is_empty() && pieces.is_empty() {
            return Err("Nothing in that SVG can be drawn: give it shapes or paths with a fill or a stroke.".into());
        }
    }
    if inks.len() + words.len() + pieces.len() > MAX_ITEMS {
        return Err(format!(
            "That drawing has {} parts; a board takes at most {MAX_ITEMS}. Draw it with fewer, larger shapes.",
            inks.len() + words.len() + pieces.len()
        ));
    }

    let place = if onto {
        Transform::identity()
    } else {
        // How big everything is, to bring a drawing of an odd size to a
        // readable one.
        let mut area: Option<(f32, f32, f32, f32)> = None;
        let mut take = |l: f32, t: f32, r: f32, b: f32| {
            area = Some(match area {
                None => (l, t, r, b),
                Some((l0, t0, r0, b0)) => (l0.min(l), t0.min(t), r0.max(r), b0.max(b)),
            });
        };
        for ink in &inks {
            if let Some(b) = ink.path.compute_tight_bounds() {
                take(b.left(), b.top(), b.right(), b.bottom());
            }
        }
        for w in &words {
            let (l, t, width, height) = text_box(w);
            take(l, t, l + width, t + height);
        }
        for p in pieces {
            let (w, h) = piece_size(p, 1.0);
            take(p.x, p.y, p.x + w, p.y + h);
        }
        let (left, top, right, bottom) = area.ok_or("Nothing in that drawing has a size.")?;
        let longest = (right - left).max(bottom - top);
        let k = if longest > LARGEST || (longest > 0.0 && longest < SMALLEST) { FIT / longest } else { 1.0 };
        Transform::from_scale(k, k)
    };
    let k = place.sx;

    let mut n = 0usize;
    let mut id = |kind: &str| {
        n += 1;
        format!("{kind}-{now_ms:x}-{n}")
    };
    let named = |data: &mut Map<String, Value>, name: &Option<Name>| {
        if let Some(name) = name {
            data.insert("name".into(), json!(name.path));
            data.insert("groupId".into(), json!(format!("grp_{now_ms:x}_{}", name.group)));
        }
    };

    let mut items = Vec::new();
    for ink in inks {
        let Some(path) = ink.path.transform(place) else { continue };
        let Some(b) = path.compute_tight_bounds() else { continue };
        if b.width() < 0.5 && b.height() < 0.5 {
            continue;
        }
        let mut data = Map::new();
        data.insert("svgPath".into(), json!(path_data(&path, b.left(), b.top())));
        data.insert("color".into(), json!(ink.color));
        data.insert("size".into(), json!(1));
        data.insert("width".into(), json!(round(b.width().max(1.0))));
        data.insert("height".into(), json!(round(b.height().max(1.0))));
        data.insert("opacity".into(), json!(round3(ink.opacity)));
        if ink.even_odd {
            data.insert("fillRule".into(), json!("evenodd"));
        }
        named(&mut data, &ink.name);
        items.push(item(id("stroke"), "stroke", b.left(), b.top(), data, now_ms));
    }
    let strokes = items.len();

    for mut w in words {
        let (x, y) = {
            let mut p = tiny_skia_path::Point::from_xy(w.x, w.y);
            place.map_point(&mut p);
            (p.x, p.y)
        };
        w.x = x;
        w.y = y;
        w.size = (w.size * k).max(LEAST_TEXT);
        let (l, t, width, _) = text_box(&w);
        let mut data = Map::new();
        data.insert("label".into(), json!(escape_markdown(&w.text)));
        data.insert("fontSize".into(), json!(round(w.size)));
        data.insert("color".into(), json!(w.color));
        data.insert("width".into(), json!(round(width)));
        data.insert(
            "textAlign".into(),
            json!(match w.anchor {
                Anchor::Start => "left",
                Anchor::Middle => "center",
                Anchor::End => "right",
            }),
        );
        if w.bold {
            data.insert("fontWeight".into(), json!("bold"));
        }
        if w.opacity < 1.0 {
            data.insert("opacity".into(), json!(round3(w.opacity)));
        }
        // Over the ink: the ink layer is stacked at zero, and so is text
        // that says nothing of its own place.
        data.insert("z".into(), json!(1));
        named(&mut data, &w.name);
        items.push(item(id("text"), "text", l, t, data, now_ms));
    }
    let words = items.len() - strokes;

    for p in pieces {
        let mut at = tiny_skia_path::Point::from_xy(p.x, p.y);
        place.map_point(&mut at);
        let (w, h) = piece_size(p, k);
        let mut node = match p.kind.as_str() {
            "sticky" => {
                let mut data = Map::new();
                data.insert("label".into(), json!(p.text.trim()));
                data.insert("color".into(), json!(paper(p.color.as_deref())));
                data.insert("width".into(), json!(round(w)));
                data.insert("height".into(), json!(round(h)));
                item(id("sticky"), "sticky", at.x, at.y, data, now_ms)
            }
            "box" => {
                let mut node = super::board::shape(
                    id("box"),
                    round(at.x) as f64,
                    round(at.y) as f64,
                    round(w) as f64,
                    round(h) as f64,
                    p.text.trim(),
                    Some(p.shape.as_deref().unwrap_or("rectangle")),
                    now_ms,
                );
                let fill = p.color.as_deref().and_then(hex_of).unwrap_or_else(|| BOX_FILL.to_string());
                node.data.insert("fillColor".into(), json!(fill));
                node.data.insert("color".into(), json!(BOX_LINE));
                node
            }
            _ => {
                let size = (p.size.unwrap_or(18.0) * k).max(LEAST_TEXT);
                let mut data = Map::new();
                data.insert("label".into(), json!(p.text.trim()));
                data.insert("fontSize".into(), json!(round(size)));
                data.insert(
                    "color".into(),
                    json!(p.color.as_deref().and_then(hex_of).unwrap_or_else(|| TEXT_COLOUR.to_string())),
                );
                data.insert("width".into(), json!(round(w)));
                data.insert("textAlign".into(), json!("left"));
                item(id("text"), "text", at.x, at.y, data, now_ms)
            }
        };
        node.data.insert("z".into(), json!(1));
        if let Some(r) = p.rotate.filter(|r| r.is_finite() && *r != 0.0) {
            node.data.insert("rotation".into(), json!(round(r)));
        }
        items.push(node);
    }
    let pieces = items.len() - strokes - words;

    Ok(Drawing { nodes: items, strokes, words, pieces, named: groups.0, left_out: notes.0, scale: k })
}

/// How big a piece is drawn, at scale `k`.
fn piece_size(p: &Piece, k: f32) -> (f32, f32) {
    let given = |v: Option<f32>, least: f32, otherwise: f32| v.filter(|v| v.is_finite() && *v > 0.0).map_or(otherwise, |v| v.max(least));
    match p.kind.as_str() {
        "sticky" => (given(p.width, 80.0, STICKY_SIZE) * k, given(p.height, 80.0, STICKY_SIZE) * k),
        "box" => (given(p.width, 16.0, 160.0) * k, given(p.height, 16.0, 56.0) * k),
        _ => {
            let size = p.size.unwrap_or(18.0).max(LEAST_TEXT);
            let longest = p.text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
            let lines = p.text.lines().count().max(1) as f32;
            (given(p.width, 40.0, longest * size * 0.62 + PAD_X * 2.0) * k, (lines * size * 1.5 + PAD_Y * 2.0) * k)
        }
    }
}

/// A colour as `#rrggbb`, from anything SVG takes for one.
pub(crate) fn hex_of(colour: &str) -> Option<String> {
    let c = svgtypes::Color::from_str(colour.trim()).ok()?;
    Some(hex(c.red, c.green, c.blue))
}

/// The sticky paper for a colour: by name, or the nearest to a hex one.
pub(crate) fn paper(colour: Option<&str>) -> &'static str {
    let Some(colour) = colour.map(|c| c.trim().to_lowercase()) else { return "yellow" };
    let colour = if colour == "grey" { "gray".to_string() } else { colour };
    if let Some((name, _)) = PAPERS.iter().find(|(n, _)| *n == colour) {
        return name;
    }
    let Ok(c) = svgtypes::Color::from_str(&colour) else { return "yellow" };
    let far = |(r, g, b): (u8, u8, u8)| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(r, c.red) + d(g, c.green) + d(b, c.blue)
    };
    PAPERS.iter().min_by_key(|(_, rgb)| far(*rgb)).map_or("yellow", |(n, _)| n)
}

/// The viewBox mapped onto the size `usvg` gave the drawing.
fn view_of(svg: &str, size: usvg::Size) -> Transform {
    let options = usvg::roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let Ok(doc) = usvg::roxmltree::Document::parse_with_options(svg, options) else {
        return Transform::identity();
    };
    doc.root_element()
        .attribute("viewBox")
        .and_then(|v| svgtypes::ViewBox::from_str(v).ok())
        .filter(|v| v.w > 0.0 && v.h > 0.0)
        .map(|v| {
            let (sx, sy) = (size.width() / v.w as f32, size.height() / v.h as f32);
            Transform::from_row(sx, 0.0, 0.0, sy, -v.x as f32 * sx, -v.y as f32 * sy)
        })
        .unwrap_or_default()
}

/// Whether an item drawn here answers to `wanted`: its whole name, or any
/// part of it — "boat" is the boat and everything on it, "sail" its sail.
pub fn answers_to(item: &Item, wanted: &str) -> bool {
    let Some(name) = item.data.get("name").and_then(Value::as_str) else { return false };
    let wanted = wanted.trim().to_lowercase();
    let name = name.to_lowercase();
    !wanted.is_empty() && (name == wanted || name.split('/').any(|part| part == wanted))
}

/// Every filled shape and every outline under `group`, back to front.
fn walk(group: &usvg::Group, opacity: f32, name: Option<&Name>, groups: &mut Groups, out: &mut Vec<Ink>, notes: &mut Notes) {
    for child in group.children() {
        match child {
            Node::Group(g) => {
                if g.clip_path().is_some() {
                    notes.add("clip paths: what they clip is drawn whole");
                }
                if g.mask().is_some() {
                    notes.add("masks: what they hide is drawn whole");
                }
                if !g.filters().is_empty() {
                    notes.add("filters (blur, shadow): drawn without them");
                }
                let inner = (!g.id().is_empty()).then(|| groups.under(name, g.id()));
                walk(g, opacity * g.opacity().get(), inner.as_ref().or(name), groups, out, notes);
            }
            Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let own = (!p.id().is_empty()).then(|| groups.under(name, p.id()));
                let name = own.or_else(|| name.cloned());
                let fill = fill_of(p, opacity, notes);
                let stroke = stroke_of(p, opacity, notes);
                let both = match p.paint_order() {
                    usvg::PaintOrder::FillAndStroke => [fill, stroke],
                    usvg::PaintOrder::StrokeAndFill => [stroke, fill],
                };
                out.extend(both.into_iter().flatten().map(|mut ink| {
                    ink.name = name.clone();
                    ink
                }));
            }
            Node::Image(_) => notes.add("images: a drawing here is drawn, not pasted"),
            Node::Text(_) => {}
        }
    }
}

fn fill_of(p: &usvg::Path, opacity: f32, notes: &mut Notes) -> Option<Ink> {
    let fill = p.fill()?;
    let (color, alpha) = colour_of(fill.paint(), notes)?;
    let path = p.data().clone().transform(p.abs_transform())?;
    Some(Ink {
        path,
        color,
        opacity: opacity * fill.opacity().get() * alpha,
        even_odd: fill.rule() == usvg::FillRule::EvenOdd,
        name: None,
    })
}

/// An outline, as the filled shape the board draws it as.
fn stroke_of(p: &usvg::Path, opacity: f32, notes: &mut Notes) -> Option<Ink> {
    let stroke = p.stroke()?;
    let (color, alpha) = colour_of(stroke.paint(), notes)?;
    let ts = p.abs_transform();
    let scale = PathStroker::compute_resolution_scale(&ts);
    let mut line = p.data().clone();
    if let Some(dashes) = stroke.dasharray() {
        if let Some(dash) = StrokeDash::new(dashes.to_vec(), stroke.dashoffset()) {
            line = line.dash(&dash, scale)?;
        }
    }
    let outline = line.stroke(&stroke.to_tiny_skia(), scale)?.transform(ts)?;
    Some(Ink { path: outline, color, opacity: opacity * stroke.opacity().get() * alpha, even_odd: false, name: None })
}

/// The one colour a paint is drawn in, and how opaque. A gradient is drawn as
/// the mean of its stops: ink is one colour.
fn colour_of(paint: &Paint, notes: &mut Notes) -> Option<(String, f32)> {
    let stops = match paint {
        Paint::Color(c) => return Some((hex(c.red, c.green, c.blue), 1.0)),
        Paint::LinearGradient(g) => g.stops(),
        Paint::RadialGradient(g) => g.stops(),
        Paint::Pattern(_) => {
            notes.add("patterns: those parts are left out");
            return None;
        }
    };
    notes.add("gradients: each is drawn in one colour, the middle of its stops");
    if stops.is_empty() {
        return None;
    }
    let n = stops.len() as f32;
    let mean = |f: &dyn Fn(&usvg::Stop) -> f32| stops.iter().map(f).sum::<f32>() / n;
    let channel = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    Some((
        hex(
            channel(mean(&|s| s.color().red as f32)),
            channel(mean(&|s| s.color().green as f32)),
            channel(mean(&|s| s.color().blue as f32)),
        ),
        mean(&|s| s.opacity().get()),
    ))
}

fn hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A path as `d`, measured from `(x0, y0)`: a stroke's outline is drawn from
/// its own corner.
fn path_data(path: &tiny_skia_path::Path, x0: f32, y0: f32) -> String {
    let p = |pt: tiny_skia_path::Point| format!("{} {}", round(pt.x - x0), round(pt.y - y0));
    let mut d = String::new();
    for seg in path.segments() {
        if !d.is_empty() {
            d.push(' ');
        }
        match seg {
            PathSegment::MoveTo(a) => d += &format!("M{}", p(a)),
            PathSegment::LineTo(a) => d += &format!("L{}", p(a)),
            PathSegment::QuadTo(a, b) => d += &format!("Q{} {}", p(a), p(b)),
            PathSegment::CubicTo(a, b, c) => d += &format!("C{} {} {}", p(a), p(b), p(c)),
            PathSegment::Close => d.push('Z'),
        }
    }
    d
}

fn item(id: String, kind: &str, x: f32, y: f32, data: Map<String, Value>, now_ms: i64) -> Item {
    Item {
        id,
        kind: kind.into(),
        position: Point { x: round(x) as f64, y: round(y) as f64 },
        data,
        updated: Some(now_ms),
        rest: Map::new(),
    }
}

fn round(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

fn round3(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 1000.0).round() / 1000.0
}

/// Where a text item for `w` goes, and how big it is: `(left, top, width,
/// height)`. Its width is a guess from the number of letters, since nothing
/// here can measure type; generous, because words that wrap move more than a
/// box a little too wide.
fn text_box(w: &Words) -> (f32, f32, f32, f32) {
    let size = w.size;
    let letters = w.text.chars().count() as f32;
    let width = letters * size * 0.62 + PAD_X * 2.0;
    let left = match w.anchor {
        Anchor::Start => w.x - PAD_X,
        Anchor::Middle => w.x - width / 2.0,
        Anchor::End => w.x - width + PAD_X,
    };
    // The baseline sits about one size below the top of the line.
    let top = w.y - size * 1.1 - PAD_Y;
    (left, top, width, size * 1.5 + PAD_Y * 2.0)
}

/// Board text is Markdown: what the picture wrote is meant as it is.
fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '_' | '#' | '[' | ']' | '<' | '>' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The `<text>` of the drawing, in its viewBox's units, as its ink is.
///
/// Read from the source rather than from `usvg`, which, built without fonts,
/// drops text. Covers what a drawing uses: position, size, colour, weight,
/// anchor, and the transforms of the groups it is in.
fn read_words(svg: &str, groups: &mut Groups, notes: &mut Notes) -> Vec<Words> {
    let options = usvg::roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let Ok(doc) = usvg::roxmltree::Document::parse_with_options(svg, options) else {
        return Vec::new();
    };
    let root = doc.root_element();

    let mut out = Vec::new();
    for node in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "text") {
        let hidden = node.ancestors().skip(1).any(|a| {
            matches!(a.tag_name().name(), "defs" | "clipPath" | "mask" | "pattern" | "symbol" | "marker")
                || style_of(a, "display").as_deref() == Some("none")
                || style_of(a, "visibility").as_deref() == Some("hidden")
        });
        if hidden {
            continue;
        }
        let text = node
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect::<Vec<_>>()
            .join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            continue;
        }
        let fill = inherited(node, "fill").unwrap_or_else(|| "#000".into());
        if fill.trim() == "none" {
            continue;
        }
        let (color, alpha) = match svgtypes::Color::from_str(fill.trim()) {
            Ok(c) => (hex(c.red, c.green, c.blue), c.alpha as f32 / 255.0),
            Err(_) => {
                if fill.contains("url(") {
                    notes.add("gradients: each is drawn in one colour, the middle of its stops");
                }
                ("#000000".into(), 1.0)
            }
        };

        // From the root down: the viewBox, then every group's transform.
        let chain: Vec<_> = node.ancestors().filter(|a| a.is_element() && *a != root).collect();
        let mut ts = Transform::identity();
        for a in chain.iter().rev() {
            if let Some(t) = a.attribute("transform").and_then(|t| svgtypes::Transform::from_str(t).ok()) {
                ts = ts.pre_concat(Transform::from_row(
                    t.a as f32, t.b as f32, t.c as f32, t.d as f32, t.e as f32, t.f as f32,
                ));
            }
        }
        let first = |name: &str| {
            std::iter::once(node)
                .chain(node.descendants().filter(|n| n.tag_name().name() == "tspan"))
                .find_map(|n| n.attribute(name))
                .and_then(|v| v.split([' ', ',']).find(|s| !s.is_empty())?.parse::<f32>().ok())
                .unwrap_or(0.0)
        };
        let mut at = tiny_skia_path::Point::from_xy(first("x"), first("y"));
        ts.map_point(&mut at);
        let scale = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
        let size = inherited(node, "font-size").and_then(|s| font_size(&s)).unwrap_or(16.0) * scale;
        let opacity = node
            .ancestors()
            .filter_map(|a| style_of(a, "opacity").and_then(|o| o.trim().parse::<f32>().ok()))
            .product::<f32>()
            * inherited(node, "fill-opacity").and_then(|o| o.trim().parse::<f32>().ok()).unwrap_or(1.0)
            * alpha;
        let anchor = match inherited(node, "text-anchor").as_deref().map(str::trim) {
            Some("middle") => Anchor::Middle,
            Some("end") => Anchor::End,
            _ => Anchor::Start,
        };
        let bold = inherited(node, "font-weight")
            .map(|w| {
                let w = w.trim();
                w == "bold" || w == "bolder" || w.parse::<u32>().is_ok_and(|n| n >= 600)
            })
            .unwrap_or(false);
        // Named as the ink around it is: by the ids of its groups, then its
        // own (`ancestors` starts at the node itself).
        let mut name: Option<Name> = None;
        for a in chain.iter().rev() {
            if let Some(id) = a.attribute("id").filter(|i| !i.trim().is_empty()) {
                name = Some(groups.under(name.as_ref(), id));
            }
        }
        out.push(Words { text, x: at.x, y: at.y, size, color, opacity, anchor, bold, name });
    }
    out
}

/// A property set on this element, by its `style` or as an attribute.
fn style_of(node: usvg::roxmltree::Node, name: &str) -> Option<String> {
    let from_style = node.attribute("style").and_then(|s| {
        s.split(';').find_map(|decl| {
            let (k, v) = decl.split_once(':')?;
            (k.trim() == name).then(|| v.trim().to_string())
        })
    });
    from_style.or_else(|| node.attribute(name).map(str::to_string))
}

/// A property as this element gets it: its own, or the nearest group's.
fn inherited(node: usvg::roxmltree::Node, name: &str) -> Option<String> {
    node.ancestors()
        .filter(|a| a.is_element())
        .find_map(|a| style_of(a, name).filter(|v| v.trim() != "inherit"))
}

fn font_size(v: &str) -> Option<f32> {
    let v = v.trim();
    let number = v.trim_end_matches("px").trim();
    number.parse::<f32>().ok().filter(|n| *n > 0.0)
}

/// What a drawing says about itself to Syn.
pub fn summary(d: &Drawing) -> Value {
    json!({
        "strokes": d.strokes,
        "words": d.words,
        "items": d.pieces,
        "named": d.named,
        "left_out": d.left_out,
        "scaled": (d.scale != 1.0).then_some(d.scale),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 0x1234;

    fn painted(svg: &str) -> Drawing {
        draw(Some(svg), &[], false, NOW).expect("painted")
    }

    fn onto(svg: &str, pieces: &[Piece]) -> Drawing {
        draw(Some(svg).filter(|s| !s.is_empty()), pieces, true, NOW).expect("drawn")
    }

    fn strokes(p: &Drawing) -> Vec<&Item> {
        p.nodes.iter().filter(|n| n.kind == "stroke").collect()
    }

    fn piece(kind: &str, text: &str, x: f32, y: f32) -> Piece {
        Piece { kind: kind.into(), text: text.into(), x, y, width: None, height: None, color: None, shape: None, size: None, rotate: None }
    }

    #[test]
    fn a_filled_shape_is_one_stroke_in_its_own_colour() {
        let p = painted(r##"<svg viewBox="0 0 800 400" xmlns="http://www.w3.org/2000/svg"><rect x="0" y="0" width="800" height="400" fill="#87ceeb"/></svg>"##);
        let s = strokes(&p);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].data["color"], "#87ceeb");
        assert_eq!(s[0].data["opacity"], 1.0, "a picture's colours are as drawn, not the pen's 0.85");
        assert_eq!(s[0].position.x, 0.0, "a new board keeps the drawing's coordinates");
        assert_eq!(s[0].position.y, 0.0);
        assert_eq!(s[0].data["width"], 800.0);
        assert_eq!(s[0].data["height"], 400.0);
        let d = s[0].data["svgPath"].as_str().unwrap();
        assert!(d.starts_with("M0 0"), "drawn from its own corner: {d}");
    }

    #[test]
    fn things_keep_their_places_and_their_order() {
        let p = painted(r##"<svg viewBox="0 0 800 460" xmlns="http://www.w3.org/2000/svg">
            <rect width="800" height="460" fill="#bfe3ff"/>
            <circle cx="650" cy="92" r="42" fill="#f59e0b"/>
            <path d="M0 250 L180 95 L345 250Z" fill="#6b7f99"/>
        </svg>"##);
        let s = strokes(&p);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].data["color"], "#bfe3ff", "the sky first, so behind");
        assert_eq!(s[1].data["color"], "#f59e0b");
        // The sun: centre 650,92, radius 42 → its box starts at 608,50.
        assert!((s[1].position.x - 608.0).abs() < 0.5, "{:?}", s[1].position);
        assert!((s[1].position.y - 50.0).abs() < 0.5, "{:?}", s[1].position);
        assert_eq!(s[2].data["color"], "#6b7f99");
    }

    #[test]
    fn an_outline_becomes_the_shape_of_the_line() {
        let p = painted(r##"<svg viewBox="0 0 400 200" xmlns="http://www.w3.org/2000/svg">
            <path d="M0 100 Q200 0 400 100" fill="none" stroke="#1a66cc" stroke-width="6"/>
        </svg>"##);
        let s = strokes(&p);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].data["color"], "#1a66cc");
        let h = s[0].data["height"].as_f64().unwrap();
        assert!(h > 50.0 && h < 60.0, "a curve 50 high and 6 wide: {h}");
        assert!(s[0].data["svgPath"].as_str().unwrap().contains('Z'), "an outline is closed");
    }

    #[test]
    fn a_shape_with_both_is_its_fill_then_its_outline() {
        let p = painted(r##"<svg viewBox="0 0 200 200" xmlns="http://www.w3.org/2000/svg">
            <circle cx="100" cy="100" r="50" fill="#ffffff" stroke="#222222" stroke-width="4"/>
        </svg>"##);
        let s = strokes(&p);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].data["color"], "#ffffff");
        assert_eq!(s[1].data["color"], "#222222");
    }

    #[test]
    fn groups_move_and_fade_what_is_in_them() {
        let p = painted(r##"<svg viewBox="0 0 400 400" xmlns="http://www.w3.org/2000/svg">
            <rect width="400" height="400" fill="#eeeeee"/>
            <g transform="translate(100 50)" opacity="0.5"><rect width="10" height="10" fill="#ff0000"/></g>
        </svg>"##);
        let s = strokes(&p);
        assert_eq!(s[1].position.x, 100.0);
        assert_eq!(s[1].position.y, 50.0);
        assert_eq!(s[1].data["opacity"], 0.5);
    }

    #[test]
    fn styles_and_named_colours_are_read() {
        let p = painted(r##"<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
            <style>.lake { fill: steelblue }</style>
            <rect class="lake" width="100" height="40"/>
            <rect y="50" width="100" height="40" style="fill: rgb(10, 20, 30)"/>
        </svg>"##);
        let s = strokes(&p);
        assert_eq!(s[0].data["color"], "#4682b4");
        assert_eq!(s[1].data["color"], "#0a141e");
    }

    #[test]
    fn a_gradient_is_one_colour_and_it_says_so() {
        let p = painted(r##"<svg viewBox="0 0 400 400" xmlns="http://www.w3.org/2000/svg">
            <defs><linearGradient id="sky"><stop offset="0" stop-color="#000000"/><stop offset="1" stop-color="#ffffff"/></linearGradient></defs>
            <rect width="400" height="400" fill="url(#sky)"/>
        </svg>"##);
        let s = strokes(&p);
        assert_eq!(s[0].data["color"], "#808080");
        assert!(p.left_out.iter().any(|n| n.starts_with("gradients")), "{:?}", p.left_out);
    }

    #[test]
    fn theme_variables_are_reported_not_guessed() {
        let p = painted(r##"<svg viewBox="0 0 400 400" xmlns="http://www.w3.org/2000/svg"><rect width="400" height="400" fill="var(--c4)"/></svg>"##);
        assert!(p.left_out.iter().any(|n| n.contains("CSS variables")), "{:?}", p.left_out);
    }

    #[test]
    fn text_becomes_board_text_over_the_ink() {
        let p = painted(r##"<svg viewBox="0 0 800 400" xmlns="http://www.w3.org/2000/svg">
            <rect width="800" height="400" fill="#bfe3ff"/>
            <text x="400" y="200" font-size="32" fill="#333" text-anchor="middle" font-weight="bold">Hồ <tspan>Gươm</tspan></text>
        </svg>"##);
        assert_eq!(p.words, 1);
        let t = p.nodes.iter().find(|n| n.kind == "text").expect("a text item");
        assert_eq!(t.data["label"], "Hồ Gươm");
        assert_eq!(t.data["fontSize"], 32.0);
        assert_eq!(t.data["color"], "#333333");
        assert_eq!(t.data["textAlign"], "center");
        assert_eq!(t.data["fontWeight"], "bold");
        assert_eq!(t.data["z"], 1);
        // Centred on x = 400 of the picture.
        let w = t.data["width"].as_f64().unwrap();
        assert!((t.position.x + w / 2.0 - 400.0).abs() < 1.0, "{} + {w}/2", t.position.x);
    }

    #[test]
    fn text_in_a_group_moves_with_it() {
        let p = painted(r##"<svg viewBox="0 0 400 400" xmlns="http://www.w3.org/2000/svg">
            <rect width="400" height="400" fill="#fff"/>
            <g transform="translate(100 0)"><text x="0" y="100" font-size="20">A</text></g>
            <defs><text x="0" y="0">hidden</text></defs>
        </svg>"##);
        assert_eq!(p.words, 1);
        let t = p.nodes.iter().find(|n| n.kind == "text").unwrap();
        assert!((t.position.x - (100.0 - PAD_X as f64)).abs() < 0.5, "{}", t.position.x);
    }

    #[test]
    fn a_tiny_picture_is_brought_up_to_a_readable_size() {
        let p = painted(r##"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><rect width="24" height="12" fill="#000"/></svg>"##);
        let s = strokes(&p);
        assert_eq!(s[0].data["width"], 1000.0);
        assert_eq!(s[0].data["height"], 500.0);
        assert!((p.scale - 1000.0 / 24.0).abs() < 0.01, "and says so: {}", p.scale);
    }

    #[test]
    fn holes_are_kept() {
        let p = painted(r##"<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg"><path fill-rule="evenodd" d="M0 0H100V100H0Z M25 25H75V75H25Z" fill="#000"/></svg>"##);
        assert_eq!(strokes(&p)[0].data["fillRule"], "evenodd");
    }

    #[test]
    fn nothing_outside_the_string_is_read() {
        let p = draw(
            Some(r##"<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><rect width="100" height="100" fill="#fff"/><image href="/etc/hosts" width="10" height="10"/><image xlink:href="https://example.com/a.png" width="10" height="10"/></svg>"##),
            &[],
            false,
            NOW,
        )
        .expect("painted");
        assert_eq!(p.nodes.len(), 1);
    }

    #[test]
    fn refuses_what_it_cannot_draw() {
        assert!(draw(Some("not svg"), &[], false, NOW).is_err());
        assert!(draw(Some(r#"<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg"></svg>"#), &[], false, NOW).is_err());
        let huge = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{}</svg>"#, "<!-- -->".repeat(MAX_SVG));
        assert!(draw(Some(&huge), &[], false, NOW).err().expect("refused").contains("at most"));
        assert!(draw(None, &[], false, NOW).err().expect("refused").contains("Nothing to draw"));
        assert!(draw(None, &[piece("arrow", "x", 0.0, 0.0)], false, NOW).is_err());
        assert!(draw(None, &[piece("sticky", " ", 0.0, 0.0)], false, NOW).is_err());
    }

    #[test]
    fn the_landscape_syn_drew_in_the_chat_draws_on_a_board() {
        // gpt-6-luna's drawing from 2026-11-19, in its own colours this time.
        let svg = r##"<svg viewBox="0 0 800 460" xmlns="http://www.w3.org/2000/svg">
<rect x="0" y="0" width="800" height="460" fill="#cfe8ff"/>
<circle cx="650" cy="92" r="42" fill="#f59e0b"/>
<path d="M0 210 Q120 145 230 205 T450 190 T800 210 L800 280 L0 280Z" fill="#8fb08a"/>
<path d="M0 250 L180 95 L345 250 L495 125 L690 270 L800 205 L800 310 L0 310Z" fill="#5b6b82"/>
<rect x="0" y="285" width="800" height="175" fill="#3f8fd2"/>
<path d="M0 355 Q150 330 300 355 T600 350 T800 350" fill="none" stroke="#ffffff" stroke-width="3" opacity=".8"/>
<g fill="#1f6b45"><path d="M95 300 L130 190 L165 300Z"/><rect x="125" y="295" width="10" height="30"/></g>
<g fill="#ffffff" opacity=".9"><ellipse cx="170" cy="75" rx="38" ry="14"/><ellipse cx="195" cy="70" rx="28" ry="18"/></g>
</svg>"##;
        let p = painted(svg);
        assert_eq!(p.strokes, 10);
        assert!(p.left_out.is_empty(), "{:?}", p.left_out);
    }

    #[test]
    fn an_id_names_what_it_marks_and_a_named_group_moves_whole() {
        let p = painted(r##"<svg viewBox="0 0 400 400" xmlns="http://www.w3.org/2000/svg">
            <rect id="sky" width="400" height="200" fill="#bfe3ff"/>
            <g id="boat"><path d="M100 300 H300 L260 340 H140Z" fill="#7c4a2d"/><path id="sail" d="M200 300 V200 L260 300Z" fill="#fff8e1"/>
              <text x="200" y="360" font-size="14">Thuyền</text></g>
            <rect width="10" height="10" fill="#000"/>
        </svg>"##);
        let s = strokes(&p);
        let name = |i: &Item| i.data.get("name").and_then(Value::as_str).map(str::to_string);
        let group = |i: &Item| i.data.get("groupId").and_then(Value::as_str).map(str::to_string);
        assert_eq!(name(s[0]).as_deref(), Some("sky"));
        assert_eq!(name(s[1]).as_deref(), Some("boat"));
        assert_eq!(name(s[2]).as_deref(), Some("boat/sail"));
        assert_eq!(name(s[3]), None, "a shape nobody named is loose ink");
        assert_eq!(group(s[1]), group(s[2]), "the boat is one group");
        assert_ne!(group(s[0]), group(s[1]));
        let t = p.nodes.iter().find(|n| n.kind == "text").unwrap();
        assert_eq!(name(t).as_deref(), Some("boat"));
        assert_eq!(group(t), group(s[1]), "its words go with it");
        assert_eq!(p.named, vec!["sky", "boat"]);
        assert!(answers_to(s[2], "boat") && answers_to(s[2], "Sail") && answers_to(s[2], "boat/sail"));
        assert!(!answers_to(s[0], "boat") && !answers_to(s[3], ""));
    }

    #[test]
    fn onto_a_board_the_drawing_is_in_the_boards_own_coordinates() {
        // A ring round a box at 400,200: not moved to a corner, not scaled up.
        let p = onto(r##"<svg viewBox="0 0 100 100" width="1000" height="1000" xmlns="http://www.w3.org/2000/svg"><rect x="40" y="20" width="12" height="6" fill="#e11d48"/></svg>"##, &[]);
        let s = strokes(&p);
        assert_eq!((s[0].position.x, s[0].position.y), (40.0, 20.0), "viewBox units, not the size it asked for");
        assert_eq!(s[0].data["width"], 12.0);
    }

    #[test]
    fn notes_boxes_and_text_are_the_boards_own_items() {
        let mut yellow = piece("sticky", "Hoa", 100.0, 100.0);
        yellow.rotate = Some(-4.0);
        let mut blueish = piece("sticky", "Sách", 330.0, 120.0);
        blueish.color = Some("#93c5fd".into());
        let mut field = piece("box", "Email", 100.0, 400.0);
        field.width = Some(300.0);
        field.height = Some(40.0);
        let mut tiny = piece("text", "**Đăng nhập**", 100.0, 40.0);
        tiny.size = Some(8.0);
        let p = onto("", &[yellow, blueish, field, tiny]);
        assert_eq!(p.pieces, 4);
        let n = &p.nodes;
        assert_eq!((n[0].kind.as_str(), n[0].data["color"].as_str()), ("sticky", Some("yellow")));
        assert_eq!(n[0].data["width"], 200.0);
        assert_eq!(n[0].data["rotation"], -4.0);
        assert_eq!(n[1].data["color"], "blue", "a hex colour goes to the nearest paper");
        assert_eq!((n[2].kind.as_str(), n[2].label()), ("shape", "Email"));
        assert_eq!(n[2].data["fillColor"], BOX_FILL);
        assert_eq!((n[2].width(), n[2].height()), (300.0, 40.0));
        assert_eq!(n[3].data["label"], "**Đăng nhập**", "text the drawing types is Markdown");
        assert_eq!(n[3].data["fontSize"], 12.0, "never under the floor");
        assert!(n.iter().all(|i| i.data["z"] == 1), "over any ink");
        assert_eq!((n[0].position.x, n[0].position.y), (100.0, 100.0));
    }

    #[test]
    fn on_a_new_board_notes_and_ink_share_the_drawings_coordinates() {
        let p = draw(
            Some(r##"<svg viewBox="0 0 800 500" xmlns="http://www.w3.org/2000/svg"><rect x="100" y="100" width="600" height="300" fill="#eee"/></svg>"##),
            &[piece("sticky", "A", 120.0, 120.0)],
            false,
            NOW,
        )
        .expect("drawn");
        let sticky = p.nodes.iter().find(|n| n.kind == "sticky").unwrap();
        assert_eq!((sticky.position.x, sticky.position.y), (120.0, 120.0), "where the drawing put it");
        assert_eq!(p.scale, 1.0);
    }
}
