//! A scanned PDF made searchable: the text Vision read, laid over each page
//! where it was read, invisible.
//!
//! The page keeps its picture; on top of it goes text in render mode 3 (no
//! fill, no stroke), each line stretched to the width of the line in the scan.
//! Selecting, copying and searching then find the words where the eye sees
//! them, as in any scanner's "searchable PDF".
//!
//! # A font for any language without shipping one
//!
//! Invisible text needs a font for its encoding, not for its shapes. So the font
//! is a Type0 font with `Identity-H` — each character is written as its UTF-16
//! code — and a `ToUnicode` map that says code *n* is U+*n*. Nothing is
//! embedded; nothing is drawn. Every advance is the font's default width, and
//! `Tz` squeezes or stretches each line to the box Vision gave it.
//!
//! Characters outside the Basic Multilingual Plane (emoji, rare CJK) become
//! U+FFFD: one code unit each is what keeps every character the same width.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::pdf::PdfError;
use super::stamp;

/// One line of recognised text, and where it sits: fractions of the picture's
/// width and height, origin at the bottom left, as Vision and PDF both count.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Line {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The lines Vision found on one page (1-based) of a PDF.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PageLines {
    pub page: u32,
    pub lines: Vec<Line>,
}

/// Lines top to bottom, and left to right within a row. Rows are formed in
/// one pass over the lines sorted by height on the page: a line joins the row
/// being built when its middle is within half a line of that row's first
/// line. (A comparator that asked "same row?" pairwise would not be a total
/// order, and Rust's sort may panic on one.)
pub fn reading_order(mut lines: Vec<Line>) -> Vec<Line> {
    let middle = |l: &Line| l.y + l.h / 2.0;
    lines.sort_by(|a, b| middle(b).total_cmp(&middle(a)));
    let mut rows: Vec<Vec<Line>> = Vec::new();
    for line in lines {
        match rows.last_mut() {
            Some(row) if (middle(&row[0]) - middle(&line)).abs() < row[0].h.min(line.h) / 2.0 => row.push(line),
            _ => rows.push(vec![line]),
        }
    }
    rows.into_iter()
        .flat_map(|mut row| {
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
            row
        })
        .collect()
}

/// The text of the lines, a row to a line.
pub fn plain_text(lines: &[Line]) -> String {
    let mut out = String::new();
    let mut previous: Option<&Line> = None;
    for line in lines {
        if let Some(p) = previous {
            let same_row = ((p.y + p.h / 2.0) - (line.y + line.h / 2.0)).abs() < p.h.min(line.h) / 2.0;
            out.push(if same_row { ' ' } else { '\n' });
        }
        out.push_str(&line.text);
        previous = Some(line);
    }
    out
}

/// The `ToUnicode` map: code *n* is U+*n*, in ranges of 256 as the format
/// requires (a range may vary only in its last byte). Surrogates are left out.
fn to_unicode() -> Vec<u8> {
    let mut ranges = String::new();
    let highs: Vec<u16> = (0x00..=0xFFu16).filter(|h| !(0xD8..=0xDF).contains(h)).collect();
    for chunk in highs.chunks(100) {
        ranges.push_str(&format!("{} beginbfrange\n", chunk.len()));
        for h in chunk {
            ranges.push_str(&format!("<{h:02X}00> <{h:02X}FF> <{h:02X}00>\n"));
        }
        ranges.push_str("endbfrange\n");
    }
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Synabit-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n{ranges}\
         endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
    .into_bytes()
}

/// The font, added once to a document.
fn add_font(doc: &mut Document) -> ObjectId {
    let to_unicode = doc.add_object(Stream::new(Dictionary::new(), to_unicode()));
    let mut info = Dictionary::new();
    info.set("Registry", Object::string_literal("Adobe"));
    info.set("Ordering", Object::string_literal("Identity"));
    info.set("Supplement", 0);
    let mut descriptor = Dictionary::new();
    descriptor.set("Type", Object::Name(b"FontDescriptor".to_vec()));
    descriptor.set("FontName", Object::Name(b"SynabitInvisible".to_vec()));
    descriptor.set("Flags", 4);
    descriptor.set("FontBBox", vec![0.into(), (-200).into(), 1000.into(), 800.into()]);
    descriptor.set("ItalicAngle", 0);
    descriptor.set("Ascent", 800);
    descriptor.set("Descent", -200);
    descriptor.set("CapHeight", 700);
    descriptor.set("StemV", 80);
    let descriptor = doc.add_object(descriptor);
    let mut cid = Dictionary::new();
    cid.set("Type", Object::Name(b"Font".to_vec()));
    cid.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
    cid.set("BaseFont", Object::Name(b"SynabitInvisible".to_vec()));
    cid.set("CIDSystemInfo", info);
    cid.set("FontDescriptor", Object::Reference(descriptor));
    cid.set("DW", 500);
    let cid = doc.add_object(cid);
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type0".to_vec()));
    font.set("BaseFont", Object::Name(b"SynabitInvisible".to_vec()));
    font.set("Encoding", Object::Name(b"Identity-H".to_vec()));
    font.set("DescendantFonts", vec![Object::Reference(cid)]);
    font.set("ToUnicode", Object::Reference(to_unicode));
    doc.add_object(font)
}

/// A line's text as `Identity-H` codes, in hex.
fn hex_codes(text: &str) -> (String, usize) {
    let mut hex = String::with_capacity(text.len() * 4);
    let mut count = 0;
    for c in text.chars() {
        let unit = if (c as u32) <= 0xFFFF { c as u32 } else { 0xFFFD };
        hex.push_str(&format!("{unit:04X}"));
        count += 1;
    }
    (hex, count)
}

fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.into() }
}

/// Lay each page's lines over it, invisible. Pages with no lines are left as
/// they are.
pub fn add_text_layer(doc: &mut Document, pages: &[PageLines]) -> Result<(), PdfError> {
    let numbered = doc.get_pages();
    if pages.iter().all(|p| p.lines.is_empty()) {
        return Ok(());
    }
    let font = add_font(doc);
    for page in pages.iter().filter(|p| !p.lines.is_empty()) {
        let id = *numbered.get(&page.page).ok_or(PdfError::NoSuchPage { page: page.page, count: numbered.len() as u32 })?;
        let seen = stamp::seen(doc, id);
        let (pw, ph) = seen.size();
        let name = stamp::add_resource(doc, id, b"Font", "SynabitOCR", font)?;
        let [a, b, c, d, e, f] = seen.matrix();
        let mut drawing = format!("q {} {} {} {} {} {} cm\nBT 3 Tr\n", a, b, c, d, e, f);
        for line in &page.lines {
            let (hex, count) = hex_codes(&line.text);
            if count == 0 {
                continue;
            }
            let (x, y) = (line.x * pw as f64, line.y * ph as f64);
            let (w, h) = (line.w * pw as f64, line.h * ph as f64);
            let size = (h * 0.85).max(1.0);
            // Every character advances half the size (DW 500); Tz makes the
            // line as wide as the box.
            let natural = count as f64 * size * 0.5;
            let stretch = (100.0 * w / natural).clamp(1.0, 2000.0);
            drawing.push_str(&format!(
                "/{name} {} Tf {} Tz 1 0 0 1 {} {} Tm <{hex}> Tj\n",
                num(size),
                num(stretch),
                num(x),
                num(y + h * 0.18),
            ));
        }
        drawing.push_str("ET\nQ\n");
        stamp::add_drawing(doc, id, drawing, false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::pdf::tests::{round_trip, sample};

    fn line(text: &str, x: f64, y: f64) -> Line {
        Line { text: text.into(), x, y, w: 0.2, h: 0.02 }
    }

    #[test]
    fn lines_read_top_to_bottom_then_left_to_right() {
        let lines = vec![line("dưới", 0.1, 0.1), line("phải", 0.6, 0.805), line("trái", 0.1, 0.8)];
        let ordered = reading_order(lines);
        assert_eq!(ordered.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["trái", "phải", "dưới"]);
        assert_eq!(plain_text(&ordered), "trái phải\ndưới");
    }

    /// A skewed table with mixed heights: the pairwise "same row" test is not
    /// transitive, which made the old comparator panic in `sort_by`.
    #[test]
    fn a_skewed_table_sorts_without_panicking() {
        let mut lines = Vec::new();
        for skew in 0..200 {
            lines.clear();
            for row in 0..40 {
                for col in 0..5 {
                    let h = if (row + col) % 3 == 0 { 0.012 } else { 0.02 };
                    let y = 0.9 - row as f64 * 0.021 + col as f64 * skew as f64 * 0.00004;
                    lines.push(Line { text: format!("{row}-{col}"), x: col as f64 * 0.2, y, w: 0.15, h });
                }
            }
            let ordered = reading_order(lines.clone());
            assert_eq!(ordered.len(), 200);
        }
    }

    #[test]
    fn text_is_written_as_its_unicode_codes() {
        assert_eq!(hex_codes("Hợp"), ("00481EE30070".into(), 3), "ợ is U+1EE3");
        assert_eq!(hex_codes("😀").0, "FFFD");
    }

    #[test]
    fn the_unicode_map_never_spans_a_high_byte() {
        let map = String::from_utf8(to_unicode()).unwrap();
        assert!(map.contains("<1E00> <1EFF> <1E00>"), "Vietnamese lives at U+1Exx");
        assert!(!map.contains("<D800>"));
        let ranges = map.lines().filter(|l| l.starts_with('<') && l.split(' ').count() == 3).count();
        assert_eq!(ranges, 248);
    }

    #[test]
    fn invisible_text_goes_on_the_pages_given() {
        let mut doc = sample(2, "a");
        let pages = [PageLines { page: 2, lines: vec![line("Hợp đồng", 0.1, 0.9)] }];
        add_text_layer(&mut doc, &pages).unwrap();
        let out = round_trip(doc);
        let content = |n: u32| String::from_utf8_lossy(&out.get_page_content(out.get_pages()[&n]).unwrap()).into_owned();
        assert!(!content(1).contains("3 Tr"));
        assert!(content(2).contains("3 Tr") && content(2).contains("Tj"));
    }

    #[test]
    fn nothing_found_changes_nothing() {
        let mut doc = sample(1, "a");
        let before = doc.objects.len();
        add_text_layer(&mut doc, &[PageLines { page: 1, lines: vec![] }]).unwrap();
        assert_eq!(doc.objects.len(), before);
    }
}
