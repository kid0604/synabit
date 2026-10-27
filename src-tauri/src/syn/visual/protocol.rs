//! `synvisual://` — the origin Syn's visuals are served from.
//!
//! Every answer carries [`CSP`], 404s included: a page that reached this scheme
//! by some path nobody planned still gets no network, no forms and no framing by
//! strangers.
//!
//! The URL differs by platform, as every custom scheme's does: `synvisual://
//! localhost/…` on macOS and Linux, `http://synvisual.localhost/…` on Windows.
//! Both are in the app's `frame-src`.

use std::borrow::Cow;
use tauri::http::{header, Method, Request, Response, StatusCode};

pub const SCHEME: &str = "synvisual";

/// What a visual may do.
///
/// - `connect-src 'none'`: no `fetch`, XHR, WebSocket or beacon — the frame
///   cannot carry anything out, nor reach Tauri's IPC over its own scheme.
/// - `img-src`, `font-src`, `style-src` stay on this origin and `data:`/`blob:`:
///   an image or a CSS `url()` pointed elsewhere is a request too.
/// - No `'unsafe-eval'`: nothing the libraries a visual is given need it.
/// - `frame-ancestors`: only the app's own page may show one — the browsing
///   pane's remote pages may not frame a visual.
/// - `form-action 'none'`, `base-uri 'none'`, `frame-src 'none'` by way of
///   `default-src 'none'`.
pub const CSP: &str = "default-src 'none'; \
script-src 'self' 'unsafe-inline'; \
style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob:; \
font-src 'self' data:; \
connect-src 'none'; \
form-action 'none'; \
base-uri 'none'; \
frame-ancestors tauri://localhost http://tauri.localhost https://tauri.localhost http://localhost:1420";

/// What a request to the scheme is answered with, before it becomes HTTP.
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    pub status: StatusCode,
    pub content_type: &'static str,
    pub body: Cow<'static, [u8]>,
}

impl Reply {
    fn page(html: &'static str) -> Reply {
        Reply { status: StatusCode::OK, content_type: "text/html; charset=utf-8", body: Cow::Borrowed(html.as_bytes()) }
    }

    fn refused(status: StatusCode) -> Reply {
        Reply { status, content_type: "text/plain; charset=utf-8", body: Cow::Borrowed(b"") }
    }
}

/// The answer for one path. Pure, so every route is tested without a webview.
///
/// Only the spike page exists until Bước 0 is signed off; everything else is a
/// 404 rather than a guess at what the path meant.
pub fn answer(method: &Method, path: &str) -> Reply {
    if method != Method::GET {
        return Reply::refused(StatusCode::METHOD_NOT_ALLOWED);
    }
    match path {
        #[cfg(debug_assertions)]
        "/spike/attack" => Reply::page(super::spike::ATTACK_PAGE),
        _ => Reply::refused(StatusCode::NOT_FOUND),
    }
}

/// The handler `lib.rs` registers for [`SCHEME`].
pub fn handle(request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let reply = answer(request.method(), request.uri().path());
    Response::builder()
        .status(reply.status)
        .header(header::CONTENT_TYPE, reply.content_type)
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::REFERRER_POLICY, "no-referrer")
        .header(header::CACHE_CONTROL, "no-store")
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

    #[test]
    fn an_unknown_path_is_a_404_and_still_carries_the_policy() {
        for path in ["/", "/v/../../etc/passwd", "/lib/anything.js", "/spike/other"] {
            let response = get(path);
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], CSP, "{path}");
        }
    }

    #[test]
    fn only_get_is_answered() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("synvisual://localhost/spike/attack")
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
            assert!(!directive.contains("http"), "{directive}");
            assert!(!directive.contains("://"), "{directive}");
        }
    }

    #[test]
    fn the_spike_page_is_served_in_a_debug_build() {
        let response = get("/spike/attack");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], CSP);
        let body = std::str::from_utf8(response.body()).unwrap();
        assert!(body.contains("synvisual-spike/result"));
    }
}
