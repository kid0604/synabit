//! Pages taken out of PDFs and put into new ones.
//!
//! Merge, split, organize and rotate are one operation seen four ways: a new
//! document whose pages are a list of `(source, page, turn)` picks. Merge
//! picks every page of every source in order; split makes one document per
//! range; organize is a list in a new order with some pages left out and some
//! turned; rotate is every page with the same turn. So there is one function,
//! [`assemble`], and the four tools are how the screen fills in its list.
//!
//! # Copying what a page needs, not the whole file
//!
//! The usual lopdf recipe for merging renumbers each source and pours all of
//! its objects into the target. That makes a split of a 500-page file into 50
//! parts cost 50 copies of the file in memory, and every part comes out the
//! size of the whole. Here a page is copied with what it refers to and nothing
//! else: its contents, fonts, images, annotations. A reference to a page that
//! is not in the new document — a link's destination, an annotation's `/P` —
//! becomes `null`, so it does not drag that page, and everything behind it,
//! along.
//!
//! # What a page inherits
//!
//! `/Resources`, `/MediaBox`, `/CropBox` and `/Rotate` may sit on a page's
//! ancestors in the page tree rather than on the page (PDF 1.7 §7.7.3.4). A
//! page moved under a new parent loses them, and comes out blank or the wrong
//! size. They are copied down onto the page before it moves.
//!
//! # What is not kept
//!
//! The outline (bookmarks), forms, the structure tree and named destinations
//! hang off the catalog, not off pages, so a new document starts without
//! them. Keeping bookmarks in a merge is listed for later in the design.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId};

/// Why a PDF could not be used.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// Not a PDF, or a PDF too damaged to read.
    #[error("not a readable PDF: {0}")]
    Unreadable(String),
    /// Encrypted with a password we do not have.
    #[error("the PDF is protected by a password")]
    Locked,
    /// Asked for a page the source does not have.
    #[error("page {page} does not exist; the file has {count}")]
    NoSuchPage { page: u32, count: u32 },
    /// Asked for a source that was not given.
    #[error("no source number {0}")]
    NoSuchSource(usize),
    /// A document with no pages in it.
    #[error("there are no pages to put in the new file")]
    Empty,
    /// Stopped by the person.
    #[error("cancelled")]
    Cancelled,
    #[error("could not write the PDF: {0}")]
    Write(String),
}

/// One run of pages from one source, all given the same turn.
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    /// Index into the sources passed to [`assemble`].
    pub source: usize,
    /// 1-based page numbers, in the order they go in. `None` is every page.
    pub pages: Option<Vec<u32>>,
    /// Degrees clockwise, added to the page's own rotation. A multiple of 90.
    #[serde(default)]
    pub rotate: i32,
}

/// The default page size when no ancestor names one: US Letter, which is what
/// PDF readers assume too.
const LETTER: [f32; 4] = [0.0, 0.0, 612.0, 792.0];

/// Open a PDF for reading pages from. A file with an owner password only —
/// one that opens without asking, but says "no editing" — is opened with the
/// empty password, as every reader does; one that needs a password to open
/// is [`PdfError::Locked`]. Decryption is `crypt`'s, which reads AES as well
/// as the RC4 `lopdf` knows.
pub fn open(path: &Path) -> Result<Document, PdfError> {
    open_with(path, "")
}

/// [`open`], with a password to try.
pub fn open_with(path: &Path, password: &str) -> Result<Document, PdfError> {
    use super::crypt::{self, CryptError};
    let doc = crypt::load(path, password).map_err(|e| match e {
        CryptError::WrongPassword => PdfError::Locked,
        CryptError::Unreadable(why) => PdfError::Unreadable(why),
        CryptError::Unsupported(what) => PdfError::Unreadable(format!("protected in a way not supported ({what})")),
    })?;
    if doc.get_pages().is_empty() {
        return Err(PdfError::Unreadable("it has no pages".into()));
    }
    Ok(doc)
}

/// How many pages a source has.
pub fn page_count(doc: &Document) -> u32 {
    doc.get_pages().len() as u32
}

/// Turn a page's rotation by `delta` degrees, kept within 0..360.
pub fn turned(current: i64, delta: i32) -> i64 {
    (((current + delta as i64) % 360) + 360) % 360
}

/// A new document made of the picked pages, in order.
///
/// `progress` is called with the number of pages copied so far and the total,
/// and stops the work by returning `false`.
pub fn assemble(
    sources: &[Document],
    picks: &[Pick],
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<Document, PdfError> {
    // Resolve the picks into one flat list of pages first, so a bad page
    // number fails before anything is copied.
    let numbered: Vec<_> = sources.iter().map(|d| d.get_pages()).collect();
    let mut wanted: Vec<(usize, ObjectId, i32)> = Vec::new();
    for pick in picks {
        let pages = numbered.get(pick.source).ok_or(PdfError::NoSuchSource(pick.source))?;
        let count = pages.len() as u32;
        match &pick.pages {
            None => wanted.extend(pages.values().map(|&id| (pick.source, id, pick.rotate))),
            Some(list) => {
                for &n in list {
                    let id = *pages.get(&n).ok_or(PdfError::NoSuchPage { page: n, count })?;
                    wanted.push((pick.source, id, pick.rotate));
                }
            }
        }
    }
    if wanted.is_empty() {
        return Err(PdfError::Empty);
    }

    let version = sources.iter().map(|d| d.version.as_str()).max().unwrap_or("1.7").to_string();
    let mut target = Document::with_version(if version.is_empty() { "1.7".into() } else { version });
    let pages_id = target.new_object_id();

    // Every page that will be in the new document, by where it came from.
    // The first time a page is placed is the one references to it point at;
    // a page placed twice is a second, separate copy.
    let mut placed: HashMap<(usize, ObjectId), ObjectId> = HashMap::new();
    let mut new_ids: Vec<ObjectId> = Vec::with_capacity(wanted.len());
    for &(source, id, _) in &wanted {
        let new_id = target.new_object_id();
        placed.entry((source, id)).or_insert(new_id);
        new_ids.push(new_id);
    }

    let mut placed_by_source: Vec<HashMap<ObjectId, ObjectId>> = vec![HashMap::new(); sources.len()];
    for ((source, old), new) in placed {
        placed_by_source[source].insert(old, new);
    }

    let mut copiers: Vec<Copier> = sources.iter().map(|_| Copier::default()).collect();
    let mut first_time: HashSet<(usize, ObjectId)> = HashSet::new();
    let total = wanted.len();
    for (i, (&(source, id, rotate), &new_id)) in wanted.iter().zip(&new_ids).enumerate() {
        if !progress(i, total) {
            return Err(PdfError::Cancelled);
        }
        let doc = &sources[source];
        let mut page = flattened_page(doc, id)?;
        let current = page.get(b"Rotate").ok().map(|o| rotation_of(doc, o)).unwrap_or(0);
        page.set("Rotate", turned(current, rotate));
        // A page placed a second time gets no annotations: one annotation on
        // two pages is not allowed, and links and form fields would act on both.
        if !first_time.insert((source, id)) {
            page.remove(b"Annots");
        }

        let copied = copiers[source].copy(doc, &mut target, Object::Dictionary(page), &placed_by_source[source], pages_id);
        // Set after copying: `pages_id` is an id in the target, and the copier
        // reads every reference it meets as one into the source.
        let Object::Dictionary(mut copied) = copied else { unreachable!("a dictionary copies to a dictionary") };
        copied.set("Parent", Object::Reference(pages_id));
        target.objects.insert(new_id, Object::Dictionary(copied));
    }
    progress(total, total);

    let kids: Vec<Object> = new_ids.iter().map(|&id| Object::Reference(id)).collect();
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Count", kids.len() as i64);
    pages.set("Kids", kids);
    target.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = target.add_object(catalog);
    target.trailer.set("Root", Object::Reference(catalog_id));

    Ok(target)
}

/// A `/Rotate` value however it is written: an integer, a real, or a
/// reference to one.
pub fn rotation_of(doc: &Document, value: &Object) -> i64 {
    let value = match value {
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(o) => o,
            Err(_) => return 0,
        },
        other => other,
    };
    match value {
        Object::Integer(i) => *i,
        Object::Real(r) => r.round() as i64,
        _ => 0,
    }
}

/// Write a document to a file.
pub fn save(doc: &mut Document, path: &Path) -> Result<(), PdfError> {
    let file = std::fs::File::create(path).map_err(|e| PdfError::Write(e.to_string()))?;
    let mut out = std::io::BufWriter::new(file);
    doc.save_to(&mut out).map_err(|e| PdfError::Write(e.to_string()))?;
    use std::io::Write;
    out.flush().map_err(|e| PdfError::Write(e.to_string()))
}

/// Keys a page may take from its ancestors (PDF 1.7, Table 30).
const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Keys left behind when a page moves: `/Parent` is set again, and `/B`
/// (article beads) would pull in threads that point at pages not copied.
const LEFT_BEHIND: [&[u8]; 2] = [b"Parent", b"B"];

/// A page's dictionary with what it inherits written onto it.
fn flattened_page(doc: &Document, id: ObjectId) -> Result<Dictionary, PdfError> {
    let mut page = doc
        .get_dictionary(id)
        .map_err(|e| PdfError::Unreadable(format!("page object {id:?}: {e}")))?
        .clone();

    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    let mut seen = HashSet::new();
    while let Some(parent_id) = parent {
        // A page tree that loops is broken; stop rather than spin.
        if !seen.insert(parent_id) {
            break;
        }
        let Ok(node) = doc.get_dictionary(parent_id) else { break };
        for key in INHERITED {
            if !page.has(key) {
                if let Ok(value) = node.get(key) {
                    page.set(key.to_vec(), value.clone());
                }
            }
        }
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
    }
    if !page.has(b"MediaBox") {
        page.set("MediaBox", LETTER.iter().map(|&v| Object::Real(v)).collect::<Vec<_>>());
    }
    for key in LEFT_BEHIND {
        page.remove(key);
    }
    Ok(page)
}

/// Copies objects from one source into the target, each at most once.
#[derive(Default)]
struct Copier {
    /// Source id → target id, for every object already copied or queued.
    mapped: HashMap<ObjectId, ObjectId>,
}

impl Copier {
    /// `object` with every reference it reaches copied into `target`.
    ///
    /// References to pages go to the page's place in the new document if it
    /// has one (`placed`), and become `null` if it does not. A reference to the
    /// source's page tree becomes the new one, so a page's `/Parent` — or an
    /// annotation's — never pulls in the old tree.
    fn copy(
        &mut self,
        doc: &Document,
        target: &mut Document,
        object: Object,
        placed: &HashMap<ObjectId, ObjectId>,
        pages_id: ObjectId,
    ) -> Object {
        let mut queue: VecDeque<(ObjectId, ObjectId)> = VecDeque::new();
        let root = self.rewrite(doc, target, object, placed, pages_id, &mut queue);
        // Breadth first with a queue rather than recursion: a long chain of
        // references (an outline, a linked list of annotations) is not a
        // stack overflow waiting for a large enough file.
        while let Some((old, new)) = queue.pop_front() {
            let Ok(found) = doc.get_object(old) else {
                target.objects.insert(new, Object::Null);
                continue;
            };
            let copied = self.rewrite(doc, target, found.clone(), placed, pages_id, &mut queue);
            target.objects.insert(new, copied);
        }
        root
    }

    fn rewrite(
        &mut self,
        doc: &Document,
        target: &mut Document,
        object: Object,
        placed: &HashMap<ObjectId, ObjectId>,
        pages_id: ObjectId,
        queue: &mut VecDeque<(ObjectId, ObjectId)>,
    ) -> Object {
        match object {
            Object::Reference(old) => {
                if let Some(&new) = placed.get(&old) {
                    return Object::Reference(new);
                }
                if let Some(&new) = self.mapped.get(&old) {
                    return Object::Reference(new);
                }
                match type_of(doc, old) {
                    Some(b"Page") => Object::Null,
                    Some(b"Pages") => Object::Reference(pages_id),
                    _ => {
                        let new = target.new_object_id();
                        self.mapped.insert(old, new);
                        queue.push_back((old, new));
                        Object::Reference(new)
                    }
                }
            }
            Object::Array(items) => Object::Array(
                items.into_iter().map(|o| self.rewrite(doc, target, o, placed, pages_id, queue)).collect(),
            ),
            Object::Dictionary(dict) => Object::Dictionary(self.rewrite_dict(doc, target, dict, placed, pages_id, queue)),
            Object::Stream(mut stream) => {
                stream.dict = self.rewrite_dict(doc, target, stream.dict, placed, pages_id, queue);
                Object::Stream(stream)
            }
            other => other,
        }
    }

    fn rewrite_dict(
        &mut self,
        doc: &Document,
        target: &mut Document,
        dict: Dictionary,
        placed: &HashMap<ObjectId, ObjectId>,
        pages_id: ObjectId,
        queue: &mut VecDeque<(ObjectId, ObjectId)>,
    ) -> Dictionary {
        let mut out = Dictionary::new();
        for (key, value) in dict.into_iter() {
            out.set(key, self.rewrite(doc, target, value, placed, pages_id, queue));
        }
        out
    }
}

/// The `/Type` of the object behind a reference, if it has one.
fn type_of(doc: &Document, id: ObjectId) -> Option<&[u8]> {
    match doc.get_object(id).ok()? {
        Object::Dictionary(d) => d.get(b"Type").and_then(Object::as_name).ok(),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use lopdf::{content::Content, content::Operation, dictionary, Stream};

    /// A document of `n` pages, each drawing its own number, with the page
    /// size and the font living on the page tree rather than on the pages —
    /// the case a careless copy gets wrong.
    pub(crate) fn sample(n: u32, label: &str) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let resources = doc.add_object(dictionary! { "Font" => lopdf::dictionary! { "F1" => font } });
        let mut kids = Vec::new();
        for i in 1..=n {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![72.into(), 720.into()]),
                    Operation::new("Tj", vec![Object::string_literal(format!("{label}-{i}"))]),
                    Operation::new("ET", vec![]),
                ],
            };
            let stream = doc.add_object(Stream::new(Dictionary::new(), content.encode().unwrap()));
            let page = doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => stream,
            });
            kids.push(Object::Reference(page));
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => kids, "Count" => n as i64,
                "Resources" => resources,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc
    }

    /// The text each page of a document draws, in page order.
    pub(crate) fn labels(doc: &Document) -> Vec<String> {
        doc.get_pages()
            .values()
            .map(|&id| {
                let bytes = doc.get_page_content(id).unwrap();
                let content = Content::decode(&bytes).unwrap();
                content
                    .operations
                    .iter()
                    .find(|op| op.operator == "Tj")
                    .map(|op| String::from_utf8_lossy(op.operands[0].as_str().unwrap()).to_string())
                    .unwrap()
            })
            .collect()
    }

    fn rotation(doc: &Document, page: u32) -> i64 {
        let id = doc.get_pages()[&page];
        doc.get_dictionary(id).unwrap().get(b"Rotate").unwrap().as_i64().unwrap()
    }

    /// Through bytes and back, so what is tested is what a reader would get.
    pub(crate) fn round_trip(mut doc: Document) -> Document {
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        Document::load_mem(&bytes).unwrap()
    }

    fn all(source: usize) -> Pick {
        Pick { source, pages: None, rotate: 0 }
    }

    fn keep_going(_: usize, _: usize) -> bool {
        true
    }

    #[test]
    fn merging_keeps_every_page_in_order() {
        let sources = [sample(3, "a"), sample(2, "b")];
        let out = round_trip(assemble(&sources, &[all(0), all(1)], keep_going).unwrap());
        assert_eq!(labels(&out), ["a-1", "a-2", "a-3", "b-1", "b-2"]);
    }

    #[test]
    fn a_moved_page_keeps_what_it_inherited() {
        let sources = [sample(2, "a")];
        let out = round_trip(assemble(&sources, &[all(0)], keep_going).unwrap());
        for (_, id) in out.get_pages() {
            let page = out.get_dictionary(id).unwrap();
            let size: Vec<f32> =
                page.get(b"MediaBox").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect();
            assert_eq!(size, [0.0, 0.0, 595.0, 842.0], "A4 came from the page tree");
            let resources = match page.get(b"Resources").unwrap() {
                Object::Reference(r) => out.get_dictionary(*r).unwrap(),
                Object::Dictionary(d) => d,
                other => panic!("resources were {other:?}"),
            };
            assert!(resources.get(b"Font").is_ok(), "the font came along");
        }
    }

    #[test]
    fn splitting_copies_only_the_pages_asked_for() {
        let sources = [sample(10, "a")];
        let part = Pick { source: 0, pages: Some(vec![4, 5]), rotate: 0 };
        let out = round_trip(assemble(&sources, &[part], keep_going).unwrap());
        assert_eq!(labels(&out), ["a-4", "a-5"]);
        // Two pages, two content streams, one font, one resource dictionary,
        // the tree and the catalog — not the eight other pages. (The
        // cross-reference stream lopdf writes is not counted.)
        let copied = out.objects.values().filter(|o| o.type_name().ok() != Some("XRef")).count();
        assert_eq!(copied, 8, "copied {:?}", out.objects);
    }

    #[test]
    fn pages_can_be_reordered_and_turned() {
        let sources = [sample(3, "a")];
        let picks = [
            Pick { source: 0, pages: Some(vec![3]), rotate: 90 },
            Pick { source: 0, pages: Some(vec![1]), rotate: -90 },
        ];
        let out = round_trip(assemble(&sources, &picks, keep_going).unwrap());
        assert_eq!(labels(&out), ["a-3", "a-1"]);
        assert_eq!(rotation(&out, 1), 90);
        assert_eq!(rotation(&out, 2), 270);
    }

    #[test]
    fn rotation_adds_to_what_the_page_had() {
        assert_eq!(turned(90, 90), 180);
        assert_eq!(turned(270, 180), 90);
        assert_eq!(turned(0, -90), 270);
        assert_eq!(turned(-90, 0), 270);
    }

    #[test]
    fn the_same_page_twice_is_two_pages() {
        let sources = [sample(2, "a")];
        let picks = [Pick { source: 0, pages: Some(vec![1, 1]), rotate: 0 }];
        let out = round_trip(assemble(&sources, &picks, keep_going).unwrap());
        assert_eq!(labels(&out), ["a-1", "a-1"]);
    }

    #[test]
    fn a_link_to_a_page_left_out_does_not_bring_it() {
        let mut doc = sample(3, "a");
        let pages = doc.get_pages();
        let (first, third) = (pages[&1], pages[&3]);
        // A link on page 1 pointing at page 3.
        let link = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Link",
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            "Dest" => vec![third.into(), "Fit".into()],
            "P" => first,
        });
        doc.get_dictionary_mut(first).unwrap().set("Annots", vec![Object::Reference(link)]);

        let out = round_trip(assemble(&[doc], &[Pick { source: 0, pages: Some(vec![1]), rotate: 0 }], keep_going).unwrap());
        assert_eq!(labels(&out), ["a-1"]);
        let page_count = out
            .objects
            .values()
            .filter(|o| matches!(o, Object::Dictionary(d) if d.get(b"Type").and_then(Object::as_name).ok() == Some(b"Page")))
            .count();
        assert_eq!(page_count, 1, "page 3 was dragged in by the link");
    }

    #[test]
    fn a_page_that_does_not_exist_is_refused() {
        let sources = [sample(2, "a")];
        let picks = [Pick { source: 0, pages: Some(vec![3]), rotate: 0 }];
        match assemble(&sources, &picks, keep_going) {
            Err(PdfError::NoSuchPage { page: 3, count: 2 }) => {}
            other => panic!("{:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn nothing_picked_is_refused() {
        let sources = [sample(2, "a")];
        assert!(matches!(
            assemble(&sources, &[Pick { source: 0, pages: Some(vec![]), rotate: 0 }], keep_going),
            Err(PdfError::Empty)
        ));
    }

    #[test]
    fn stopping_stops() {
        let sources = [sample(5, "a")];
        let result = assemble(&sources, &[all(0)], |done, _| done < 2);
        assert!(matches!(result, Err(PdfError::Cancelled)));
    }

    /// Real files, for measuring: every PDF in `UTILITIES_SAMPLES` merged,
    /// then split in two, written to `UTILITIES_OUT`, with the time and size
    /// printed. `cargo test --lib real_files -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_files() {
        let dir = std::path::PathBuf::from(std::env::var("UTILITIES_SAMPLES").expect("UTILITIES_SAMPLES"));
        let out = std::path::PathBuf::from(std::env::var("UTILITIES_OUT").expect("UTILITIES_OUT"));
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
            .collect();
        paths.sort();
        let started = std::time::Instant::now();
        let sources: Vec<_> = paths.iter().map(|p| open(p).unwrap()).collect();
        let picks: Vec<_> = (0..sources.len()).map(all).collect();
        let mut merged = assemble(&sources, &picks, keep_going).unwrap();
        let total = page_count(&merged);
        save(&mut merged, &out.join("merged.pdf")).unwrap();
        let half = total / 2;
        for (name, pages) in [("first-half.pdf", (1..=half).collect::<Vec<_>>()), ("second-half.pdf", (half + 1..=total).collect())] {
            let reopened = open(&out.join("merged.pdf")).unwrap();
            let mut part = assemble(&[reopened], &[Pick { source: 0, pages: Some(pages), rotate: 90 }], keep_going).unwrap();
            save(&mut part, &out.join(name)).unwrap();
        }
        println!("{} files, {total} pages, {:?}", paths.len(), started.elapsed());

        // And every JPEG beside them, one per page.
        let pictures: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg")))
            .map(|p| {
                let bytes = std::fs::read(&p).unwrap();
                (crate::utilities::images::read_jpeg(&bytes).unwrap(), bytes)
            })
            .collect();
        if !pictures.is_empty() {
            use crate::utilities::images::{jpegs_to_pdf, Layout, Orientation, PageSize};
            let layout = Layout { size: PageSize::A4, orientation: Orientation::Auto, margin: 18.0 };
            let mut doc = jpegs_to_pdf(&pictures, layout, keep_going).unwrap();
            save(&mut doc, &out.join("images.pdf")).unwrap();
        }
    }

    #[test]
    fn a_file_that_is_not_a_pdf_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.pdf");
        std::fs::write(&path, b"hello").unwrap();
        assert!(matches!(open(&path), Err(PdfError::Unreadable(_))));
    }
}
