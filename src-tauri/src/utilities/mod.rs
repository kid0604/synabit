//! Utilities: small tools for files — merge a PDF, split one, make a PDF out of
//! photos — run on this machine, with nothing sent anywhere.
//!
//! Design: `docs/utilities-2026-11-11.md`. This module is the engine and has
//! no Tauri in it; `commands::utilities` is the way in from the screen.
//!
//! - [`pdf`] puts pages from PDFs into new ones: merge, split, organize and
//!   rotate are all one function; [`catalog`] carries bookmarks, page labels,
//!   forms and layers along.
//! - [`images`] makes a PDF from JPEGs without decoding them.
//! - [`compress`] shrinks a PDF's pictures, with the WebView doing the JPEG.
//! - `room` (macOS) runs LibreOffice and pdf2docx in a sealed room.
//! - [`stamp`] draws on pages: a watermark, page numbers.
//! - [`frame`] crops pages, or puts them on paper of another size.
//! - [`form`] fills in a PDF form, or flattens it.
//! - [`crypt`] opens password-protected PDFs and protects them.
//! - [`metadata`] takes EXIF and the like out of pictures without re-encoding.
//! - [`searchable`] lays recognised text over a scan, invisible.
//! - [`redact`] blacks out parts of pages and takes what was under them out.
//! - `vision` (macOS) finds a photo's subject and the text in a scan.
//! - [`workspace`] is where inputs and results wait, and how they are named.

pub mod catalog;
pub mod compress;
pub mod crypt;
pub mod form;
pub mod frame;
pub mod images;
pub mod metadata;
pub mod pdf;
pub mod redact;
#[cfg(target_os = "macos")]
pub mod room;
pub mod searchable;
pub mod stamp;
#[cfg(target_os = "macos")]
pub mod vision;
pub mod workspace;
