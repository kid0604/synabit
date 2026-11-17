//! Where a page's edges are: cut in to a box, or the whole page put on paper
//! of another size.
//!
//! Boxes come from the screen as shares of the page *as it is shown* — left,
//! top, right, bottom, 0 to 1, from the top left — the same way the preview
//! draws them whatever `/Rotate` and `/CropBox` the page has. [`stamp::seen`]
//! knows how the shown page sits in the page's own coordinates, so a box is
//! mapped back through it.
//!
//! - **Cropping** sets `/CropBox`, as every crop tool does: nothing is thrown
//!   away, and a reader that wants the whole page can still find it.
//! - **A new paper size** draws the old page, scaled to fit and centred, on a
//!   new one. Annotations move with it.
//!
//! [`stamp::seen`]: super::stamp

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::pdf::PdfError;
use super::stamp;

/// A box on a page as shown: left, top, right, bottom, each 0–1 from the top left.
pub type Share = [f32; 4];

/// The paper a page is put on.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paper {
    /// Points, portrait (width ≤ height).
    pub width: f32,
    pub height: f32,
    pub orientation: super::images::Orientation,
}

type Matrix = [f32; 6];

/// `a` then `b`, in PDF's row-vector order: a point goes through `a` first.
fn then(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn inverse(m: Matrix) -> Matrix {
    let det = m[0] * m[3] - m[1] * m[2];
    let det = if det.abs() < f32::EPSILON { 1.0 } else { det };
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    [a, b, c, d, -(m[4] * a + m[5] * c), -(m[4] * b + m[5] * d)]
}

fn apply(m: Matrix, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn bounds(points: impl IntoIterator<Item = (f32, f32)>) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, y) in points {
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    b
}

fn real_array(values: &[f32]) -> Object {
    Object::Array(values.iter().map(|&v| Object::Real(v)).collect())
}

/// A share box as a `/CropBox` for this page, or `None` if it would leave
/// nothing (or is the whole page, which is no change).
fn crop_box(doc: &Document, page: ObjectId, share: Share) -> Option<[f32; 4]> {
    let [l, t, r, b] = share.map(|v| v.clamp(0.0, 1.0));
    if r - l < 0.01 || b - t < 0.01 {
        return None;
    }
    if l <= 0.0 && t <= 0.0 && r >= 1.0 && b >= 1.0 {
        return None;
    }
    let seen = stamp::seen(doc, page);
    let (sw, sh) = seen.size();
    let m = seen.matrix();
    let corners = [(l * sw, (1.0 - b) * sh), (r * sw, (1.0 - b) * sh), (l * sw, (1.0 - t) * sh), (r * sw, (1.0 - t) * sh)];
    Some(bounds(corners.map(|(x, y)| apply(m, x, y))))
}

/// Cut each page in to its box: one box for every page, or one per page
/// (`None` leaves that page as it is).
pub fn crop(doc: &mut Document, boxes: &[Option<Share>], mut progress: impl FnMut(usize, usize) -> bool) -> Result<(), PdfError> {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    if boxes.len() != 1 && boxes.len() != pages.len() {
        return Err(PdfError::Unreadable(format!("{} boxes for {} pages", boxes.len(), pages.len())));
    }
    for (i, &page) in pages.iter().enumerate() {
        if !progress(i, pages.len()) {
            return Err(PdfError::Cancelled);
        }
        let share = if boxes.len() == 1 { boxes[0] } else { boxes[i] };
        let Some(cropped) = share.and_then(|s| crop_box(doc, page, s)) else { continue };
        let dict = doc.get_dictionary_mut(page).map_err(|e| PdfError::Unreadable(e.to_string()))?;
        dict.set("CropBox", real_array(&cropped));
        // A trim or art box larger than what is left would say otherwise.
        for key in [&b"TrimBox"[..], b"ArtBox", b"BleedBox"] {
            dict.remove(key);
        }
    }
    progress(pages.len(), pages.len());
    Ok(())
}

/// Where the shown page lands on paper of this size: the page size, and the
/// matrix from the shown page (origin bottom left) onto it.
fn on_paper(shown: (f32, f32), paper: Paper) -> ((f32, f32), Matrix) {
    use super::images::Orientation;
    let (sw, sh) = shown;
    let (short, long) = (paper.width.min(paper.height), paper.width.max(paper.height));
    let landscape = match paper.orientation {
        Orientation::Portrait => false,
        Orientation::Landscape => true,
        Orientation::Auto => sw > sh,
    };
    let (w, h) = if landscape { (long, short) } else { (short, long) };
    let s = (w / sw).min(h / sh);
    ((w, h), [s, 0.0, 0.0, s, (w - sw * s) / 2.0, (h - sh * s) / 2.0])
}

/// Move an annotation's geometry by `m`: its rectangle and the points some
/// kinds carry (highlights, ink, lines, polygons).
fn move_annotation(doc: &mut Document, id: ObjectId, m: Matrix) {
    let Ok(annot) = doc.get_dictionary_mut(id) else { return };
    let numbers = |o: &Object| -> Vec<f32> { o.as_array().map(|a| a.iter().filter_map(|v| v.as_float().ok()).collect()).unwrap_or_default() };
    let moved = |values: Vec<f32>| -> Vec<f32> {
        values.chunks_exact(2).flat_map(|p| {
            let (x, y) = apply(m, p[0], p[1]);
            [x, y]
        }).collect()
    };
    if let Ok(rect) = annot.get(b"Rect").map(numbers) {
        if rect.len() == 4 {
            let corners = [(rect[0], rect[1]), (rect[2], rect[1]), (rect[0], rect[3]), (rect[2], rect[3])];
            annot.set("Rect", real_array(&bounds(corners.map(|(x, y)| apply(m, x, y)))));
        }
    }
    for key in [&b"QuadPoints"[..], b"L", b"Vertices", b"CL"] {
        if let Ok(values) = annot.get(key).map(numbers) {
            if !values.is_empty() {
                annot.set(key.to_vec(), real_array(&moved(values)));
            }
        }
    }
    if let Ok(Object::Array(strokes)) = annot.get(b"InkList").cloned() {
        let strokes: Vec<Object> = strokes.iter().map(|s| real_array(&moved(numbers(s)))).collect();
        annot.set("InkList", strokes);
    }
}

/// Put each page on paper of this size: scaled to fit, centred, upright.
pub fn resize(doc: &mut Document, paper: Paper, mut progress: impl FnMut(usize, usize) -> bool) -> Result<(), PdfError> {
    if !(paper.width >= 72.0 && paper.height >= 72.0 && paper.width <= 14_400.0 && paper.height <= 14_400.0) {
        return Err(PdfError::Unreadable("the paper size is out of range".into()));
    }
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for (i, &page) in pages.iter().enumerate() {
        if !progress(i, pages.len()) {
            return Err(PdfError::Cancelled);
        }
        let seen = stamp::seen(doc, page);
        let shown = seen.size();
        if shown.0 < 1.0 || shown.1 < 1.0 {
            continue;
        }
        let ((w, h), onto) = on_paper(shown, paper);
        // The page's own coordinates → as shown → on the new paper.
        let whole = then(inverse(seen.matrix()), onto);
        let (cx, cy) = (onto[4], onto[5]);
        let (cw, ch) = (shown.0 * onto[0], shown.1 * onto[3]);
        let before = format!(
            "q {} {} {} {} re W n {} cm\n",
            cx, cy, cw, ch,
            whole.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")
        );

        let existing: Vec<Object> = match doc.get_dictionary(page).ok().and_then(|d| d.get(b"Contents").ok()) {
            Some(Object::Array(items)) => items.clone(),
            Some(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Array(items)) => items.clone(),
                _ => vec![Object::Reference(*id)],
            },
            _ => Vec::new(),
        };
        let mut contents = Vec::with_capacity(existing.len() + 2);
        contents.push(Object::Reference(doc.add_object(Stream::new(Dictionary::new(), before.into_bytes()))));
        contents.extend(existing);
        contents.push(Object::Reference(doc.add_object(Stream::new(Dictionary::new(), b"\nQ\n".to_vec()))));

        let annots: Vec<ObjectId> = match doc.get_dictionary(page).ok().and_then(|d| d.get(b"Annots").ok()) {
            Some(Object::Array(items)) => items.iter().filter_map(|o| o.as_reference().ok()).collect(),
            Some(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Array(items)) => items.iter().filter_map(|o| o.as_reference().ok()).collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        for annot in annots {
            move_annotation(doc, annot, whole);
        }

        let dict = doc.get_dictionary_mut(page).map_err(|e| PdfError::Unreadable(e.to_string()))?;
        dict.set("Contents", contents);
        dict.set("MediaBox", real_array(&[0.0, 0.0, w, h]));
        // Set, not removed: a page may inherit a turn from its parent.
        dict.set("Rotate", 0);
        for key in [&b"CropBox"[..], b"TrimBox", b"ArtBox", b"BleedBox"] {
            dict.remove(key);
        }
    }
    progress(pages.len(), pages.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::images::Orientation;
    use crate::utilities::pdf::tests::{labels, round_trip, sample};

    fn numbers(doc: &Document, page: u32, key: &[u8]) -> Vec<f32> {
        let id = doc.get_pages()[&page];
        doc.get_dictionary(id).unwrap().get(key).unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect()
    }

    fn turn(doc: &mut Document, page: u32, degrees: i64) {
        let id = doc.get_pages()[&page];
        doc.get_dictionary_mut(id).unwrap().set("Rotate", degrees);
    }

    #[test]
    fn a_box_on_the_shown_page_becomes_the_crop_box() {
        // sample pages are A4, 595 × 842, from the page tree.
        let mut doc = sample(2, "p");
        crop(&mut doc, &[Some([0.1, 0.2, 0.9, 0.8])], |_, _| true).unwrap();
        let doc = round_trip(doc);
        let b = numbers(&doc, 2, b"CropBox");
        assert!((b[0] - 59.5).abs() < 0.1 && (b[2] - 535.5).abs() < 0.1, "{b:?}");
        // "Top" is the top: 20% down from the top is 80% up from the bottom.
        assert!((b[1] - 168.4).abs() < 0.1 && (b[3] - 673.6).abs() < 0.1, "{b:?}");
    }

    #[test]
    fn a_turned_page_is_cut_where_it_was_seen() {
        let mut doc = sample(1, "p");
        turn(&mut doc, 1, 90);
        // Shown 842 wide, 595 tall: keep the left half of what is seen.
        crop(&mut doc, &[Some([0.0, 0.0, 0.5, 1.0])], |_, _| true).unwrap();
        let b = numbers(&doc, 1, b"CropBox");
        // Turned 90° clockwise the page's bottom is shown on the left, so the
        // shown left half is the page's bottom half.
        assert!((b[0] - 0.0).abs() < 0.1 && (b[2] - 595.0).abs() < 0.1, "{b:?}");
        assert!((b[1] - 0.0).abs() < 0.1 && (b[3] - 421.0).abs() < 0.1, "{b:?}");
    }

    #[test]
    fn a_page_with_no_box_or_the_whole_box_is_left_alone() {
        let mut doc = sample(2, "p");
        crop(&mut doc, &[None, Some([0.0, 0.0, 1.0, 1.0])], |_, _| true).unwrap();
        let id = doc.get_pages()[&1];
        assert!(doc.get_dictionary(id).unwrap().get(b"CropBox").is_err());
        assert!(crop(&mut doc, &[None, None, None], |_, _| true).is_err(), "three boxes for two pages");
    }

    #[test]
    fn a_page_goes_on_new_paper_scaled_and_centred() {
        let mut doc = sample(2, "p");
        turn(&mut doc, 2, 90);
        // A5 portrait, but "auto" lays a landscape page on its side.
        let paper = Paper { width: 420.0, height: 595.0, orientation: Orientation::Auto };
        resize(&mut doc, paper, |_, _| true).unwrap();
        let doc = round_trip(doc);
        assert_eq!(numbers(&doc, 1, b"MediaBox"), [0.0, 0.0, 420.0, 595.0]);
        assert_eq!(numbers(&doc, 2, b"MediaBox"), [0.0, 0.0, 595.0, 420.0]);
        let id = doc.get_pages()[&2];
        assert_eq!(doc.get_dictionary(id).unwrap().get(b"Rotate").unwrap().as_i64().unwrap(), 0);
        // The text is still there, and still drawn.
        assert_eq!(labels(&doc), ["p-1", "p-2"]);
        let content = String::from_utf8(doc.get_page_content(doc.get_pages()[&1]).unwrap()).unwrap();
        assert!(content.starts_with("q 0 "), "{content}");
    }

    #[test]
    fn the_matrices_undo_each_other() {
        let m = [0.0, 1.0, -1.0, 0.0, 842.0, 0.0];
        let back = then(m, inverse(m));
        for (a, b) in back.iter().zip([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]) {
            assert!((a - b).abs() < 1e-4, "{back:?}");
        }
    }
}
