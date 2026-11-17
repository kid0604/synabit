//! What a picture says about where and how it was taken, removed without
//! touching the picture.
//!
//! A phone photo carries its GPS position, the camera, the time to the second,
//! sometimes the owner's name. Redrawing it on a canvas would drop all that,
//! and also re-encode it: a JPEG would lose quality every time. So JPEG and
//! PNG are rewritten here block by block, keeping the image data byte for byte
//! and leaving out the blocks that describe it. Other formats (HEIC, WebP) are
//! redrawn by the WebView, which the screen says.
//!
//! One thing is kept on purpose: which way up a JPEG goes. Dropping EXIF drops
//! the orientation with it, and the photo would come out lying on its side; so
//! a JPEG that had one gets a new, minimal EXIF block holding only that.

/// Why a picture could not be cleaned here.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MetadataError {
    #[error("this kind of picture is cleaned by redrawing it")]
    NotHandled,
    #[error("the picture is damaged")]
    Damaged,
}

/// What was removed, for the screen to report.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    /// EXIF: camera, time, GPS.
    pub exif: bool,
    pub xmp: bool,
    /// IPTC and Photoshop's own block: captions, owner, keywords.
    pub iptc: bool,
    pub comments: bool,
    /// PNG text chunks and the like.
    pub text: bool,
}

/// The picture without its metadata, and a list of what went.
pub fn strip(bytes: &[u8]) -> Result<(Vec<u8>, Removed), MetadataError> {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        strip_jpeg(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        strip_png(bytes)
    } else {
        Err(MetadataError::NotHandled)
    }
}

/// A minimal EXIF block holding one tag, the orientation.
fn orientation_block(orientation: u8) -> Vec<u8> {
    let mut tiff = Vec::new();
    tiff.extend(b"MM\x00\x2a");
    tiff.extend(8u32.to_be_bytes());
    tiff.extend(1u16.to_be_bytes());
    tiff.extend(0x0112u16.to_be_bytes());
    tiff.extend(3u16.to_be_bytes());
    tiff.extend(1u32.to_be_bytes());
    tiff.extend((orientation as u16).to_be_bytes());
    tiff.extend([0, 0]);
    tiff.extend(0u32.to_be_bytes());
    let mut body = b"Exif\0\0".to_vec();
    body.extend(tiff);
    let mut out = vec![0xFF, 0xE1];
    out.extend(((body.len() + 2) as u16).to_be_bytes());
    out.extend(body);
    out
}

/// How a picture is turned: clockwise quarter turns, then mirrored left to
/// right and top to bottom — as the person sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub quarters: u8,
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
}

/// An EXIF orientation as the 2 × 2 matrix that turns the stored picture
/// into the one shown (x right, y down): `[a, b, c, d]` maps (x, y) to
/// (ax + by, cx + dy).
fn orientation_matrix(orientation: u8) -> [i8; 4] {
    match orientation {
        2 => [-1, 0, 0, 1],
        3 => [-1, 0, 0, -1],
        4 => [1, 0, 0, -1],
        5 => [0, 1, 1, 0],
        6 => [0, -1, 1, 0],
        7 => [0, -1, -1, 0],
        8 => [0, 1, -1, 0],
        _ => [1, 0, 0, 1],
    }
}

fn times(a: [i8; 4], b: [i8; 4]) -> [i8; 4] {
    [a[0] * b[0] + a[1] * b[2], a[0] * b[1] + a[1] * b[3], a[2] * b[0] + a[3] * b[2], a[2] * b[1] + a[3] * b[3]]
}

/// The orientation a picture shown as `orientation` has once turned.
pub fn turned_orientation(orientation: u8, turn: Turn) -> u8 {
    let mut m = orientation_matrix(orientation);
    for _ in 0..turn.quarters % 4 {
        m = times(orientation_matrix(6), m);
    }
    if turn.flip_h {
        m = times(orientation_matrix(2), m);
    }
    if turn.flip_v {
        m = times(orientation_matrix(4), m);
    }
    (1..=8).find(|&o| orientation_matrix(o) == m).unwrap_or(1)
}

/// Where the orientation's two bytes are in a JPEG's EXIF, and whether they
/// are little-endian.
fn orientation_slot(bytes: &[u8]) -> Option<(usize, bool)> {
    let mut at = 2;
    while at + 4 <= bytes.len() && bytes[at] == 0xFF {
        let marker = bytes[at + 1];
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        if len < 2 || at + 2 + len > bytes.len() {
            return None;
        }
        let body = &bytes[at + 4..at + 2 + len];
        if marker == 0xE1 && body.starts_with(b"Exif\0\0") {
            let tiff_at = at + 4 + 6;
            let tiff = &bytes[tiff_at..at + 2 + len];
            let little = match tiff.get(0..2)? {
                b"II" => true,
                b"MM" => false,
                _ => return None,
            };
            let u16_at = |i: usize| tiff.get(i..i + 2).map(|b| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) });
            let ifd = tiff.get(4..8).map(|b| {
                let b = [b[0], b[1], b[2], b[3]];
                if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }
            })? as usize;
            let entries = u16_at(ifd)? as usize;
            for i in 0..entries.min(256) {
                let entry = ifd + 2 + i * 12;
                if u16_at(entry)? == 0x0112 && u16_at(entry + 2)? == 3 {
                    return Some((tiff_at + entry + 8, little));
                }
            }
            return None;
        }
        at += 2 + len;
    }
    None
}

/// A JPEG turned without touching its image data: only the orientation it
/// says it is shown at changes, so nothing is lost however often it is
/// turned. Everything else in its EXIF — the date, the camera — stays.
pub fn turn_jpeg(bytes: &[u8], turn: Turn) -> Result<Vec<u8>, MetadataError> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return Err(MetadataError::NotHandled);
    }
    let orientation = turned_orientation(orientation_of(bytes), turn);
    if let Some((slot, little)) = orientation_slot(bytes) {
        let mut out = bytes.to_vec();
        let value = orientation as u16;
        out[slot..slot + 2].copy_from_slice(&if little { value.to_le_bytes() } else { value.to_be_bytes() });
        return Ok(out);
    }
    // No orientation to change: one is added, first, so it is the one read.
    let mut out = vec![0xFF, 0xD8];
    out.extend(orientation_block(orientation));
    out.extend_from_slice(&bytes[2..]);
    Ok(out)
}

/// The EXIF orientation, read straight from the APP1 blocks before the image
/// data, whatever kind of JPEG this is.
fn orientation_of(bytes: &[u8]) -> u8 {
    let mut at = 2;
    while at + 4 <= bytes.len() && bytes[at] == 0xFF {
        let marker = bytes[at + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        if marker == 0xFF || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            at += if marker == 0xFF { 1 } else { 2 };
            continue;
        }
        let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        if len < 2 || at + 2 + len > bytes.len() {
            break;
        }
        let body = &bytes[at + 4..at + 2 + len];
        if marker == 0xE1 && body.starts_with(b"Exif\0\0") {
            if let Some(o) = super::images::exif_orientation(&body[6..]) {
                return o;
            }
        }
        at += 2 + len;
    }
    1
}

/// Where the first image's data ends: the EOI marker after its scans. Bytes
/// in the entropy-coded data are never `FF D9` (a real `FF` is stuffed as
/// `FF 00`), so the first one after a scan starts is the end.
fn end_of_image(bytes: &[u8], from: usize) -> Option<usize> {
    bytes[from..].windows(2).position(|w| w == [0xFF, 0xD9]).map(|i| from + i + 2)
}

/// A JPEG's bytes with its EXIF blocks taken out and nothing else changed:
/// what `compress` hands the WebView, which would otherwise apply the
/// orientation in them a second time.
pub fn without_exif(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    let mut at = 2;
    while at + 4 <= bytes.len() && bytes[at] == 0xFF {
        let marker = bytes[at + 1];
        if marker == 0xDA || marker == 0x01 || (0xD0..=0xD9).contains(&marker) || marker == 0xFF {
            break;
        }
        let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        if len < 2 || at + 2 + len > bytes.len() {
            break;
        }
        let segment = &bytes[at..at + 2 + len];
        if !(marker == 0xE1 && segment[4..].starts_with(b"Exif\0")) {
            out.extend_from_slice(segment);
        }
        at += 2 + len;
    }
    if at >= bytes.len() || !bytes.starts_with(&[0xFF, 0xD8]) {
        return bytes.to_vec();
    }
    out.extend_from_slice(&bytes[at..]);
    out
}

fn strip_jpeg(bytes: &[u8]) -> Result<(Vec<u8>, Removed), MetadataError> {
    let orientation = orientation_of(bytes);
    let mut removed = Removed::default();
    let mut out = vec![0xFF, 0xD8];
    if orientation != 1 {
        out.extend(orientation_block(orientation));
    }
    let mut at = 2;
    loop {
        if at + 4 > bytes.len() || bytes[at] != 0xFF {
            return Err(MetadataError::Damaged);
        }
        let marker = bytes[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        // Start of scan: from here to the end of the image is image data,
        // kept whole. What follows the end is not the picture: a Motion
        // Photo's video, a depth map, a second image with its own EXIF.
        if marker == 0xDA {
            let end = end_of_image(bytes, at).unwrap_or(bytes.len());
            out.extend_from_slice(&bytes[at..end]);
            if end < bytes.len() {
                removed.exif = true;
            }
            return Ok((out, removed));
        }
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            out.extend_from_slice(&bytes[at..at + 2]);
            at += 2;
            continue;
        }
        let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        if len < 2 || at + 2 + len > bytes.len() {
            return Err(MetadataError::Damaged);
        }
        let segment = &bytes[at..at + 2 + len];
        let body = &segment[4..];
        let drop = match marker {
            0xE1 if body.starts_with(b"Exif\0") => {
                removed.exif = true;
                true
            }
            0xE1 if body.starts_with(b"http://ns.adobe.com/xap/") || body.starts_with(b"http://ns.adobe.com/xmp/") => {
                removed.xmp = true;
                true
            }
            0xED => {
                removed.iptc = true;
                true
            }
            // APP2 "MPF": the index of the images after this one, which go.
            0xE2 if body.starts_with(b"MPF\0") => {
                removed.exif = true;
                true
            }
            0xFE => {
                removed.comments = true;
                true
            }
            // Other application blocks — maker notes, MPF thumbnails, FLIR,
            // Samsung's own — carry no picture. JFIF (E0), the colour
            // profile (E2, ICC) and Adobe's colour transform (EE) do: keep them.
            0xE3..=0xEC | 0xEF => {
                removed.exif = true;
                true
            }
            _ => false,
        };
        if !drop {
            out.extend_from_slice(segment);
        }
        at += 2 + len;
    }
}

fn strip_png(bytes: &[u8]) -> Result<(Vec<u8>, Removed), MetadataError> {
    let mut removed = Removed::default();
    let mut out = bytes[..8].to_vec();
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let kind = &bytes[at + 4..at + 8];
        let end = match (at + 12).checked_add(len) {
            Some(end) if end <= bytes.len() => end,
            _ => return Err(MetadataError::Damaged),
        };
        let drop = match kind {
            b"eXIf" => {
                removed.exif = true;
                true
            }
            b"tEXt" | b"zTXt" | b"iTXt" | b"tIME" => {
                removed.text = true;
                true
            }
            _ => false,
        };
        if !drop {
            out.extend_from_slice(&bytes[at..end]);
        }
        at = end;
        if kind == b"IEND" {
            return Ok((out, removed));
        }
    }
    Err(MetadataError::Damaged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(marker: u8, body: &[u8]) -> Vec<u8> {
        let mut s = vec![0xFF, marker];
        s.extend(((body.len() + 2) as u16).to_be_bytes());
        s.extend(body);
        s
    }

    fn frame() -> Vec<u8> {
        segment(0xC0, &[8, 0, 10, 0, 20, 3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0])
    }

    fn jpeg(parts: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8];
        for p in parts {
            b.extend(p);
        }
        b.extend([0xFF, 0xDA, 0, 2, 0xAB, 0xCD, 0xFF, 0xD9]);
        b
    }

    #[test]
    fn a_jpeg_loses_its_exif_and_keeps_its_picture() {
        let exif = segment(0xE1, b"Exif\0\0MM\0\x2a\0\0\0\x08\0\0");
        let icc = segment(0xE2, b"ICC_PROFILE\0data");
        let comment = segment(0xFE, b"taken at home");
        let input = jpeg(&[segment(0xE0, b"JFIF\0"), exif, icc.clone(), comment, frame()]);
        let (out, removed) = strip(&input).unwrap();
        assert!(removed.exif && removed.comments);
        assert!(out.windows(4).any(|w| w == b"JFIF"));
        assert!(out.windows(icc.len()).any(|w| w == icc.as_slice()), "the colour profile stays");
        assert!(!out.windows(4).any(|w| w == b"Exif"));
        assert!(out.ends_with(&[0xFF, 0xDA, 0, 2, 0xAB, 0xCD, 0xFF, 0xD9]), "the image data is untouched");
        assert!(crate::utilities::images::read_jpeg(&out).is_ok());
    }

    #[test]
    fn a_turned_photo_stays_turned() {
        // An EXIF block with orientation 6 and a GPS pointer.
        let mut tiff = b"MM\0\x2a\0\0\0\x08\0\x02".to_vec();
        tiff.extend([0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0]);
        tiff.extend([0x88, 0x25, 0, 4, 0, 0, 0, 1, 0, 0, 0, 0x40]);
        tiff.extend([0, 0, 0, 0]);
        let mut body = b"Exif\0\0".to_vec();
        body.extend(tiff);
        let input = jpeg(&[segment(0xE1, &body), frame()]);
        let (out, _) = strip(&input).unwrap();
        assert_eq!(crate::utilities::images::read_jpeg(&out).unwrap().orientation, 6);
        assert!(!out.windows(2).any(|w| w == [0x88, 0x25]), "the GPS pointer is gone");
    }

    #[test]
    fn what_follows_the_image_goes() {
        let mut input = jpeg(&[frame()]);
        input.extend(b"ftypmp42 a motion photo's video");
        let (out, removed) = strip(&input).unwrap();
        assert!(out.ends_with(&[0xFF, 0xD9]));
        assert!(!out.windows(4).any(|w| w == b"ftyp"));
        assert!(removed.exif);
    }

    #[test]
    fn exif_comes_out_for_the_webview_and_nothing_else_does() {
        let exif = segment(0xE1, b"Exif\0\0MM\0\x2a\0\0\0\x08\0\0");
        let input = jpeg(&[segment(0xE0, b"JFIF\0"), exif, frame()]);
        let out = without_exif(&input);
        assert!(!out.windows(4).any(|w| w == b"Exif"));
        assert!(out.windows(4).any(|w| w == b"JFIF"));
        assert!(out.ends_with(&[0xFF, 0xD9]));
    }

    #[test]
    fn a_png_loses_its_text_chunks() {
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend(kind);
            c.extend(data);
            c.extend([0, 0, 0, 0]);
            c
        };
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(chunk(b"IHDR", &[0; 13]));
        png.extend(chunk(b"tEXt", b"Author\0Me"));
        png.extend(chunk(b"eXIf", b"MM"));
        png.extend(chunk(b"IDAT", b"pixels"));
        png.extend(chunk(b"IEND", b""));
        let (out, removed) = strip(&png).unwrap();
        assert!(removed.text && removed.exif);
        assert!(out.windows(6).any(|w| w == b"pixels"));
        assert!(!out.windows(6).any(|w| w == b"Author"));
    }

    #[test]
    fn other_formats_are_left_to_the_webview() {
        assert_eq!(strip(b"RIFF\0\0\0\0WEBP").err(), Some(MetadataError::NotHandled));
    }

    #[test]
    fn turns_compose_like_the_pictures_do() {
        let right = Turn { quarters: 1, ..Turn::default() };
        // Upright, turned right once, is shown turned right (6); twice, upside down.
        assert_eq!(turned_orientation(1, right), 6);
        assert_eq!(turned_orientation(6, right), 3);
        assert_eq!(turned_orientation(3, Turn { quarters: 2, ..Turn::default() }), 1);
        // Four quarter turns, or two mirrors of one kind, change nothing.
        for o in 1..=8 {
            assert_eq!(turned_orientation(o, Turn { quarters: 4, ..Turn::default() }), o);
            assert_eq!(turned_orientation(turned_orientation(o, Turn { flip_h: true, ..Turn::default() }), Turn { flip_h: true, ..Turn::default() }), o);
        }
        // Mirrored both ways is half a turn.
        assert_eq!(turned_orientation(1, Turn { quarters: 0, flip_h: true, flip_v: true }), 3);
    }

    #[test]
    fn a_jpeg_is_turned_by_its_orientation_alone() {
        // EXIF says 6 (turned right), with a date beside it that must stay.
        let mut tiff = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
        tiff.extend([2, 0]);
        tiff.extend([0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0]);
        tiff.extend([0x32, 0x01, 2, 0, 4, 0, 0, 0, b'2', b'0', b'2', 0]);
        tiff.extend([0, 0, 0, 0]);
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(&tiff);
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpeg.extend(((app1.len() + 2) as u16).to_be_bytes());
        jpeg.extend(&app1);
        jpeg.extend([0xFF, 0xDA, 0, 2, 1, 2, 3, 0xFF, 0xD9]);
        let out = turn_jpeg(&jpeg, Turn { quarters: 1, ..Turn::default() }).unwrap();
        assert_eq!(out.len(), jpeg.len(), "only two bytes change");
        assert_eq!(orientation_of(&out), 3);
        assert!(out.windows(3).any(|w| w == b"202"), "the date is still there");
        assert_eq!(&out[out.len() - 7..], &jpeg[jpeg.len() - 7..], "the image data is untouched");

        // A JPEG with no EXIF gets one, saying only how it is turned.
        let bare = vec![0xFF, 0xD8, 0xFF, 0xDA, 0, 2, 1, 0xFF, 0xD9];
        let out = turn_jpeg(&bare, Turn { quarters: 3, ..Turn::default() }).unwrap();
        assert_eq!(orientation_of(&out), 8);
    }
}
