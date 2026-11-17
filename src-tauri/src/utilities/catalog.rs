//! What hangs off a PDF's catalog rather than off its pages, carried into a
//! document made of some of its pages (`pdf::assemble`).
//!
//! A merge, a split or a new page order starts a new catalog, and everything
//! the old one held was lost with it: the bookmarks of a 300-page manual, the
//! "i, ii, iii, 1, 2" its pages were numbered by, the form fields a page's
//! widgets belong to, the named places its links jump to, and the layers that
//! were off — which, with no configuration left to say so, came out on.
//!
//! Each is kept for the pages that came along and dropped for those that did
//! not: a bookmark to a page left out of a split goes, unless a bookmark under
//! it is to a page that stayed.
//!
//! - **Bookmarks** keep their tree, their titles, colour and style. A merge can
//!   also put each file's under one bookmark named after the file.
//! - **Page labels** follow their page: page 7 of a split that showed "iv"
//!   still shows "iv". Pages from a file with none count from 1 in that file.
//! - **Form fields** a copied widget belongs to are listed again, with the
//!   resources their appearances name. Two files with a field of the same name
//!   would fill in together, so the later one is renamed.
//! - **Named destinations** are kept for pages that came along, so links that
//!   name them still go somewhere.
//! - **Layers** keep their default state.
//!
//! Not kept: the structure tree (tags), whose page references cannot be cut
//! down without rebuilding it, and XFA, which describes the whole old form.

use std::collections::{BTreeMap, HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};

use super::pdf::Copier;

/// How the bookmarks of the new document are laid out.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outline {
    /// Each source's bookmarks, one after another.
    #[default]
    Keep,
    /// One bookmark per source, named as given, with its own under it.
    ByFile(Vec<String>),
}

/// Enough to stop a crafted file from making any of this run for ever.
const MOST_ITEMS: usize = 100_000;
const MOST_DEPTH: usize = 64;

/// What `assemble` knows once the pages are copied.
pub(super) struct Placed<'a> {
    pub sources: &'a [Document],
    pub copiers: &'a mut [Copier],
    /// Per source: an old page and where it went first.
    pub pages_of: &'a [HashMap<ObjectId, ObjectId>],
    /// Per new page, in order: the source and the old page.
    pub order: &'a [(usize, ObjectId)],
    pub pages_id: ObjectId,
}

/// Fill in `catalog` with what the sources' catalogs held for these pages.
pub(super) fn carry(placed: Placed<'_>, target: &mut Document, catalog: &mut Dictionary, outline: &Outline) {
    let named: Vec<HashMap<Vec<u8>, Object>> = placed.sources.iter().map(named_dests).collect();

    if let Some(root) = bookmarks(&placed, &named, target, outline) {
        catalog.set("Outlines", Object::Reference(root));
        catalog.set("PageMode", Object::Name(b"UseOutlines".to_vec()));
    }
    if let Some(labels) = page_labels(&placed) {
        catalog.set("PageLabels", labels);
    }
    keep_named_dests(&placed, &named, target, catalog);
    if let Some(form) = form(placed.sources, placed.copiers, placed.pages_of, target, placed.pages_id) {
        catalog.set("AcroForm", form);
    }
    if let Some(layers) = layers(placed.sources, placed.copiers) {
        catalog.set("OCProperties", layers);
    }
    // The language a screen reader speaks the text in, from the first that says.
    if let Some(lang) = placed.sources.iter().find_map(|d| root_of(d).and_then(|c| c.get(b"Lang").ok()).cloned()) {
        if matches!(lang, Object::String(..)) {
            catalog.set("Lang", lang);
        }
    }
}

fn root_of(doc: &Document) -> Option<&Dictionary> {
    let id = doc.trailer.get(b"Root").and_then(Object::as_reference).ok()?;
    doc.get_dictionary(id).ok()
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        other => other,
    }
}

fn dict<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    resolve(doc, object).as_dict().ok()
}

// ── Destinations ──────────────────────────────────────────────────────────

/// Every named destination of a source, by name: the `/Dests` dictionary
/// (names) and the `/Names /Dests` tree (strings) alike, each to its array.
fn named_dests(doc: &Document) -> HashMap<Vec<u8>, Object> {
    let mut out = HashMap::new();
    let Some(catalog) = root_of(doc) else { return out };
    if let Some(dests) = catalog.get(b"Dests").ok().and_then(|o| dict(doc, o)) {
        for (name, value) in dests.iter() {
            if out.len() >= MOST_ITEMS {
                break;
            }
            out.insert(name.clone(), resolve(doc, value).clone());
        }
    }
    let tree = catalog.get(b"Names").ok().and_then(|o| dict(doc, o)).and_then(|n| n.get(b"Dests").ok()).cloned();
    if let Some(tree) = tree {
        let mut seen = HashSet::new();
        name_tree(doc, &tree, &mut out, &mut seen, 0);
    }
    out
}

fn name_tree(doc: &Document, node: &Object, out: &mut HashMap<Vec<u8>, Object>, seen: &mut HashSet<ObjectId>, depth: usize) {
    if depth > MOST_DEPTH || out.len() >= MOST_ITEMS {
        return;
    }
    if let Object::Reference(id) = node {
        if !seen.insert(*id) {
            return;
        }
    }
    let Some(node) = dict(doc, node) else { return };
    if let Ok(names) = node.get(b"Names").map(|o| resolve(doc, o)).and_then(Object::as_array) {
        for pair in names.chunks_exact(2) {
            if let Ok(key) = resolve(doc, &pair[0]).as_str() {
                out.entry(key.to_vec()).or_insert_with(|| resolve(doc, &pair[1]).clone());
            }
        }
    }
    if let Ok(kids) = node.get(b"Kids").map(|o| resolve(doc, o)).and_then(Object::as_array) {
        for kid in kids {
            name_tree(doc, kid, out, seen, depth + 1);
        }
    }
}

/// A destination as an explicit array: `[page /XYZ …]`, from an array, a
/// name of one, or a dictionary holding one under `/D`.
fn explicit(doc: &Document, dest: &Object, named: &HashMap<Vec<u8>, Object>) -> Option<Vec<Object>> {
    let mut current = resolve(doc, dest).clone();
    // A name leads to an array, or to a dictionary with one; no further.
    for _ in 0..3 {
        current = match current {
            Object::Array(items) => return Some(items),
            Object::Name(name) | Object::String(name, _) => resolve(doc, named.get(&name)?).clone(),
            Object::Dictionary(d) => resolve(doc, d.get(b"D").ok()?).clone(),
            _ => return None,
        };
    }
    None
}

/// An explicit destination with its page moved to the new document, or
/// `None` if that page did not come along. The rest — `/XYZ`, `/Fit` and their
/// numbers — is kept as it is.
fn moved(dest: Vec<Object>, pages_of: &HashMap<ObjectId, ObjectId>) -> Option<Vec<Object>> {
    let page = dest.first()?.as_reference().ok()?;
    let new = *pages_of.get(&page)?;
    let mut out = vec![Object::Reference(new)];
    out.extend(dest.into_iter().skip(1).filter(|o| !matches!(o, Object::Reference(_))));
    Some(out)
}

/// Where an outline item or link goes: its `/Dest`, or a GoTo action's `/D`.
fn target_of(doc: &Document, item: &Dictionary, named: &HashMap<Vec<u8>, Object>) -> Option<Vec<Object>> {
    if let Ok(dest) = item.get(b"Dest") {
        return explicit(doc, dest, named);
    }
    let action = dict(doc, item.get(b"A").ok()?)?;
    if action.get(b"S").and_then(Object::as_name).ok()? != b"GoTo" {
        return None;
    }
    explicit(doc, action.get(b"D").ok()?, named)
}

fn keep_named_dests(placed: &Placed<'_>, named: &[HashMap<Vec<u8>, Object>], target: &mut Document, catalog: &mut Dictionary) {
    // Strings for the name tree, names for the old `/Dests` dictionary: a link
    // asks by one or the other, so each keeps the kind it was.
    let mut strings: BTreeMap<Vec<u8>, Object> = BTreeMap::new();
    let mut names = Dictionary::new();
    for (source, doc) in placed.sources.iter().enumerate() {
        let Some(old) = root_of(doc) else { continue };
        let as_names: HashSet<Vec<u8>> = old
            .get(b"Dests")
            .ok()
            .and_then(|o| dict(doc, o))
            .map(|d| d.iter().map(|(k, _)| k.clone()).collect())
            .unwrap_or_default();
        for (key, value) in &named[source] {
            let Some(dest) = explicit(doc, value, &named[source]).and_then(|d| moved(d, &placed.pages_of[source])) else {
                continue;
            };
            if as_names.contains(key) {
                if !names.has(key) {
                    names.set(key.clone(), Object::Array(dest));
                }
            } else {
                strings.entry(key.clone()).or_insert(Object::Array(dest));
            }
        }
    }
    if !names.is_empty() {
        catalog.set("Dests", Object::Reference(target.add_object(names)));
    }
    if !strings.is_empty() {
        let flat: Vec<Object> = strings
            .into_iter()
            .flat_map(|(k, v)| [Object::String(k, StringFormat::Literal), v])
            .collect();
        let mut tree = Dictionary::new();
        tree.set("Names", flat);
        let mut names_dict = Dictionary::new();
        names_dict.set("Dests", Object::Reference(target.add_object(tree)));
        catalog.set("Names", names_dict);
    }
}

// ── Bookmarks ─────────────────────────────────────────────────────────────

struct Item {
    title: Object,
    dest: Option<Vec<Object>>,
    /// Colour and bold/italic, as they were.
    style: Vec<(Vec<u8>, Object)>,
    open: bool,
    kids: Vec<Item>,
}

/// A source's bookmarks, cut down to those that lead to a page that came
/// along, or that hold one that does.
fn read_items(
    doc: &Document,
    first: Option<&Object>,
    named: &HashMap<Vec<u8>, Object>,
    pages_of: &HashMap<ObjectId, ObjectId>,
    seen: &mut HashSet<ObjectId>,
    depth: usize,
) -> Vec<Item> {
    let mut out = Vec::new();
    if depth > MOST_DEPTH {
        return out;
    }
    let mut next = first.and_then(|o| o.as_reference().ok());
    while let Some(id) = next {
        // A loop in the list, or a list without end, stops here.
        if !seen.insert(id) || seen.len() > MOST_ITEMS {
            break;
        }
        let Ok(node) = doc.get_dictionary(id) else { break };
        next = node.get(b"Next").and_then(Object::as_reference).ok();
        let kids = read_items(doc, node.get(b"First").ok(), named, pages_of, seen, depth + 1);
        let dest = target_of(doc, node, named).and_then(|d| moved(d, pages_of));
        if dest.is_none() && kids.is_empty() {
            continue;
        }
        let title = match node.get(b"Title").map(|o| resolve(doc, o)) {
            Ok(t @ Object::String(..)) => t.clone(),
            _ => Object::string_literal(""),
        };
        let style = [&b"C"[..], b"F"]
            .iter()
            .filter_map(|k| node.get(k).ok().map(|v| (k.to_vec(), resolve(doc, v).clone())))
            .filter(|(_, v)| !matches!(v, Object::Reference(_)))
            .collect();
        let open = node.get(b"Count").and_then(Object::as_i64).map(|c| c > 0).unwrap_or(false);
        out.push(Item { title, dest, style, open, kids });
    }
    out
}

/// A PDF text string: plain bytes when it is ASCII, UTF-16 with a byte order
/// mark otherwise (PDF 1.7 §7.9.2.2).
pub(super) fn text_string(text: &str) -> Object {
    if text.is_ascii() {
        return Object::string_literal(text);
    }
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

fn bookmarks(placed: &Placed<'_>, named: &[HashMap<Vec<u8>, Object>], target: &mut Document, outline: &Outline) -> Option<ObjectId> {
    let mut per_source: Vec<Vec<Item>> = Vec::with_capacity(placed.sources.len());
    for (source, doc) in placed.sources.iter().enumerate() {
        let first = root_of(doc)
            .and_then(|c| c.get(b"Outlines").ok())
            .and_then(|o| dict(doc, o))
            .and_then(|o| o.get(b"First").ok());
        let mut seen = HashSet::new();
        per_source.push(read_items(doc, first, &named[source], &placed.pages_of[source], &mut seen, 0));
    }
    let top: Vec<Item> = match outline {
        Outline::Keep => per_source.into_iter().flatten().collect(),
        Outline::ByFile(titles) => {
            let mut top = Vec::new();
            for (source, kids) in per_source.into_iter().enumerate() {
                // The file's first page in the new document.
                let Some(&(_, old)) = placed.order.iter().find(|(s, _)| *s == source) else { continue };
                let Some(&new) = placed.pages_of[source].get(&old) else { continue };
                let title = titles.get(source).map(|t| text_string(t)).unwrap_or_else(|| Object::string_literal(""));
                let dest = vec![Object::Reference(new), Object::Name(b"Fit".to_vec())];
                top.push(Item { title, dest: Some(dest), style: Vec::new(), open: false, kids });
            }
            top
        }
    };
    if top.is_empty() {
        return None;
    }
    let root = target.new_object_id();
    let (first, last, count) = write_items(target, root, &top);
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Outlines".to_vec()));
    dict.set("First", Object::Reference(first));
    dict.set("Last", Object::Reference(last));
    dict.set("Count", count);
    target.objects.insert(root, Object::Dictionary(dict));
    Some(root)
}

/// Write a list of items under `parent`: its first and last, and how many
/// are visible below it when it is open.
fn write_items(target: &mut Document, parent: ObjectId, items: &[Item]) -> (ObjectId, ObjectId, i64) {
    let ids: Vec<ObjectId> = items.iter().map(|_| target.new_object_id()).collect();
    let mut visible = items.len() as i64;
    for (i, item) in items.iter().enumerate() {
        let mut dict = Dictionary::new();
        dict.set("Title", item.title.clone());
        dict.set("Parent", Object::Reference(parent));
        if i > 0 {
            dict.set("Prev", Object::Reference(ids[i - 1]));
        }
        if i + 1 < ids.len() {
            dict.set("Next", Object::Reference(ids[i + 1]));
        }
        if let Some(dest) = &item.dest {
            dict.set("Dest", dest.clone());
        }
        for (key, value) in &item.style {
            dict.set(key.clone(), value.clone());
        }
        if !item.kids.is_empty() {
            let (first, last, below) = write_items(target, ids[i], &item.kids);
            dict.set("First", Object::Reference(first));
            dict.set("Last", Object::Reference(last));
            // Positive when open, negative when closed (PDF 1.7 Table 153).
            dict.set("Count", if item.open { below } else { -below });
            if item.open {
                visible += below;
            }
        }
        target.objects.insert(ids[i], Object::Dictionary(dict));
    }
    (ids[0], ids[ids.len() - 1], visible)
}

// ── Page labels ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct Label {
    style: Option<Vec<u8>>,
    prefix: Option<Object>,
    number: i64,
}

/// A source's label ranges, by the page index each starts at.
fn label_ranges(doc: &Document) -> Option<Vec<(i64, Dictionary)>> {
    let tree = root_of(doc)?.get(b"PageLabels").ok()?.clone();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    number_tree(doc, &tree, &mut out, &mut seen, 0);
    out.sort_by_key(|(start, _)| *start);
    (!out.is_empty()).then_some(out)
}

fn number_tree(doc: &Document, node: &Object, out: &mut Vec<(i64, Dictionary)>, seen: &mut HashSet<ObjectId>, depth: usize) {
    if depth > MOST_DEPTH || out.len() >= MOST_ITEMS {
        return;
    }
    if let Object::Reference(id) = node {
        if !seen.insert(*id) {
            return;
        }
    }
    let Some(node) = dict(doc, node) else { return };
    if let Ok(nums) = node.get(b"Nums").map(|o| resolve(doc, o)).and_then(Object::as_array) {
        for pair in nums.chunks_exact(2) {
            if let (Ok(start), Some(range)) = (resolve(doc, &pair[0]).as_i64(), dict(doc, &pair[1])) {
                out.push((start, range.clone()));
            }
        }
    }
    if let Ok(kids) = node.get(b"Kids").map(|o| resolve(doc, o)).and_then(Object::as_array) {
        for kid in kids {
            number_tree(doc, kid, out, seen, depth + 1);
        }
    }
}

fn label_of(doc: &Document, ranges: Option<&Vec<(i64, Dictionary)>>, index: i64) -> Label {
    let range = ranges.and_then(|r| r.iter().rev().find(|(start, _)| *start <= index));
    match range {
        Some((start, d)) => Label {
            style: d.get(b"S").and_then(Object::as_name).ok().map(<[u8]>::to_vec),
            prefix: d.get(b"P").ok().map(|p| resolve(doc, p).clone()).filter(|p| matches!(p, Object::String(..))),
            number: d.get(b"St").and_then(Object::as_i64).unwrap_or(1) + (index - start),
        },
        None => Label { style: Some(b"D".to_vec()), prefix: None, number: index + 1 },
    }
}

fn page_labels(placed: &Placed<'_>) -> Option<Object> {
    let ranges: Vec<Option<Vec<(i64, Dictionary)>>> = placed.sources.iter().map(label_ranges).collect();
    if ranges.iter().all(Option::is_none) {
        return None;
    }
    // Where each old page sits in its source.
    let index_of: Vec<HashMap<ObjectId, i64>> = placed
        .sources
        .iter()
        .map(|d| d.get_pages().into_iter().map(|(n, id)| (id, n as i64 - 1)).collect())
        .collect();
    let mut nums = Vec::new();
    let mut previous: Option<Label> = None;
    for (k, &(source, old)) in placed.order.iter().enumerate() {
        let index = index_of[source].get(&old).copied().unwrap_or(0);
        let label = label_of(&placed.sources[source], ranges[source].as_ref(), index);
        let continues = previous
            .as_ref()
            .is_some_and(|p| p.style == label.style && p.prefix == label.prefix && p.number + 1 == label.number);
        if !continues {
            let mut range = Dictionary::new();
            if let Some(style) = &label.style {
                range.set("S", Object::Name(style.clone()));
            }
            if let Some(prefix) = &label.prefix {
                range.set("P", prefix.clone());
            }
            if label.number != 1 {
                range.set("St", label.number);
            }
            nums.push(Object::Integer(k as i64));
            nums.push(Object::Dictionary(range));
        }
        previous = Some(label);
    }
    let mut tree = Dictionary::new();
    tree.set("Nums", nums);
    Some(Object::Dictionary(tree))
}

// ── Form fields ───────────────────────────────────────────────────────────

fn form(
    sources: &[Document],
    copiers: &mut [Copier],
    pages_of: &[HashMap<ObjectId, ObjectId>],
    target: &mut Document,
    pages_id: ObjectId,
) -> Option<Object> {
    let mut fields = Vec::new();
    let mut taken: HashSet<Vec<u8>> = HashSet::new();
    let mut fonts = Dictionary::new();
    let mut settings: Option<Dictionary> = None;
    for (source, doc) in sources.iter().enumerate() {
        let Some(old) = root_of(doc).and_then(|c| c.get(b"AcroForm").ok()).and_then(|o| dict(doc, o)) else { continue };
        let Ok(list) = old.get(b"Fields").map(|o| resolve(doc, o)).and_then(Object::as_array) else { continue };
        let mut mine = Vec::new();
        for field in list {
            // Listed only if a widget on a page that came along reached it.
            let Some(new) = field.as_reference().ok().and_then(|id| copiers[source].mapped(id)) else { continue };
            mine.push(new);
        }
        if mine.is_empty() {
            continue;
        }
        let mut names_here = Vec::new();
        for &new in &mine {
            let Ok(dict) = target.get_dictionary_mut(new) else { continue };
            let Some(name) = dict.get(b"T").ok().and_then(|t| t.as_str().ok()).map(<[u8]>::to_vec) else { continue };
            if taken.contains(&name) {
                // Two files' "Họ tên" would fill in together; the later one gets its own.
                let mut renamed = name.clone();
                renamed.extend_from_slice(format!("_{}", source + 1).as_bytes());
                dict.set("T", Object::String(renamed.clone(), StringFormat::Literal));
                names_here.push(renamed);
            } else {
                names_here.push(name);
            }
        }
        taken.extend(names_here);
        fields.extend(mine.into_iter().map(Object::Reference));

        // The fonts appearances name: each file's, the first of a name kept.
        if let Some(dr) = old.get(b"DR").ok().cloned() {
            let copied = copiers[source].copy(doc, target, dr, &pages_of[source], pages_id);
            if let Some(found) = dict_in(target, &copied).and_then(|d| d.get(b"Font").ok().cloned()) {
                if let Some(found) = dict_in(target, &found) {
                    for (k, v) in found.iter() {
                        if !fonts.has(k) {
                            fonts.set(k.clone(), v.clone());
                        }
                    }
                }
            }
        }
        if settings.is_none() {
            let mut s = Dictionary::new();
            for key in [&b"DA"[..], b"Q", b"NeedAppearances"] {
                if let Ok(v) = old.get(key) {
                    let v = resolve(doc, v);
                    if !matches!(v, Object::Reference(_)) {
                        s.set(key.to_vec(), v.clone());
                    }
                }
            }
            settings = Some(s);
        }
    }
    if fields.is_empty() {
        return None;
    }
    let mut form = settings.unwrap_or_default();
    form.set("Fields", fields);
    if !fonts.is_empty() {
        let mut dr = Dictionary::new();
        dr.set("Font", fonts);
        form.set("DR", dr);
    }
    Some(Object::Dictionary(form))
}

fn dict_in(target: &Document, object: &Object) -> Option<Dictionary> {
    match object {
        Object::Reference(id) => target.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(d) => Some(d.clone()),
        _ => None,
    }
}

// ── Layers ────────────────────────────────────────────────────────────────

fn layers(sources: &[Document], copiers: &[Copier]) -> Option<Object> {
    let mut groups = Vec::new();
    let mut on = Vec::new();
    let mut off = Vec::new();
    let mut base: Option<Object> = None;
    let mapped_list = |doc: &Document, copier: &Copier, list: Option<&Object>| -> Vec<Object> {
        list.map(|o| resolve(doc, o))
            .and_then(|o| o.as_array().ok())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|o| o.as_reference().ok().and_then(|id| copier.mapped(id)))
                    .map(Object::Reference)
                    .collect()
            })
            .unwrap_or_default()
    };
    for (source, doc) in sources.iter().enumerate() {
        let Some(props) = root_of(doc).and_then(|c| c.get(b"OCProperties").ok()).and_then(|o| dict(doc, o)) else { continue };
        // Only the layers something on a copied page uses came along.
        groups.extend(mapped_list(doc, &copiers[source], props.get(b"OCGs").ok()));
        if let Some(d) = props.get(b"D").ok().and_then(|o| dict(doc, o)) {
            on.extend(mapped_list(doc, &copiers[source], d.get(b"ON").ok()));
            off.extend(mapped_list(doc, &copiers[source], d.get(b"OFF").ok()));
            if base.is_none() {
                base = d.get(b"BaseState").ok().cloned();
            }
        }
    }
    if groups.is_empty() {
        return None;
    }
    let mut d = Dictionary::new();
    if let Some(base) = base.filter(|b| matches!(b, Object::Name(_))) {
        d.set("BaseState", base);
    }
    if !on.is_empty() {
        d.set("ON", on);
    }
    if !off.is_empty() {
        d.set("OFF", off);
    }
    // The order layers are listed in: here, as they were met.
    d.set("Order", groups.clone());
    let mut props = Dictionary::new();
    props.set("OCGs", groups);
    props.set("D", d);
    Some(Object::Dictionary(props))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::pdf::tests::{labels, round_trip, sample};
    use crate::utilities::pdf::{assemble, Pick};
    use lopdf::dictionary;

    fn keep_going(_: usize, _: usize) -> bool {
        true
    }

    fn catalog_mut(doc: &mut Document) -> &mut Dictionary {
        let id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(id).unwrap()
    }

    fn page(doc: &Document, n: u32) -> ObjectId {
        doc.get_pages()[&n]
    }

    /// A bookmark per page ("p-1"…), the second with a child, and labels
    /// "i, ii, 1, 2, …" (two roman pages first).
    fn with_catalog(n: u32, label: &str) -> Document {
        let mut doc = sample(n, label);
        let root = doc.new_object_id();
        let ids: Vec<ObjectId> = (0..n).map(|_| doc.new_object_id()).collect();
        let child = doc.new_object_id();
        for (i, &id) in ids.iter().enumerate() {
            let mut d = dictionary! {
                "Title" => Object::string_literal(format!("{label}-{}", i + 1)),
                "Parent" => root,
                "Dest" => vec![Object::Reference(page(&doc, i as u32 + 1)), "Fit".into()],
            };
            if i > 0 {
                d.set("Prev", ids[i - 1]);
            }
            if i + 1 < ids.len() {
                d.set("Next", ids[i + 1]);
            }
            if i == 1 {
                d.set("First", child);
                d.set("Last", child);
                d.set("Count", 1);
            }
            doc.objects.insert(id, Object::Dictionary(d));
        }
        // The child goes to the last page, by name.
        doc.objects.insert(child, Object::Dictionary(dictionary! {
            "Title" => Object::string_literal("child"), "Parent" => ids[1], "Dest" => Object::string_literal("end"),
        }));
        doc.objects.insert(root, Object::Dictionary(dictionary! {
            "Type" => "Outlines", "First" => ids[0], "Last" => ids[n as usize - 1], "Count" => n as i64,
        }));
        let last = page(&doc, n);
        let names = doc.add_object(dictionary! {
            "Names" => vec![Object::string_literal("end"), vec![Object::Reference(last), "Fit".into()].into()],
        });
        let labels = dictionary! { "Nums" => vec![
            0.into(), dictionary! { "S" => "r" }.into(),
            2.into(), dictionary! { "S" => "D" }.into(),
        ]};
        let catalog = catalog_mut(&mut doc);
        catalog.set("Outlines", root);
        catalog.set("Names", dictionary! { "Dests" => names });
        catalog.set("PageLabels", labels);
        doc
    }

    fn outline_titles(doc: &Document) -> Vec<String> {
        fn walk(doc: &Document, first: Option<ObjectId>, depth: usize, out: &mut Vec<String>) {
            let mut next = first;
            while let Some(id) = next {
                let d = doc.get_dictionary(id).unwrap();
                let title = String::from_utf8_lossy(d.get(b"Title").unwrap().as_str().unwrap()).to_string();
                let page = d.get(b"Dest").ok().and_then(|o| o.as_array().ok()).map(|a| {
                    let id = a[0].as_reference().unwrap();
                    doc.get_pages().into_iter().find(|(_, p)| *p == id).map(|(n, _)| n).unwrap()
                });
                out.push(format!("{}{title}{}", "  ".repeat(depth), page.map(|p| format!(" →{p}")).unwrap_or_default()));
                walk(doc, d.get(b"First").ok().and_then(|o| o.as_reference().ok()), depth + 1, out);
                next = d.get(b"Next").ok().and_then(|o| o.as_reference().ok());
            }
        }
        let root = doc.catalog().unwrap().get(b"Outlines").unwrap().as_reference().unwrap();
        let first = doc.get_dictionary(root).unwrap().get(b"First").unwrap().as_reference().ok();
        let mut out = Vec::new();
        walk(doc, first, 0, &mut out);
        out
    }

    fn label_starts(doc: &Document) -> Vec<(i64, String, i64)> {
        let nums = doc.catalog().unwrap().get(b"PageLabels").unwrap().as_dict().unwrap().get(b"Nums").unwrap().as_array().unwrap().clone();
        nums.chunks(2)
            .map(|p| {
                let d = p[1].as_dict().unwrap();
                let s = d.get(b"S").ok().map(|s| String::from_utf8_lossy(s.as_name().unwrap()).to_string()).unwrap_or_default();
                (p[0].as_i64().unwrap(), s, d.get(b"St").and_then(Object::as_i64).unwrap_or(1))
            })
            .collect()
    }

    #[test]
    fn a_merge_keeps_each_files_bookmarks_and_can_put_them_under_its_name() {
        let sources = [with_catalog(3, "a"), with_catalog(2, "b")];
        let picks = [Pick { source: 0, pages: None, rotate: 0 }, Pick { source: 1, pages: None, rotate: 0 }];
        let kept = round_trip(assemble(&sources, &picks, &Outline::Keep, keep_going).unwrap());
        assert_eq!(outline_titles(&kept), ["a-1 →1", "a-2 →2", "  child →3", "a-3 →3", "b-1 →4", "b-2 →5", "  child →5"]);

        let by_file = Outline::ByFile(vec!["Hợp đồng".into(), "Phụ lục".into()]);
        let out = round_trip(assemble(&sources, &picks, &by_file, keep_going).unwrap());
        let titles = outline_titles(&out);
        assert_eq!(titles.len(), 9);
        assert!(titles[0].ends_with("→1") && titles[5].ends_with("→4"), "{titles:?}");
        // The file's name, written so a reader shows the Vietnamese.
        let root = out.catalog().unwrap().get(b"Outlines").unwrap().as_reference().unwrap();
        let first = out.get_dictionary(root).unwrap().get(b"First").unwrap().as_reference().unwrap();
        let title = out.get_dictionary(first).unwrap().get(b"Title").unwrap().as_str().unwrap().to_vec();
        assert_eq!(&title[..2], [0xFE, 0xFF]);
    }

    #[test]
    fn a_split_keeps_only_the_bookmarks_to_its_pages() {
        let sources = [with_catalog(4, "a")];
        let part = [Pick { source: 0, pages: Some(vec![3, 4]), rotate: 0 }];
        let out = round_trip(assemble(&sources, &part, &Outline::Keep, keep_going).unwrap());
        // a-2 goes to a page left out but holds "child", which stayed.
        assert_eq!(outline_titles(&out), ["a-2", "  child →2", "a-3 →1", "a-4 →2"]);
        assert_eq!(labels(&out), ["a-3", "a-4"]);
    }

    #[test]
    fn page_labels_follow_their_pages() {
        let sources = [with_catalog(4, "a"), sample(2, "b")];
        // Pages ii, 2 of the first, then the second file, which has none.
        let picks = [Pick { source: 0, pages: Some(vec![2, 4]), rotate: 0 }, Pick { source: 1, pages: None, rotate: 0 }];
        let out = round_trip(assemble(&sources, &picks, &Outline::Keep, keep_going).unwrap());
        assert_eq!(label_starts(&out), [(0, "r".into(), 2), (1, "D".into(), 2), (2, "D".into(), 1)]);
    }

    #[test]
    fn a_link_by_name_still_has_somewhere_to_go() {
        let sources = [with_catalog(3, "a")];
        let out = round_trip(assemble(&sources, &[Pick { source: 0, pages: Some(vec![3]), rotate: 0 }], &Outline::Keep, keep_going).unwrap());
        let names = named_dests(&out);
        let dest = names.get(&b"end".to_vec()).expect("the name is kept");
        assert_eq!(dest.as_array().unwrap()[0].as_reference().unwrap(), page(&out, 1));

        // A split without that page has no use for it.
        let out = round_trip(assemble(&sources, &[Pick { source: 0, pages: Some(vec![1]), rotate: 0 }], &Outline::Keep, keep_going).unwrap());
        assert!(named_dests(&out).is_empty());
    }

    /// A text field on page 1 with one widget, named `name`.
    fn with_field(label: &str, name: &str) -> Document {
        let mut doc = sample(2, label);
        let first = page(&doc, 1);
        let widget = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal(name),
            "Rect" => vec![72.into(), 600.into(), 300.into(), 620.into()], "P" => first,
        });
        doc.get_dictionary_mut(first).unwrap().set("Annots", vec![Object::Reference(widget)]);
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        catalog_mut(&mut doc).set("AcroForm", dictionary! {
            "Fields" => vec![Object::Reference(widget)], "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } },
        });
        doc
    }

    #[test]
    fn form_fields_come_along_and_two_of_one_name_stay_apart() {
        let sources = [with_field("a", "ho_ten"), with_field("b", "ho_ten")];
        let picks = [Pick { source: 0, pages: None, rotate: 0 }, Pick { source: 1, pages: None, rotate: 0 }];
        let out = round_trip(assemble(&sources, &picks, &Outline::Keep, keep_going).unwrap());
        let form = out.catalog().unwrap().get(b"AcroForm").unwrap().as_dict().unwrap();
        let fields = form.get(b"Fields").unwrap().as_array().unwrap();
        let names: Vec<String> = fields
            .iter()
            .map(|f| String::from_utf8_lossy(out.get_dictionary(f.as_reference().unwrap()).unwrap().get(b"T").unwrap().as_str().unwrap()).to_string())
            .collect();
        assert_eq!(names, ["ho_ten", "ho_ten_2"]);
        assert!(form.get(b"DR").unwrap().as_dict().unwrap().get(b"Font").unwrap().as_dict().unwrap().has(b"Helv"));

        // A split of page 2 only has no field to list, so no form.
        let out = round_trip(assemble(&sources[..1], &[Pick { source: 0, pages: Some(vec![2]), rotate: 0 }], &Outline::Keep, keep_going).unwrap());
        assert!(out.catalog().unwrap().get(b"AcroForm").is_err());
    }

    #[test]
    fn a_layer_that_was_off_stays_off() {
        let mut doc = sample(1, "a");
        let layer = doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Draft") });
        let p = page(&doc, 1);
        // The page uses it, as a page with layers does.
        let resources = dictionary! { "Properties" => dictionary! { "L1" => layer } };
        doc.get_dictionary_mut(p).unwrap().set("Resources", resources);
        catalog_mut(&mut doc).set("OCProperties", dictionary! {
            "OCGs" => vec![Object::Reference(layer)], "D" => dictionary! { "OFF" => vec![Object::Reference(layer)] },
        });
        let out = round_trip(assemble(&[doc], &[Pick { source: 0, pages: None, rotate: 0 }], &Outline::Keep, keep_going).unwrap());
        let props = out.catalog().unwrap().get(b"OCProperties").unwrap().as_dict().unwrap();
        let off = props.get(b"D").unwrap().as_dict().unwrap().get(b"OFF").unwrap().as_array().unwrap();
        assert_eq!(off.len(), 1);
        let name = out.get_dictionary(off[0].as_reference().unwrap()).unwrap().get(b"Name").unwrap().as_str().unwrap().to_vec();
        assert_eq!(name, b"Draft");
    }
}
