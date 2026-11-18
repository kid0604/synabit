//! The Utilities app, as Syn reaches it: `run_utility`.
//!
//! # The same work, not a second copy of it
//!
//! A call runs the very commands the Utilities screen runs
//! (`commands::utilities`): the same workspace, the same job with its stop
//! flag, the same sealed room for ffmpeg, LibreOffice and pdf2docx. What is
//! Syn's here is only the edges:
//!
//! - **In** are files of the vault, by node id or path, copied into the
//!   workspace as a file dragged in from Files is. Nothing else: no path on
//!   this Mac outside what Files shows, and the Files lock refuses the call
//!   (`policy`).
//! - **Settings** are a small object per tool, read here into the typed tasks
//!   the commands take, with the screen's defaults. Every argument ffmpeg sees
//!   is still written in Rust from a fixed task; the model picks among choices.
//! - **Out** are new files beside the first input, as "Save beside the
//!   original" puts them, or in `assets/` for a file from a source folder.
//!   Never over anything: a taken name gets " (2)".
//!
//! # What is left to the screen
//!
//! Tools whose settings are something seen — a signature placed, boxes drawn,
//! a page's margins found by drawing it — and the picture tools, which draw in
//! the WebView: Rust cannot run them on its own. Asked for one, the tool says
//! which tool in Utilities to open, so the model can say so.
//!
//! Password protection too: a password typed to Syn would sit in the
//! conversation and at the model's provider. Utilities asks for it on its
//! own screen.
//!
//! # Whose words
//!
//! None come back. A result is a file, and what the model reads is its name
//! and size, so a call neither reads somebody else's words nor is refused
//! after the run has.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::commands::utilities as commands;
use crate::db::Hold;
use crate::error::{AppError, AppResult};
use crate::models::syn::{FunctionDefinition, ToolDefinition};
use crate::syn::tools::ToolContext;
use crate::utilities::workspace::{clean_name, unused_path, Job, Workspace};
use crate::utilities::{audio, frame, images, metadata, pdf, stamp, video};

pub const TOOL: &str = "run_utility";

/// What the call may make at most from one run: a video's frames are each a
/// file, and a folder of five hundred is not an answer.
const MOST_KEPT: usize = 500;

/// How often a running tool looks at the stop button.
const STOP_POLL: Duration = Duration::from_millis(250);

/// Tools that need the screen, and what Utilities calls them.
const ON_SCREEN: &[(&str, &str)] = &[
    ("compress", "Compress PDF"),
    ("watermark", "Watermark"),
    ("sign", "Sign & fill"),
    ("fillform", "Fill form"),
    ("redact", "Redact"),
    ("ocr", "Make searchable"),
    ("pdf2img", "PDF to images"),
    ("pdf2text", "PDF to text"),
    ("protect", "Protect PDF"),
    ("unlock", "Unlock PDF"),
    ("removebg", "Remove background"),
    ("imgtext", "Text from image"),
    ("imgcompress", "Compress image"),
    ("resize", "Resize image"),
    ("crop", "Crop image"),
    ("idphoto", "ID photo"),
    ("imgmark", "Watermark image"),
    ("collage", "Combine images"),
    ("imgblur", "Blur image"),
    ("imgconvert", "Convert image"),
];

pub fn definition() -> ToolDefinition {
    let mac = if cfg!(target_os = "macos") {
        " On this Mac, in a sealed room: office2pdf (Word, Excel, PowerPoint, OpenDocument, CSV to PDF; needs LibreOffice), pdf2word (needs pdf2docx, installed from Utilities). \
         Video, by ffmpeg: vidcompress (height: 480|720|1080|1440, 0 keeps it, default 1080; quality: high|balanced|small; codec: h264|hevc; or target_mb to aim at a size), \
         vidconvert (format: mp4|mov|mkv|webm), vidtrim (start, end in seconds; exact: true cuts to the frame, re-encoding), vidaudio (format: m4a|mp3|wav|flac|opus), \
         vidmerge (2+ files, in order), vidgif (start, end, at most 60 s; width 480; fps 12), vidrotate (turn: right|left|half), vidmute, \
         vidframes (every: seconds, or at: one second; png: true), vidstrip (location, dates, device taken out), vidloud (as audloud; the picture is copied). \
         Audio, by ffmpeg; a video gives its sound: audconvert (format: mp3|m4a|opus|flac|alac|wav; level: high|medium|small), audtrim (start, end, fade_in, fade_out), \
         audmerge (2+ files; crossfade 0-5 s), audloud (mode: podcast = -16 LUFS, music = -14 LUFS, speech evens a voice, gain = louder by `gain` dB, negative quieter), \
         auddenoise (strength: light|medium|strong), audsilence (pause: every longer silence is cut to this many seconds), audspeed (factor 0.5-2, pitch kept), \
         audring (start, end, at most 40 s: an iPhone .m4r ringtone)."
    } else {
        ""
    };
    let screen: Vec<&str> = ON_SCREEN.iter().map(|(id, _)| *id).collect();
    ToolDefinition {
        tool_type: "function".to_string(),
        function: FunctionDefinition {
            name: TOOL.to_string(),
            description: format!(
                "Make new files from files in the vault with a tool of the Utilities app, without writing code: \
                 prefer this to a room command for anything it does. The results are NEW files beside the first file, \
                 never over one, and Files shows them. `files` are node_ids from search_files, or paths in the vault, in order. \
                 Tools and their `options`, every one optional unless said: \
                 merge (2+ PDFs; by_file: true gives each file a bookmark), split (1 PDF; ranges: \"1-3,4-10\" makes one file each, or every: N pages), \
                 organize (1 PDF; pages: \"3,1,2,5-7\" keeps those pages in that order and drops the rest), rotate (degrees: 90|180|270 clockwise; pages: \"2,4\", default all), \
                 numbers (position: bottom-center|bottom-left|bottom-right|top-center|top-left|top-right; format: \"{{n}}\" or \"{{n}} / {{total}}\"; start: 1; skip_first: true leaves a cover unnumbered; size: 10), \
                 pdfcrop (paper: a4|a5|letter|legal, required; orientation: auto|portrait|landscape: every page scaled onto that paper), \
                 img2pdf (JPEG photos only, one page each; size: a4|letter|fit; orientation; margin in points; per_page: 1|2|4), \
                 exif (JPEG and PNG: location, camera and dates taken out, not re-encoded), imgrotate (JPEG: quarters: 1-3 clockwise, not re-encoded).{mac} \
                 `name` names a single result. Not here, because they need the user at the Utilities screen: {}. For one of those, say which tool to open there.",
                screen.join(", ")
            ),
            parameters: json!({
                "type": "object",
                "required": ["tool", "files"],
                "properties": {
                    "tool": { "type": "string", "description": "A tool id from the list, e.g. merge, vidcompress, audloud." },
                    "files": { "type": "array", "items": { "type": "string" }, "description": "node_ids or vault paths, in the order the tool uses them." },
                    "options": { "type": "object", "description": "The tool's settings, as listed. Leave out for its defaults." },
                    "name": { "type": "string", "description": "The result's file name, for a tool that makes one file. Default: the original's name with what was done." }
                }
            }),
        },
    }
}

// ═══════════════════════════════════════════════════════════════
//  WHAT WAS ASKED
// ═══════════════════════════════════════════════════════════════

/// What kind of file a tool takes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Takes {
    Pdf,
    Jpeg,
    JpegPng,
    Office,
    Video,
    /// Sound, or a video's sound.
    Sound,
}

impl Takes {
    fn fits(self, ext: &str) -> bool {
        match self {
            Takes::Pdf => ext == "pdf",
            Takes::Jpeg => matches!(ext, "jpg" | "jpeg"),
            Takes::JpegPng => matches!(ext, "jpg" | "jpeg" | "png"),
            Takes::Office => OFFICE.contains(&ext),
            Takes::Video => video::EXTENSIONS.contains(&ext),
            Takes::Sound => audio::EXTENSIONS.contains(&ext) || video::EXTENSIONS.contains(&ext),
        }
    }

    fn said(self) -> &'static str {
        match self {
            Takes::Pdf => "PDF files",
            Takes::Jpeg => "JPEG photos",
            Takes::JpegPng => "JPEG or PNG pictures",
            Takes::Office => "Word, Excel, PowerPoint or OpenDocument files",
            Takes::Video => "video files",
            Takes::Sound => "sound or video files",
        }
    }
}

/// What LibreOffice is handed, as the screen hands it.
const OFFICE: &[&str] = &["docx", "doc", "odt", "rtf", "xlsx", "xls", "ods", "csv", "pptx", "ppt", "odp"];

/// One run, read and checked, before any file is touched.
#[derive(Debug, Clone)]
enum Work {
    Merge { by_file: bool },
    Split(Vec<Vec<u32>>),
    /// Every `n` pages, counted once the file is open.
    SplitEvery(u32),
    Organize(Vec<u32>),
    Rotate { degrees: i32, pages: Option<Vec<u32>> },
    Numbers(stamp::Numbering),
    Paper(frame::Paper),
    ImagesToPdf(images::Layout),
    Exif,
    TurnPhoto(metadata::Turn),
    OfficeToPdf,
    PdfToWord,
    Video(video::Task),
    VideoMerge,
    Audio { task: audio::AudioTask, keep_picture: bool },
    AudioMerge { crossfade: f64 },
}

/// A tool id, what it takes, how many, and what its result is called.
struct Spec {
    takes: Takes,
    least: usize,
    most: Option<usize>,
    /// Added to the name, as the screen adds it: "Report-gop.pdf".
    suffix: &'static str,
    room: Option<Needs>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Needs {
    Ffmpeg,
    LibreOffice,
    Pdf2docx,
}

fn spec(tool: &str) -> Option<Spec> {
    let s = |takes, least, most, suffix, room| Some(Spec { takes, least, most, suffix, room });
    use Needs::*;
    use Takes::*;
    match tool {
        "merge" => s(Pdf, 2, None, "gop", None),
        "split" => s(Pdf, 1, Some(1), "phan", None),
        "organize" => s(Pdf, 1, Some(1), "sap-xep", None),
        "rotate" => s(Pdf, 1, None, "xoay", None),
        "numbers" => s(Pdf, 1, None, "danh-so", None),
        "pdfcrop" => s(Pdf, 1, None, "kho-giay", None),
        "img2pdf" => s(Jpeg, 1, None, "", None),
        "exif" => s(JpegPng, 1, None, "sach", None),
        "imgrotate" => s(Jpeg, 1, None, "xoay", None),
        "office2pdf" => s(Office, 1, None, "", Some(LibreOffice)),
        "pdf2word" => s(Pdf, 1, None, "", Some(Pdf2docx)),
        "vidcompress" => s(Video, 1, None, "nen", Some(Ffmpeg)),
        "vidconvert" | "vidaudio" | "vidgif" | "vidframes" => s(Video, 1, if tool == "vidconvert" || tool == "vidaudio" { None } else { Some(1) }, "", Some(Ffmpeg)),
        "vidtrim" => s(Video, 1, Some(1), "cat", Some(Ffmpeg)),
        "vidmerge" => s(Video, 2, None, "gop", Some(Ffmpeg)),
        "vidrotate" => s(Video, 1, None, "xoay", Some(Ffmpeg)),
        "vidmute" => s(Video, 1, None, "tat-tieng", Some(Ffmpeg)),
        "vidstrip" => s(Video, 1, None, "sach", Some(Ffmpeg)),
        "vidloud" => s(Video, 1, None, "chuan-am", Some(Ffmpeg)),
        "audconvert" => s(Sound, 1, None, "", Some(Ffmpeg)),
        "audtrim" => s(Sound, 1, Some(1), "cat", Some(Ffmpeg)),
        "audmerge" => s(Sound, 2, None, "gop", Some(Ffmpeg)),
        "audloud" => s(Sound, 1, None, "chuan-am", Some(Ffmpeg)),
        "auddenoise" => s(Sound, 1, None, "loc-on", Some(Ffmpeg)),
        "audsilence" => s(Sound, 1, None, "bo-lang", Some(Ffmpeg)),
        "audspeed" => s(Sound, 1, None, "toc-do", Some(Ffmpeg)),
        "audring" => s(Sound, 1, Some(1), "", Some(Ffmpeg)),
        _ => None,
    }
}

/// Page numbers from "1-3, 5, 9-7": 1-based, in the order written, at most
/// `most` of them. A range written backwards runs backwards.
fn pages(text: &str, most: usize) -> Result<Vec<u32>, String> {
    let mut out = Vec::new();
    for part in text.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let number = |s: &str| s.trim().parse::<u32>().ok().filter(|n| *n >= 1).ok_or_else(|| format!("'{part}' is not a page or a range like 2-5."));
        let (from, to) = match part.split_once('-') {
            Some((a, b)) => (number(a)?, number(b)?),
            None => (number(part)?, number(part)?),
        };
        let run: Vec<u32> = if from <= to { (from..=to).collect() } else { (to..=from).rev().collect() };
        if out.len() + run.len() > most {
            return Err(format!("That is more than {most} pages."));
        }
        out.extend(run);
    }
    if out.is_empty() {
        return Err("No pages were given.".into());
    }
    Ok(out)
}

/// The most pages a list may name: more than any PDF here, fewer than would
/// make a list too big to hold.
const MOST_PAGES: usize = 100_000;

fn num(options: &Value, key: &str) -> Option<f64> {
    options.get(key).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))).filter(|n| n.is_finite())
}

fn text<'a>(options: &'a Value, key: &str) -> Option<&'a str> {
    options.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

fn flag(options: &Value, key: &str) -> Option<bool> {
    options.get(key).and_then(Value::as_bool)
}

/// One of `choices`, by name, or the first when none was given.
fn choice<T: Copy>(options: &Value, key: &str, choices: &[(&str, T)]) -> Result<T, String> {
    match text(options, key) {
        None => Ok(choices[0].1),
        Some(given) => choices.iter().find(|(name, _)| name.eq_ignore_ascii_case(given)).map(|(_, v)| *v).ok_or_else(|| {
            let names: Vec<&str> = choices.iter().map(|(n, _)| *n).collect();
            format!("`{key}` is one of {}, not '{given}'.", names.join(", "))
        }),
    }
}

/// Seconds from 90, "90", "1:30" or "01:02:03".
fn seconds(options: &Value, key: &str) -> Result<Option<f64>, String> {
    let Some(v) = options.get(key).filter(|v| !v.is_null()) else { return Ok(None) };
    if let Some(n) = v.as_f64() {
        return Ok(Some(n));
    }
    let s = v.as_str().map(str::trim).unwrap_or_default();
    let mut total = 0.0;
    for part in s.split(':') {
        let n: f64 = part.trim().parse().map_err(|_| format!("`{key}` is seconds, like 90 or \"1:30\", not '{s}'."))?;
        total = total * 60.0 + n;
    }
    Ok(Some(total).filter(|t: &f64| t.is_finite() && *t >= 0.0))
}

fn needed_seconds(options: &Value, key: &str) -> Result<f64, String> {
    seconds(options, key)?.ok_or_else(|| format!("`{key}` is needed, in seconds."))
}

/// Read a tool's options into the work it does.
fn work(tool: &str, options: &Value) -> Result<Work, String> {
    use Work::*;
    Ok(match tool {
        "merge" => Merge { by_file: flag(options, "by_file").unwrap_or(false) },
        "split" => match (text(options, "ranges"), num(options, "every")) {
            (Some(ranges), _) => {
                let mut runs = Vec::new();
                for range in ranges.split([',', ';']).map(str::trim).filter(|r| !r.is_empty()) {
                    runs.push(pages(range, MOST_PAGES)?);
                }
                if runs.is_empty() {
                    return Err("`ranges` names no pages.".into());
                }
                Split(runs)
            }
            (None, Some(n)) if n >= 1.0 => SplitEvery(n as u32),
            _ => SplitEvery(1),
        },
        "organize" => Organize(pages(text(options, "pages").ok_or("`pages` is needed: the pages to keep, in order, like \"3,1,2,5-7\".")?, MOST_PAGES)?),
        "rotate" => {
            let degrees = num(options, "degrees").unwrap_or(90.0) as i32;
            if ![90, 180, 270, -90].contains(&degrees) {
                return Err("`degrees` is 90, 180 or 270, clockwise.".into());
            }
            let pages = text(options, "pages").map(|p| pages(p, MOST_PAGES)).transpose()?;
            Rotate { degrees: degrees.rem_euclid(360), pages }
        }
        "numbers" => {
            let position = choice(
                options,
                "position",
                &[("bottom-center", 4u8), ("bottom-left", 3), ("bottom-right", 5), ("top-left", 0), ("top-center", 1), ("top-right", 2)],
            )?;
            let format = text(options, "format").unwrap_or("{n}").to_string();
            if !format.contains("{n}") {
                return Err("`format` needs {n} where the number goes, like \"{n}\" or \"Page {n} of {total}\".".into());
            }
            Numbers(stamp::Numbering {
                position,
                format,
                start: num(options, "start").map(|n| n.max(0.0) as u32).unwrap_or(1),
                skip_first: flag(options, "skip_first").unwrap_or(false),
                size: num(options, "size").map(|n| n.clamp(6.0, 48.0) as f32).unwrap_or(10.0),
            })
        }
        "pdfcrop" => {
            let Some(_) = text(options, "paper") else {
                return Err("`paper` is needed: a4, a5, letter or legal. Trimming the margins is done on the Utilities screen (Crop & page size), where the pages are seen.".into());
            };
            let (width, height) = choice(options, "paper", &[("a4", (595.28, 841.89)), ("a5", (419.53, 595.28)), ("letter", (612.0, 792.0)), ("legal", (612.0, 1008.0))])?;
            Paper(frame::Paper { width, height, orientation: orientation(options)? })
        }
        "img2pdf" => {
            let size = choice(options, "size", &[("a4", images::PageSize::A4), ("letter", images::PageSize::Letter), ("fit", images::PageSize::Fit)])?;
            let per_page = num(options, "per_page").unwrap_or(1.0) as u8;
            if ![1, 2, 4].contains(&per_page) {
                return Err("`per_page` is 1, 2 or 4.".into());
            }
            ImagesToPdf(images::Layout { size, orientation: orientation(options)?, margin: num(options, "margin").map(|m| m.clamp(0.0, 144.0) as f32).unwrap_or(0.0), per_page })
        }
        "exif" => Exif,
        "imgrotate" => {
            let quarters = num(options, "quarters").unwrap_or(1.0) as i64;
            TurnPhoto(metadata::Turn { quarters: quarters.rem_euclid(4) as u8, flip_h: false, flip_v: false })
        }
        "office2pdf" => OfficeToPdf,
        "pdf2word" => PdfToWord,
        "vidcompress" => {
            let height = num(options, "height").unwrap_or(1080.0) as u32;
            if ![0, 480, 720, 1080, 1440].contains(&height) {
                return Err("`height` is 480, 720, 1080 or 1440, or 0 to keep the size.".into());
            }
            let target_mb = num(options, "target_mb").filter(|m| *m > 0.0);
            Video(video::Task::Compress {
                height: (height > 0).then_some(height),
                codec: choice(options, "codec", &[("h264", video::Codec::H264), ("hevc", video::Codec::Hevc)])?,
                quality: choice(options, "quality", &[("balanced", video::Quality::Balanced), ("high", video::Quality::High), ("small", video::Quality::Small)])?,
                target_mb,
            })
        }
        "vidconvert" => Video(video::Task::Convert {
            format: choice(options, "format", &[("mp4", video::Container::Mp4), ("mov", video::Container::Mov), ("mkv", video::Container::Mkv), ("webm", video::Container::Webm)])?,
        }),
        "vidtrim" => Video(video::Task::Trim {
            start: seconds(options, "start")?.unwrap_or(0.0),
            end: needed_seconds(options, "end")?,
            exact: flag(options, "exact").unwrap_or(false),
        }),
        "vidaudio" => Video(video::Task::Audio {
            format: choice(
                options,
                "format",
                &[("m4a", video::AudioFormat::M4a), ("mp3", video::AudioFormat::Mp3), ("wav", video::AudioFormat::Wav), ("flac", video::AudioFormat::Flac), ("opus", video::AudioFormat::Opus)],
            )?,
        }),
        "vidmerge" => VideoMerge,
        "vidgif" => Video(video::Task::Gif {
            start: seconds(options, "start")?.unwrap_or(0.0),
            end: needed_seconds(options, "end")?,
            width: num(options, "width").map(|w| w.clamp(120.0, 1280.0) as u32).unwrap_or(480),
            fps: num(options, "fps").map(|f| f.clamp(5.0, 30.0) as u32).unwrap_or(12),
        }),
        "vidrotate" => Video(video::Task::Rotate {
            turn: choice(options, "turn", &[("right", video::Turn::Right), ("left", video::Turn::Left), ("half", video::Turn::Half)])?,
        }),
        "vidmute" => Video(video::Task::Mute),
        "vidstrip" => Video(video::Task::Strip),
        "vidframes" => {
            let at = seconds(options, "at")?;
            let every = match at {
                Some(_) => None,
                None => Some(seconds(options, "every")?.filter(|e| *e > 0.0).unwrap_or(5.0)),
            };
            Video(video::Task::Frames { every, at, png: flag(options, "png").unwrap_or(false) })
        }
        "vidloud" => Audio { task: loudness(options)?, keep_picture: true },
        "audconvert" => Audio {
            task: audio::AudioTask::Convert {
                format: choice(
                    options,
                    "format",
                    &[
                        ("mp3", audio::Format::Mp3),
                        ("m4a", audio::Format::M4a),
                        ("opus", audio::Format::Opus),
                        ("flac", audio::Format::Flac),
                        ("alac", audio::Format::Alac),
                        ("wav", audio::Format::Wav),
                    ],
                )?,
                level: choice(options, "level", &[("medium", audio::Level::Medium), ("high", audio::Level::High), ("small", audio::Level::Small)])?,
            },
            keep_picture: false,
        },
        "audtrim" => Audio {
            task: audio::AudioTask::Trim {
                start: seconds(options, "start")?.unwrap_or(0.0),
                end: needed_seconds(options, "end")?,
                fade_in: seconds(options, "fade_in")?.unwrap_or(0.0).min(10.0),
                fade_out: seconds(options, "fade_out")?.unwrap_or(0.0).min(10.0),
            },
            keep_picture: false,
        },
        "audmerge" => AudioMerge { crossfade: num(options, "crossfade").unwrap_or(0.0).clamp(0.0, 5.0) },
        "audloud" => Audio { task: loudness(options)?, keep_picture: false },
        "auddenoise" => Audio {
            task: audio::AudioTask::Denoise {
                strength: choice(options, "strength", &[("medium", audio::Strength::Medium), ("light", audio::Strength::Light), ("strong", audio::Strength::Strong)])?,
            },
            keep_picture: false,
        },
        "audsilence" => Audio {
            task: audio::AudioTask::Silence { pause: seconds(options, "pause")?.unwrap_or(1.0).clamp(0.2, 10.0) },
            keep_picture: false,
        },
        "audspeed" => {
            let factor = num(options, "factor").unwrap_or(1.5);
            if !(0.5..=2.0).contains(&factor) {
                return Err("`factor` is from 0.5 (half speed) to 2 (twice as fast).".into());
            }
            Audio { task: audio::AudioTask::Speed { factor }, keep_picture: false }
        }
        "audring" => {
            let start = seconds(options, "start")?.unwrap_or(0.0);
            let end = seconds(options, "end")?.unwrap_or(start + 30.0);
            if end - start > audio::LONGEST_RINGTONE {
                return Err(format!("A ringtone is at most {} seconds.", audio::LONGEST_RINGTONE));
            }
            Audio { task: audio::AudioTask::Ringtone { start, end }, keep_picture: false }
        }
        _ => return Err(format!("'{tool}' is not a tool here.")),
    })
}

fn orientation(options: &Value) -> Result<images::Orientation, String> {
    choice(options, "orientation", &[("auto", images::Orientation::Auto), ("portrait", images::Orientation::Portrait), ("landscape", images::Orientation::Landscape)])
}

fn loudness(options: &Value) -> Result<audio::AudioTask, String> {
    let mode = choice(
        options,
        "mode",
        &[("podcast", audio::Loudness::Podcast), ("music", audio::Loudness::Music), ("speech", audio::Loudness::Speech), ("gain", audio::Loudness::Gain)],
    )?;
    let gain = num(options, "gain").unwrap_or(6.0).clamp(-30.0, 30.0);
    Ok(audio::AudioTask::Loudness { mode, gain })
}

// ═══════════════════════════════════════════════════════════════
//  NAMES
// ═══════════════════════════════════════════════════════════════

fn stem(name: &str) -> String {
    Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "file".into())
}

fn ext_of(name: &str) -> String {
    Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// What a result made from `source` is called, before a taken name gets " (2)".
/// A video or a sound's extension is the plan's, which replaces the one here.
fn result_name(tool: &str, suffix: &str, source: &str) -> String {
    let base = if suffix.is_empty() { stem(source) } else { format!("{}-{suffix}", stem(source)) };
    let ext = match tool {
        "office2pdf" | "img2pdf" => "pdf".to_string(),
        "pdf2word" => "docx".to_string(),
        _ => ext_of(source),
    };
    if ext.is_empty() { base } else { format!("{base}.{ext}") }
}

/// The name the model gave, for a result whose kind is fixed: its extension is
/// the result's, whatever was typed.
fn given_name(given: &str, made: &str) -> String {
    let ext = ext_of(made);
    let stem = stem(&clean_name(given));
    if ext.is_empty() { stem } else { format!("{stem}.{ext}") }
}

// ═══════════════════════════════════════════════════════════════
//  RUNNING
// ═══════════════════════════════════════════════════════════════

fn error(said: impl Into<String>) -> String {
    json!({ "error": said.into() }).to_string()
}

/// A file of the vault, where it is on disk, and its name.
struct Source {
    path: PathBuf,
    name: String,
}

/// The inputs in the workspace, by id and name, and each PDF's page count.
type Staged = (Vec<(String, String)>, Vec<u32>);

/// The file `asked` names: a node id or a path inside the vault, and only a
/// file the Files app shows.
fn locate<R: tauri::Runtime>(ctx: &ToolContext<'_, R>, asked: &str) -> Result<Source, String> {
    let asked = asked.trim().trim_start_matches("./");
    if asked.is_empty() {
        return Err("A file was left empty.".into());
    }
    let db = ctx.db.hold();
    let roots = crate::commands::files::allowed_roots_in(&db, ctx.vault_path);
    let by_path = crate::path_utils::resolve_safe_path(ctx.vault_path, asked).ok().filter(|p| p.is_file());
    let path = match by_path {
        Some(path) => path,
        None => {
            let located = db.file_locations_for_node(asked).map_err(|e| e.to_string())?;
            let Some(location) = located.into_iter().next() else {
                return Err(format!("There is no file '{}' in the vault. Find it with search_files and give its node_id.", crate::syn::machine::sanitize::shown(asked)));
            };
            PathBuf::from(location.abs_path)
        }
    };
    drop(db);
    let real = std::fs::canonicalize(&path).map_err(|_| format!("'{}' is not there any more.", crate::syn::machine::sanitize::shown(asked)))?;
    let refs: Vec<&str> = roots.iter().map(String::as_str).collect();
    if crate::path_utils::enforce_within_roots(&real, &refs).is_err() || !real.is_file() {
        return Err(format!("'{}' is not a file Files shows.", crate::syn::machine::sanitize::shown(asked)));
    }
    let name = real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    Ok(Source { path: real, name })
}

/// What the commands' error codes mean, for the model to say.
fn explained(e: &AppError, tool: &str) -> String {
    let said = match e {
        AppError::General(s) | AppError::InvalidPath(s) => s.clone(),
        AppError::UnsupportedCapability(_) => return "This tool needs a sealed room, which only a Mac with macOS 14 or later has.".into(),
        other => other.to_string(),
    };
    let rest = |code: &str| said.strip_prefix(code).map(|r| r.trim_start_matches(':').trim().to_string());
    if said == "cancelled" {
        return "Stopped: the user pressed stop. Nothing was kept.".into();
    }
    if let Some(why) = rest("not-smaller") {
        return format!("Nothing was kept: {why}. Say so; a smaller result needs other settings.");
    }
    if let Some(least) = rest("video-too-small") {
        return format!("That size is too small for this video's length: at least {least} MB would do. Ask, or try that.");
    }
    for (code, means) in [
        ("missing-ffmpeg", "ffmpeg is not installed. The user installs it once: Utilities shows the command on any video tool (Homebrew)."),
        ("missing-libreoffice", "LibreOffice is not installed. The user installs it from libreoffice.org; Utilities' Office to PDF says how."),
        ("missing-pdf2docx", "pdf2docx is not installed. The user installs it once from Utilities' PDF to Word, with one button."),
        ("video-busy", "Another video or sound job is running on this Mac. Try again when it ends."),
        ("video-no-video", "That file has no picture, only sound: use an aud… tool."),
        ("video-no-audio", "That file has no sound."),
        ("video-bad-range", "The start and end are not inside the file, or end is not after start."),
        ("video-too-long", "That part is longer than this tool makes (a GIF 60 s, a ringtone 40 s)."),
        ("video-silent", "The sound is silent throughout, so there is nothing to make louder."),
        ("video-failed", "ffmpeg could not read or make it: the file may be damaged or of a kind it does not know."),
        ("no-room", "The sealed room is not available on this Mac right now."),
        ("locked", "That PDF needs a password to open. The user unlocks it in Utilities (Unlock PDF) first."),
        ("unreadable", "It could not be read as that kind of file."),
    ] {
        if let Some(detail) = rest(code) {
            return if detail.is_empty() { means.to_string() } else { format!("{means} ({detail})") };
        }
    }
    format!("{tool} failed: {said}")
}

/// Run the command for `work` on the staged inputs, under job `job`.
async fn launch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    tool: &str,
    job: &str,
    work: &Work,
    inputs: &[(String, String)],
    names: &[String],
    page_counts: &[u32],
) -> AppResult<Job> {
    use commands::PerFile;
    let app = app.clone();
    let job = job.to_string();
    let ids: Vec<String> = inputs.iter().map(|(id, _)| id.clone()).collect();
    let per: Vec<PerFile> = ids.iter().zip(names).map(|(input, name)| PerFile { input: input.clone(), name: name.clone() }).collect();
    let all = |source| pdf::Pick { source, pages: None, rotate: 0 };
    let output = |name: &str, picks| commands::PdfOutput { name: name.to_string(), picks, outline: pdf::Outline::Keep };
    match work.clone() {
        Work::Merge { by_file } => {
            let picks = (0..ids.len()).map(all).collect();
            let mut out = output(&names[0], picks);
            if by_file {
                out.outline = pdf::Outline::ByFile(inputs.iter().map(|(_, n)| stem(n)).collect());
            }
            commands::utility_assemble(app, job, ids, vec![out]).await
        }
        Work::Split(runs) => {
            let outputs = runs
                .into_iter()
                .enumerate()
                .map(|(i, run)| output(&numbered(&names[0], i + 1), vec![pdf::Pick { source: 0, pages: Some(run), rotate: 0 }]))
                .collect();
            commands::utility_assemble(app, job, ids, outputs).await
        }
        Work::SplitEvery(n) => {
            let count = page_counts.first().copied().unwrap_or(0);
            if count == 0 {
                // Not opened: the run says why when it opens it.
                return commands::utility_assemble(app, job, ids, vec![output(&names[0], vec![all(0)])]).await;
            }
            let outputs = (1..=count)
                .step_by(n.max(1) as usize)
                .enumerate()
                .map(|(i, first)| {
                    let run = (first..=(first + n - 1).min(count)).collect();
                    output(&numbered(&names[0], i + 1), vec![pdf::Pick { source: 0, pages: Some(run), rotate: 0 }])
                })
                .collect();
            commands::utility_assemble(app, job, ids, outputs).await
        }
        Work::Organize(pages) => commands::utility_assemble(app, job, ids, vec![output(&names[0], vec![pdf::Pick { source: 0, pages: Some(pages), rotate: 0 }])]).await,
        Work::Rotate { degrees, pages } => {
            let outputs = ids
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let count = page_counts.get(i).copied().unwrap_or(0);
                    let picks = match &pages {
                        // Not opened: the run says why when it opens it.
                        _ if count == 0 => vec![pdf::Pick { source: i, pages: None, rotate: degrees }],
                        None => vec![pdf::Pick { source: i, pages: None, rotate: degrees }],
                        // Page by page, so those not named keep their turn.
                        Some(turned) => (1..=count)
                            .map(|p| pdf::Pick { source: i, pages: Some(vec![p]), rotate: if turned.contains(&p) { degrees } else { 0 } })
                            .collect(),
                    };
                    output(&names[i], picks)
                })
                .collect();
            commands::utility_assemble(app, job, ids, outputs).await
        }
        Work::Numbers(numbering) => commands::utility_number_pages(app, job, per, numbering).await,
        Work::Paper(paper) => {
            let framed = per.into_iter().map(|f| commands::FramedFile { input: f.input, name: f.name, boxes: Vec::new() }).collect();
            commands::utility_frame(app, job, framed, Some(paper)).await
        }
        Work::ImagesToPdf(layout) => commands::utility_images_to_pdf(app, job, ids, layout, names[0].clone()).await,
        Work::Exif => commands::utility_strip_metadata(app, job, per).await,
        Work::TurnPhoto(turn) => commands::utility_turn_jpegs(app, job, per, turn).await,
        Work::OfficeToPdf => commands::utility_office_to_pdf(app, job, per).await,
        Work::PdfToWord => commands::utility_pdf_to_word(app, job, per).await,
        Work::Video(task) => commands::utility_video(app, job, per, task).await,
        Work::VideoMerge => commands::utility_video_merge(app, job, ids, names[0].clone()).await,
        Work::Audio { task, keep_picture } => commands::utility_audio(app, job, per, task, keep_picture).await,
        Work::AudioMerge { crossfade } => commands::utility_audio_merge(app, job, ids, names[0].clone(), crossfade).await,
    }
    .map_err(|e| {
        log::info!("[syn utility] {tool} ended: {e}");
        e
    })
}

/// "Report-phan.pdf" as "Report-phan-2.pdf".
fn numbered(name: &str, n: usize) -> String {
    let ext = ext_of(name);
    if ext.is_empty() { format!("{}-{n}", stem(name)) } else { format!("{}-{n}.{ext}", stem(name)) }
}

/// The run's answer to the model.
pub async fn call<R: tauri::Runtime>(ctx: &ToolContext<'_, R>, args: &Value, stop: &(dyn Fn() -> bool + Sync)) -> String {
    let tool = args.get("tool").and_then(Value::as_str).map(|t| t.trim().to_lowercase()).unwrap_or_default();
    if let Some((_, name)) = ON_SCREEN.iter().find(|(id, _)| *id == tool) {
        return json!({
            "error": format!("{name} needs the user at the Utilities screen, where what it does is seen and placed."),
            "try": format!("Say so: open Utilities, then {name}, and add the file there (or right-click it in Files, Utilities)."),
        })
        .to_string();
    }
    let Some(spec) = spec(&tool) else {
        return error(format!("'{tool}' is not a tool of run_utility. Its description lists them."));
    };
    match crate::syn::tools::locked_refusal(ctx, TOOL, args) {
        Ok(Some(why)) => return crate::syn::locks::refusal_json(&why),
        Err(e) => return error(e.to_string()),
        Ok(None) => {}
    }
    let options = args.get("options").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let work = match work(&tool, &options) {
        Ok(work) => work,
        Err(said) => return error(said),
    };

    // The files, checked before anything is copied.
    let asked: Vec<String> = match args.get("files") {
        Some(Value::Array(list)) => list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        Some(Value::String(one)) => vec![one.clone()],
        _ => Vec::new(),
    };
    if asked.len() < spec.least {
        return error(format!("{tool} takes {} file{} at least.", spec.least, if spec.least == 1 { "" } else { "s" }));
    }
    if let Some(most) = spec.most.filter(|m| asked.len() > *m) {
        return error(format!("{tool} takes {most} file at a time. Call it once for each."));
    }
    let mut sources = Vec::with_capacity(asked.len());
    for one in &asked {
        let source = match locate(ctx, one) {
            Ok(source) => source,
            Err(said) => return error(said),
        };
        if !spec.takes.fits(&ext_of(&source.name)) {
            return error(format!("{tool} takes {}; '{}' is not one.", spec.takes.said(), crate::syn::machine::sanitize::shown(&source.name)));
        }
        sources.push(source);
    }
    if let Some(needs) = spec.room {
        let have = commands::utility_office_status(ctx.app.clone());
        let missing = match needs {
            _ if !have.room => Some("no-room: this needs macOS 14 or later"),
            Needs::Ffmpeg if !have.ffmpeg => Some("missing-ffmpeg"),
            Needs::LibreOffice if !have.libreoffice => Some("missing-libreoffice"),
            Needs::Pdf2docx if !have.pdf2docx => Some("missing-pdf2docx"),
            _ => None,
        };
        if let Some(code) = missing {
            return error(explained(&AppError::General(code.into()), &tool));
        }
    }

    // Into the workspace, as files dragged in from Files go.
    let root = match workspace_root(ctx.app) {
        Ok(root) => root,
        Err(e) => return error(e),
    };
    let ws = Workspace::new(root.clone());
    let paths: Vec<PathBuf> = sources.iter().map(|s| s.path.clone()).collect();
    let vault = std::fs::canonicalize(ctx.vault_path).unwrap_or_else(|_| PathBuf::from(ctx.vault_path));
    let staged = {
        let (ws, vault) = (Workspace::new(root.clone()), vault.clone());
        tokio::task::spawn_blocking(move || -> AppResult<Staged> {
            let mut inputs = Vec::new();
            let mut counts = Vec::new();
            for path in &paths {
                let input = if path.starts_with(&vault) { ws.add_vault_file(path)? } else { ws.add_file(path)? };
                // Page counts for the PDF tools that pick pages one by one.
                if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                    counts.push(pdf::open(path).map(|d| pdf::page_count(&d)).unwrap_or(0));
                }
                inputs.push((input.id, input.name));
            }
            Ok((inputs, counts))
        })
        .await
    };
    let (inputs, counts) = match staged {
        Ok(Ok(staged)) => staged,
        Ok(Err(e)) => return error(format!("The files could not be copied for the tool: {e}")),
        Err(e) => return error(e.to_string()),
    };
    let forget_inputs = || {
        for (id, _) in &inputs {
            ws.remove_input(id);
        }
    };

    let given = args.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty());
    let names: Vec<String> = sources
        .iter()
        .map(|s| {
            let made = result_name(&tool, spec.suffix, &s.name);
            match given {
                Some(given) if sources.len() == 1 || matches!(work, Work::Merge { .. } | Work::VideoMerge | Work::AudioMerge { .. } | Work::ImagesToPdf(_)) => given_name(given, &made),
                _ => made,
            }
        })
        .collect();

    let job = match commands::utility_new_job(ctx.app.clone(), tool.clone()) {
        Ok(job) => job.id,
        Err(e) => {
            forget_inputs();
            return error(e.to_string());
        }
    };
    // The stop button stops the job, as Cancel on the screen does.
    let running = launch(ctx.app, &tool, &job, &work, &inputs, &names, &counts);
    tokio::pin!(running);
    let finished = loop {
        tokio::select! {
            done = &mut running => break done,
            _ = tokio::time::sleep(STOP_POLL) => {
                if stop() {
                    commands::utility_cancel(job.clone());
                }
            }
        }
    };
    forget_inputs();
    let record = match finished {
        Ok(record) => record,
        Err(e) => return error(explained(&e, &tool)),
    };

    let kept = keep(ctx, &ws, &record, &vault, &tool, &sources[0].name).await;
    let _ = ws.discard(&record.id);
    match kept {
        Ok(made) => json!({
            "made": made,
            "_note": "New files, in the Files app; nothing was overwritten. Name them to the user, with the folder.",
        })
        .to_string(),
        Err(said) => error(said),
    }
}

/// The job's outputs into the vault: beside the first input when it came from
/// the vault, else `assets/`. Frames of a video go into a folder of their own.
async fn keep<R: tauri::Runtime>(ctx: &ToolContext<'_, R>, ws: &Workspace, record: &Job, vault: &Path, tool: &str, first: &str) -> Result<Vec<Value>, String> {
    if record.outputs.is_empty() {
        return Err("The tool made nothing.".into());
    }
    if record.outputs.len() > MOST_KEPT {
        return Err(format!("That made {} files, more than are kept at once. Ask for fewer (a longer `every`).", record.outputs.len()));
    }
    let beside = record
        .origin_dir
        .as_deref()
        .map(|d| std::fs::canonicalize(d).unwrap_or_else(|_| PathBuf::from(d)))
        .filter(|d| d.starts_with(vault) && d.is_dir());
    let mut dir = beside.unwrap_or_else(|| vault.join("assets"));
    if tool == "vidframes" && record.outputs.len() > 1 {
        dir = unused_path(&dir, &clean_name(&format!("{}-khung-hinh", stem(first))));
    }
    let mut made = Vec::new();
    for output in &record.outputs {
        let from = ws.output_file(&record.id, &output.name).map_err(|e| e.to_string())?;
        let to_dir = dir.clone();
        let name = clean_name(&output.name);
        let copied = tokio::task::spawn_blocking(move || -> std::io::Result<PathBuf> {
            std::fs::create_dir_all(&to_dir)?;
            let to = unused_path(&to_dir, &name);
            std::fs::copy(&from, &to)?;
            Ok(to)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("A result could not be put in the vault: {e}"))?;
        let file_id = crate::commands::files::index_written_file(ctx.db, &copied).ok().flatten();
        let _ = ws.mark_saved(&record.id, &copied);
        made.push(json!({
            "path": copied.strip_prefix(vault).unwrap_or(&copied).to_string_lossy(),
            "file_id": file_id,
            "bytes": output.size,
        }));
    }
    Ok(made)
}

/// Where the app keeps Utilities' workspace.
fn workspace_root<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path().app_cache_dir().map(|d| d.join("utilities")).map_err(|e| format!("No cache folder: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same work, as far as its settings show.
    fn same(got: Result<Work, String>, want: Work) {
        assert_eq!(format!("{:?}", got.unwrap()), format!("{want:?}"));
    }

    #[test]
    fn pages_are_read_in_the_order_written() {
        assert_eq!(pages("3,1,2,5-7", 100).unwrap(), vec![3, 1, 2, 5, 6, 7]);
        assert_eq!(pages("4-2", 100).unwrap(), vec![4, 3, 2]);
        assert!(pages("0", 100).is_err());
        assert!(pages("a-b", 100).is_err());
        assert!(pages("1-500", 100).is_err());
    }

    #[test]
    fn options_become_the_screens_tasks_with_its_defaults() {
        same(work("vidcompress", &json!({})),
            Work::Video(video::Task::Compress { height: Some(1080), codec: video::Codec::H264, quality: video::Quality::Balanced, target_mb: None })
        );
        same(work("audloud", &json!({ "mode": "music" })),
            Work::Audio { task: audio::AudioTask::Loudness { mode: audio::Loudness::Music, gain: 6.0 }, keep_picture: false }
        );
        assert!(matches!(work("vidloud", &json!({})).unwrap(), Work::Audio { keep_picture: true, .. }));
        same(work("audtrim", &json!({ "start": "1:30", "end": 120 })),
            Work::Audio { task: audio::AudioTask::Trim { start: 90.0, end: 120.0, fade_in: 0.0, fade_out: 0.0 }, keep_picture: false }
        );
        same(work("split", &json!({ "ranges": "1-2, 5" })), Work::Split(vec![vec![1, 2], vec![5]]));
        same(work("split", &json!({ "every": 3 })), Work::SplitEvery(3));
    }

    #[test]
    fn a_bad_choice_says_what_the_choices_are() {
        let said = work("vidconvert", &json!({ "format": "avi" })).unwrap_err();
        assert!(said.contains("mp4") && said.contains("avi"), "{said}");
        assert!(work("rotate", &json!({ "degrees": 45 })).is_err());
        assert!(work("audring", &json!({ "start": 0, "end": 60 })).is_err());
        assert!(work("pdfcrop", &json!({})).unwrap_err().contains("Utilities screen"));
        assert!(work("vidtrim", &json!({ "start": 3 })).unwrap_err().contains("end"));
    }

    #[test]
    fn results_are_named_as_the_screen_names_them() {
        assert_eq!(result_name("merge", "gop", "Hợp đồng.pdf"), "Hợp đồng-gop.pdf");
        assert_eq!(result_name("office2pdf", "", "Báo cáo.docx"), "Báo cáo.pdf");
        assert_eq!(result_name("pdf2word", "", "a.pdf"), "a.docx");
        assert_eq!(given_name("Bản cuối.docx", "a-gop.pdf"), "Bản cuối.pdf");
        assert_eq!(numbered("a-phan.pdf", 2), "a-phan-2.pdf");
    }

    #[test]
    fn every_tool_on_offer_has_a_spec_and_none_on_screen_does() {
        for tool in [
            "merge", "split", "organize", "rotate", "numbers", "pdfcrop", "img2pdf", "exif", "imgrotate", "office2pdf", "pdf2word", "vidcompress", "vidconvert",
            "vidtrim", "vidaudio", "vidmerge", "vidgif", "vidrotate", "vidmute", "vidframes", "vidstrip", "vidloud", "audconvert", "audtrim", "audmerge",
            "audloud", "auddenoise", "audsilence", "audspeed", "audring",
        ] {
            assert!(spec(tool).is_some(), "{tool}");
            let described = definition().function.description.contains(tool);
            assert!(described || !cfg!(target_os = "macos") && spec(tool).unwrap().room.is_some(), "{tool} is not described");
        }
        for (tool, _) in ON_SCREEN {
            assert!(spec(tool).is_none(), "{tool}");
        }
    }

    #[test]
    fn what_went_wrong_is_said_in_words() {
        assert!(explained(&AppError::General("missing-ffmpeg".into()), "vidcompress").contains("Homebrew"));
        assert!(explained(&AppError::General("video-too-small: 3.2".into()), "vidcompress").contains("3.2 MB"));
        assert!(explained(&AppError::General("locked: “a.pdf” is protected by a password".into()), "merge").contains("Unlock PDF"));
        assert!(explained(&AppError::General("cancelled".into()), "merge").contains("stop"));
    }

    /// A vault with two PDFs in `Docs/`, and the app a run would have.
    fn vault() -> (tempfile::TempDir, String, tauri::App<tauri::test::MockRuntime>) {
        use tauri::Manager;
        let holder = tempfile::tempdir().expect("temp");
        let vault = std::fs::canonicalize(holder.path()).expect("canonical");
        std::fs::create_dir_all(vault.join("Docs")).unwrap();
        for (name, n) in [("a.pdf", 3), ("b.pdf", 2)] {
            let mut doc = crate::utilities::pdf::tests::sample(n, name);
            doc.save(vault.join("Docs").join(name)).unwrap();
        }
        std::fs::write(vault.join("Docs/notes.txt"), "not a pdf").unwrap();
        let app = tauri::test::mock_builder().build(tauri::test::mock_context(tauri::test::noop_assets())).expect("mock app");
        app.handle().manage(crate::db::DbState::new(crate::db::DbBridge::new_in_memory_full().expect("schema")));
        (holder, vault.to_string_lossy().into_owned(), app)
    }

    async fn run(app: &tauri::App<tauri::test::MockRuntime>, vault_path: &str, args: Value) -> Value {
        use tauri::Manager;
        let state = app.handle().state::<crate::db::DbState>();
        let ctx = ToolContext { db: &state, vault_path, app: app.handle(), run_id: None, model: None };
        serde_json::from_str(&call(&ctx, &args, &|| false).await).expect("JSON")
    }

    fn pages_in(path: &Path) -> u32 {
        pdf::page_count(&pdf::open(path).unwrap())
    }

    #[tokio::test]
    async fn pdfs_of_the_vault_become_new_files_beside_them() {
        let (_holder, vault_path, app) = vault();
        let vault = PathBuf::from(&vault_path);

        let merged = run(&app, &vault_path, json!({ "tool": "merge", "files": ["Docs/a.pdf", "Docs/b.pdf"] })).await;
        assert_eq!(merged["made"][0]["path"], "Docs/a-gop.pdf", "{merged}");
        assert_eq!(pages_in(&vault.join("Docs/a-gop.pdf")), 5);

        // Again: a new name, never over the first.
        let again = run(&app, &vault_path, json!({ "tool": "merge", "files": ["Docs/a.pdf", "Docs/b.pdf"] })).await;
        assert_eq!(again["made"][0]["path"], "Docs/a-gop (2).pdf", "{again}");

        let kept = run(&app, &vault_path, json!({ "tool": "organize", "files": ["Docs/a.pdf"], "options": { "pages": "3,1" }, "name": "Chọn" })).await;
        assert_eq!(kept["made"][0]["path"], "Docs/Chọn.pdf", "{kept}");
        assert_eq!(pages_in(&vault.join("Docs/Chọn.pdf")), 2);

        let split = run(&app, &vault_path, json!({ "tool": "split", "files": ["Docs/a.pdf"], "options": { "every": 2 } })).await;
        let made: Vec<&str> = split["made"].as_array().unwrap().iter().map(|m| m["path"].as_str().unwrap()).collect();
        assert_eq!(made, ["Docs/a-phan-1.pdf", "Docs/a-phan-2.pdf"], "{split}");
        assert_eq!(pages_in(&vault.join("Docs/a-phan-2.pdf")), 1);

        let turned = run(&app, &vault_path, json!({ "tool": "rotate", "files": ["Docs/b.pdf"], "options": { "degrees": 90, "pages": "2" } })).await;
        assert_eq!(turned["made"][0]["path"], "Docs/b-xoay.pdf", "{turned}");

        // The originals are as they were.
        assert_eq!(pages_in(&vault.join("Docs/a.pdf")), 3);
    }

    #[tokio::test]
    async fn what_it_cannot_do_is_refused_before_anything_is_copied() {
        let (_holder, vault_path, app) = vault();
        let said = |v: Value| v["error"].as_str().unwrap_or_default().to_string();

        let outside = run(&app, &vault_path, json!({ "tool": "merge", "files": ["../a.pdf", "Docs/b.pdf"] })).await;
        assert!(said(outside.clone()).contains("no file"), "{outside}");
        let wrong = run(&app, &vault_path, json!({ "tool": "merge", "files": ["Docs/a.pdf", "Docs/notes.txt"] })).await;
        assert!(said(wrong.clone()).contains("PDF files"), "{wrong}");
        let one = run(&app, &vault_path, json!({ "tool": "merge", "files": ["Docs/a.pdf"] })).await;
        assert!(said(one.clone()).contains("2 files"), "{one}");
        let screen = run(&app, &vault_path, json!({ "tool": "sign", "files": ["Docs/a.pdf"] })).await;
        assert!(said(screen.clone()).contains("Utilities screen"), "{screen}");
        let unknown = run(&app, &vault_path, json!({ "tool": "teleport", "files": ["Docs/a.pdf"] })).await;
        assert!(said(unknown.clone()).contains("not a tool"), "{unknown}");
        let missing = run(&app, &vault_path, json!({ "tool": "merge", "files": ["Docs/zzz.pdf", "Docs/b.pdf"] })).await;
        assert!(said(missing.clone()).contains("search_files"), "{missing}");
    }

    /// A sound file of the vault, through ffmpeg in a real sealed room, the
    /// way a run reaches it. Needs Homebrew's ffmpeg and macOS 14.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore]
    async fn real_room() {
        use tauri::Manager;
        let (_holder, vault_path, app) = vault();
        let vault = PathBuf::from(&vault_path);
        let rooms_dir = tempfile::tempdir().unwrap();
        let rooms = std::sync::Arc::new(crate::syn::machine::rooms::Rooms::at(&rooms_dir.path().join("machine")).unwrap());
        rooms.ready().await.unwrap();
        app.handle().manage(rooms);
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=300:duration=6", "-c:a", "libmp3lame"])
            .arg(vault.join("Docs/memo.mp3"))
            .status()
            .unwrap();
        assert!(made.success());

        let fast = run(&app, &vault_path, json!({ "tool": "audspeed", "files": ["Docs/memo.mp3"], "options": { "factor": 2 } })).await;
        assert_eq!(fast["made"][0]["path"], "Docs/memo-toc-do.mp3", "{fast}");
        let loud = run(&app, &vault_path, json!({ "tool": "audloud", "files": ["Docs/memo.mp3"], "options": { "mode": "podcast" } })).await;
        assert_eq!(loud["made"][0]["path"], "Docs/memo-chuan-am.mp3", "{loud}");
        let ring = run(&app, &vault_path, json!({ "tool": "audring", "files": ["Docs/memo.mp3"], "options": { "start": 0, "end": 5 }, "name": "Chuông" })).await;
        assert_eq!(ring["made"][0]["path"], "Docs/Chuông.m4r", "{ring}");
        for made in ["Docs/memo-toc-do.mp3", "Docs/memo-chuan-am.mp3", "Docs/Chuông.m4r"] {
            assert!(std::fs::metadata(vault.join(made)).unwrap().len() > 1000, "{made}");
        }
    }
}
