//! Video tools: ffmpeg in a sealed room.
//!
//! ffmpeg reads more formats than anything else on the Mac, which is also why
//! it is run where a file that broke it would find nothing: a room with no
//! network and nothing of the person's (`syn::machine`), the same one the
//! office tools use. What it reads comes through a door: the one folder the
//! workspace keeps the file in (`workspace` puts each input in its own), read
//! only. What it makes is copied out by descriptor (`inside::copy_out`), never
//! by following a path the room could have changed.
//!
//! No model is involved and nothing the screen sends is run. The screen picks
//! a tool and its settings from fixed choices ([`Task`]); every argument is
//! written here.
//!
//! - **ffmpeg** is not shipped. It is found where Homebrew puts it, which a
//!   closed room may already read and run (`profile::homebrew`).
//! - Work is a job (`rooms::start_job`): it can run for hours, says how far it
//!   got by ffmpeg's `time=`, and is stopped with everything it started.
//!
//! The planning half ([`plan`], [`plan_merge`], [`parse_probe`]) is plain
//! functions, tested without ffmpeg; the running half is macOS only.

use serde::{Deserialize, Serialize};

/// The files the video tools take, by extension.
pub const EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "3gp", "mts", "m2ts", "ts", "wmv", "flv", "mpg", "mpeg"];

/// The most pictures "a frame every N seconds" makes.
pub const MOST_FRAMES: usize = 500;

/// The longest GIF, in seconds: past this one is tens of megabytes.
pub const LONGEST_GIF: f64 = 60.0;

/// Fewest kilobits a second a size target may leave the picture.
const LEAST_VIDEO_KBPS: f64 = 150.0;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoStream {
    pub codec: String,
    /// As stored, before turning.
    pub width: u32,
    pub height: u32,
    /// Degrees counter-clockwise the picture is turned for showing, as ffprobe
    /// and `-display_rotation` both count them: an iPhone held upright is -90.
    pub rotation: i32,
    pub fps: f64,
    pub pix_fmt: String,
    pub bitrate: Option<u64>,
}

impl VideoStream {
    /// Width and height as the picture is seen.
    pub fn shown(&self) -> (u32, u32) {
        if self.rotation.rem_euclid(180) == 90 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    fn ten_bit(&self) -> bool {
        self.pix_fmt.contains("10")
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioStream {
    pub codec: String,
    pub channels: u32,
    pub rate: u32,
    pub bitrate: Option<u64>,
}

/// What a file holds, as ffprobe says. The first video and first audio stream
/// are the ones every tool works on; a cover picture is not a video.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub duration: f64,
    pub bytes: u64,
    /// ffprobe's names for the container, such as `mov,mp4,m4a,3gp,3g2,mj2`.
    pub format: String,
    pub video: Option<VideoStream>,
    pub audio: Option<AudioStream>,
    pub subtitles: Vec<String>,
}

fn number(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// `30000/1001` as 29.97.
fn rate(text: Option<&str>) -> Option<f64> {
    let (top, bottom) = text?.split_once('/')?;
    let (top, bottom): (f64, f64) = (top.parse().ok()?, bottom.parse().ok()?);
    (bottom > 0.0 && top > 0.0).then(|| top / bottom)
}

/// ffprobe's `-show_format -show_streams -of json`.
pub fn parse_probe(json: &str) -> Result<Probe, String> {
    let all: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let format = all.get("format").ok_or("no format")?;
    let streams = all.get("streams").and_then(|s| s.as_array()).cloned().unwrap_or_default();
    let str_of = |v: &serde_json::Value, key: &str| v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let video = streams
        .iter()
        .filter(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("video"))
        .find(|s| s.pointer("/disposition/attached_pic").and_then(|a| a.as_i64()) != Some(1))
        .map(|s| {
            let side = s.get("side_data_list").and_then(|l| l.as_array()).cloned().unwrap_or_default();
            let rotation = side
                .iter()
                .find_map(|d| d.get("rotation").and_then(number))
                // Older files: a `rotate` tag, clockwise.
                .or_else(|| s.pointer("/tags/rotate").and_then(number).map(|r| -r))
                .unwrap_or(0.0);
            VideoStream {
                codec: str_of(s, "codec_name"),
                width: s.get("width").and_then(|w| w.as_u64()).unwrap_or(0) as u32,
                height: s.get("height").and_then(|h| h.as_u64()).unwrap_or(0) as u32,
                rotation: normalised(rotation.round() as i32),
                fps: rate(s.get("avg_frame_rate").and_then(|r| r.as_str()))
                    .or_else(|| rate(s.get("r_frame_rate").and_then(|r| r.as_str())))
                    .unwrap_or(0.0),
                pix_fmt: str_of(s, "pix_fmt"),
                bitrate: s.get("bit_rate").and_then(number).map(|b| b as u64),
            }
        });
    let audio = streams.iter().find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("audio")).map(|s| AudioStream {
        codec: str_of(s, "codec_name"),
        channels: s.get("channels").and_then(|c| c.as_u64()).unwrap_or(0) as u32,
        rate: s.get("sample_rate").and_then(number).unwrap_or(0.0) as u32,
        bitrate: s.get("bit_rate").and_then(number).map(|b| b as u64),
    });
    let subtitles = streams
        .iter()
        .filter(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("subtitle"))
        .map(|s| str_of(s, "codec_name"))
        .collect();
    let duration = format
        .get("duration")
        .and_then(number)
        .or_else(|| streams.iter().find_map(|s| s.get("duration").and_then(number)))
        .unwrap_or(0.0);
    Ok(Probe {
        duration,
        bytes: format.get("size").and_then(number).unwrap_or(0.0) as u64,
        format: str_of(format, "format_name"),
        video,
        audio,
        subtitles,
    })
}

/// Degrees in (-180, 180].
fn normalised(degrees: i32) -> i32 {
    let d = degrees.rem_euclid(360);
    if d > 180 {
        d - 360
    } else {
        d
    }
}

// ── What the screen asks for ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    Hevc,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    High,
    Balanced,
    Small,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mp4,
    Mov,
    Mkv,
    Webm,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    M4a,
    Mp3,
    Wav,
}

/// A quarter turn to the right or left, or half a turn, as the picture is seen.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Turn {
    Right,
    Left,
    Half,
}

/// One tool and its settings. Every choice is one of a few; numbers are
/// checked against the file before anything runs.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "tool", rename_all = "camelCase")]
pub enum Task {
    /// Smaller: a shorter side of at most `height`, at a quality, or aiming
    /// at a size in megabytes (two passes).
    Compress {
        height: Option<u32>,
        codec: Codec,
        quality: Quality,
        #[serde(rename = "targetMb")]
        target_mb: Option<f64>,
    },
    Convert { format: Container },
    /// From `start` to `end` seconds: as stored (`exact: false`, from the key
    /// frame before `start`, in seconds) or encoded again to the frame.
    Trim { start: f64, end: f64, exact: bool },
    Audio { format: AudioFormat },
    /// Without the location, dates, device and the like.
    Strip,
    Rotate { turn: Turn },
    Mute,
    /// One picture at `at` seconds, or one every `every` seconds.
    Frames { every: Option<f64>, at: Option<f64>, png: bool },
    Gif { start: f64, end: f64, width: u32, fps: u32 },
}

/// Why a plan could not be made, as a code the screen has words for.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PlanError {
    #[error("no-video")]
    NoVideo,
    #[error("no-audio")]
    NoAudio,
    #[error("bad-range")]
    BadRange,
    #[error("too-long")]
    TooLong,
    /// The size asked for is too small for this length; the least that would do, in MB.
    #[error("too-small: {0:.1}")]
    TooSmall(f64),
}

/// One run of ffmpeg, and how many seconds of the result it works through:
/// what its `time=` counts up to.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub args: Vec<String>,
    pub seconds: f64,
}

/// What a plan leaves in the room.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Made {
    /// `out.<ext>`.
    One,
    /// `frame-0001.<ext>` and on, up to `MOST_FRAMES`.
    Numbered,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// The result's extension.
    pub ext: String,
    pub made: Made,
    /// Files written into the room before the first step: name and text.
    pub files: Vec<(String, String)>,
}

impl Plan {
    fn one(steps: Vec<Step>, ext: &str) -> Self {
        Self { steps, ext: ext.to_string(), made: Made::One, files: Vec::new() }
    }

    pub fn seconds(&self) -> f64 {
        self.steps.iter().map(|s| s.seconds).sum()
    }
}

/// Every run starts so: quiet but for errors and its progress, twice a second.
const START: &[&str] = &["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-stats", "-stats_period", "0.5"];

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}

fn start() -> Vec<String> {
    args(START)
}

/// An input for ffmpeg: `file:` first, so no part of a name is read as a protocol.
pub fn input_url(path: &str) -> String {
    format!("file:{path}")
}

/// Containers the MP4 family's mover of the index (`faststart`) applies to.
fn mp4_family(ext: &str) -> bool {
    matches!(ext, "mp4" | "mov" | "m4v" | "3gp")
}

fn finish(mut a: Vec<String>, ext: &str) -> Vec<String> {
    if mp4_family(ext) {
        a.extend(args(&["-movflags", "+faststart"]));
    }
    a.push(format!("out.{ext}"));
    a
}

/// The extension a result keeps when it is the input's own container.
fn own_ext(ext: &str) -> String {
    let ext = ext.to_lowercase();
    if EXTENSIONS.contains(&ext.as_str()) {
        ext
    } else {
        "mkv".into()
    }
}

fn even(x: f64) -> u32 {
    ((x / 2.0).round() as u32).max(1) * 2
}

/// Width and height with the shorter side at most `short`, or None when it
/// already is.
fn fit_short(w: u32, h: u32, short: u32) -> Option<(u32, u32)> {
    let least = w.min(h);
    if least == 0 || least <= short {
        return None;
    }
    let s = short as f64 / least as f64;
    Some((even(w as f64 * s), even(h as f64 * s)))
}

fn x264(crf: u32) -> Vec<String> {
    vec!["-c:v".into(), "libx264".into(), "-preset".into(), "medium".into(), "-crf".into(), crf.to_string(), "-pix_fmt".into(), "yuv420p".into()]
}

/// HEVC tagged `hvc1`, which is what QuickTime and iPhones play. `rate` is
/// a `-crf` or a `-b:v`; `params` go to x265 itself.
fn x265(ten_bit: bool, rate: &[String], params: String) -> Vec<String> {
    let mut a = args(&["-c:v", "libx265", "-preset", "medium"]);
    a.extend(rate.iter().cloned());
    a.extend(["-pix_fmt".into(), if ten_bit { "yuv420p10le" } else { "yuv420p" }.into()]);
    a.extend(args(&["-tag:v", "hvc1", "-x265-params"]));
    a.push(params);
    a
}

fn vp9() -> Vec<String> {
    args(&["-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "2"])
}

fn aac(kbps: u32) -> Vec<String> {
    vec!["-c:a".into(), "aac".into(), "-b:a".into(), format!("{kbps}k")]
}

/// Audio as it is when the container takes it, AAC (or Opus for WebM) when not.
fn audio_into(audio: &Option<AudioStream>, container: &str) -> Vec<String> {
    let Some(a) = audio else { return Vec::new() };
    let fits = match container {
        "mp4" | "m4v" | "3gp" => matches!(a.codec.as_str(), "aac" | "mp3" | "alac" | "ac3" | "eac3" | "opus" | "flac"),
        "mov" => matches!(a.codec.as_str(), "aac" | "mp3" | "alac" | "ac3" | "eac3" | "pcm_s16le" | "pcm_s24le" | "pcm_f32le"),
        "webm" => matches!(a.codec.as_str(), "opus" | "vorbis"),
        _ => true,
    };
    if fits {
        args(&["-c:a", "copy"])
    } else if container == "webm" {
        args(&["-c:a", "libopus", "-b:a", "128k"])
    } else {
        aac(192)
    }
}

fn range(start: f64, end: f64, duration: f64) -> Result<(f64, f64), PlanError> {
    let ok = start.is_finite() && end.is_finite() && start >= 0.0 && end - start >= 0.1 && (duration <= 0.0 || end <= duration + 0.5);
    if ok {
        Ok((start, end - start))
    } else {
        Err(PlanError::BadRange)
    }
}

fn secs(x: f64) -> String {
    format!("{x:.3}")
}

/// What to run for one file. `input` is its path where the room reads it;
/// `ext` its extension.
pub fn plan(task: &Task, input: &str, ext: &str, probe: &Probe) -> Result<Plan, PlanError> {
    let url = input_url(input);
    let duration = probe.duration.max(0.0);
    let has_video = probe.video.is_some();
    let video_only = |p: &Probe| p.video.clone().ok_or(PlanError::NoVideo);
    match *task {
        Task::Compress { height, codec, quality, target_mb } => {
            let v = video_only(probe)?;
            let (w, h) = v.shown();
            let audio_kbps = if quality == Quality::Small { 96 } else { 128 };
            // Input, streams and size, then the picture's codec, then the sound.
            let build = |picture: Vec<String>, sound: bool| -> Vec<String> {
                let mut a = start();
                a.extend(["-i".into(), url.clone(), "-map".into(), "0:v:0".into()]);
                if sound {
                    a.extend(args(&["-map", "0:a:0"]));
                }
                if let Some((sw, sh)) = height.and_then(|s| fit_short(w, h, s)) {
                    a.extend(["-vf".into(), format!("scale={sw}:{sh}")]);
                }
                a.extend(picture);
                if sound {
                    a.extend(aac(audio_kbps));
                }
                a
            };
            let has_audio = probe.audio.is_some();
            let Some(mb) = target_mb else {
                let crf = match (codec, quality) {
                    (Codec::H264, Quality::High) => 20,
                    (Codec::H264, Quality::Balanced) => 23,
                    (Codec::H264, Quality::Small) => 28,
                    (Codec::Hevc, Quality::High) => 22,
                    (Codec::Hevc, Quality::Balanced) => 26,
                    (Codec::Hevc, Quality::Small) => 30,
                };
                let picture = match codec {
                    Codec::H264 => x264(crf),
                    Codec::Hevc => x265(v.ten_bit(), &["-crf".into(), crf.to_string()], "log-level=error".into()),
                };
                return Ok(Plan::one(vec![Step { args: finish(build(picture, has_audio), "mp4"), seconds: duration }], "mp4"));
            };
            // A size: so many kilobits a second, the picture getting what the
            // sound and the container's few percent leave. Two passes, the first
            // only measuring, so the size comes out where it was aimed.
            if !(mb.is_finite() && mb > 0.0) || duration <= 0.0 {
                return Err(PlanError::BadRange);
            }
            let sound = if has_audio { audio_kbps as f64 } else { 0.0 };
            let picture = mb * 8000.0 / duration * 0.97 - sound;
            if picture < LEAST_VIDEO_KBPS {
                return Err(PlanError::TooSmall(((LEAST_VIDEO_KBPS + sound) * duration / 8000.0 / 0.97 * 10.0).ceil() / 10.0));
            }
            let rate = vec!["-b:v".to_string(), format!("{}k", picture.floor() as u64)];
            let pass = |n: u32| -> Vec<String> {
                match codec {
                    Codec::H264 => {
                        let mut a = args(&["-c:v", "libx264", "-preset", "medium", "-pix_fmt", "yuv420p"]);
                        a.extend(rate.clone());
                        a.extend(["-pass".into(), n.to_string(), "-passlogfile".into(), "pass".into()]);
                        a
                    }
                    Codec::Hevc => x265(v.ten_bit(), &rate, format!("log-level=error:pass={n}:stats=pass.log")),
                }
            };
            let mut first = build(pass(1), false);
            first.extend(args(&["-an", "-f", "null", "-"]));
            let second = finish(build(pass(2), has_audio), "mp4");
            Ok(Plan::one(vec![Step { args: first, seconds: duration }, Step { args: second, seconds: duration }], "mp4"))
        }
        Task::Convert { format } => {
            let v = video_only(probe)?;
            let out = match format {
                Container::Mp4 => "mp4",
                Container::Mov => "mov",
                Container::Mkv => "mkv",
                Container::Webm => "webm",
            };
            let mut a = start();
            a.extend(["-i".into(), url]);
            if out == "mkv" {
                // Matroska takes nearly anything as it is. Subtitles only of the
                // kinds it holds; QuickTime's text becomes SubRip.
                a.extend(args(&["-map", "0:v:0", "-map", "0:a?", "-c:v", "copy", "-c:a", "copy"]));
                let keep = probe.subtitles.iter().all(|c| matches!(c.as_str(), "subrip" | "ass" | "ssa" | "webvtt" | "mov_text" | "hdmv_pgs_subtitle" | "dvd_subtitle"));
                if !probe.subtitles.is_empty() && keep {
                    a.extend(args(&["-map", "0:s?"]));
                    let text = probe.subtitles.iter().any(|c| c == "mov_text");
                    a.extend(args(&["-c:s", if text { "srt" } else { "copy" }]));
                }
                return Ok(Plan::one(vec![Step { args: finish(a, out), seconds: duration }], out));
            }
            a.extend(args(&["-map", "0:v:0", "-map", "0:a:0?", "-sn"]));
            let copies = match out {
                "webm" => matches!(v.codec.as_str(), "vp8" | "vp9" | "av1"),
                _ => matches!(v.codec.as_str(), "h264" | "hevc") || (out == "mov" && matches!(v.codec.as_str(), "prores" | "mjpeg")),
            };
            if copies {
                a.extend(args(&["-c:v", "copy"]));
                if v.codec == "hevc" {
                    a.extend(args(&["-tag:v", "hvc1"]));
                }
            } else if out == "webm" {
                a.extend(vp9());
            } else {
                a.extend(x264(20));
            }
            a.extend(audio_into(&probe.audio, out));
            Ok(Plan::one(vec![Step { args: finish(a, out), seconds: duration }], out))
        }
        Task::Trim { start: from, end, exact } => {
            let (from, length) = range(from, end, duration)?;
            let mut a = start();
            a.extend(["-ss".into(), secs(from), "-i".into(), url, "-t".into(), secs(length)]);
            if exact && has_video {
                a.extend(args(&["-map", "0:v:0", "-map", "0:a:0?"]));
                a.extend(x264(18));
                if probe.audio.is_some() {
                    a.extend(aac(192));
                }
                return Ok(Plan::one(vec![Step { args: finish(a, "mp4"), seconds: length }], "mp4"));
            }
            let out = own_ext(ext);
            a.extend(args(&["-map", "0:v?", "-map", "0:a?", "-c", "copy", "-avoid_negative_ts", "make_zero"]));
            Ok(Plan::one(vec![Step { args: finish(a, &out), seconds: length }], &out))
        }
        Task::Audio { format } => {
            let audio = probe.audio.as_ref().ok_or(PlanError::NoAudio)?;
            let mut a = start();
            a.extend(["-i".into(), url, "-map".into(), "0:a:0".into(), "-vn".into(), "-sn".into(), "-dn".into()]);
            let out = match format {
                AudioFormat::M4a => {
                    a.extend(if audio.codec == "aac" { args(&["-c:a", "copy"]) } else { aac(192) });
                    "m4a"
                }
                AudioFormat::Mp3 => {
                    a.extend(if audio.codec == "mp3" { args(&["-c:a", "copy"]) } else { args(&["-c:a", "libmp3lame", "-q:a", "2"]) });
                    "mp3"
                }
                AudioFormat::Wav => {
                    a.extend(args(&["-c:a", "pcm_s16le"]));
                    "wav"
                }
            };
            if out == "m4a" {
                a.extend(args(&["-movflags", "+faststart"]));
            }
            a.push(format!("out.{out}"));
            Ok(Plan::one(vec![Step { args: a, seconds: duration }], out))
        }
        Task::Strip => {
            let out = own_ext(ext);
            let mut a = start();
            a.extend(["-i".into(), url]);
            // Picture, sound and subtitles as they are; no other track (an
            // iPhone keeps where it was in one), no chapters, no tags, and no
            // date or encoder name written in their place.
            a.extend(args(&[
                "-map", "0:v?", "-map", "0:a?", "-map", "0:s?", "-c", "copy", "-map_metadata", "-1", "-map_chapters", "-1", "-fflags", "+bitexact",
            ]));
            Ok(Plan::one(vec![Step { args: finish(a, &out), seconds: duration }], &out))
        }
        Task::Rotate { turn } => {
            let v = video_only(probe)?;
            let own = own_ext(ext);
            let clockwise = match turn {
                Turn::Right => 90,
                Turn::Left => -90,
                Turn::Half => 180,
            };
            let mut a = start();
            if mp4_family(&own) {
                // Only the note of how to show it changes: nothing is encoded again.
                a.extend(["-display_rotation:v:0".into(), normalised(v.rotation - clockwise).to_string(), "-i".into(), url]);
                a.extend(args(&["-map", "0:v?", "-map", "0:a?", "-map", "0:s?", "-c", "copy"]));
                return Ok(Plan::one(vec![Step { args: finish(a, &own), seconds: duration }], &own));
            }
            // Elsewhere the picture is turned and encoded again, after ffmpeg has
            // already turned it as it was meant to be seen.
            let out = if own == "webm" || own == "mkv" { own } else { "mp4".to_string() };
            let filter = match turn {
                Turn::Right => "transpose=1",
                Turn::Left => "transpose=2",
                Turn::Half => "hflip,vflip",
            };
            a.extend(["-i".into(), url, "-map".into(), "0:v:0".into(), "-map".into(), "0:a:0?".into(), "-vf".into(), filter.into()]);
            a.extend(if out == "webm" { vp9() } else { x264(18) });
            a.extend(audio_into(&probe.audio, &out));
            Ok(Plan::one(vec![Step { args: finish(a, &out), seconds: duration }], &out))
        }
        Task::Mute => {
            video_only(probe)?;
            let out = own_ext(ext);
            let mut a = start();
            a.extend(["-i".into(), url]);
            a.extend(args(&["-map", "0:v", "-map", "0:s?", "-c", "copy", "-an"]));
            Ok(Plan::one(vec![Step { args: finish(a, &out), seconds: duration }], &out))
        }
        Task::Frames { every, at, png } => {
            video_only(probe)?;
            let ext = if png { "png" } else { "jpg" };
            let quality: Vec<String> = if png { args(&["-c:v", "png"]) } else { args(&["-q:v", "2"]) };
            let mut a = start();
            if let Some(at) = at {
                if !(at.is_finite() && at >= 0.0 && (duration <= 0.0 || at <= duration)) {
                    return Err(PlanError::BadRange);
                }
                a.extend(["-ss".into(), secs(at), "-i".into(), url, "-map".into(), "0:v:0".into(), "-frames:v".into(), "1".into()]);
                a.extend(quality);
                a.extend(["-update".into(), "1".into(), format!("out.{ext}")]);
                return Ok(Plan::one(vec![Step { args: a, seconds: 0.0 }], ext));
            }
            let every = every.filter(|e| e.is_finite() && *e >= 0.1).ok_or(PlanError::BadRange)?;
            a.extend(["-i".into(), url, "-map".into(), "0:v:0".into(), "-vf".into(), format!("fps=1/{every}")]);
            a.extend(["-frames:v".into(), MOST_FRAMES.to_string()]);
            a.extend(quality);
            a.push(format!("frame-%04d.{ext}"));
            Ok(Plan { steps: vec![Step { args: a, seconds: duration }], ext: ext.into(), made: Made::Numbered, files: Vec::new() })
        }
        Task::Gif { start: from, end, width, fps } => {
            let v = video_only(probe)?;
            let (from, length) = range(from, end, duration)?;
            if length > LONGEST_GIF + 0.05 {
                return Err(PlanError::TooLong);
            }
            let (w, _) = v.shown();
            let width = width.clamp(64, 1280).min(w.max(64));
            let fps = fps.clamp(5, 30);
            let mut a = start();
            a.extend(["-ss".into(), secs(from), "-t".into(), secs(length), "-i".into(), url, "-map".into(), "0:v:0".into()]);
            // One palette made for these frames, and used with a light, even
            // dither that only redraws what changed: smaller and steadier than
            // ffmpeg's default of 256 web colours.
            a.extend([
                "-vf".into(),
                format!(
                    "fps={fps},scale={width}:-2:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle"
                ),
                "-loop".into(),
                "0".into(),
                "out.gif".into(),
            ]);
            Ok(Plan::one(vec![Step { args: a, seconds: length }], "gif"))
        }
    }
}

/// Whether two files can be joined as they are: the same kind of picture at
/// the same size and rate, turned the same way, and the same kind of sound.
fn joins_as_is(a: &Probe, b: &Probe) -> bool {
    let (Some(va), Some(vb)) = (&a.video, &b.video) else { return false };
    let same_video = va.codec == vb.codec
        && va.width == vb.width
        && va.height == vb.height
        && va.pix_fmt == vb.pix_fmt
        && va.rotation == vb.rotation
        && (va.fps - vb.fps).abs() < 0.01;
    let same_audio = match (&a.audio, &b.audio) {
        (None, None) => true,
        (Some(x), Some(y)) => x.codec == y.codec && x.rate == y.rate && x.channels == y.channels,
        _ => false,
    };
    same_video && same_audio
}

/// One line of a concat list: a path in single quotes, any quote in it closed,
/// escaped and opened again.
fn concat_line(path: &str) -> String {
    format!("file '{}'\n", path.replace('\'', "'\\''"))
}

/// Files one after another, in order. Joined as they are when they match
/// (`joins_as_is`), which is fast and loses nothing; otherwise each is fitted
/// into the first one's frame and rate and encoded again, with silence for
/// any that has no sound.
pub fn plan_merge(inputs: &[(String, String, Probe)]) -> Result<Plan, PlanError> {
    let Some((_, first_ext, first)) = inputs.first() else { return Err(PlanError::NoVideo) };
    let v0 = first.video.clone().ok_or(PlanError::NoVideo)?;
    if inputs.iter().any(|(_, _, p)| p.video.is_none()) {
        return Err(PlanError::NoVideo);
    }
    let seconds: f64 = inputs.iter().map(|(_, _, p)| p.duration.max(0.0)).sum();
    if inputs.iter().all(|(_, _, p)| joins_as_is(first, p)) {
        let out = own_ext(first_ext);
        let list: String = inputs.iter().map(|(path, _, _)| concat_line(path)).collect();
        let mut a = start();
        a.extend(args(&["-f", "concat", "-safe", "0", "-i", "list.txt", "-map", "0:v?", "-map", "0:a?", "-c", "copy"]));
        return Ok(Plan {
            steps: vec![Step { args: finish(a, &out), seconds }],
            ext: out,
            made: Made::One,
            files: vec![("list.txt".into(), list)],
        });
    }
    let (w, h) = v0.shown();
    let (w, h) = (even(w as f64), even(h as f64));
    let fps = if v0.fps > 0.0 { v0.fps.min(60.0) } else { 30.0 };
    let sound = inputs.iter().any(|(_, _, p)| p.audio.is_some());
    let mut a = start();
    for (path, _, _) in inputs {
        a.extend(["-i".into(), input_url(path)]);
    }
    // Silence, as long as each file that has none.
    let mut silent = Vec::new();
    if sound {
        for (i, (_, _, p)) in inputs.iter().enumerate() {
            if p.audio.is_none() {
                silent.push(i);
                a.extend(args(&["-f", "lavfi", "-t"]));
                a.push(secs(p.duration.max(0.1)));
                a.extend(args(&["-i", "anullsrc=r=48000:cl=stereo"]));
            }
        }
    }
    let mut graph = String::new();
    let mut joined = String::new();
    for i in 0..inputs.len() {
        graph.push_str(&format!(
            "[{i}:v:0]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps:.3},format=yuv420p[v{i}];"
        ));
        joined.push_str(&format!("[v{i}]"));
        if sound {
            let from = match silent.iter().position(|&s| s == i) {
                Some(k) => format!("[{}:a]", inputs.len() + k),
                None => format!("[{i}:a:0]"),
            };
            graph.push_str(&format!("{from}aresample=48000,aformat=channel_layouts=stereo[a{i}];"));
            joined.push_str(&format!("[a{i}]"));
        }
    }
    graph.push_str(&format!("{joined}concat=n={}:v=1:a={}[v]{}", inputs.len(), u8::from(sound), if sound { "[a]" } else { "" }));
    a.extend(["-filter_complex".into(), graph, "-map".into(), "[v]".into()]);
    if sound {
        a.extend(args(&["-map", "[a]"]));
    }
    a.extend(x264(20));
    if sound {
        a.extend(aac(192));
    }
    Ok(Plan::one(vec![Step { args: finish(a, "mp4"), seconds }], "mp4"))
}

// ── Running it ─────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub use running::*;

#[cfg(target_os = "macos")]
mod running {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use super::{Made, Plan, Probe, MOST_FRAMES};
    use crate::syn::machine::cell::Limits;
    use crate::syn::machine::inside;
    use crate::syn::machine::jobs::Ended;
    use crate::syn::machine::rooms::Rooms;
    use crate::utilities::room::{blocking, borrow, quoted, tail, Borrowed, RoomError};

    /// Where Homebrew puts ffmpeg and ffprobe: the folders a closed room may
    /// read and run (`profile::homebrew`).
    const PLACES: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

    /// The largest file read back out of a room: the room's own disk ceiling.
    const MOST_OUT: u64 = 20 << 30;

    /// ffmpeg and ffprobe, if Homebrew has them.
    pub fn programs() -> Option<(PathBuf, PathBuf)> {
        PLACES.iter().map(Path::new).find_map(|bin| {
            let (ffmpeg, ffprobe) = (bin.join("ffmpeg"), bin.join("ffprobe"));
            (ffmpeg.is_file() && ffprobe.is_file()).then_some((ffmpeg, ffprobe))
        })
    }

    fn missing() -> RoomError {
        RoomError::Missing("ffmpeg")
    }

    /// A long job's limits: hours of wall time, and CPU for every core of them.
    fn long() -> Limits {
        Limits { wall: Duration::from_secs(8 * 3600), cpu: 8 * 3600 * 16, ..Limits::default() }
    }

    fn short() -> Limits {
        Limits { wall: Duration::from_secs(120), cpu: 600, ..Limits::default() }
    }

    /// The folder a file is in, as the kernel sees it: what the room's door is.
    /// Each input has a folder of its own in the workspace, so the door shows
    /// that one file.
    pub fn door_of(input: &Path) -> Result<(PathBuf, PathBuf), RoomError> {
        let real = input.canonicalize().map_err(|e| RoomError::Io(e.to_string()))?;
        let folder = real.parent().ok_or_else(|| RoomError::Io("no folder".into()))?.to_path_buf();
        Ok((folder, real))
    }

    fn script(program: &Path, args: &[String]) -> String {
        let mut text = quoted(&program.to_string_lossy());
        for a in args {
            text.push(' ');
            text.push_str(&quoted(a));
        }
        text
    }

    /// What a file holds. A short run in a room of its own.
    pub async fn probe(rooms: &Arc<Rooms>, name: &str, input: &Path) -> Result<Probe, RoomError> {
        let (_, ffprobe) = programs().ok_or_else(missing)?;
        let (door, real) = door_of(input)?;
        let room = borrow(rooms, name).await?;
        let cell = room.cell.clone().with_doors(vec![door]);
        let a = super::args(&["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]);
        let mut a = a;
        a.push(super::input_url(&real.to_string_lossy()));
        let watched = rooms.run(&cell, &script(&ffprobe, &a), short()).await.map_err(|e| RoomError::Io(e.to_string()))?;
        if !watched.ran.ok() {
            return Err(RoomError::Failed(tail(&watched.ran.stderr)));
        }
        super::parse_probe(&watched.ran.stdout).map_err(RoomError::Failed)
    }

    /// One picture of the video at `at` seconds, at most `width` wide, as a
    /// JPEG: for the screen, where the WebView cannot play the file itself.
    pub async fn picture(rooms: &Arc<Rooms>, name: &str, input: &Path, at: f64, width: u32) -> Result<Vec<u8>, RoomError> {
        let (ffmpeg, _) = programs().ok_or_else(missing)?;
        let (door, real) = door_of(input)?;
        let room = borrow(rooms, name).await?;
        let cell = room.cell.clone().with_doors(vec![door]);
        let mut a = super::start();
        a.extend(["-ss".into(), super::secs(at.max(0.0)), "-i".into(), super::input_url(&real.to_string_lossy())]);
        a.extend(super::args(&["-map", "0:v:0", "-frames:v", "1", "-vf"]));
        a.push(format!("scale='min({width},iw)':-2"));
        a.extend(super::args(&["-q:v", "4", "-update", "1", "out.jpg"]));
        let watched = rooms.run(&cell, &script(&ffmpeg, &a), short()).await.map_err(|e| RoomError::Io(e.to_string()))?;
        let cell2 = cell.clone();
        let read = blocking(move || inside::read(&cell2, Path::new("out.jpg"), 16 << 20).map_err(|e| RoomError::Io(e.to_string()))).await;
        match read {
            Ok((bytes, false)) if bytes.starts_with(&[0xff, 0xd8]) => Ok(bytes),
            _ => Err(RoomError::Failed(tail(&watched.ran.stderr))),
        }
    }

    /// Runs a plan's steps in a room that reads `inputs`' folders, one job
    /// after another, telling `progress` how far through (0–1) it is and
    /// stopping when `stopped` says so. The room is handed back with what it
    /// made, to be copied out with [`take`]; it is emptied when dropped.
    pub async fn run(
        rooms: &Arc<Rooms>,
        name: &str,
        inputs: &[PathBuf],
        plan: &Plan,
        progress: &(dyn Fn(f64) + Sync),
        stopped: &(dyn Fn() -> bool + Sync),
    ) -> Result<Borrowed, RoomError> {
        let (ffmpeg, _) = programs().ok_or_else(missing)?;
        let mut doors = Vec::new();
        for input in inputs {
            let (door, _) = door_of(input)?;
            if !doors.contains(&door) {
                doors.push(door);
            }
        }
        let room = borrow(rooms, name).await?;
        let cell = room.cell.clone().with_doors(doors);
        for (file, text) in &plan.files {
            inside::write(&cell, Path::new(file), text.as_bytes()).map_err(|e| RoomError::Io(e.to_string()))?;
        }
        let total = plan.seconds().max(0.001);
        let mut before = 0.0;
        for step in &plan.steps {
            let job = rooms
                .start_job(cell.clone(), name, "utilities", &script(&ffmpeg, &step.args), long())
                .await
                .map_err(|why| RoomError::Unavailable(format!("busy: {why}")))?;
            // A poll waits on it the whole time, so its end is never told to a
            // conversation (`on_job_end`), and progress is read meanwhile.
            let tick = async {
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let done = job.progress().and_then(|p| p.get("seconds_done").and_then(|s| s.as_f64())).unwrap_or(0.0);
                    progress(((before + done.min(step.seconds)) / total).clamp(0.0, 1.0));
                }
            };
            let ended = tokio::select! {
                ended = job.wait(Duration::from_secs(9 * 3600), stopped) => ended,
                () = tick => unreachable!(),
            };
            if !ended {
                let _ = rooms.stop_job(&job).await;
                return Err(RoomError::Io("cancelled".into()));
            }
            match job.ended() {
                Some(Ended::Exited { code: Some(0) }) => {}
                Some(Ended::Stopped) => return Err(RoomError::Io("cancelled".into())),
                how => return Err(RoomError::Failed(format!("{how:?}\n{}", job.last_lines(12)))),
            }
            before += step.seconds;
            progress((before / total).clamp(0.0, 1.0));
        }
        Ok(room)
    }

    /// Copies what a plan made out of its room: to `to` for one file, or to
    /// `to(n)` for the n-th of several (from 1). Returns the paths written.
    pub async fn take(room: &Borrowed, plan: &Plan, to: impl Fn(usize) -> PathBuf + Send + 'static) -> Result<Vec<PathBuf>, RoomError> {
        let (cell, made, ext) = (room.cell.clone(), plan.made, plan.ext.clone());
        blocking(move || {
            let copy = |from: &str, dest: &Path| -> Result<bool, RoomError> {
                let mut file = std::fs::File::create(dest).map_err(|e| RoomError::Io(e.to_string()))?;
                match inside::copy_out(&cell, Path::new(from), &mut file, MOST_OUT) {
                    Ok(n) if n > 0 => Ok(true),
                    Ok(_) | Err(crate::syn::machine::export::Refused::Missing) => {
                        drop(file);
                        let _ = std::fs::remove_file(dest);
                        Ok(false)
                    }
                    Err(e) => {
                        drop(file);
                        let _ = std::fs::remove_file(dest);
                        Err(RoomError::Io(e.to_string()))
                    }
                }
            };
            let mut written = Vec::new();
            match made {
                Made::One => {
                    let dest = to(1);
                    if !copy(&format!("out.{ext}"), &dest)? {
                        return Err(RoomError::Failed("it made nothing".into()));
                    }
                    written.push(dest);
                }
                Made::Numbered => {
                    for n in 1..=MOST_FRAMES {
                        let dest = to(n);
                        if !copy(&format!("frame-{n:04}.{ext}"), &dest)? {
                            break;
                        }
                        written.push(dest);
                    }
                    if written.is_empty() {
                        return Err(RoomError::Failed("it made nothing".into()));
                    }
                }
            }
            Ok(written)
        })
        .await
    }
}

#[cfg(all(test, target_os = "macos"))]
mod real {
    use super::*;
    use crate::syn::machine::rooms::Rooms;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Arc;

    fn ffprobe(path: &Path) -> Probe {
        let out = Command::new("ffprobe")
            .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
            .arg(path)
            .output()
            .unwrap();
        parse_probe(&String::from_utf8_lossy(&out.stdout)).unwrap()
    }

    fn tags(path: &Path) -> String {
        let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format_tags:stream_tags", "-of", "compact"]).arg(path).output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Every tool, through ffmpeg in a sealed room, on a clip made here: an
    /// upright phone video with a place and a date in it, in a folder whose
    /// name has a space and a quote.
    #[tokio::test]
    #[ignore]
    async fn real_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Arc::new(Rooms::at(&dir.path().join("Application Support/machine")).unwrap());
        rooms.ready().await.unwrap();
        let folder = dir.path().join("in/it's here");
        std::fs::create_dir_all(&folder).unwrap();
        let input = folder.join("clip.mov");
        let plain = folder.join("plain.mov");
        let made = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"])
            .args(["-t", "6", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
            .arg(&plain)
            .status()
            .unwrap();
        assert!(made.success());
        let made = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-display_rotation:v:0", "-90", "-i"])
            .arg(&plain)
            .args(["-c", "copy", "-metadata", "location=+10.7769+106.7009/", "-metadata", "creation_time=2026-11-01T10:00:00Z", "-metadata", "title=Secret"])
            .arg(&input)
            .status()
            .unwrap();
        std::fs::remove_file(&plain).unwrap();
        assert!(made.success());
        let p = probe(&rooms, "v-probe", &input).await.unwrap();
        println!("probe: {p:?}");
        assert_eq!(p.video.as_ref().unwrap().rotation, -90);
        assert!((p.duration - 6.0).abs() < 0.1);

        let pic = picture(&rooms, "v-pic", &input, 1.0, 320).await.unwrap();
        assert!(pic.starts_with(&[0xff, 0xd8]));

        let real = input.canonicalize().unwrap().to_string_lossy().into_owned();
        let tasks = [
            ("compress", Task::Compress { height: Some(240), codec: Codec::H264, quality: Quality::Balanced, target_mb: None }),
            ("hevc", Task::Compress { height: None, codec: Codec::Hevc, quality: Quality::Small, target_mb: None }),
            ("target", Task::Compress { height: None, codec: Codec::H264, quality: Quality::Balanced, target_mb: Some(0.5) }),
            ("webm", Task::Convert { format: Container::Webm }),
            ("mkv", Task::Convert { format: Container::Mkv }),
            ("trim", Task::Trim { start: 1.0, end: 3.0, exact: false }),
            ("exact", Task::Trim { start: 1.0, end: 3.0, exact: true }),
            ("mp3", Task::Audio { format: AudioFormat::Mp3 }),
            ("m4a", Task::Audio { format: AudioFormat::M4a }),
            ("strip", Task::Strip),
            ("right", Task::Rotate { turn: Turn::Right }),
            ("mute", Task::Mute),
            ("frames", Task::Frames { every: Some(2.0), at: None, png: false }),
            ("gif", Task::Gif { start: 0.0, end: 2.0, width: 240, fps: 10 }),
        ];
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        for (name, task) in tasks {
            let planned = super::plan(&task, &real, "mov", &p).unwrap();
            let started = std::time::Instant::now();
            let steps = std::sync::Mutex::new(Vec::new());
            let room = run(&rooms, &format!("v-{name}"), std::slice::from_ref(&input), &planned, &|f| steps.lock().unwrap().push(f), &|| false)
                .await
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let (o, ext) = (out.clone(), planned.ext.clone());
            let label = name.to_string();
            let written = take(&room, &planned, move |n| o.join(format!("{label}-{n}.{ext}"))).await.unwrap();
            let seen = steps.lock().unwrap().clone();
            let shown = if planned.ext == "jpg" { String::new() } else { format!("{:?}", ffprobe(&written[0]).video.map(|v| (v.shown(), v.codec, v.rotation))) };
            println!("{name}: {} file(s), {} bytes, {:?}, progress {:?}, {shown}", written.len(), std::fs::metadata(&written[0]).unwrap().len(), started.elapsed(), seen.last());
            if name == "strip" {
                let t = tags(&written[0]);
                println!("strip tags: {t}");
                assert!(!t.contains("Secret") && !t.contains("106.7009") && !t.contains("2026"), "{t}");
                assert_eq!(ffprobe(&written[0]).video.unwrap().rotation, -90, "the turn is kept");
            }
            if name == "right" {
                assert_eq!(ffprobe(&written[0]).video.unwrap().rotation, 180);
            }
            if name == "target" {
                let bytes = std::fs::metadata(&written[0]).unwrap().len();
                assert!(bytes <= 520_000, "{bytes}");
            }
            if name == "frames" {
                assert_eq!(written.len(), 3);
            }
        }

        // Two of the same join as they are; with a different one, encoded again.
        let a = (real.clone(), "mov".to_string(), p.clone());
        let planned = plan_merge(&[a.clone(), a.clone()]).unwrap();
        assert!(planned.files.len() == 1);
        let room = run(&rooms, "v-merge", std::slice::from_ref(&input), &planned, &|_| {}, &|| false).await.unwrap();
        let o = out.join("merge.mov");
        let written = take(&room, &planned, move |_| o.clone()).await.unwrap();
        let merged = ffprobe(&written[0]);
        println!("merge: {merged:?}");
        assert!((merged.duration - 12.0).abs() < 0.2);
        let other = out.join("webm-1.webm");
        let b = (other.canonicalize().unwrap().to_string_lossy().into_owned(), "webm".to_string(), ffprobe(&other));
        let planned = plan_merge(&[a, b]).unwrap();
        let room = run(&rooms, "v-merge2", &[input.clone(), other.clone()], &planned, &|_| {}, &|| false).await.unwrap();
        let o = out.join("merge2.mp4");
        let written = take(&room, &planned, move |_| o.clone()).await.unwrap();
        println!("merge2: {:?}", ffprobe(&written[0]));

        // Stopped: it ends, and says so.
        let slow = super::plan(&Task::Compress { height: None, codec: Codec::Hevc, quality: Quality::High, target_mb: None }, &real, "mov", &p).unwrap();
        let t = std::time::Instant::now();
        let stop = move || t.elapsed() > std::time::Duration::from_millis(300);
        let r = run(&rooms, "v-stop", std::slice::from_ref(&input), &slow, &|_| {}, &stop).await;
        println!("stopped: {:?}", r.as_ref().err());
        assert!(matches!(r, Err(crate::utilities::room::RoomError::Io(ref w)) if w == "cancelled"));
        let _ = PathBuf::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(json: &str) -> Probe {
        parse_probe(json).unwrap()
    }

    const IPHONE: &str = r#"{
      "streams": [
        {"codec_type":"video","codec_name":"hevc","width":1920,"height":1080,"pix_fmt":"yuv420p10le","avg_frame_rate":"30000/1001",
         "bit_rate":"8000000","side_data_list":[{"side_data_type":"Display Matrix","rotation":-90}],"disposition":{"attached_pic":0}},
        {"codec_type":"audio","codec_name":"aac","channels":2,"sample_rate":"44100","bit_rate":"160000"},
        {"codec_type":"data","codec_name":"none"}
      ],
      "format": {"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"12.500000","size":"12500000"}
    }"#;

    fn iphone() -> Probe {
        probe(IPHONE)
    }

    fn joined(plan: &Plan) -> Vec<String> {
        plan.steps.iter().map(|s| s.args.join(" ")).collect()
    }

    #[test]
    fn a_probe_reads_the_first_real_video_and_how_it_is_turned() {
        let p = iphone();
        let v = p.video.as_ref().unwrap();
        assert_eq!((v.codec.as_str(), v.width, v.height, v.rotation), ("hevc", 1920, 1080, -90));
        assert_eq!(v.shown(), (1080, 1920));
        assert!((v.fps - 29.97).abs() < 0.01);
        assert_eq!(p.audio.as_ref().unwrap().rate, 44100);
        assert_eq!((p.duration, p.bytes), (12.5, 12_500_000));

        // A cover picture is not the video, and an old `rotate` tag is clockwise.
        let p = probe(
            r#"{"streams":[{"codec_type":"video","codec_name":"mjpeg","width":500,"height":500,"disposition":{"attached_pic":1}},
            {"codec_type":"video","codec_name":"h264","width":640,"height":360,"tags":{"rotate":"90"}}],"format":{"duration":"1"}}"#,
        );
        let v = p.video.unwrap();
        assert_eq!((v.codec.as_str(), v.rotation), ("h264", -90));
        assert!(p.audio.is_none());
    }

    #[test]
    fn every_run_names_its_input_as_a_file_and_writes_in_the_room() {
        let plan = plan(&Task::Strip, "/c/in/x/a:b.mp4", "mp4", &iphone()).unwrap();
        let a = &plan.steps[0].args;
        assert!(a.contains(&"file:/c/in/x/a:b.mp4".to_string()));
        assert_eq!(a.last().unwrap(), "out.mp4");
        assert!(a.starts_with(&args(START)));
    }

    #[test]
    fn compress_scales_the_shorter_side_as_the_picture_is_seen() {
        let task = Task::Compress { height: Some(720), codec: Codec::H264, quality: Quality::Balanced, target_mb: None };
        let p = plan(&task, "/in.mov", "mov", &iphone()).unwrap();
        let a = joined(&p)[0].clone();
        assert!(a.contains("-vf scale=720:1280"), "{a}");
        assert!(a.contains("-c:v libx264") && a.contains("-crf 23"));
        assert!(a.contains("-map 0:v:0 -map 0:a:0"), "{a}");
        assert!(a.contains("-c:a aac -b:a 128k"));
        assert!(a.ends_with("-movflags +faststart out.mp4"));
        assert_eq!(p.ext, "mp4");

        // Never larger: a 480p file asked for 720 keeps its size.
        let mut small = iphone();
        small.video.as_mut().unwrap().width = 854;
        small.video.as_mut().unwrap().height = 480;
        let a = joined(&plan(&task, "/in.mov", "mov", &small).unwrap())[0].clone();
        assert!(!a.contains("-vf"));
    }

    #[test]
    fn hevc_keeps_ten_bits_and_is_tagged_for_quicktime() {
        let task = Task::Compress { height: None, codec: Codec::Hevc, quality: Quality::High, target_mb: None };
        let a = joined(&plan(&task, "/in.mov", "mov", &iphone()).unwrap())[0].clone();
        assert!(a.contains("-c:v libx265") && a.contains("-crf 22") && a.contains("yuv420p10le") && a.contains("-tag:v hvc1"), "{a}");
    }

    #[test]
    fn a_size_target_takes_two_passes_at_the_bitrate_it_leaves() {
        let task = Task::Compress { height: None, codec: Codec::H264, quality: Quality::Balanced, target_mb: Some(5.0) };
        let p = plan(&task, "/in.mov", "mov", &iphone()).unwrap();
        assert_eq!(p.steps.len(), 2);
        // 5 MB over 12.5 s is 3200 kbit/s; 97 % of it, less 128 for the sound.
        let [one, two] = [joined(&p)[0].clone(), joined(&p)[1].clone()];
        assert!(one.contains("-b:v 2976k -pass 1 -passlogfile pass") && one.ends_with("-an -f null -"), "{one}");
        assert!(two.contains("-b:v 2976k -pass 2") && two.contains("-c:a aac") && two.ends_with("out.mp4"), "{two}");
        assert!(!one.contains("-crf"));

        let hevc = Task::Compress { height: None, codec: Codec::Hevc, quality: Quality::Balanced, target_mb: Some(5.0) };
        let p = plan(&hevc, "/in.mov", "mov", &iphone()).unwrap();
        let one = joined(&p)[0].clone();
        assert!(one.contains("-b:v 2976k -pix_fmt yuv420p10le -tag:v hvc1 -x265-params log-level=error:pass=1:stats=pass.log"), "{one}");
        assert!(!one.contains("-crf"), "{one}");

        // Too small for the length: says what would do.
        let tiny = Task::Compress { height: None, codec: Codec::H264, quality: Quality::Balanced, target_mb: Some(0.2) };
        assert_eq!(plan(&tiny, "/in.mov", "mov", &iphone()), Err(PlanError::TooSmall(0.5)));
    }

    #[test]
    fn convert_copies_what_the_container_takes_and_encodes_the_rest() {
        let mp4 = joined(&plan(&Task::Convert { format: Container::Mp4 }, "/in.mov", "mov", &iphone()).unwrap())[0].clone();
        assert!(mp4.contains("-c:v copy -tag:v hvc1 -c:a copy"), "{mp4}");
        let webm = joined(&plan(&Task::Convert { format: Container::Webm }, "/in.mov", "mov", &iphone()).unwrap())[0].clone();
        assert!(webm.contains("libvpx-vp9") && webm.contains("libopus") && webm.ends_with("out.webm"), "{webm}");
        assert!(!webm.contains("faststart"));
        let mut avi = iphone();
        avi.video.as_mut().unwrap().codec = "mpeg4".into();
        avi.audio.as_mut().unwrap().codec = "mp3".into();
        let mp4 = joined(&plan(&Task::Convert { format: Container::Mp4 }, "/in.avi", "avi", &avi).unwrap())[0].clone();
        assert!(mp4.contains("-c:v libx264") && mp4.contains("-c:a copy"), "{mp4}");
        let mut subs = iphone();
        subs.subtitles = vec!["mov_text".into()];
        let mkv = joined(&plan(&Task::Convert { format: Container::Mkv }, "/in.mp4", "mp4", &subs).unwrap())[0].clone();
        assert!(mkv.contains("-map 0:s? -c:s srt") && mkv.ends_with("out.mkv"), "{mkv}");
    }

    #[test]
    fn trim_is_checked_against_the_length_and_copies_unless_exact() {
        let fast = plan(&Task::Trim { start: 2.0, end: 5.5, exact: false }, "/in.mov", "MOV", &iphone()).unwrap();
        let a = joined(&fast)[0].clone();
        assert!(a.contains("-ss 2.000 -i file:/in.mov -t 3.500") && a.contains("-c copy") && a.ends_with("out.mov"), "{a}");
        assert_eq!(fast.steps[0].seconds, 3.5);
        let exact = plan(&Task::Trim { start: 2.0, end: 5.5, exact: true }, "/in.mov", "mov", &iphone()).unwrap();
        assert!(joined(&exact)[0].contains("-crf 18") && exact.ext == "mp4");
        for (s, e) in [(5.0, 4.0), (-1.0, 2.0), (0.0, 30.0), (1.0, 1.05), (f64::NAN, 2.0)] {
            assert_eq!(plan(&Task::Trim { start: s, end: e, exact: false }, "/in", "mov", &iphone()), Err(PlanError::BadRange), "{s} {e}");
        }
    }

    #[test]
    fn audio_is_copied_when_it_already_is_what_was_asked() {
        let m4a = plan(&Task::Audio { format: AudioFormat::M4a }, "/in.mov", "mov", &iphone()).unwrap();
        assert!(joined(&m4a)[0].contains("-c:a copy") && m4a.ext == "m4a");
        let mp3 = plan(&Task::Audio { format: AudioFormat::Mp3 }, "/in.mov", "mov", &iphone()).unwrap();
        assert!(joined(&mp3)[0].contains("libmp3lame -q:a 2"));
        let mut quiet = iphone();
        quiet.audio = None;
        assert_eq!(plan(&Task::Audio { format: AudioFormat::Wav }, "/in.mov", "mov", &quiet), Err(PlanError::NoAudio));
    }

    #[test]
    fn strip_keeps_the_streams_and_drops_every_tag_and_other_track() {
        let a = joined(&plan(&Task::Strip, "/in.mov", "mov", &iphone()).unwrap())[0].clone();
        assert!(a.contains("-map 0:v? -map 0:a? -map 0:s? -c copy -map_metadata -1 -map_chapters -1 -fflags +bitexact"), "{a}");
        assert!(!a.contains("-map 0 "));
    }

    #[test]
    fn a_turn_adds_to_the_turn_the_file_already_has() {
        // Upright iPhone (-90), a quarter to the right: -180, which is 180.
        let a = joined(&plan(&Task::Rotate { turn: Turn::Right }, "/in.mov", "mov", &iphone()).unwrap())[0].clone();
        assert!(a.contains("-display_rotation:v:0 180 -i file:/in.mov") && a.contains("-c copy"), "{a}");
        let a = joined(&plan(&Task::Rotate { turn: Turn::Left }, "/in.mov", "mov", &iphone()).unwrap())[0].clone();
        assert!(a.contains("-display_rotation:v:0 0 "), "{a}");
        // Not an MP4: turned and encoded again.
        let p = plan(&Task::Rotate { turn: Turn::Right }, "/in.avi", "avi", &iphone()).unwrap();
        assert!(joined(&p)[0].contains("-vf transpose=1") && p.ext == "mp4");
        assert_eq!(normalised(-270), 90);
        assert_eq!(normalised(180), 180);
        assert_eq!(normalised(-180), 180);
    }

    #[test]
    fn mute_and_frames() {
        let a = joined(&plan(&Task::Mute, "/in.mkv", "mkv", &iphone()).unwrap())[0].clone();
        assert!(a.contains("-map 0:v -map 0:s? -c copy -an") && a.ends_with("out.mkv"));
        let every = plan(&Task::Frames { every: Some(2.0), at: None, png: false }, "/in.mov", "mov", &iphone()).unwrap();
        assert_eq!(every.made, Made::Numbered);
        assert!(joined(&every)[0].contains("-vf fps=1/2") && joined(&every)[0].ends_with("-q:v 2 frame-%04d.jpg"));
        let one = plan(&Task::Frames { every: None, at: Some(3.0), png: true }, "/in.mov", "mov", &iphone()).unwrap();
        assert!(joined(&one)[0].contains("-ss 3.000") && one.ext == "png" && one.made == Made::One);
        assert_eq!(plan(&Task::Frames { every: None, at: Some(99.0), png: true }, "/in", "mov", &iphone()), Err(PlanError::BadRange));
        assert_eq!(plan(&Task::Frames { every: Some(0.0), at: None, png: true }, "/in", "mov", &iphone()), Err(PlanError::BadRange));
    }

    #[test]
    fn a_gif_is_short_and_no_wider_than_the_picture() {
        let p = plan(&Task::Gif { start: 1.0, end: 4.0, width: 4000, fps: 99 }, "/in.mov", "mov", &iphone()).unwrap();
        let a = joined(&p)[0].clone();
        assert!(a.contains("fps=30,scale=1080:-2") && a.contains("palettegen") && a.ends_with("-loop 0 out.gif"), "{a}");
        let mut long = iphone();
        long.duration = 120.0;
        assert_eq!(plan(&Task::Gif { start: 0.0, end: 70.0, width: 480, fps: 10 }, "/in", "mov", &long), Err(PlanError::TooLong));
    }

    #[test]
    fn files_that_match_are_joined_as_they_are() {
        let a = ("/w/in/a/it's.mov".to_string(), "mov".to_string(), iphone());
        let b = ("/w/in/b/b.mov".to_string(), "mov".to_string(), iphone());
        let p = plan_merge(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(p.files, vec![("list.txt".to_string(), "file '/w/in/a/it'\\''s.mov'\nfile '/w/in/b/b.mov'\n".to_string())]);
        assert!(joined(&p)[0].contains("-f concat -safe 0 -i list.txt") && p.ext == "mov");
        assert_eq!(p.seconds(), 25.0);
    }

    #[test]
    fn files_that_differ_are_fitted_to_the_first_with_silence_where_there_is_no_sound() {
        let a = ("/a.mov".to_string(), "mov".to_string(), iphone());
        let mut other = iphone();
        other.video.as_mut().unwrap().rotation = 0;
        other.audio = None;
        other.duration = 3.0;
        let p = plan_merge(&[a, ("/b.mp4".to_string(), "mp4".to_string(), other)]).unwrap();
        let s = joined(&p)[0].clone();
        assert!(s.contains("-i file:/a.mov -i file:/b.mp4 -f lavfi -t 3.000 -i anullsrc=r=48000:cl=stereo"), "{s}");
        assert!(s.contains("[0:v:0]scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920"), "{s}");
        assert!(s.contains("[2:a]aresample=48000") && s.contains("[v0][a0][v1][a1]concat=n=2:v=1:a=1[v][a]"), "{s}");
        assert!(s.ends_with("out.mp4") && p.ext == "mp4");
        assert_eq!(plan_merge(&[]), Err(PlanError::NoVideo));
    }

    #[test]
    fn tasks_come_from_the_screen_as_tagged_json() {
        let t: Task = serde_json::from_str(r#"{"tool":"compress","height":720,"codec":"hevc","quality":"small","targetMb":null}"#).unwrap();
        assert_eq!(t, Task::Compress { height: Some(720), codec: Codec::Hevc, quality: Quality::Small, target_mb: None });
        let t: Task = serde_json::from_str(r#"{"tool":"rotate","turn":"left"}"#).unwrap();
        assert_eq!(t, Task::Rotate { turn: Turn::Left });
        assert!(serde_json::from_str::<Task>(r#"{"tool":"shell","command":"rm"}"#).is_err());
    }
}
