//! Pictures made into a PDF, one per page.
//!
//! A JPEG goes into a PDF as it is: PDF's `DCTDecode` filter *is* JPEG, so the
//! file's bytes become the image stream unchanged, with no decoding, no
//! re-encoding and no loss. That is why this needs no image crate (see
//! `commands::thumbnails` for why the app has none). What it does need is the
//! few facts a PDF must state about the picture — its size, its colour space
//! — and the one a phone hides in EXIF: which way up it was held.
//!
//! Anything that is not a JPEG is turned into one by the WebView first, on a
//! canvas, which knows every format the system does (HEIC, PNG, WebP).

use lopdf::{Dictionary, Document, Object, Stream};

/// What a PDF has to say about a JPEG to show it.
#[derive(Debug, Clone, PartialEq)]
pub struct Jpeg {
    pub width: u32,
    pub height: u32,
    /// 1 grey, 3 colour, 4 CMYK.
    pub components: u8,
    /// Adobe's CMYK JPEGs store the ink inverted (APP14 with a transform).
    pub adobe_inverted: bool,
    /// EXIF orientation, 1 to 8; 1 when the file does not say.
    pub orientation: u8,
}

impl Jpeg {
    /// Width and height as the picture is meant to be seen.
    pub fn shown_size(&self) -> (u32, u32) {
        if (5..=8).contains(&self.orientation) {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

/// Why a picture could not be read.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ImageError {
    #[error("not a JPEG")]
    NotJpeg,
    #[error("a JPEG this app cannot place: {0}")]
    Unsupported(String),
}

/// Read the facts [`Jpeg`] holds from a file's bytes, without decoding it.
pub fn read_jpeg(bytes: &[u8]) -> Result<Jpeg, ImageError> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err(ImageError::NotJpeg);
    }
    let mut at = 2;
    let mut orientation = 1;
    let mut adobe_inverted = false;
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xFF {
            return Err(ImageError::NotJpeg);
        }
        let marker = bytes[at + 1];
        // Fill bytes before a marker.
        if marker == 0xFF {
            at += 1;
            continue;
        }
        // Markers that stand alone, with no length.
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            at += 2;
            continue;
        }
        let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        if len < 2 || at + 2 + len > bytes.len() {
            return Err(ImageError::NotJpeg);
        }
        let body = &bytes[at + 4..at + 2 + len];
        match marker {
            // Start of frame: every kind except the differential and
            // arithmetic-coded ones nobody writes, which PDF readers refuse.
            0xC0..=0xC2 => {
                if body.len() < 6 {
                    return Err(ImageError::NotJpeg);
                }
                if body[0] != 8 {
                    return Err(ImageError::Unsupported(format!("{}-bit samples", body[0])));
                }
                let height = u16::from_be_bytes([body[1], body[2]]) as u32;
                let width = u16::from_be_bytes([body[3], body[4]]) as u32;
                let components = body[5];
                if width == 0 || height == 0 {
                    return Err(ImageError::Unsupported("no size given in the frame".into()));
                }
                if ![1, 3, 4].contains(&components) {
                    return Err(ImageError::Unsupported(format!("{components} colour components")));
                }
                return Ok(Jpeg { width, height, components, adobe_inverted, orientation });
            }
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(ImageError::Unsupported("lossless or arithmetic coding".into()));
            }
            // APP1: EXIF.
            0xE1 if body.starts_with(b"Exif\0\0") => {
                if let Some(o) = exif_orientation(&body[6..]) {
                    orientation = o;
                }
            }
            // APP14: Adobe. A transform of 0 on four components means the
            // ink values are stored inverted.
            0xEE if body.starts_with(b"Adobe") && body.len() >= 12 => {
                adobe_inverted = true;
            }
            // Start of scan before any frame: not a file we understand.
            0xDA => return Err(ImageError::NotJpeg),
            _ => {}
        }
        at += 2 + len;
    }
    Err(ImageError::NotJpeg)
}

/// The orientation tag (0x0112) from a TIFF block, if it is there and sane.
pub(super) fn exif_orientation(tiff: &[u8]) -> Option<u8> {
    let little = match tiff.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b = tiff.get(i..i + 2)?;
        Some(if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b = tiff.get(i..i + 4)?;
        let b = [b[0], b[1], b[2], b[3]];
        Some(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    let ifd = u32_at(4)? as usize;
    let entries = u16_at(ifd)? as usize;
    for i in 0..entries.min(256) {
        let entry = ifd + 2 + i * 12;
        if u16_at(entry)? == 0x0112 {
            let value = u16_at(entry + 8)?;
            return (1..=8).contains(&value).then_some(value as u8);
        }
    }
    None
}

/// The page a picture goes on.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageSize {
    A4,
    Letter,
    /// The page is the picture, at 96 pixels to the inch — a screenshot
    /// comes out the size it was on screen.
    Fit,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Landscape for a wide picture, portrait otherwise.
    Auto,
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub size: PageSize,
    pub orientation: Orientation,
    /// Space around the picture, in points (1/72 inch). Ignored for `Fit`.
    pub margin: f32,
}

const A4: (f32, f32) = (595.28, 841.89);
const LETTER: (f32, f32) = (612.0, 792.0);
const FIT_SCALE: f32 = 72.0 / 96.0;

/// Where a picture of this size lands: the page size, and the box the
/// picture fills on it.
pub fn place(shown: (u32, u32), layout: Layout) -> ((f32, f32), (f32, f32, f32, f32)) {
    let (w, h) = (shown.0 as f32, shown.1 as f32);
    if layout.size == PageSize::Fit {
        let page = (w * FIT_SCALE, h * FIT_SCALE);
        return (page, (0.0, 0.0, page.0, page.1));
    }
    let (short, long) = if layout.size == PageSize::A4 { A4 } else { LETTER };
    let landscape = match layout.orientation {
        Orientation::Portrait => false,
        Orientation::Landscape => true,
        Orientation::Auto => w > h,
    };
    let page = if landscape { (long, short) } else { (short, long) };
    let margin = layout.margin.clamp(0.0, page.0.min(page.1) / 4.0);
    let (room_w, room_h) = (page.0 - 2.0 * margin, page.1 - 2.0 * margin);
    let scale = (room_w / w).min(room_h / h);
    let (fw, fh) = (w * scale, h * scale);
    (page, ((page.0 - fw) / 2.0, (page.1 - fh) / 2.0, fw, fh))
}

/// The matrix that turns the stored picture the way EXIF says, inside the
/// unit square the image is drawn in (PDF 1.7 §8.9.4, y pointing up).
fn orientation_matrix(orientation: u8) -> [f32; 6] {
    match orientation {
        2 => [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0],
        3 => [-1.0, 0.0, 0.0, -1.0, 1.0, 1.0],
        4 => [1.0, 0.0, 0.0, -1.0, 0.0, 1.0],
        5 => [0.0, -1.0, -1.0, 0.0, 1.0, 1.0],
        6 => [0.0, -1.0, 1.0, 0.0, 0.0, 1.0],
        7 => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
        8 => [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

fn num(v: f32) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// A PDF with one page per JPEG, in order. `progress` as in `pdf::assemble`.
pub fn jpegs_to_pdf(
    pictures: &[(Jpeg, Vec<u8>)],
    layout: Layout,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<Document, super::pdf::PdfError> {
    use super::pdf::PdfError;
    if pictures.is_empty() {
        return Err(PdfError::Empty);
    }
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for (i, (jpeg, bytes)) in pictures.iter().enumerate() {
        if !progress(i, pictures.len()) {
            return Err(PdfError::Cancelled);
        }
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Image".to_vec()));
        dict.set("Width", jpeg.width as i64);
        dict.set("Height", jpeg.height as i64);
        dict.set("BitsPerComponent", 8);
        dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        let space: &[u8] = match jpeg.components {
            1 => b"DeviceGray",
            4 => b"DeviceCMYK",
            _ => b"DeviceRGB",
        };
        dict.set("ColorSpace", Object::Name(space.to_vec()));
        if jpeg.components == 4 && jpeg.adobe_inverted {
            dict.set("Decode", [1, 0, 1, 0, 1, 0, 1, 0].iter().map(|&v| Object::Integer(v)).collect::<Vec<_>>());
        }
        // Already compressed; deflating JPEG bytes again only costs time.
        let image = doc.add_object(Stream::new(dict, bytes.clone()).with_compression(false));

        let (page, (x, y, w, h)) = place(jpeg.shown_size(), layout);
        let m = orientation_matrix(jpeg.orientation);
        let drawing = format!(
            "q {} 0 0 {} {} {} cm {} cm /Im0 Do Q",
            num(w),
            num(h),
            num(x),
            num(y),
            m.iter().map(|&v| num(v)).collect::<Vec<_>>().join(" ")
        );
        let contents = doc.add_object(Stream::new(Dictionary::new(), drawing.into_bytes()));
        let mut xobjects = Dictionary::new();
        xobjects.set("Im0", Object::Reference(image));
        let mut resources = Dictionary::new();
        resources.set("XObject", xobjects);
        let mut page_dict = Dictionary::new();
        page_dict.set("Type", Object::Name(b"Page".to_vec()));
        page_dict.set("Parent", Object::Reference(pages_id));
        page_dict.set("MediaBox", vec![0.into(), 0.into(), Object::Real(page.0), Object::Real(page.1)]);
        page_dict.set("Resources", resources);
        page_dict.set("Contents", Object::Reference(contents));
        kids.push(Object::Reference(doc.add_object(page_dict)));
    }
    progress(pictures.len(), pictures.len());

    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Count", kids.len() as i64);
    pages.set("Kids", kids);
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest JPEG header that says what we need: an optional EXIF
    /// block with an orientation, an optional Adobe block, and a frame.
    fn header(width: u16, height: u16, components: u8, orientation: Option<u16>, little: bool, adobe: bool) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8];
        if let Some(o) = orientation {
            let mut tiff: Vec<u8> = Vec::new();
            let w16 = |v: u16| if little { v.to_le_bytes() } else { v.to_be_bytes() };
            let w32 = |v: u32| if little { v.to_le_bytes() } else { v.to_be_bytes() };
            tiff.extend(if little { b"II" } else { b"MM" });
            tiff.extend(w16(42));
            tiff.extend(w32(8));
            tiff.extend(w16(1)); // one entry
            tiff.extend(w16(0x0112));
            tiff.extend(w16(3)); // SHORT
            tiff.extend(w32(1));
            tiff.extend(w16(o));
            tiff.extend([0, 0]);
            tiff.extend(w32(0));
            let mut body = b"Exif\0\0".to_vec();
            body.extend(tiff);
            b.extend([0xFF, 0xE1]);
            b.extend(((body.len() + 2) as u16).to_be_bytes());
            b.extend(body);
        }
        if adobe {
            let body = b"Adobe\0\x64\0\0\0\0\0";
            b.extend([0xFF, 0xEE]);
            b.extend(((body.len() + 2) as u16).to_be_bytes());
            b.extend(body);
        }
        let mut frame = vec![8];
        frame.extend(height.to_be_bytes());
        frame.extend(width.to_be_bytes());
        frame.push(components);
        for c in 0..components {
            frame.extend([c + 1, 0x11, 0]);
        }
        b.extend([0xFF, 0xC0]);
        b.extend(((frame.len() + 2) as u16).to_be_bytes());
        b.extend(frame);
        b.extend([0xFF, 0xDA, 0, 2, 0xFF, 0xD9]);
        b
    }

    #[test]
    fn reads_the_size_and_colours() {
        let j = read_jpeg(&header(640, 480, 3, None, false, false)).unwrap();
        assert_eq!((j.width, j.height, j.components, j.orientation), (640, 480, 3, 1));
    }

    #[test]
    fn reads_the_orientation_either_byte_order() {
        for little in [true, false] {
            let j = read_jpeg(&header(4000, 3000, 3, Some(6), little, false)).unwrap();
            assert_eq!(j.orientation, 6);
            assert_eq!(j.shown_size(), (3000, 4000), "held upright, it is tall");
        }
    }

    #[test]
    fn notices_adobe_cmyk() {
        let j = read_jpeg(&header(10, 10, 4, None, false, true)).unwrap();
        assert!(j.adobe_inverted);
        assert_eq!(j.components, 4);
    }

    #[test]
    fn refuses_what_is_not_a_jpeg() {
        assert_eq!(read_jpeg(b"\x89PNG\r\n\x1a\n"), Err(ImageError::NotJpeg));
        assert_eq!(read_jpeg(&[0xFF, 0xD8]), Err(ImageError::NotJpeg));
        assert_eq!(read_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xFF]), Err(ImageError::NotJpeg));
    }

    #[test]
    fn a_wide_picture_gets_a_landscape_page() {
        let layout = Layout { size: PageSize::A4, orientation: Orientation::Auto, margin: 0.0 };
        let (page, (x, y, w, h)) = place((4000, 3000), layout);
        assert_eq!(page, (841.89, 595.28));
        assert!((h - 595.28).abs() < 0.01 && (w - 793.71).abs() < 0.01);
        assert!(x > 0.0 && y.abs() < 0.01, "centred across, filling top to bottom");
    }

    #[test]
    fn a_margin_is_kept_on_every_side() {
        let layout = Layout { size: PageSize::Letter, orientation: Orientation::Portrait, margin: 36.0 };
        let (_, (x, y, w, h)) = place((1000, 1000), layout);
        assert!(x >= 36.0 - 0.01 && y >= 36.0 - 0.01);
        assert!((w - 540.0).abs() < 0.01 && (h - 540.0).abs() < 0.01);
    }

    #[test]
    fn fit_makes_the_page_the_picture() {
        let layout = Layout { size: PageSize::Fit, orientation: Orientation::Auto, margin: 20.0 };
        assert_eq!(place((1920, 1080), layout), ((1440.0, 810.0), (0.0, 0.0, 1440.0, 810.0)));
    }

    /// Every orientation's matrix sends the unit square onto itself, so the
    /// picture fills its box whichever way it was turned.
    #[test]
    fn every_orientation_stays_in_its_box() {
        for o in 1..=8 {
            let [a, b, c, d, e, f] = orientation_matrix(o);
            let mut corners: Vec<(i32, i32)> = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
                .iter()
                .map(|&(u, v): &(f32, f32)| ((a * u + c * v + e) as i32, (b * u + d * v + f) as i32))
                .collect();
            corners.sort();
            assert_eq!(corners, [(0, 0), (0, 1), (1, 0), (1, 1)], "orientation {o}");
        }
    }

    /// Orientation 6 is a phone held upright: the top of the stored picture
    /// must end up on the right.
    #[test]
    fn six_turns_the_top_to_the_right() {
        let [a, b, c, d, e, f] = orientation_matrix(6);
        let (u, v) = (0.0, 1.0); // stored top-left
        assert_eq!((a * u + c * v + e, b * u + d * v + f), (1.0, 1.0)); // shown top-right
    }

    #[test]
    fn makes_one_page_per_picture() {
        let a = header(800, 600, 3, None, false, false);
        let b = header(600, 800, 1, Some(8), true, false);
        let pictures = vec![(read_jpeg(&a).unwrap(), a), (read_jpeg(&b).unwrap(), b)];
        let layout = Layout { size: PageSize::A4, orientation: Orientation::Auto, margin: 18.0 };
        let doc = super::super::pdf::tests::round_trip(jpegs_to_pdf(&pictures, layout, |_, _| true).unwrap());
        assert_eq!(doc.get_pages().len(), 2);
        let first = doc.get_dictionary(doc.get_pages()[&1]).unwrap();
        let size = first.get(b"MediaBox").unwrap().as_array().unwrap()[2].as_float().unwrap();
        assert!((size - 841.89).abs() < 0.01, "a wide picture on a landscape page");
    }
}
