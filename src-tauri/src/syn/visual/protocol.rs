//! `synvisual://` — the origin Syn's visuals are served from.
//!
//! Every answer carries [`CSP`], 404s included: a page that reached this scheme
//! by some path nobody planned still gets no network, no forms and no framing by
//! strangers.
//!
//! The URL differs by platform, as every custom scheme's does: `synvisual://
//! localhost/…` on macOS and Linux, `http://synvisual.localhost/…` on Windows.
//! Both are in the app's `frame-src`.
//!
//! What is here:
//!
//! - `/kit.js`, `/kit.css` — the bridge and the colours every visual is given.
//!   See `assets`.
//! - `/lib/…` — the libraries shipped with the app, desktop builds only.
//! - `/v/<id>` — a visual Syn drew, once they are kept (PR 3 of the plan).
//! - `/spike/…` — Bước 0's pages, debug builds only.

use std::borrow::Cow;
use tauri::http::{header, Method, Request, Response, StatusCode};

use super::assets;

pub const SCHEME: &str = "synvisual";

/// What a visual may do.
///
/// - `connect-src 'none'`: no `fetch`, XHR, WebSocket or beacon — the frame
///   cannot carry anything out, nor reach Tauri's IPC over its own scheme.
/// - `img-src`, `font-src`, `style-src` stay on this origin and `data:`/`blob:`:
///   an image or a CSS `url()` pointed elsewhere is a request too.
/// - This origin is named, not only `'self'`: in a frame sandboxed to an
///   opaque origin, WebKit matched `'self'` to nothing, and `/kit.js` never
///   ran — naming it was the whole fix (Bước 0 on macOS 26, 2026-10-27).
///   Both spellings, as the scheme is spelled on each platform.
/// - No `'unsafe-eval'`: nothing the libraries a visual is given need it.
/// - `frame-ancestors`: only the app's own page may show one — the browsing
///   pane's remote pages may not frame a visual.
/// - `form-action 'none'`, `base-uri 'none'`, `frame-src 'none'` by way of
///   `default-src 'none'`.
pub const CSP: &str = "default-src 'none'; \
script-src 'self' synvisual: http://synvisual.localhost 'unsafe-inline'; \
style-src 'self' synvisual: http://synvisual.localhost 'unsafe-inline'; \
img-src 'self' synvisual: http://synvisual.localhost data: blob:; \
font-src 'self' synvisual: http://synvisual.localhost data:; \
connect-src 'none'; \
form-action 'none'; \
base-uri 'none'; \
frame-ancestors tauri://localhost http://tauri.localhost https://tauri.localhost http://localhost:1420";

/// [`CSP`] as a `<meta>` can carry it: everything but `frame-ancestors`,
/// which a meta tag may not set. The header is the policy; this is for a
/// WebView that drops a custom scheme's headers (the plan's §8, WebKitGTK).
fn meta_csp() -> String {
    CSP.split(';').map(str::trim).filter(|d| !d.starts_with("frame-ancestors")).collect::<Vec<_>>().join("; ")
}

/// The theme a page is drawn in, from `?theme=`. The host can change it
/// afterwards; this is what shows before the kit has heard from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    fn from_query(query: Option<&str>) -> Theme {
        let dark = query.is_some_and(|q| q.split('&').any(|pair| pair == "theme=dark"));
        if dark {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    fn name(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// A fragment of a visual made a page: the kit loaded first, the theme set,
/// the policy repeated in a `<meta>`. What the model wrote goes in `<body>`
/// as written — the frame, not this, is what holds it.
pub fn assemble(fragment: &str, theme: Theme) -> String {
    format!(
        "<!doctype html>\n<html lang=\"vi\" data-theme=\"{theme}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <link rel=\"stylesheet\" href=\"/kit.css\">\n<script src=\"/kit.js\"></script>\n</head>\n<body>\n{fragment}\n</body>\n</html>\n",
        theme = theme.name(),
        csp = meta_csp(),
    )
}

/// What a request to the scheme is answered with, before it becomes HTTP.
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    pub status: StatusCode,
    pub content_type: &'static str,
    pub body: Cow<'static, [u8]>,
    /// Whether the answer never changes for this path, so the WebView may keep it.
    pub immutable: bool,
}

impl Reply {
    fn fixed(text: &'static str, content_type: &'static str) -> Reply {
        Reply { status: StatusCode::OK, content_type, body: Cow::Borrowed(text.as_bytes()), immutable: false }
    }

    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    fn page(html: String) -> Reply {
        Reply { status: StatusCode::OK, content_type: "text/html; charset=utf-8", body: Cow::Owned(html.into_bytes()), immutable: false }
    }

    fn library(library: &'static assets::Library) -> Reply {
        Reply { status: StatusCode::OK, content_type: library.content_type, body: Cow::Borrowed(library.bytes), immutable: true }
    }

    fn refused(status: StatusCode) -> Reply {
        Reply { status, content_type: "text/plain; charset=utf-8", body: Cow::Borrowed(b""), immutable: false }
    }
}

/// The answer for one path. Pure, so every route is tested without a webview.
pub fn answer(method: &Method, path: &str, query: Option<&str>) -> Reply {
    if method != Method::GET {
        return Reply::refused(StatusCode::METHOD_NOT_ALLOWED);
    }
    #[cfg(not(debug_assertions))]
    let _ = query;
    match path {
        "/kit.js" => Reply::fixed(assets::KIT_JS, "text/javascript; charset=utf-8"),
        "/kit.css" => Reply::fixed(assets::KIT_CSS, "text/css; charset=utf-8"),
        #[cfg(debug_assertions)]
        "/spike/attack" => Reply::fixed(super::spike::ATTACK_PAGE, "text/html; charset=utf-8"),
        #[cfg(debug_assertions)]
        "/spike/kit" => Reply::page(assemble(super::spike::KIT_PAGE, Theme::from_query(query))),
        _ => match path.strip_prefix("/lib/").and_then(assets::library) {
            Some(library) => Reply::library(library),
            None => Reply::refused(StatusCode::NOT_FOUND),
        },
    }
}

/// The handler `lib.rs` registers for [`SCHEME`].
pub fn handle(request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let reply = answer(request.method(), request.uri().path(), request.uri().query());
    Response::builder()
        .status(reply.status)
        .header(header::CONTENT_TYPE, reply.content_type)
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::REFERRER_POLICY, "no-referrer")
        .header(header::CACHE_CONTROL, if reply.immutable { "max-age=31536000, immutable" } else { "no-store" })
        .body(reply.body)
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(&b""[..])))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(path: &str) -> Response<Cow<'static, [u8]>> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("synvisual://localhost{path}"))
            .body(Vec::new())
            .unwrap();
        handle(&request)
    }

    fn text(response: &Response<Cow<'static, [u8]>>) -> String {
        String::from_utf8(response.body().to_vec()).expect("utf8")
    }

    #[test]
    fn an_unknown_path_is_a_404_and_still_carries_the_policy() {
        for path in ["/", "/v/../../etc/passwd", "/lib/anything.js", "/lib/../kit.js", "/lib/fonts/../../Cargo.toml", "/spike/other"] {
            let response = get(path);
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], CSP, "{path}");
        }
    }

    #[test]
    fn only_get_is_answered() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("synvisual://localhost/kit.js")
            .body(b"{}".to_vec())
            .unwrap();
        assert_eq!(handle(&request).status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn the_policy_lets_nothing_out() {
        assert!(CSP.contains("default-src 'none'"));
        assert!(CSP.contains("connect-src 'none'"));
        assert!(CSP.contains("form-action 'none'"));
        assert!(CSP.contains("frame-ancestors "));
        assert!(!CSP.contains("unsafe-eval"));
        assert!(!CSP.contains('*'));
        // Only `frame-ancestors` names an origin, and only the app's own. A
        // source a visual could load from or send to is this origin or nothing.
        for directive in CSP.split(';').map(str::trim).filter(|d| !d.starts_with("frame-ancestors")) {
            for source in directive.split_whitespace().skip(1) {
                let own = source.starts_with('\'') || matches!(source, "synvisual:" | "http://synvisual.localhost" | "data:" | "blob:");
                assert!(own, "{directive}: `{source}` is not this origin");
            }
        }
    }

    #[test]
    fn the_kit_is_served_with_the_policy() {
        let js = get("/kit.js");
        assert_eq!(js.status(), StatusCode::OK);
        assert!(js.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/javascript"));
        assert_eq!(js.headers()[header::CONTENT_SECURITY_POLICY], CSP);
        assert!(text(&js).contains("ui/initialize"));
        let css = get("/kit.css");
        assert!(text(&css).contains("[data-theme=\"dark\"]"));
    }

    /// The page around a fragment: the kit before anything the model wrote,
    /// the theme asked for, and the policy again in a meta for a WebView that
    /// drops the header — with no `frame-ancestors`, which a meta may not set.
    #[test]
    fn a_fragment_is_made_a_page_with_the_kit_first() {
        let page = assemble("<p id=\"x\">hi</p>", Theme::Dark);
        assert!(page.contains("data-theme=\"dark\""));
        let kit = page.find("/kit.js").expect("kit");
        assert!(kit < page.find("<p id=\"x\">").expect("fragment"), "the kit loads before what the model wrote");
        assert!(page.contains("http-equiv=\"Content-Security-Policy\""));
        assert!(page.contains("connect-src 'none'"));
        assert!(!page.contains("frame-ancestors"), "a meta policy cannot set it");
    }

    #[test]
    fn the_theme_is_light_unless_dark_is_asked() {
        assert_eq!(Theme::from_query(None), Theme::Light);
        assert_eq!(Theme::from_query(Some("theme=dark")), Theme::Dark);
        assert_eq!(Theme::from_query(Some("x=1&theme=dark")), Theme::Dark);
        assert_eq!(Theme::from_query(Some("theme=darkest")), Theme::Light);
        assert_eq!(Theme::from_query(Some("theme=\"><script>")), Theme::Light, "nothing from the query reaches the page");
    }

    #[cfg(desktop)]
    #[test]
    fn the_libraries_are_served_and_kept() {
        let d3 = get("/lib/d3.v7.min.js");
        assert_eq!(d3.status(), StatusCode::OK);
        assert!(d3.headers()[header::CACHE_CONTROL].to_str().unwrap().contains("immutable"));
        assert!(text(&d3).starts_with("// https://d3js.org v7.9.0"));
        let font = get("/lib/fonts/KaTeX_Main-Regular.woff2");
        assert_eq!(font.headers()[header::CONTENT_TYPE], "font/woff2");
    }

    #[test]
    fn the_spike_pages_are_served_in_a_debug_build() {
        let attack = get("/spike/attack");
        assert_eq!(attack.status(), StatusCode::OK);
        assert_eq!(attack.headers()[header::CONTENT_SECURITY_POLICY], CSP);
        assert!(text(&attack).contains("synvisual-spike/result"));
        let kit = get("/spike/kit?theme=dark");
        assert!(text(&kit).contains("data-theme=\"dark\""));
        assert!(text(&kit).contains("synabit/selftest"));
    }
}
