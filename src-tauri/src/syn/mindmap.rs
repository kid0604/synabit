//! Mind maps on a board, as Syn makes, grows and reads them.
//!
//! The app's own mind maps (`src/mini-apps/whiteboard/mindmap.ts`) are
//! `mindmap` items joined by lines from parent to child; a line marked
//! `relation` is a link between two ideas, not a branch. Nothing else marks a
//! map, so the tree is worked out from the items and lines each time. What is
//! here keeps to the app's rules — its colours, its sizes, its layout — so a
//! map Syn draws is one the person goes on with by Tab and Enter, and the
//! app's next tidy leaves it where it is.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};

use crate::syn::board::{new_board, Board, Item, Link, Point};

/// The app's palette (`MINDMAP_COLORS`): the central idea's first, then one
/// for each main branch in turn.
pub const COLORS: [&str; 10] = [
    "#7c3aed", "#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#ec4899", "#8b5cf6", "#06b6d4", "#84cc16", "#f97316",
];

/// The most ideas one map is drawn with: past this it is a document.
pub const MAX_IDEAS: usize = 150;

const H_GAP: f64 = 64.0;
const V_GAP: f64 = 16.0;
/// The widest an item grows before its words wrap (`.wb-mindmap-node`).
const MAX_WIDTH: f64 = 320.0;

/// An idea and what grows from it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Idea {
    pub label: String,
    pub children: Vec<Idea>,
    /// A checkbox: `Some(done)`.
    pub task: Option<bool>,
    /// A mark before the words (`ICONS`).
    pub icon: Option<String>,
}

/// The marks an idea can carry, as the app has them (`ICONS` in mindmap.ts).
pub const ICONS: [&str; 16] = ["⭐", "❗", "❓", "💡", "⚠️", "🔥", "🎯", "📌", "💰", "📅", "👤", "🚩", "1️⃣", "2️⃣", "3️⃣", "✅"];

/// A list line's checkbox and mark, taken off its words.
fn marks(words: &str) -> (Option<bool>, Option<String>, &str) {
    let mut rest = words.trim();
    let mut task = None;
    for (b, done) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
        if let Some(r) = rest.strip_prefix(b) {
            task = Some(done);
            rest = r.trim_start();
        }
    }
    let icon = ICONS.iter().find(|i| rest.starts_with(&format!("{i} "))).map(|i| i.to_string());
    if let Some(i) = &icon {
        rest = rest[i.len()..].trim_start();
    }
    (task, icon, rest)
}

impl Idea {
    fn count(&self) -> usize {
        1 + self.children.iter().map(Idea::count).sum::<usize>()
    }
}

/// An indented list, or headings with lists under them, read as a tree — what
/// a markmap block holds and what a model writes when asked for a map. A line
/// with no marker counts by its indent. One top idea is the root; several
/// share one named `title`. `None` when there is nothing in it.
pub fn read(text: &str, title: &str) -> Option<Idea> {
    let mut top = Idea { label: title.trim().to_string(), ..Idea::default() };
    // The path from the top to the idea last added, with each one's depth.
    let mut path: Vec<usize> = Vec::new();
    let mut depths: Vec<usize> = Vec::new();
    let mut heading = 0usize;
    let mut fenced = false;
    let mut count = 0usize;
    // A markmap block's options, between `---` lines at its top, are not ideas.
    let body = match text.trim_start().strip_prefix("---\n") {
        Some(rest) => rest.split_once("\n---").map(|(_, after)| after).unwrap_or(rest),
        None => text,
    };
    for raw in body.lines() {
        let line = raw.replace('\t', "  ");
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || trimmed.is_empty() || trimmed == "---" {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let (depth, words) = if let Some(rest) = heading_text(trimmed) {
            heading = rest.0 * 100;
            (heading, rest.1)
        } else {
            (heading + 1 + indent, list_text(trimmed))
        };
        let (task, icon, words) = marks(words);
        let label = plain(words);
        if label.is_empty() {
            continue;
        }
        if count >= MAX_IDEAS {
            break;
        }
        count += 1;
        while depths.last().is_some_and(|&d| d >= depth) {
            depths.pop();
            path.pop();
        }
        let mut at = &mut top;
        for &i in &path {
            at = &mut at.children[i];
        }
        at.children.push(Idea { label, children: Vec::new(), task, icon });
        path.push(at.children.len() - 1);
        depths.push(depth);
    }
    // One top idea is the centre, whatever the title says; several share one
    // named by it.
    match top.children.len() {
        0 => None,
        1 => top.children.pop(),
        _ => Some(top),
    }
}

/// `## Words` as its level and words.
fn heading_text(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        Some((hashes, line[hashes..].trim().trim_end_matches('#').trim()))
    } else {
        None
    }
}

/// A list line's words, without its marker: `-`, `*`, `+`, `1.`, `1)`.
fn list_text(line: &str) -> &str {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest;
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return rest;
        }
    }
    line
}

/// Words without Markdown's marks: a checkbox, emphasis, code, a link's target.
fn plain(words: &str) -> String {
    let mut s = words.trim();
    for check in ["[ ] ", "[x] ", "[X] "] {
        if let Some(rest) = s.strip_prefix(check) {
            s = rest;
        }
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '`' | '~' => {}
            '_' if out.is_empty() || out.ends_with(' ') || chars.peek().is_none_or(|n| *n == ' ') => {}
            // `[words](target)` is its words; `[[Note]]` its note's name.
            '[' => {}
            ']' => {
                if chars.peek() == Some(&'(') {
                    for c in chars.by_ref() {
                        if c == ')' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(c),
        }
    }
    let one_line = out.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.chars().take(120).collect()
}

/// Whether a line is a branch of a map rather than a link drawn between two ideas.
fn is_branch(minds: &HashSet<&str>, link: &Link) -> bool {
    minds.contains(link.source.as_str())
        && minds.contains(link.target.as_str())
        && link.source != link.target
        && link.data.get("relation").and_then(Value::as_bool) != Some(true)
}

/// Each mind-map item's children, in the order their lines were drawn.
pub fn children(board: &Board) -> HashMap<String, Vec<String>> {
    let minds: HashSet<&str> = board.nodes.iter().filter(|n| n.kind == "mindmap").map(|n| n.id.as_str()).collect();
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for e in &board.edges {
        if !is_branch(&minds, e) {
            continue;
        }
        let list = out.entry(e.source.clone()).or_default();
        if !list.contains(&e.target) {
            list.push(e.target.clone());
        }
    }
    out
}

/// Each mind-map item's parent, by the branches only.
fn parents(kids: &HashMap<String, Vec<String>>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (p, list) in kids {
        for k in list {
            out.entry(k.clone()).or_insert_with(|| p.clone());
        }
    }
    out
}

/// The root of the map an item is in.
pub fn root_of(board: &Board, id: &str) -> String {
    let up = parents(&children(board));
    let mut seen = HashSet::new();
    let mut at = id.to_string();
    while let Some(p) = up.get(&at) {
        if !seen.insert(at.clone()) {
            break;
        }
        at = p.clone();
    }
    at
}

/// The roots of every map on the board, in board order: mind-map items no branch leads to.
pub fn roots(board: &Board) -> Vec<String> {
    let up = parents(&children(board));
    board.nodes.iter().filter(|n| n.kind == "mindmap" && !up.contains_key(&n.id)).map(|n| n.id.clone()).collect()
}

fn level(item: &Item) -> u64 {
    item.data.get("level").and_then(Value::as_u64).unwrap_or(1)
}

/// The size the app guesses for an item before it is drawn (`estimateSize`).
fn size_of(item: &Item) -> (f64, f64) {
    let root = level(item) == 0;
    let per_char = if root { 9.0 } else { 8.0 };
    // `#tags` are drawn as chips under the words; a box and a mark before them.
    let words: Vec<&str> = item.label().split(' ').collect();
    let tagged = words.iter().any(|w| w.len() > 1 && w.starts_with('#'));
    let label: String = words.iter().filter(|w| !(w.len() > 1 && w.starts_with('#'))).copied().collect::<Vec<_>>().join(" ");
    let extra = if item.data.get("task").and_then(Value::as_bool) == Some(true) { 22.0 } else { 0.0 }
        + if item.data.get("icon").and_then(Value::as_str).is_some() { 22.0 } else { 0.0 };
    let lines: Vec<usize> = label.trim().split('\n').map(|l| l.chars().count()).collect();
    let longest = lines.iter().copied().max().unwrap_or(0) as f64;
    let width = (longest * per_char + 40.0 + extra).min(MAX_WIDTH).max(if root { 140.0 } else { 100.0 });
    let wrapped: f64 = lines.iter().map(|&l| ((l as f64 * per_char) / (MAX_WIDTH - 40.0 - extra)).ceil().max(1.0)).sum();
    let line = if root { 20.0 } else { 18.0 };
    (width, (if root { 44.0 } else { 38.0 }) + (wrapped - 1.0) * line + if tagged { 22.0 } else { 0.0 })
}

/// A map's layout, kept on its root: `down` (an org chart), `right` (a logic
/// chart) or the default, branches on both sides.
pub fn layout_of(board: &Board, root_id: &str) -> &'static str {
    match board.nodes.iter().find(|n| n.id == root_id).and_then(|n| n.data.get("layout")).and_then(Value::as_str) {
        Some("down") => "down",
        Some("right") => "right",
        _ => "map",
    }
}

/// The handles a branch joins by, as `handlesFor` in the app.
fn handles(layout: &str, left: bool) -> (&'static str, &'static str) {
    match (layout, left) {
        ("down", _) => ("bottom-source", "top-target"),
        (_, true) => ("left-source", "right-target"),
        _ => ("right-source", "left-target"),
    }
}

/// `tidy` for an org chart: each row under its parent, centred on it.
fn tidy_down(board: &mut Board, root_id: &str, now_ms: i64) {
    const V: f64 = 56.0;
    const H: f64 = 24.0;
    let kids = children(board);
    let index: HashMap<String, usize> = board.nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();
    let Some(&root) = index.get(root_id) else { return };
    let nodes = &board.nodes;
    let kids_of = |i: usize| -> Vec<usize> {
        if nodes[i].data.get("collapsed").and_then(Value::as_bool) == Some(true) {
            return Vec::new();
        }
        kids.get(&nodes[i].id).map(|l| l.iter().filter_map(|k| index.get(k).copied()).collect()).unwrap_or_default()
    };
    let pinned = |i: usize| nodes[i].data.get("pinned").and_then(Value::as_bool) == Some(true);
    fn width(i: usize, nodes: &[Item], kids_of: &dyn Fn(usize) -> Vec<usize>, pinned: &dyn Fn(usize) -> bool, memo: &mut HashMap<usize, f64>, seen: &mut HashSet<usize>) -> f64 {
        if let Some(&w) = memo.get(&i) {
            return w;
        }
        let own = size_of(&nodes[i]).0;
        if !seen.insert(i) {
            return own;
        }
        let ks: Vec<usize> = kids_of(i).into_iter().filter(|&k| !pinned(k)).collect();
        let row: f64 = ks.iter().map(|&k| width(k, nodes, kids_of, pinned, memo, seen)).sum::<f64>() + H * ks.len().saturating_sub(1) as f64;
        let w = own.max(row);
        memo.insert(i, w);
        w
    }
    let mut memo = HashMap::new();
    let mut out: HashMap<usize, Point> = HashMap::new();
    let mut seen = HashSet::from([root]);
    let mut work = vec![(root, nodes[root].position)];
    while let Some((parent, at)) = work.pop() {
        let (pw, ph) = size_of(&nodes[parent]);
        let all = kids_of(parent);
        for &k in all.iter().filter(|&&k| pinned(k)) {
            if seen.insert(k) {
                work.push((k, nodes[k].position));
            }
        }
        let row: Vec<usize> = all.into_iter().filter(|&k| !pinned(k)).collect();
        if row.is_empty() {
            continue;
        }
        let mut s = HashSet::new();
        let total: f64 = row.iter().map(|&k| width(k, nodes, &kids_of, &pinned, &mut memo, &mut s)).sum::<f64>() + H * (row.len() - 1) as f64;
        let mut x = at.x + pw / 2.0 - total / 2.0;
        for k in row {
            let w = width(k, nodes, &kids_of, &pinned, &mut memo, &mut s);
            if !seen.insert(k) {
                continue;
            }
            let pos = Point { x: (x + w / 2.0 - size_of(&nodes[k]).0 / 2.0).round(), y: (at.y + ph + V).round() };
            out.insert(k, pos);
            work.push((k, pos));
            x += w + H;
        }
    }
    for (i, p) in out {
        let n = &mut board.nodes[i];
        if n.position.x != p.x || n.position.y != p.y {
            n.position = p;
            n.updated = Some(now_ms);
        }
    }
}

/// Lay out the map from `root_id` as the app's tidy does (`tidyTree`): each
/// branch in a column beside its parent, as tall as it needs; the root where
/// it is, a folded branch as small as its item, an item placed by hand
/// (`pinned`) left where it was put with its branch laid out from there.
pub fn tidy(board: &mut Board, root_id: &str, now_ms: i64) {
    if layout_of(board, root_id) == "down" {
        tidy_down(board, root_id, now_ms);
        return;
    }
    let kids = children(board);
    let index: HashMap<String, usize> = board.nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();
    let Some(&root) = index.get(root_id) else { return };
    let nodes = &board.nodes;
    let kids_of = |id: &str| -> Vec<usize> {
        let Some(&i) = index.get(id) else { return Vec::new() };
        if nodes[i].data.get("collapsed").and_then(Value::as_bool) == Some(true) {
            return Vec::new();
        }
        kids.get(id).map(|l| l.iter().filter_map(|k| index.get(k).copied()).collect()).unwrap_or_default()
    };
    let pinned = |i: usize| nodes[i].data.get("pinned").and_then(Value::as_bool) == Some(true);

    fn height(
        i: usize,
        nodes: &[Item],
        kids_of: &dyn Fn(&str) -> Vec<usize>,
        pinned: &dyn Fn(usize) -> bool,
        memo: &mut HashMap<usize, f64>,
        seen: &mut HashSet<usize>,
    ) -> f64 {
        if let Some(&h) = memo.get(&i) {
            return h;
        }
        let own = size_of(&nodes[i]).1;
        if !seen.insert(i) {
            return own;
        }
        let ks: Vec<usize> = kids_of(&nodes[i].id).into_iter().filter(|&k| !pinned(k)).collect();
        let stacked: f64 = ks.iter().map(|&k| height(k, nodes, kids_of, pinned, memo, seen)).sum::<f64>()
            + V_GAP * ks.len().saturating_sub(1) as f64;
        let h = own.max(stacked);
        memo.insert(i, h);
        h
    }

    let mut memo = HashMap::new();
    let mut out: HashMap<usize, Point> = HashMap::new();
    let mut seen: HashSet<usize> = HashSet::from([root]);
    // Parents to lay out, each with where it stands, its children to place and the side.
    let mut work: Vec<(usize, Point, Vec<usize>, bool)> = Vec::new();
    let root_kids = kids_of(root_id);
    let side = |k: &usize| nodes[*k].data.get("direction").and_then(Value::as_str) == Some("left");
    let at = nodes[root].position;
    work.push((root, at, root_kids.iter().copied().filter(|k| side(k)).collect(), true));
    work.push((root, at, root_kids.iter().copied().filter(|k| !side(k)).collect(), false));
    while let Some((parent, at, all, left)) = work.pop() {
        let (pw, ph) = size_of(&nodes[parent]);
        for &k in all.iter().filter(|&&k| pinned(k)) {
            if !seen.insert(k) {
                continue;
            }
            let (kw, _) = size_of(&nodes[k]);
            let on_left = nodes[k].position.x + kw / 2.0 < at.x + pw / 2.0;
            work.push((k, nodes[k].position, kids_of(&nodes[k].id), on_left));
        }
        let placed: Vec<usize> = all.into_iter().filter(|&k| !pinned(k)).collect();
        if placed.is_empty() {
            continue;
        }
        let mut seen_h = HashSet::new();
        let total: f64 = placed.iter().map(|&k| height(k, nodes, &kids_of, &pinned, &mut memo, &mut seen_h)).sum::<f64>()
            + V_GAP * (placed.len() - 1) as f64;
        let mut y = at.y + ph / 2.0 - total / 2.0;
        for k in placed {
            let branch = height(k, nodes, &kids_of, &pinned, &mut memo, &mut seen_h);
            if !seen.insert(k) {
                continue;
            }
            let (kw, kh) = size_of(&nodes[k]);
            let x = if left { at.x - H_GAP - kw } else { at.x + pw + H_GAP };
            let pos = Point { x: x.round(), y: (y + branch / 2.0 - kh / 2.0).round() };
            out.insert(k, pos);
            work.push((k, pos, kids_of(&nodes[k].id), left));
            y += branch + V_GAP;
        }
    }
    for (i, p) in out {
        let n = &mut board.nodes[i];
        if n.position.x != p.x || n.position.y != p.y {
            n.position = p;
            n.updated = Some(now_ms);
        }
    }
}

fn item(id: String, label: &str, colour: &str, level: usize, left: bool, now_ms: i64) -> Item {
    let mut data = Map::new();
    data.insert("label".into(), json!(label));
    data.insert("color".into(), json!(colour));
    data.insert("level".into(), json!(level));
    data.insert("direction".into(), json!(if left { "left" } else { "right" }));
    Item { id, kind: "mindmap".into(), position: Point { x: 0.0, y: 0.0 }, data, updated: Some(now_ms), rest: Map::new() }
}

fn branch(id: String, from: &str, to: &str, left: bool, now_ms: i64) -> Link {
    Link {
        id,
        source: from.to_string(),
        source_handle: Some(if left { "left-source" } else { "right-source" }.into()),
        target: to.to_string(),
        target_handle: Some(if left { "right-target" } else { "left-target" }.into()),
        kind: "default".into(),
        data: Map::new(),
        updated: Some(now_ms),
        rest: Map::new(),
    }
}

/// A new board holding one map, laid out: the root's branches shared between
/// its sides when there are more than three, each main branch a colour of its
/// own — as the app draws a map read from a list (`buildMindmap`).
pub fn draw(title: &str, idea: &Idea, now_ms: i64) -> Board {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut n = 0usize;
    let mut id = |prefix: &str| {
        n += 1;
        format!("{prefix}-{now_ms:x}-{n}")
    };
    let mains = idea.children.len();
    let right = if mains > 3 { mains.div_ceil(2) } else { mains };
    // Depth first, as the app's own builder goes, so the order of lines is
    // the order of the outline.
    let mut stack: Vec<(&Idea, usize, Option<String>, String, bool)> = vec![(idea, 0, None, COLORS[0].to_string(), false)];
    while let Some((at, depth, parent, colour, left)) = stack.pop() {
        let me = id("mind");
        let mut made = item(me.clone(), &at.label, &colour, depth, left, now_ms);
        if let Some(done) = at.task {
            made.data.insert("task".into(), json!(true));
            if done {
                made.data.insert("done".into(), json!(true));
            }
        }
        if let Some(icon) = &at.icon {
            made.data.insert("icon".into(), json!(icon));
        }
        nodes.push(made);
        if let Some(p) = parent {
            edges.push(branch(id("e"), &p, &me, left, now_ms));
        }
        for (i, child) in at.children.iter().enumerate().rev() {
            let (c, l) = if depth == 0 { (COLORS[1 + i % (COLORS.len() - 1)].to_string(), i >= right) } else { (colour.clone(), left) };
            stack.push((child, depth + 1, Some(me.clone()), c, l));
        }
    }
    let root = nodes[0].id.clone();
    let mut board = new_board(title, nodes, edges);
    tidy(&mut board, &root, now_ms);
    board
}

/// Lay a map out another way, as the app's `setLayout`: the layout kept on
/// its root (`down`, `right`, or the default map), each item's side set for
/// it, the lines joined to match, and laid out again.
pub fn lay_out_as(board: &mut Board, root_id: &str, layout: &str, now_ms: i64) {
    let layout = match layout {
        "down" | "org" | "org chart" => "down",
        "right" | "logic" => "right",
        _ => "map",
    };
    let kids = children(board);
    let mains = kids.get(root_id).cloned().unwrap_or_default();
    let right = if layout == "map" && mains.len() > 1 { mains.len().div_ceil(2) } else { mains.len() };
    let mut side: HashMap<String, bool> = HashMap::new();
    let mut stack: Vec<(String, bool)> = mains.iter().enumerate().map(|(i, m)| (m.clone(), i >= right)).collect();
    while let Some((id, left)) = stack.pop() {
        if side.insert(id.clone(), left).is_some() {
            continue;
        }
        for k in kids.get(&id).map(Vec::as_slice).unwrap_or_default() {
            stack.push((k.clone(), left));
        }
    }
    for n in &mut board.nodes {
        if n.id == root_id {
            if layout == "map" {
                n.data.remove("layout");
            } else {
                n.data.insert("layout".into(), json!(layout));
            }
            n.updated = Some(now_ms);
        } else if let Some(&left) = side.get(&n.id) {
            n.data.insert("direction".into(), json!(if left { "left" } else { "right" }));
            n.data.remove("pinned");
            n.updated = Some(now_ms);
        }
    }
    for e in &mut board.edges {
        if let Some(&left) = side.get(&e.target) {
            if kids.get(&e.source).is_some_and(|l| l.contains(&e.target)) {
                let (from, to) = handles(layout, left);
                e.source_handle = Some(from.into());
                e.target_handle = Some(to.into());
                e.updated = Some(now_ms);
            }
        }
    }
    tidy(board, root_id, now_ms);
}

/// How many ideas a map read from `text` has, for saying so.
pub fn size(idea: &Idea) -> usize {
    idea.count()
}

/// A new branch under a mind-map item, and the map laid out again, as the
/// app does on every branch added. Off a central idea it goes on the emptier
/// side in a colour of its own; further out it grows on its parent's side, in
/// its colour. A folded parent is unfolded, so what was added is seen.
pub fn add_under(board: &mut Board, parent_at: usize, label: &str, task: bool, id: String, edge_id: String, now_ms: i64) -> String {
    let kids = children(board);
    let layout = layout_of(board, &root_of(board, &board.nodes[parent_at].id.clone()));
    let parent = board.nodes[parent_at].clone();
    let siblings: Vec<&Item> = kids
        .get(&parent.id)
        .map(|l| l.iter().filter_map(|k| board.nodes.iter().find(|n| &n.id == k)).collect())
        .unwrap_or_default();
    let depth = level(&parent) as usize + 1;
    let (colour, left) = if depth == 1 {
        let lefts = siblings.iter().filter(|s| s.data.get("direction").and_then(Value::as_str) == Some("left")).count();
        let own = parent.data.get("color").and_then(Value::as_str).unwrap_or(COLORS[0]).to_lowercase();
        let taken: HashSet<String> = siblings
            .iter()
            .filter_map(|s| s.data.get("color").and_then(Value::as_str))
            .map(str::to_lowercase)
            .collect();
        let choices: Vec<&str> = COLORS.iter().copied().filter(|c| c.to_lowercase() != own).collect();
        let colour = choices.iter().find(|c| !taken.contains(&c.to_lowercase())).copied().unwrap_or(choices[siblings.len() % choices.len()]);
        (colour.to_string(), layout == "map" && siblings.len() > 3 && lefts < siblings.len() - lefts)
    } else {
        (
            parent.data.get("color").and_then(Value::as_str).unwrap_or(COLORS[1]).to_string(),
            parent.data.get("direction").and_then(Value::as_str) == Some("left"),
        )
    };
    let mut new = item(id.clone(), label, &colour, depth, left, now_ms);
    new.position = parent.position;
    if task {
        new.data.insert("task".into(), json!(true));
    }
    board.nodes.push(new);
    let mut line = branch(edge_id, &parent.id, &id, left, now_ms);
    let (from, to) = handles(layout, left);
    line.source_handle = Some(from.into());
    line.target_handle = Some(to.into());
    board.edges.push(line);
    if board.nodes[parent_at].data.remove("collapsed").is_some() {
        board.nodes[parent_at].updated = Some(now_ms);
    }
    let root = root_of(board, &parent.id);
    tidy(board, &root, now_ms);
    id
}

/// Whether `label` is already a branch straight under this item.
pub fn has_child(board: &Board, parent_id: &str, label: &str) -> bool {
    let wanted = label.trim().to_lowercase();
    children(board)
        .get(parent_id)
        .is_some_and(|l| l.iter().any(|k| board.nodes.iter().any(|n| &n.id == k && n.label().trim().to_lowercase() == wanted)))
}

/// Every map on the board as an indented outline, for `read_board`: what is
/// under what, folded branches marked, and how many ideas each holds.
pub fn outlines(board: &Board, max_lines: usize) -> Vec<String> {
    let kids = children(board);
    let by_id: HashMap<&str, &Item> = board.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut out = Vec::new();
    for root in roots(board) {
        // A lone item no branch leads to and none grows from is writing, not a map.
        if !kids.contains_key(&root) && by_id.get(root.as_str()).is_some_and(|n| level(n) != 0) {
            continue;
        }
        let mut lines = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![(root.clone(), 0usize)];
        let mut total = 0;
        while let Some((id, depth)) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(n) = by_id.get(id.as_str()) else { continue };
            total += 1;
            if lines.len() < max_lines {
                let words = n.label().split_whitespace().collect::<Vec<_>>().join(" ");
                let task = n.data.get("task").and_then(Value::as_bool) == Some(true);
                let boxed = match (task, n.data.get("done").and_then(Value::as_bool) == Some(true)) {
                    (true, true) => "[x] ",
                    (true, false) => "[ ] ",
                    _ => "",
                };
                let icon = n.data.get("icon").and_then(Value::as_str).map(|i| format!("{i} ")).unwrap_or_default();
                let words = format!("{boxed}{icon}{words}");
                let folded = if n.data.get("collapsed").and_then(Value::as_bool) == Some(true) { " (folded)" } else { "" };
                let linked = if n.data.get("link").and_then(Value::as_str).is_some_and(|l| l.starts_with("synabit://note/")) { " (links to a note)" } else { "" };
                let linked = if linked.is_empty() && n.data.get("link").and_then(Value::as_str).is_some_and(|l| l.starts_with("synabit://task/")) { " (a task in Tasks)" } else { linked };
                lines.push(format!("{}- {}{folded}{linked}", "  ".repeat(depth), words));
            }
            if let Some(list) = kids.get(&id) {
                for k in list.iter().rev() {
                    stack.push((k.clone(), depth + 1));
                }
            }
        }
        if total > lines.len() {
            lines.push(format!("  …and {} more ideas.", total - lines.len()));
        }
        match layout_of(board, &root) {
            "down" => lines.insert(0, "(laid out as an org chart, downwards)".into()),
            "right" => lines.insert(0, "(laid out as a logic chart, to the right)".into()),
            _ => {}
        }
        out.push(lines.join("\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    fn labels(i: &Idea) -> Vec<String> {
        let mut out = vec![i.label.clone()];
        for c in &i.children {
            out.extend(labels(c).into_iter().map(|l| format!(">{l}")));
        }
        out
    }

    #[test]
    fn an_outline_is_read_as_a_tree() {
        let idea = read("- Trip\n  - Before\n    - [x] **Book** flights\n  - During\n\t\tSee the [old town](https://x.example)\n", "").unwrap();
        assert_eq!(labels(&idea), ["Trip", ">Before", ">>Book flights", ">During", ">>See the old town"]);
        // Headings are levels, lists under them below; several tops share the title.
        let idea = read("# A\n- a1\n# B\n## B1\n- b", "Map").unwrap();
        assert_eq!(labels(&idea), ["Map", ">A", ">>a1", ">B", ">>B1", ">>>b"]);
        assert!(read("\n\n", "x").is_none());
        // A markmap block's options are left out.
        let idea = read("---\nmarkmap:\n  maxWidth: 300\n---\n# Root\n- leaf", "").unwrap();
        assert_eq!(labels(&idea), ["Root", ">leaf"]);
    }

    #[test]
    fn a_map_is_drawn_balanced_coloured_and_apart() {
        let idea = read("- Centre\n  - A\n    - a\n  - B\n  - C\n  - D\n  - E", "").unwrap();
        let board = draw("Plan", &idea, NOW);
        assert_eq!(board.nodes.len(), 7);
        assert_eq!(board.edges.len(), 6);
        let by = |l: &str| board.nodes.iter().find(|n| n.label() == l).unwrap();
        let centre = by("Centre");
        assert_eq!(level(centre), 0);
        assert_eq!(centre.data["color"], COLORS[0]);
        for l in ["A", "B", "C"] {
            assert!(by(l).position.x > centre.position.x, "{l} on the right");
        }
        for l in ["D", "E"] {
            assert!(by(l).position.x < centre.position.x, "{l} on the left");
            assert_eq!(by(l).data["direction"], "left");
        }
        assert!(by("a").position.x > by("A").position.x);
        assert_eq!(by("a").data["color"], by("A").data["color"]);
        let colours: HashSet<&str> = ["A", "B", "C", "D", "E"].iter().map(|l| by(l).data["color"].as_str().unwrap()).collect();
        assert_eq!(colours.len(), 5);
        // Siblings on one side do not overlap.
        let (a, b) = (by("A"), by("B"));
        assert!(b.position.y >= a.position.y + size_of(a).1);
        // The app reads it back as the same tree.
        assert_eq!(roots(&board), vec![centre.id.clone()]);
        assert_eq!(outlines(&board, 50)[0], "- Centre\n  - A\n    - a\n  - B\n  - C\n  - D\n  - E");
    }

    #[test]
    fn a_branch_added_under_an_item_grows_from_it_and_the_map_makes_room() {
        let idea = read("- Centre\n  - A\n    - a1\n  - B", "").unwrap();
        let mut board = draw("Plan", &idea, NOW);
        let at = board.nodes.iter().position(|n| n.label() == "A").unwrap();
        board.nodes[at].data.insert("collapsed".into(), json!(true));
        let id = add_under(&mut board, at, "a2", false, "mind-new".into(), "e-new".into(), NOW + 1);
        let new = board.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(level(new), 2);
        assert_eq!(new.data["color"], board.nodes[at].data["color"]);
        assert!(!board.nodes[at].data.contains_key("collapsed"), "unfolded to show it");
        let a1 = board.nodes.iter().find(|n| n.label() == "a1").unwrap();
        assert!(new.position.y > a1.position.y, "below its sibling");
        assert_eq!(new.position.x, a1.position.x);
        assert!(has_child(&board, &board.nodes[at].id.clone(), "A2"));
        // Off the centre: a colour none of the main branches has.
        let centre = board.nodes.iter().position(|n| n.label() == "Centre").unwrap();
        let c = add_under(&mut board, centre, "C", false, "mind-c".into(), "e-c".into(), NOW + 2);
        let colour = board.nodes.iter().find(|n| n.id == c).unwrap().data["color"].clone();
        let others: Vec<Value> = ["A", "B"].iter().map(|l| board.nodes.iter().find(|n| n.label() == *l).unwrap().data["color"].clone()).collect();
        assert!(!others.contains(&colour) && colour != COLORS[0]);
    }

    #[test]
    fn a_pinned_item_stays_where_it_was_put() {
        let idea = read("- Centre\n  - A\n  - B", "").unwrap();
        let mut board = draw("Plan", &idea, NOW);
        let b = board.nodes.iter().position(|n| n.label() == "B").unwrap();
        board.nodes[b].position = Point { x: 900.0, y: -400.0 };
        board.nodes[b].data.insert("pinned".into(), json!(true));
        let root = roots(&board)[0].clone();
        tidy(&mut board, &root, NOW + 1);
        assert_eq!((board.nodes[b].position.x, board.nodes[b].position.y), (900.0, -400.0));
    }

    #[test]
    fn ticks_and_marks_are_read_drawn_and_said() {
        let idea = read("- Launch\n  - [x] 🎯 Pick a date\n  - [ ] Book the hall #venue\n  - Ideas", "").unwrap();
        assert_eq!(idea.children[0].task, Some(true));
        assert_eq!(idea.children[0].icon.as_deref(), Some("🎯"));
        assert_eq!(idea.children[0].label, "Pick a date");
        assert_eq!(idea.children[1].task, Some(false));
        assert_eq!(idea.children[2].task, None);
        let board = draw("Launch", &idea, NOW);
        let by = |l: &str| board.nodes.iter().find(|n| n.label().starts_with(l)).unwrap();
        assert_eq!(by("Pick").data["done"], true);
        assert_eq!(by("Pick").data["icon"], "🎯");
        assert!(!by("Book").data.contains_key("done"));
        assert_eq!(outlines(&board, 50)[0], "- Launch\n  - [x] 🎯 Pick a date\n  - [ ] Book the hall #venue\n  - Ideas");
        // A tag is a chip under the words: the item is taller for it.
        assert!(size_of(by("Book")).1 > size_of(by("Ideas")).1);
    }

    #[test]
    fn an_org_chart_grows_downwards() {
        let idea = read("- CEO\n  - CTO\n    - Dev\n  - CFO", "").unwrap();
        let mut board = draw("Org", &idea, NOW);
        let root = roots(&board)[0].clone();
        board.nodes.iter_mut().find(|n| n.id == root).unwrap().data.insert("layout".into(), json!("down"));
        tidy(&mut board, &root, NOW + 1);
        let by = |b: &Board, l: &str| b.nodes.iter().find(|n| n.label() == l).unwrap().position;
        let (ceo, cto, cfo, dev) = (by(&board, "CEO"), by(&board, "CTO"), by(&board, "CFO"), by(&board, "Dev"));
        assert!(cto.y > ceo.y && cfo.y == cto.y, "a row under the root");
        assert!(cto.x < cfo.x, "in order across");
        assert!(dev.y > cto.y);
        // A branch added under joins by the bottom and the top.
        let at = board.nodes.iter().position(|n| n.label() == "CFO").unwrap();
        let id = add_under(&mut board, at, "Accounts", true, "m-x".into(), "e-x".into(), NOW + 2);
        let line = board.edges.iter().find(|e| e.target == id).unwrap();
        assert_eq!((line.source_handle.as_deref(), line.target_handle.as_deref()), (Some("bottom-source"), Some("top-target")));
        assert_eq!(board.nodes.iter().find(|n| n.id == id).unwrap().data["task"], true);
        assert!(by(&board, "Accounts").y > by(&board, "CFO").y);
        assert!(outlines(&board, 50)[0].starts_with("(laid out as an org chart, downwards)\n- CEO"));

        // Back to a map, and to the right: sides and joins follow.
        lay_out_as(&mut board, &root, "map", NOW + 3);
        assert!(!board.nodes.iter().find(|n| n.id == root).unwrap().data.contains_key("layout"));
        // Two main branches are shared out, one a side, as the app shares them.
        let cfo = board.nodes.iter().find(|n| n.label() == "CFO").unwrap().id.clone();
        let to_cfo = board.edges.iter().find(|e| e.target == cfo).unwrap();
        assert_eq!(to_cfo.source_handle.as_deref(), Some("left-source"));
        assert!(by(&board, "CFO").x < by(&board, "CEO").x);
        lay_out_as(&mut board, &root, "org chart", NOW + 4);
        assert_eq!(layout_of(&board, &root), "down");
        assert!(board.edges.iter().all(|e| e.source_handle.as_deref() == Some("bottom-source")));
    }

    #[test]
    fn a_relationship_is_not_a_branch() {
        let idea = read("- Centre\n  - A\n  - B", "").unwrap();
        let mut board = draw("Plan", &idea, NOW);
        let (a, b) = (board.nodes[1].id.clone(), board.nodes[2].id.clone());
        let mut rel = branch("rel".into(), &a, &b, false, NOW);
        rel.data.insert("relation".into(), json!(true));
        board.edges.push(rel);
        assert_eq!(outlines(&board, 50)[0], "- Centre\n  - A\n  - B");
    }
}
