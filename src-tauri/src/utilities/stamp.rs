//! Things drawn on top of existing pages: a watermark, page numbers.
//!
//! # A watermark is a picture
//!
//! The standard PDF fonts know Latin-1 and nothing else: no "ạ", no "ữ".
//! Writing Vietnamese as real text would mean embedding and subsetting a
//! TrueType font, which `lopdf` does not do. So the WebView draws the
//! watermark — text in the system font, or a logo — on a canvas, and hands
//! over its pixels. Here they become one image with an alpha mask, stored once
//! and placed on every page (design §4.2). Text in a watermark cannot be
//! selected; for a watermark that is what is wanted.
//!
//! # Page numbers are text
//!
//! Digits and a few ASCII words ("Page", "Trang", "of", "/") are all Latin-1,
//! so numbers are real Helvetica text: sharp at any zoom, and found by search.
//!
//! # Where "up" is
//!
//! A page with `/Rotate 90` is shown turned, and its visible area may not start
//! at 0,0. Both are drawn in the page as it is *seen*: [`seen`] works out the
//! visible box and turn, and a matrix maps what is drawn into the page's own
//! coordinates, so "bottom centre" is bottom centre on screen whatever the page
//! says about itself.

use std::io::Write;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::pdf::PdfError;

/// Pixels to stamp, RGBA, row by row from the top.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Where a watermark goes.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatermarkLayout {
    /// 0–8, reading order from the top left; ignored when `tile`.
    pub position: u8,
    /// Repeat across the whole page.
    pub tile: bool,
    /// The watermark's width as a share of the page's width, 0.05–1.
    pub size: f32,
    /// Degrees, clockwise as CSS turns things, so the preview and the PDF agree.
    pub rotate: f32,
    /// Behind the page's content rather than over it.
    pub under: bool,
}

/// How pages are numbered.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Numbering {
    /// 0–2 along the top, 3–5 along the bottom; left, centre, right.
    pub position: u8,
    /// With `{n}` for the number and optionally `{total}`.
    pub format: String,
    /// The number the first numbered page gets.
    pub start: u32,
    /// Leave the first page (a cover) without a number.
    pub skip_first: bool,
    /// Points.
    pub size: f32,
}

/// The page as it is seen: its visible box and the turn it is shown at.
pub(super) struct Seen {
    x0: f32,
    y0: f32,
    /// Size of the box in the page's own coordinates.
    w: f32,
    h: f32,
    rotate: i64,
}

impl Seen {
    /// Width and height on screen.
    pub(super) fn size(&self) -> (f32, f32) {
        if self.rotate % 180 == 0 { (self.w, self.h) } else { (self.h, self.w) }
    }

    /// The matrix from on-screen coordinates (origin bottom left) to the page's.
    pub(super) fn matrix(&self) -> [f32; 6] {
        let (w, h, x0, y0) = (self.w, self.h, self.x0, self.y0);
        match self.rotate {
            90 => [0.0, 1.0, -1.0, 0.0, w + x0, y0],
            180 => [-1.0, 0.0, 0.0, -1.0, w + x0, h + y0],
            270 => [0.0, -1.0, 1.0, 0.0, x0, h + y0],
            _ => [1.0, 0.0, 0.0, 1.0, x0, y0],
        }
    }
}

/// A key of a page, looking up the page tree when the page does not say.
fn inherited(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    let mut id = Some(page);
    let mut hops = 0;
    while let Some(current) = id {
        let dict = doc.get_dictionary(current).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(value.clone());
        }
        id = dict.get(b"Parent").and_then(Object::as_reference).ok();
        hops += 1;
        if hops > 64 {
            return None;
        }
    }
    None
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        other => other,
    }
}

fn rect(doc: &Document, object: Option<Object>) -> Option<[f32; 4]> {
    let object = object?;
    let array = resolve(doc, &object).as_array().ok()?;
    let n: Vec<f32> = array.iter().filter_map(|o| resolve(doc, o).as_float().ok()).collect();
    (n.len() == 4).then(|| [n[0].min(n[2]), n[1].min(n[3]), n[0].max(n[2]), n[1].max(n[3])])
}

pub(super) fn seen(doc: &Document, page: ObjectId) -> Seen {
    let media = rect(doc, inherited(doc, page, b"MediaBox")).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let crop = rect(doc, inherited(doc, page, b"CropBox")).unwrap_or(media);
    let rotate = inherited(doc, page, b"Rotate").map(|o| super::pdf::rotation_of(doc, &o)).unwrap_or(0);
    Seen {
        x0: crop[0],
        y0: crop[1],
        w: crop[2] - crop[0],
        h: crop[3] - crop[1],
        rotate: super::pdf::turned(rotate, 0),
    }
}

fn num(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.into() }
}

fn cm(m: [f32; 6]) -> String {
    format!("{} cm", m.iter().map(|&v| num(v)).collect::<Vec<_>>().join(" "))
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(bytes).expect("writing to memory");
    z.finish().expect("writing to memory")
}

/// The picture as an image with a soft mask, stored once in the document.
fn add_picture(doc: &mut Document, picture: &Picture) -> Result<ObjectId, PdfError> {
    // Checked: the size comes from the WebView, and 65536 × 65536 is 0 in u32.
    let pixels = (picture.width as usize).checked_mul(picture.height as usize).filter(|&p| p <= 64 << 20);
    let Some(pixels) = pixels else {
        return Err(PdfError::Unreadable("the watermark picture is too large".into()));
    };
    if picture.width == 0 || picture.height == 0 || picture.rgba.len() != pixels * 4 {
        return Err(PdfError::Unreadable("the watermark picture is the wrong size".into()));
    }
    let mut rgb = Vec::with_capacity(pixels * 3);
    let mut alpha = Vec::with_capacity(pixels);
    for px in picture.rgba.chunks_exact(4) {
        rgb.extend_from_slice(&px[..3]);
        alpha.push(px[3]);
    }
    let image_dict = |space: &[u8]| {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Image".to_vec()));
        d.set("Width", picture.width as i64);
        d.set("Height", picture.height as i64);
        d.set("ColorSpace", Object::Name(space.to_vec()));
        d.set("BitsPerComponent", 8);
        d.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        d.set("Interpolate", true);
        d
    };
    let mask = doc.add_object(Stream::new(image_dict(b"DeviceGray"), deflate(&alpha)).with_compression(false));
    let mut dict = image_dict(b"DeviceRGB");
    dict.set("SMask", Object::Reference(mask));
    Ok(doc.add_object(Stream::new(dict, deflate(&rgb)).with_compression(false)))
}

/// Give a page its own resource dictionary with `name` → `object` under
/// `kind` (`XObject` or `Font`), choosing another name if that one is taken.
/// Returns the name used.
pub(super) fn add_resource(doc: &mut Document, page: ObjectId, kind: &[u8], name: &str, object: ObjectId) -> Result<String, PdfError> {
    let mut resources = match inherited(doc, page, b"Resources") {
        Some(o) => resolve(doc, &o).as_dict().cloned().unwrap_or_default(),
        None => Dictionary::new(),
    };
    let mut group = match resources.get(kind) {
        Ok(o) => resolve(doc, o).as_dict().cloned().unwrap_or_default(),
        Err(_) => Dictionary::new(),
    };
    let mut chosen = name.to_string();
    let mut n = 2;
    while group.has(chosen.as_bytes()) {
        chosen = format!("{name}{n}");
        n += 1;
    }
    group.set(chosen.as_bytes().to_vec(), Object::Reference(object));
    resources.set(kind.to_vec(), group);
    let dict = doc.get_dictionary_mut(page).map_err(|e| PdfError::Unreadable(e.to_string()))?;
    dict.set("Resources", resources);
    Ok(chosen)
}

/// Draw `drawing` on a page, over its content or under it. Over, the existing
/// content is wrapped in `q … Q` first, so whatever state it leaves behind
/// cannot move or recolour what is added.
pub(super) fn add_drawing(doc: &mut Document, page: ObjectId, drawing: String, under: bool) -> Result<(), PdfError> {
    let existing: Vec<Object> = match doc.get_dictionary(page).ok().and_then(|d| d.get(b"Contents").ok()) {
        Some(Object::Array(items)) => items.clone(),
        Some(Object::Reference(id)) => {
            // A reference to an array of streams, or to one stream.
            match doc.get_object(*id) {
                Ok(Object::Array(items)) => items.clone(),
                _ => vec![Object::Reference(*id)],
            }
        }
        _ => Vec::new(),
    };
    let mut contents = Vec::with_capacity(existing.len() + 2);
    if under {
        contents.push(Object::Reference(doc.add_object(Stream::new(Dictionary::new(), drawing.into_bytes()))));
        contents.extend(existing);
    } else {
        contents.push(Object::Reference(doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()))));
        contents.extend(existing);
        let tail = format!("\nQ\n{drawing}");
        contents.push(Object::Reference(doc.add_object(Stream::new(Dictionary::new(), tail.into_bytes()))));
    }
    let dict = doc.get_dictionary_mut(page).map_err(|e| PdfError::Unreadable(e.to_string()))?;
    dict.set("Contents", contents);
    Ok(())
}

/// The centres a watermark of this on-screen size goes at.
fn centres(layout: &WatermarkLayout, page: (f32, f32), bbox: (f32, f32)) -> Vec<(f32, f32)> {
    let (pw, ph) = page;
    let (bw, bh) = bbox;
    if layout.tile {
        let (sx, sy) = (bw * 1.35 + 12.0, bh * 1.8 + 12.0);
        let mut out = Vec::new();
        let mut row = 0;
        let mut y = ph - bh / 2.0;
        while y > -bh / 2.0 {
            // Every other row shifted by half, like bricks.
            let mut x = if row % 2 == 0 { bw / 2.0 } else { bw / 2.0 + sx / 2.0 };
            while x < pw + bw / 2.0 {
                out.push((x, y));
                x += sx;
            }
            y -= sy;
            row += 1;
        }
        return out;
    }
    let margin = 0.05 * pw.min(ph);
    let col = (layout.position % 3) as f32;
    let row = (layout.position.min(8) / 3) as f32;
    let x = [margin + bw / 2.0, pw / 2.0, pw - margin - bw / 2.0][col as usize];
    let y = [ph - margin - bh / 2.0, ph / 2.0, margin + bh / 2.0][row as usize];
    vec![(x, y)]
}

/// Stamp `picture` on the given pages (1-based; empty is every page).
pub fn watermark(
    doc: &mut Document,
    picture: &Picture,
    layout: &WatermarkLayout,
    pages: &[u32],
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<(), PdfError> {
    let image = add_picture(doc, picture)?;
    let targets = targets(doc, pages)?;
    let theta = (-layout.rotate).to_radians();
    let (sin, cos) = theta.sin_cos();
    for (i, page) in targets.iter().enumerate() {
        if !progress(i, targets.len()) {
            return Err(PdfError::Cancelled);
        }
        let seen = seen(doc, *page);
        let (pw, ph) = seen.size();
        let w = (layout.size.clamp(0.05, 1.0)) * pw;
        let h = w * picture.height as f32 / picture.width as f32;
        let bbox = (w * cos.abs() + h * sin.abs(), w * sin.abs() + h * cos.abs());
        let name = add_resource(doc, *page, b"XObject", "SynabitMark", image)?;
        let mut drawing = format!("q {}\n", cm(seen.matrix()));
        for (cx, cy) in centres(layout, (pw, ph), bbox) {
            drawing.push_str(&format!(
                "q {} {} {} /{name} Do Q\n",
                cm([1.0, 0.0, 0.0, 1.0, cx, cy]),
                cm([cos, sin, -sin, cos, 0.0, 0.0]),
                cm([w, 0.0, 0.0, h, -w / 2.0, -h / 2.0]),
            ));
        }
        drawing.push_str("Q\n");
        add_drawing(doc, *page, drawing, layout.under)?;
    }
    progress(targets.len(), targets.len());
    Ok(())
}

/// Page object ids for 1-based numbers; every page when `pages` is empty.
fn targets(doc: &Document, pages: &[u32]) -> Result<Vec<ObjectId>, PdfError> {
    let all = doc.get_pages();
    if pages.is_empty() {
        return Ok(all.values().copied().collect());
    }
    pages
        .iter()
        .map(|n| all.get(n).copied().ok_or(PdfError::NoSuchPage { page: *n, count: all.len() as u32 }))
        .collect()
}

/// Helvetica's advance widths, in thousandths of the font size, for the
/// characters page numbers may use. From Adobe's Helvetica AFM.
fn helvetica_width(c: char) -> Option<f32> {
    let w = match c {
        '0'..='9' => 556,
        ' ' | '/' | '.' | ',' | ':' | 'f' | 't' => 278,
        '-' | '(' | ')' | 'r' => 333,
        'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'n' | 'o' | 'p' | 'q' | 'u' => 556,
        'c' | 'k' | 's' | 'v' | 'x' | 'y' | 'z' | 'J' => 500,
        'i' | 'j' | 'l' => 222,
        'm' | 'M' => 833,
        'w' | 'C' | 'D' | 'H' | 'N' | 'R' | 'U' => 722,
        'A' | 'B' | 'E' | 'K' | 'P' | 'S' | 'V' | 'X' | 'Y' => 667,
        'F' | 'T' | 'Z' => 611,
        'G' | 'O' | 'Q' => 778,
        'I' => 278,
        'L' => 556,
        'W' => 944,
        _ => return None,
    };
    Some(w as f32)
}

/// The text a page gets, or why the format cannot be drawn.
pub fn number_text(format: &str, n: u32, total: u32) -> Result<String, PdfError> {
    let text = format.replace("{n}", &n.to_string()).replace("{total}", &total.to_string());
    if let Some(bad) = text.chars().find(|c| helvetica_width(*c).is_none()) {
        return Err(PdfError::Unreadable(format!("“{bad}” cannot be written in a page number")));
    }
    Ok(text)
}

fn text_width(text: &str, size: f32) -> f32 {
    text.chars().filter_map(helvetica_width).sum::<f32>() * size / 1000.0
}

/// Number pages: every page, or every page but the first.
pub fn number_pages(doc: &mut Document, numbering: &Numbering, mut progress: impl FnMut(usize, usize) -> bool) -> Result<(), PdfError> {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let numbered: Vec<ObjectId> = pages.iter().copied().skip(usize::from(numbering.skip_first)).collect();
    if numbered.is_empty() {
        return Err(PdfError::Empty);
    }
    let total = numbering.start + numbered.len() as u32 - 1;
    // Fail on a bad format before anything is changed.
    number_text(&numbering.format, total, total)?;

    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    let font = doc.add_object(font);

    let size = numbering.size.clamp(6.0, 36.0);
    for (i, page) in numbered.iter().enumerate() {
        if !progress(i, numbered.len()) {
            return Err(PdfError::Cancelled);
        }
        let text = number_text(&numbering.format, numbering.start + i as u32, total)?;
        let seen = seen(doc, *page);
        let (pw, ph) = seen.size();
        let margin = (0.045 * pw.min(ph)).max(18.0);
        let width = text_width(&text, size);
        let x = match numbering.position % 3 {
            0 => margin,
            1 => (pw - width) / 2.0,
            _ => pw - margin - width,
        };
        let y = if numbering.position < 3 { ph - margin - size * 0.75 } else { margin };
        let name = add_resource(doc, *page, b"Font", "SynabitNum", font)?;
        let escaped = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
        let drawing = format!(
            "q {}\nBT /{name} {} Tf 0.25 0.25 0.25 rg {} {} Td ({escaped}) Tj ET\nQ\n",
            cm(seen.matrix()),
            num(size),
            num(x),
            num(y),
        );
        add_drawing(doc, *page, drawing, false)?;
    }
    progress(numbered.len(), numbered.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::pdf::tests::{labels, round_trip, sample};

    fn red_square() -> Picture {
        Picture { width: 2, height: 2, rgba: [255, 0, 0, 128].repeat(4) }
    }

    fn layout() -> WatermarkLayout {
        WatermarkLayout { position: 4, tile: false, size: 0.5, rotate: -35.0, under: false }
    }

    fn page_content(doc: &Document, n: u32) -> String {
        String::from_utf8_lossy(&doc.get_page_content(doc.get_pages()[&n]).unwrap()).into_owned()
    }

    /// A real file through both stamps: `UTILITIES_STAMP_IN` to
    /// `UTILITIES_STAMP_OUT`, with a translucent red bar as the watermark.
    #[test]
    #[ignore]
    fn real_stamp() {
        let input = std::env::var("UTILITIES_STAMP_IN").unwrap();
        let output = std::env::var("UTILITIES_STAMP_OUT").unwrap();
        let mut doc = crate::utilities::pdf::open(std::path::Path::new(&input)).unwrap();
        let (w, h) = (400u32, 80u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let edge = x < 6 || y < 6 || x >= w - 6 || y >= h - 6;
                rgba.extend([217, 56, 56, if edge { 200 } else { 60 }]);
            }
        }
        let picture = Picture { width: w, height: h, rgba };
        watermark(&mut doc, &picture, &WatermarkLayout { position: 4, tile: false, size: 0.7, rotate: -35.0, under: false }, &[], |_, _| true).unwrap();
        let numbering = Numbering { position: 4, format: "Trang {n} / {total}".into(), start: 1, skip_first: false, size: 11.0 };
        number_pages(&mut doc, &numbering, |_, _| true).unwrap();
        crate::utilities::pdf::save(&mut doc, std::path::Path::new(&output)).unwrap();
    }

    #[test]
    fn a_watermark_goes_on_every_page_and_keeps_the_text() {
        let mut doc = sample(3, "a");
        watermark(&mut doc, &red_square(), &layout(), &[], |_, _| true).unwrap();
        let out = round_trip(doc);
        assert_eq!(labels(&out), ["a-1", "a-2", "a-3"], "the pages' own text is still there");
        for n in 1..=3 {
            let content = page_content(&out, n);
            assert!(content.starts_with("q\n"), "the old content is wrapped: {content}");
            assert!(content.contains("/SynabitMark Do"), "{content}");
        }
        // One picture shared, not one per page.
        let images = out.objects.values().filter(|o| matches!(o, Object::Stream(s) if s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))).count();
        assert_eq!(images, 2, "the image and its mask");
    }

    #[test]
    fn a_watermark_can_go_on_some_pages_only() {
        let mut doc = sample(3, "a");
        watermark(&mut doc, &red_square(), &layout(), &[2], |_, _| true).unwrap();
        let out = round_trip(doc);
        assert!(!page_content(&out, 1).contains("SynabitMark"));
        assert!(page_content(&out, 2).contains("SynabitMark"));
    }

    #[test]
    fn under_draws_before_the_content() {
        let mut doc = sample(1, "a");
        watermark(&mut doc, &red_square(), &WatermarkLayout { under: true, ..layout() }, &[], |_, _| true).unwrap();
        let content = page_content(&round_trip(doc), 1);
        assert!(content.find("SynabitMark").unwrap() < content.find("Tj").unwrap());
    }

    #[test]
    fn a_picture_of_the_wrong_size_is_refused() {
        let mut doc = sample(1, "a");
        let bad = Picture { width: 3, height: 3, rgba: vec![0; 4] };
        assert!(watermark(&mut doc, &bad, &layout(), &[], |_, _| true).is_err());
    }

    #[test]
    fn tiles_cover_the_page() {
        let centres = centres(&WatermarkLayout { tile: true, ..layout() }, (600.0, 800.0), (200.0, 60.0));
        assert!(centres.len() >= 10);
        assert!(centres.iter().any(|&(x, y)| x < 200.0 && y > 700.0));
        assert!(centres.iter().any(|&(x, y)| x > 400.0 && y < 200.0));
    }

    #[test]
    fn the_seen_page_follows_its_rotation() {
        let s = Seen { x0: 0.0, y0: 0.0, w: 600.0, h: 800.0, rotate: 90 };
        assert_eq!(s.size(), (800.0, 600.0));
        // The seen bottom-left corner is the page's bottom-right... turned.
        let [a, b, c, d, e, f] = s.matrix();
        let at = |u: f32, v: f32| (a * u + c * v + e, b * u + d * v + f);
        let corners = [at(0.0, 0.0), at(800.0, 0.0), at(0.0, 600.0), at(800.0, 600.0)];
        for (x, y) in corners {
            assert!((0.0..=600.0).contains(&x) && (0.0..=800.0).contains(&y), "({x}, {y}) is off the page");
        }
    }

    #[test]
    fn numbers_are_text_on_the_pages_asked_for() {
        let mut doc = sample(3, "a");
        let numbering = Numbering { position: 4, format: "Trang {n} / {total}".into(), start: 1, skip_first: true, size: 10.0 };
        number_pages(&mut doc, &numbering, |_, _| true).unwrap();
        let out = round_trip(doc);
        assert!(!page_content(&out, 1).contains("Trang"), "the cover is left alone");
        assert!(page_content(&out, 2).contains("(Trang 1 / 2) Tj"));
        assert!(page_content(&out, 3).contains("(Trang 2 / 2) Tj"));
    }

    #[test]
    fn a_number_format_outside_latin_is_refused() {
        assert!(number_text("Trang {n}", 3, 9).is_ok());
        assert!(number_text("Tr\u{1ea1}ng {n}", 3, 9).is_err());
    }
}
