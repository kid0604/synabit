//! What a person is shown of a room's words.
//!
//! A room picks its file names, and a name is the one thing that reaches a card
//! unchanged. A right-to-left override turns `exe.gpj` into something that reads
//! as a picture, a newline forges a second line, and forty spaces push the real
//! ending out of view. So nothing is removed from what is shown, because removing
//! is its own way of lying: what cannot be seen is written out instead.
//!
//! This is about invisible characters. A Cyrillic `а` that looks like a Latin one
//! is still shown as itself.

use std::ffi::OsStr;

/// Spaces in a row before they are counted rather than shown.
const SPACE_RUN: usize = 2;
/// What a macOS file name can hold, and more than enough to read.
const NAME_BYTES: usize = 255;

/// Takes up no room, changes the direction of the text around it, or looks like
/// an ordinary space. Combining marks are not here: a Vietnamese name that macOS
/// has decomposed is made of them.
fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            // Soft hyphen, combining grapheme joiner, Arabic letter mark.
            '\u{00AD}' | '\u{034F}' | '\u{061C}'
            // Fillers that draw nothing.
            | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}' | '\u{3164}' | '\u{FFA0}'
            | '\u{180B}'..='\u{180F}'
            // Spaces that are not the space bar's, zero-width ones, line breaks,
            // and the direction marks and overrides.
            | '\u{00A0}' | '\u{1680}'
            | '\u{2000}'..='\u{200F}'
            | '\u{2028}'..='\u{202F}'
            | '\u{205F}'..='\u{206F}'
            | '\u{3000}'
            // Variation selectors, byte order mark, interlinear annotations, tags.
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0000}'..='\u{E0FFF}'
        )
}

/// True when `text` has anything [`shown`] would have to write out. A command is
/// refused for this rather than shown, since it is about to be run.
pub fn has_hidden(text: &str) -> bool {
    text.chars().any(hidden)
}

/// `text` with everything visible and nothing dropped or cut short. A backslash
/// is doubled, so a room cannot type something that reads like an escape.
pub fn shown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut spaces = 0;
    let flush = |out: &mut String, spaces: &mut usize| {
        if *spaces > SPACE_RUN {
            out.push_str(&format!("\\s{{{spaces}}}"));
        } else {
            out.extend(std::iter::repeat_n(' ', *spaces));
        }
        *spaces = 0;
    };
    for c in text.chars() {
        if c == ' ' {
            spaces += 1;
            continue;
        }
        flush(&mut out, &mut spaces);
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if hidden(c) => out.push_str(&format!("\\u{{{:04X}}}", c as u32)),
            c => out.push(c),
        }
    }
    flush(&mut out, &mut spaces);
    out
}

/// A name that is safe to give a file outside the room, or None when there is
/// nothing to save it under. Hidden characters become `_`, and it cannot be a
/// dotfile or read as an option. Too long is refused: cutting it could cut off
/// the extension, and the extension is what decides how it opens.
pub fn file_name(raw: &OsStr) -> Option<String> {
    let text = raw.to_str()?;
    if text.contains('/') {
        return None;
    }
    let visible: String = text
        .chars()
        .map(|c| if hidden(c) { '_' } else { c })
        .collect();
    let name = visible
        .trim_start_matches(['.', ' ', '-'])
        .trim_end_matches(['.', ' ']);
    if name.is_empty() || name.len() > NAME_BYTES {
        return None;
    }
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn nothing_invisible_is_left_and_nothing_is_dropped() {
        let forged = "report\u{202E}fdp.exe\nsecond line\u{200B}\u{FEFF}\t\u{E0041}";
        assert_eq!(
            shown(forged),
            "report\\u{202E}fdp.exe\\nsecond line\\u{200B}\\u{FEFF}\\t\\u{E0041}"
        );
        assert!(has_hidden(forged));
    }

    #[test]
    fn a_long_run_of_spaces_cannot_push_the_ending_out_of_view() {
        let padded = format!("photo.jpg{}.command", " ".repeat(40));
        assert_eq!(shown(&padded), "photo.jpg\\s{40}.command");
        assert_eq!(shown("a  b"), "a  b");
    }

    #[test]
    fn a_room_cannot_type_what_reads_as_an_escape() {
        assert_eq!(shown("a\\nb"), "a\\\\nb");
        assert_ne!(shown("a\\nb"), shown("a\nb"));
    }

    /// macOS hands back decomposed names, and this app is used in Vietnamese.
    #[test]
    fn vietnamese_stays_as_it_is_either_way_it_is_composed() {
        for name in ["Báo cáo tháng 11.pdf", "Ba\u{0301}o ca\u{0301}o tha\u{0301}ng 11.pdf"] {
            assert!(!has_hidden(name), "{name}");
            assert_eq!(shown(name), name);
            assert_eq!(file_name(OsStr::new(name)).as_deref(), Some(name));
        }
    }

    #[test]
    fn a_saved_name_is_visible_and_not_special() {
        let saved = |raw: &str| file_name(OsStr::new(raw));
        assert_eq!(saved("a\u{202E}gpj.exe\n").as_deref(), Some("a_gpj.exe_"));
        assert_eq!(saved(".zshrc").as_deref(), Some("zshrc"));
        assert_eq!(saved("--force").as_deref(), Some("force"));
        assert_eq!(saved("out.txt. . ").as_deref(), Some("out.txt"));
        for nothing in ["", ".", "..", " . ", "a/b"] {
            assert_eq!(saved(nothing), None, "{nothing:?}");
        }
    }

    #[test]
    fn a_name_that_would_have_to_be_cut_or_guessed_is_refused() {
        let long = format!("{}.command", "a".repeat(255));
        assert_eq!(file_name(OsStr::new(&long)), None);
        assert_eq!(file_name(&OsString::from_vec(vec![b'a', 0xFF])), None);
    }
}
