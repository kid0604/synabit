//! Black boxes that take what is under them out of the file, not only out of
//! sight.
//!
//! A black rectangle drawn over a PDF's text leaves the text in the file:
//! select it, copy it, or open the file in anything that ignores the drawing,
//! and it is there. So a page with boxes on it is replaced by a picture of
//! itself with the boxes painted in — the WebView draws it (`render.ts`), this
//! puts it in the page's place — and every object nothing points at any more
//! (the old text, its fonts, the old page's annotations, the object streams
//! they were packed in) is dropped before the file is written.
//!
//! The words on the page that were not blacked out can be put back as an
//! invisible layer read from the picture (`searchable`), so the page can still
//! be searched; what was under a box is not on the picture to be read.
//!
//! Pages without a box are left as they were.

use std::collections::{HashSet, VecDeque};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::images;
use super::pdf::PdfError;
use super::searchable::{self, Line, PageLines};
use super::stamp;

/// A page to replace: its picture with the boxes painted in, and the text
/// read from that picture.
pub struct Blacked {
    /// 1-based.
    pub page: u32,
    pub jpeg: Vec<u8>,
    pub lines: Vec<Line>,
}

/// Replace each page with its blacked-out picture. With `clean`, the title,
/// author and the rest of the document's information go too.
pub fn redact(doc: &mut Document, pages: &[Blacked], clean: bool, mut progress: impl FnMut(usize, usize) -> bool) -> Result<(), PdfError> {
    let numbered = doc.get_pages();
    let mut widgets_gone = false;
    for (i, blacked) in pages.iter().enumerate() {
        if !progress(i, pages.len()) {
            return Err(PdfError::Cancelled);
        }
        let id = *numbered.get(&blacked.page).ok_or(PdfError::NoSuchPage { page: blacked.page, count: numbered.len() as u32 })?;
        let jpeg = images::read_jpeg(&blacked.jpeg).map_err(|e| PdfError::Unreadable(format!("page {}: {e}", blacked.page)))?;
        let (w, h) = stamp::seen(doc, id).size();
        widgets_gone |= has_widgets(doc, id);

        let mut image = Dictionary::new();
        image.set("Type", Object::Name(b"XObject".to_vec()));
        image.set("Subtype", Object::Name(b"Image".to_vec()));
        image.set("Width", jpeg.width as i64);
        image.set("Height", jpeg.height as i64);
        image.set("BitsPerComponent", 8);
        image.set("ColorSpace", Object::Name(if jpeg.components == 1 { b"DeviceGray".to_vec() } else { b"DeviceRGB".to_vec() }));
        image.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        let image = doc.add_object(Stream::new(image, blacked.jpeg.clone()).with_compression(false));
        let drawing = format!("q {w} 0 0 {h} 0 0 cm /SynabitRedacted Do Q");
        let contents = doc.add_object(Stream::new(Dictionary::new(), drawing.into_bytes()));

        let parent = doc.get_dictionary(id).ok().and_then(|d| d.get(b"Parent").ok()).cloned();
        let mut xobjects = Dictionary::new();
        xobjects.set("SynabitRedacted", Object::Reference(image));
        let mut resources = Dictionary::new();
        resources.set("XObject", xobjects);
        // A new page, not the old one changed: nothing of the old one — its
        // annotations, thumbnail, private data — can stay behind by accident.
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        if let Some(parent) = parent {
            page.set("Parent", parent);
        }
        let media = vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)];
        page.set("MediaBox", media.clone());
        // Set, not left out: the page tree could hand down another.
        page.set("CropBox", media);
        page.set("Rotate", 0);
        page.set("Resources", resources);
        page.set("Contents", Object::Reference(contents));
        doc.objects.insert(id, Object::Dictionary(page));
    }

    let layer: Vec<PageLines> = pages
        .iter()
        .filter(|b| !b.lines.is_empty())
        .map(|b| PageLines { page: b.page, lines: b.lines.clone() })
        .collect();
    searchable::add_text_layer(doc, &layer)?;

    let root = doc.trailer.get(b"Root").and_then(Object::as_reference).ok();
    if let Some(catalog) = root.and_then(|id| doc.get_dictionary_mut(id).ok()) {
        // A form whose boxes were on a replaced page would still list their
        // values; the form goes rather than keep what was blacked out.
        if widgets_gone {
            catalog.remove(b"AcroForm");
        }
        if clean {
            catalog.remove(b"Metadata");
        }
    }
    if clean {
        doc.trailer.remove(b"Info");
    }
    prune(doc);
    progress(pages.len(), pages.len());
    Ok(())
}

fn has_widgets(doc: &Document, page: ObjectId) -> bool {
    let annots = match doc.get_dictionary(page).ok().and_then(|d| d.get(b"Annots").ok()) {
        Some(Object::Array(items)) => items.clone(),
        Some(Object::Reference(id)) => doc.get_object(*id).ok().and_then(|o| o.as_array().ok()).cloned().unwrap_or_default(),
        _ => return false,
    };
    annots.iter().any(|a| {
        let dict = match a {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        };
        dict.is_some_and(|d| d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Widget"))
    })
}

/// Drop every object nothing reachable from the trailer points at. lopdf has
/// `prune_objects`, but it looks each reference up in a list, which is slow
/// past a few thousand objects.
pub fn prune(doc: &mut Document) {
    let mut seen: HashSet<ObjectId> = HashSet::new();
    let mut queue: VecDeque<ObjectId> = VecDeque::new();
    fn refs(object: &Object, out: &mut Vec<ObjectId>) {
        match object {
            Object::Reference(id) => out.push(*id),
            Object::Array(items) => items.iter().for_each(|o| refs(o, out)),
            Object::Dictionary(d) => d.iter().for_each(|(_, o)| refs(o, out)),
            Object::Stream(s) => s.dict.iter().for_each(|(_, o)| refs(o, out)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    doc.trailer.iter().for_each(|(_, o)| refs(o, &mut found));
    for id in found.drain(..) {
        if seen.insert(id) {
            queue.push_back(id);
        }
    }
    while let Some(id) = queue.pop_front() {
        if let Some(object) = doc.objects.get(&id) {
            refs(object, &mut found);
            for next in found.drain(..) {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }
    doc.objects.retain(|id, _| seen.contains(id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::pdf::tests::{round_trip, sample};

    /// The smallest baseline JPEG there is: 1 × 1, grey.
    pub(crate) fn tiny_jpeg() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A,
            0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
            0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39,
            0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00,
            0xFF, 0xC4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x09, 0xFF, 0xC4, 0x00, 0x14, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x2A, 0x9F, 0xFF, 0xD9,
        ]
    }

    fn page_text(doc: &Document, page: u32) -> String {
        String::from_utf8_lossy(&doc.get_page_content(doc.get_pages()[&page]).unwrap()).into_owned()
    }

    #[test]
    fn the_text_under_a_box_is_gone_from_the_file() {
        let mut doc = sample(2, "secret");
        let info = doc.add_object(lopdf::dictionary! { "Author" => Object::string_literal("A") });
        doc.trailer.set("Info", Object::Reference(info));
        let pages = [Blacked { page: 2, jpeg: tiny_jpeg(), lines: Vec::new() }];
        redact(&mut doc, &pages, true, |_, _| true).unwrap();
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        // Page 1 untouched; page 2's words nowhere in the file.
        assert!(text.contains("secret-1"));
        assert!(!text.contains("secret-2"), "the old page's text is still in the file");
        let doc = round_trip(doc);
        assert!(page_text(&doc, 2).contains("/SynabitRedacted Do"));
        assert!(doc.trailer.get(b"Info").is_err());
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn the_words_left_can_still_be_found() {
        let mut doc = sample(1, "p");
        let lines = vec![Line { text: "Họ tên".into(), x: 0.1, y: 0.8, w: 0.3, h: 0.03 }];
        redact(&mut doc, &[Blacked { page: 1, jpeg: tiny_jpeg(), lines }], false, |_, _| true).unwrap();
        assert!(page_text(&round_trip(doc), 1).contains("3 Tr"), "an invisible text layer");
    }

    #[test]
    fn pruning_keeps_what_is_reachable() {
        let mut doc = sample(2, "p");
        let orphan = doc.add_object(Object::string_literal("nobody points here"));
        prune(&mut doc);
        assert!(doc.get_object(orphan).is_err());
        assert_eq!(doc.get_pages().len(), 2);
    }
}
