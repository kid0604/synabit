//! What a visual may load: the kit, and the libraries the app ships with it.
//!
//! Nothing comes from a CDN. Synabit is local-first and a visual has no
//! network at all (`protocol::CSP`), so every library it may use is here,
//! served from `/lib/` on its own origin.
//!
//! The libraries are copies of the app's own dependencies — d3 7.9.0 and
//! KaTeX 0.16.47, KaTeX's woff2 fonts with it — and only in desktop builds,
//! where visuals are drawn: the phone build stays the size it was. Refreshing
//! one means copying the same file from `node_modules` again; a test in
//! `src/shared/syn/__tests__/visualAssets.spec.ts` says when they drift.

/// The bridge to the app. See `kit.js`.
pub const KIT_JS: &str = include_str!("kit.js");

/// The app's colours in both themes, and plain controls. See `kit.css`.
pub const KIT_CSS: &str = include_str!("kit.css");

/// One file under `/lib/`: its name there, its bytes, its type.
pub struct Library {
    pub name: &'static str,
    pub bytes: &'static [u8],
    pub content_type: &'static str,
}

#[allow(unused_macros)]
macro_rules! font {
    ($name:literal) => {
        Library {
            name: concat!("fonts/", $name, ".woff2"),
            bytes: include_bytes!(concat!("lib/fonts/", $name, ".woff2")),
            content_type: "font/woff2",
        }
    };
}

#[cfg(desktop)]
pub const LIBRARIES: &[Library] = &[
    Library { name: "d3.v7.min.js", bytes: include_bytes!("lib/d3.v7.min.js"), content_type: "text/javascript; charset=utf-8" },
    Library { name: "katex.min.js", bytes: include_bytes!("lib/katex.min.js"), content_type: "text/javascript; charset=utf-8" },
    Library { name: "katex.min.css", bytes: include_bytes!("lib/katex.min.css"), content_type: "text/css; charset=utf-8" },
    font!("KaTeX_AMS-Regular"),
    font!("KaTeX_Caligraphic-Bold"),
    font!("KaTeX_Caligraphic-Regular"),
    font!("KaTeX_Fraktur-Bold"),
    font!("KaTeX_Fraktur-Regular"),
    font!("KaTeX_Main-Bold"),
    font!("KaTeX_Main-BoldItalic"),
    font!("KaTeX_Main-Italic"),
    font!("KaTeX_Main-Regular"),
    font!("KaTeX_Math-BoldItalic"),
    font!("KaTeX_Math-Italic"),
    font!("KaTeX_SansSerif-Bold"),
    font!("KaTeX_SansSerif-Italic"),
    font!("KaTeX_SansSerif-Regular"),
    font!("KaTeX_Script-Regular"),
    font!("KaTeX_Size1-Regular"),
    font!("KaTeX_Size2-Regular"),
    font!("KaTeX_Size3-Regular"),
    font!("KaTeX_Size4-Regular"),
    font!("KaTeX_Typewriter-Regular"),
];

/// None on a phone: visuals are a desktop thing (plan §7).
#[cfg(not(desktop))]
pub const LIBRARIES: &[Library] = &[];

/// The library at `/lib/{name}`, if there is one.
pub fn library(name: &str) -> Option<&'static Library> {
    LIBRARIES.iter().find(|l| l.name == name)
}

#[cfg(all(test, desktop))]
mod tests {
    use super::*;

    /// Every font KaTeX's stylesheet asks for in woff2 is served — the one
    /// format WebKit, WebView2 and WebKitGTK all read first.
    #[test]
    fn every_font_katex_asks_for_is_here() {
        let css = std::str::from_utf8(library("katex.min.css").expect("css").bytes).expect("utf8");
        let asked: std::collections::BTreeSet<&str> = css
            .split("url(")
            .skip(1)
            .filter_map(|rest| rest.split(')').next())
            .filter(|url| url.ends_with(".woff2"))
            .collect();
        assert_eq!(asked.len(), 20, "{asked:?}");
        for url in asked {
            assert!(library(url).is_some(), "{url} is not served");
        }
    }
}
