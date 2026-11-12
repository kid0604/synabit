//! A PDF made smaller by making its pictures smaller.
//!
//! Most of a heavy PDF is its pictures: scans, photos, a phone camera's 12
//! megapixels on an A4 page. So the pictures are what is shrunk: each one is
//! handed to the WebView, which already decodes every format a PDF carries
//! pictures in and encodes JPEG, scaled down to what the page needs and
//! re-encoded; the smaller result goes back in its place. Text, vector
//! drawings and fonts are not touched.
//!
//! The design had this in a room with pikepdf (§3). Done this way it needs no
//! download, runs on every platform, and needs no image codec in Rust.
//!
//! # What is left alone
//!
//! A picture is only taken out if the WebView can read it and the PDF reader
//! will read the JPEG put back the same way: 8 bits a channel, grey or RGB,
//! stored as JPEG (`DCTDecode`) or plain deflate. CMYK, masks drawn as stencils,
//! JPEG 2000, fax and JBIG2 encodings, and pictures with a `Decode` array stay as
//! they are. So does a picture whose replacement is not smaller.

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::images::read_jpeg;
use super::pdf::PdfError;

/// How a picture is stored, as far as the WebView needs to know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stored {
    /// The bytes are a JPEG file.
    Jpeg,
    /// The bytes are raw pixels, three bytes each.
    Rgb,
    /// The bytes are raw pixels, one byte each.
    Gray,
}

/// A picture that can be made smaller.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
    /// The object, as "12 0".
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub stored: Stored,
    /// What it takes in the file now.
    pub bytes: u64,
}

fn id_text(id: ObjectId) -> String {
    format!("{} {}", id.0, id.1)
}

pub fn parse_id(text: &str) -> Option<ObjectId> {
    let (a, b) = text.split_once(' ')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn name_of<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    let object = dict.get(key).ok()?;
    let object = match object {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    match object {
        Object::Name(n) => Some(n.as_slice()),
        // A one-filter array is the same as the filter.
        Object::Array(items) if items.len() == 1 => items[0].as_name().ok(),
        _ => None,
    }
}

/// How many colour components a colour space has, for the ones handled.
fn components(doc: &Document, dict: &Dictionary) -> Option<u8> {
    let space = dict.get(b"ColorSpace").ok()?;
    let space = match space {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    match space {
        Object::Name(n) if n == b"DeviceRGB" || n == b"CalRGB" => Some(3),
        Object::Name(n) if n == b"DeviceGray" || n == b"CalGray" => Some(1),
        // [/ICCBased stream] says its count in /N.
        Object::Array(items) if items.first().and_then(|o| o.as_name().ok()) == Some(b"ICCBased") => {
            let profile = items.get(1)?.as_reference().ok()?;
            let n = doc.get_object(profile).ok()?.as_stream().ok()?.dict.get(b"N").ok()?.as_i64().ok()?;
            match n {
                3 => Some(3),
                1 => Some(1),
                _ => None,
            }
        }
        Object::Array(items) if matches!(items.first().and_then(|o| o.as_name().ok()), Some(b"CalRGB")) => Some(3),
        Object::Array(items) if matches!(items.first().and_then(|o| o.as_name().ok()), Some(b"CalGray")) => Some(1),
        _ => None,
    }
}

/// Whether this picture can go to the WebView and come back as a JPEG, and how.
fn handled(doc: &Document, dict: &Dictionary, content: &[u8]) -> Option<(u32, u32, Stored)> {
    if dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Image") {
        return None;
    }
    if dict.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false) || dict.has(b"Decode") || dict.has(b"Mask") {
        return None;
    }
    if dict.get(b"BitsPerComponent").and_then(Object::as_i64).ok() != Some(8) {
        return None;
    }
    let width = dict.get(b"Width").and_then(Object::as_i64).ok()? as u32;
    let height = dict.get(b"Height").and_then(Object::as_i64).ok()? as u32;
    // Small pictures — icons, rules — gain nothing and risk looking worse.
    if (width as u64) * (height as u64) < 128 * 128 {
        return None;
    }
    let channels = components(doc, dict)?;
    let stored = match name_of(doc, dict, b"Filter") {
        Some(b"DCTDecode") => {
            let jpeg = read_jpeg(content).ok()?;
            if jpeg.components != channels {
                return None;
            }
            Stored::Jpeg
        }
        Some(b"FlateDecode") if !dict.has(b"DecodeParms") => {
            if channels == 3 { Stored::Rgb } else { Stored::Gray }
        }
        _ => return None,
    };
    Some((width, height, stored))
}

/// Pictures that are another picture's soft mask: alpha, which must stay a
/// one-channel, exactly-sized image, so they are never recompressed.
fn soft_masks(doc: &Document) -> std::collections::HashSet<ObjectId> {
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter_map(|s| s.dict.get(b"SMask").and_then(Object::as_reference).ok())
        .collect()
}

/// The pictures in a document that can be made smaller, largest first.
pub fn pictures(doc: &Document) -> Vec<Picture> {
    let masks = soft_masks(doc);
    let mut found: Vec<Picture> = doc
        .objects
        .iter()
        .filter(|(id, _)| !masks.contains(id))
        .filter_map(|(id, object)| {
            let stream = object.as_stream().ok()?;
            let (width, height, stored) = handled(doc, &stream.dict, &stream.content)?;
            Some(Picture { id: id_text(*id), width, height, stored, bytes: stream.content.len() as u64 })
        })
        .collect();
    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    found
}

/// A picture's bytes as the WebView takes them: the JPEG as it is, or the
/// pixels, inflated.
pub fn picture_bytes(doc: &Document, id: ObjectId) -> Result<Vec<u8>, PdfError> {
    let stream = doc
        .get_object(id)
        .and_then(Object::as_stream)
        .map_err(|e| PdfError::Unreadable(e.to_string()))?;
    let (width, height, stored) =
        handled(doc, &stream.dict, &stream.content).ok_or_else(|| PdfError::Unreadable("not a picture this can shrink".into()))?;
    match stored {
        // Without its EXIF: the WebView would turn the picture by the
        // orientation in it, while the PDF already places it upright.
        Stored::Jpeg => Ok(super::metadata::without_exif(&stream.content)),
        Stored::Rgb | Stored::Gray => {
            // `lopdf` will not inflate a picture's stream, by design; flate2 will.
            let mut pixels = Vec::new();
            use std::io::Read;
            // No more than the picture's size: a small stream can inflate
            // to gigabytes.
            let per = if stored == Stored::Rgb { 3u64 } else { 1 };
            flate2::read::ZlibDecoder::new(stream.content.as_slice())
                .take(width as u64 * height as u64 * per)
                .read_to_end(&mut pixels)
                .map_err(|e| PdfError::Unreadable(e.to_string()))?;
            let per = if stored == Stored::Rgb { 3 } else { 1 };
            let want = width as usize * height as usize * per;
            if pixels.len() < want {
                return Err(PdfError::Unreadable("the picture's pixels are cut short".into()));
            }
            Ok(pixels[..want].to_vec())
        }
    }
}

/// Put a smaller JPEG in a picture's place, if it is smaller. Returns whether
/// it was put.
pub fn replace(doc: &mut Document, id: ObjectId, jpeg: Vec<u8>) -> Result<bool, PdfError> {
    let info = read_jpeg(&jpeg).map_err(|e| PdfError::Unreadable(e.to_string()))?;
    // Only a picture this would have offered, and not a soft mask: the id
    // comes from a file name the screen chose.
    let channels = {
        let stream = doc.get_object(id).and_then(Object::as_stream).map_err(|e| PdfError::Unreadable(e.to_string()))?;
        if soft_masks(doc).contains(&id) || handled(doc, &stream.dict, &stream.content).is_none() {
            return Ok(false);
        }
        components(doc, &stream.dict)
    };
    let stream = doc
        .get_object_mut(id)
        .and_then(Object::as_stream_mut)
        .map_err(|e| PdfError::Unreadable(e.to_string()))?;
    if jpeg.len() >= stream.content.len() {
        return Ok(false);
    }
    let dict = &mut stream.dict;
    dict.set("Width", info.width as i64);
    dict.set("Height", info.height as i64);
    dict.set("BitsPerComponent", 8);
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    dict.remove(b"DecodeParms");
    // The colour space stays only if it still has as many channels as the
    // new JPEG (an RGB ICC profile under an RGB JPEG); otherwise it is the
    // device space of the JPEG's channels. A canvas always writes RGB.
    if channels != Some(info.components) {
        let space: &[u8] = if info.components == 1 { b"DeviceGray" } else { b"DeviceRGB" };
        dict.set("ColorSpace", Object::Name(space.to_vec()));
    }
    // A soft mask is its own picture, of any size; it stays as it is.
    stream.set_content(jpeg);
    stream.allows_compression = false;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Stream;

    /// A document with one big deflated RGB picture and one tiny one.
    fn with_pictures() -> (Document, ObjectId, ObjectId) {
        let mut doc = crate::utilities::pdf::tests::sample(1, "a");
        let big = {
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"XObject".to_vec()));
            d.set("Subtype", Object::Name(b"Image".to_vec()));
            d.set("Width", 200);
            d.set("Height", 150);
            d.set("BitsPerComponent", 8);
            d.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
            let mut s = Stream::new(d, vec![7u8; 200 * 150 * 3]);
            s.compress().unwrap();
            doc.add_object(s)
        };
        let tiny = {
            let mut d = Dictionary::new();
            d.set("Subtype", Object::Name(b"Image".to_vec()));
            d.set("Width", 10);
            d.set("Height", 10);
            d.set("BitsPerComponent", 8);
            d.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
            doc.add_object(Stream::new(d, vec![0u8; 100]))
        };
        (doc, big, tiny)
    }

    fn jpeg_of(width: u16, height: u16, filler: usize) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8];
        b.extend(height.to_be_bytes());
        b.extend(width.to_be_bytes());
        b.extend([3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
        b.extend([0xFF, 0xDA, 0, 2]);
        b.extend(std::iter::repeat_n(0x55, filler));
        b.extend([0xFF, 0xD9]);
        b
    }

    #[test]
    fn finds_the_pictures_worth_shrinking() {
        let (doc, big, _) = with_pictures();
        let found = pictures(&doc);
        assert_eq!(found.len(), 1, "the tiny one is left alone");
        assert_eq!(found[0].id, id_text(big));
        assert_eq!(found[0].stored, Stored::Rgb);
        assert_eq!(picture_bytes(&doc, big).unwrap().len(), 200 * 150 * 3);
    }

    #[test]
    fn a_smaller_jpeg_takes_the_place() {
        let (mut doc, big, _) = with_pictures();
        // A real-sized compressed stream of identical bytes is tiny, so make it
        // large first: the point is the swap, not the sizes.
        doc.get_object_mut(big).unwrap().as_stream_mut().unwrap().set_content(vec![1u8; 50_000]);
        assert!(replace(&mut doc, big, jpeg_of(100, 75, 1000)).unwrap());
        let s = doc.get_object(big).unwrap().as_stream().unwrap();
        assert_eq!(s.dict.get(b"Filter").unwrap().as_name().unwrap(), b"DCTDecode");
        assert_eq!(s.dict.get(b"Width").unwrap().as_i64().unwrap(), 100);
        let saved = crate::utilities::pdf::tests::round_trip(doc);
        assert!(saved.get_object(big).is_ok());
    }

    #[test]
    fn a_bigger_jpeg_does_not() {
        let (mut doc, big, _) = with_pictures();
        let before = doc.get_object(big).unwrap().as_stream().unwrap().content.len();
        assert!(!replace(&mut doc, big, jpeg_of(200, 150, before + 10)).unwrap());
    }

    /// A real PDF: each JPEG picture shrunk to 1200px at quality 70 by `sips`,
    /// standing in for the WebView, and put back. `UTILITIES_HEAVY` in,
    /// `UTILITIES_LIGHT` out.
    #[test]
    #[ignore]
    fn real_compress() {
        let input = std::env::var("UTILITIES_HEAVY").unwrap();
        let output = std::env::var("UTILITIES_LIGHT").unwrap();
        let mut doc = crate::utilities::pdf::open(std::path::Path::new(&input)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        for p in pictures(&doc) {
            if p.stored != Stored::Jpeg {
                continue;
            }
            let id = parse_id(&p.id).unwrap();
            let src = dir.path().join("in.jpg");
            let dst = dir.path().join("out.jpg");
            std::fs::write(&src, picture_bytes(&doc, id).unwrap()).unwrap();
            let ok = std::process::Command::new("sips")
                .args(["-Z", "1200", "-s", "formatOptions", "70"])
                .arg(&src)
                .arg("--out")
                .arg(&dst)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok);
            println!("{} {}x{} {} bytes -> replaced {}", p.id, p.width, p.height, p.bytes, replace(&mut doc, id, std::fs::read(&dst).unwrap()).unwrap());
        }
        doc.compress();
        crate::utilities::pdf::save(&mut doc, std::path::Path::new(&output)).unwrap();
        println!("{} -> {} bytes", std::fs::metadata(&input).unwrap().len(), std::fs::metadata(&output).unwrap().len());
    }

    #[test]
    fn a_soft_mask_is_never_offered_or_replaced() {
        let (mut doc, big, _) = with_pictures();
        let mut mask = Dictionary::new();
        mask.set("Subtype", Object::Name(b"Image".to_vec()));
        mask.set("Width", 200);
        mask.set("Height", 150);
        mask.set("BitsPerComponent", 8);
        mask.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
        let mut s = Stream::new(mask, vec![9u8; 200 * 150]);
        s.compress().unwrap();
        let mask = doc.add_object(s);
        doc.get_object_mut(big).unwrap().as_stream_mut().unwrap().dict.set("SMask", Object::Reference(mask));
        assert!(pictures(&doc).iter().all(|p| p.id != id_text(mask)));
        assert!(!replace(&mut doc, mask, jpeg_of(10, 10, 10)).unwrap());
    }

    #[test]
    fn a_grey_picture_under_an_icc_profile_gets_a_matching_space() {
        let (mut doc, big, _) = with_pictures();
        let mut icc = Dictionary::new();
        icc.set("N", 1);
        let icc = doc.add_object(Stream::new(icc, vec![0; 10]));
        {
            let d = &mut doc.get_object_mut(big).unwrap().as_stream_mut().unwrap().dict;
            d.set("ColorSpace", vec![Object::Name(b"ICCBased".to_vec()), Object::Reference(icc)]);
        }
        doc.get_object_mut(big).unwrap().as_stream_mut().unwrap().set_content(vec![1u8; 50_000]);
        // jpeg_of writes three components: an RGB JPEG under a grey profile
        // must not stay.
        assert!(replace(&mut doc, big, jpeg_of(100, 75, 1000)).unwrap());
        let s = doc.get_object(big).unwrap().as_stream().unwrap();
        assert_eq!(s.dict.get(b"ColorSpace").unwrap().as_name().unwrap(), b"DeviceRGB");
    }

    #[test]
    fn ids_read_back() {
        assert_eq!(parse_id("12 0"), Some((12, 0)));
        assert_eq!(parse_id("12"), None);
    }
}
