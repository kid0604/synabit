//! Utilities: small tools for files — merge a PDF, split one, make a PDF out of
//! photos — run on this machine, with nothing sent anywhere.
//!
//! Design: `docs/utilities-2026-11-11.md`. This module is the engine and has
//! no Tauri in it; `commands::utilities` is the way in from the screen.
//!
//! - [`pdf`] puts pages from PDFs into new ones: merge, split, organize and
//!   rotate are all one function.
//! - [`images`] makes a PDF from JPEGs without decoding them.
//! - [`compress`] shrinks a PDF's pictures, with the WebView doing the JPEG.
//! - `room` (macOS) runs LibreOffice and pdf2docx in a sealed room.
//! - [`stamp`] draws on pages: a watermark, page numbers.
//! - [`crypt`] opens password-protected PDFs and protects them.
//! - [`metadata`] takes EXIF and the like out of pictures without re-encoding.
//! - [`searchable`] lays recognised text over a scan, invisible.
//! - `vision` (macOS) finds a photo's subject and the text in a scan.
//! - [`workspace`] is where inputs and results wait, and how they are named.

pub mod compress;
pub mod crypt;
pub mod images;
pub mod metadata;
pub mod pdf;
#[cfg(target_os = "macos")]
pub mod room;
pub mod searchable;
pub mod stamp;
#[cfg(target_os = "macos")]
pub mod vision;
pub mod workspace;
