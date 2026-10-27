//! Visuals Syn draws: HTML and SVG the model writes, run where it can reach
//! nothing.
//!
//! The plan is `docs/syn-visuals-plan-2026-10-26.md`. This is Bước 0 of it: the
//! origin a visual is served from, and the page that tries to break out of it.
//!
//! # Why a scheme of its own
//!
//! An `<iframe srcdoc>` inherits the page's CSP, and the app's is
//! `script-src 'self'` — a visual's own script would never run. Loosening the
//! app's CSP to let it would loosen it for every page of the app. So a visual is
//! served from `synvisual://`, a different origin with a policy of its own, and
//! shown in an iframe sandboxed down to `allow-scripts`.
//!
//! # What holds it in
//!
//! Tauri counts any scheme the app registers as local, so an invoke from
//! `synvisual://` would be judged like one from the app itself, and
//! `app_shell::gate` keys on the webview, which a frame shares with its page.
//! What stands between a visual and the app's commands is:
//!
//! - the invoke key, which Tauri injects into the main frame only;
//! - the sandbox, which gives the frame an opaque origin and no way to reach
//!   its parent but `postMessage`;
//! - this module's CSP, under which the frame can open no connection at all.
//!
//! Bước 0 is the proof that those hold on macOS, Windows and Linux, written
//! down in the plan's appendix A.
//!
//! # What a visual is given
//!
//! The kit (`kit.js`, `kit.css`) and the libraries in `assets`, all served
//! from this origin: a visual has no network to fetch anything else with.

pub mod assets;
pub mod check;
pub mod protocol;
pub mod store;

#[cfg(debug_assertions)]
pub mod spike;
