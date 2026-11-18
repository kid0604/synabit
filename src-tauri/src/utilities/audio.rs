//! Audio tools: ffmpeg in the same sealed room as the video tools.
//!
//! Everything about the room (the read-only door, the job, copying the result
//! out) is [`super::video`]'s; this module only plans. The screen picks a tool
//! and settings from fixed choices ([`AudioTask`]); every argument is written
//! here.
//!
//! The tools take audio files and video files alike: given a video, they work
//! on its first sound track and make a sound file, except loudness for a video
//! (`keep_picture`), which copies the picture and makes the sound even.
//!
//! Loudness to a target is two runs: the first only measures (EBU R128, by
//! `loudnorm`), and the second applies one fixed gain worked out from that,
//! so the music's own dynamics are left alone (`linear=true`). [`measure`]
//! plans the first, [`parse_measured`] reads what it printed, and [`plan`]
//! takes the result.

use serde::{Deserialize, Serialize};

use super::video::{aac, args, concat_line, finish, input_url, range, secs, start, Made, Plan, PlanError, Probe, Step, EXTENSIONS as VIDEO};

/// The files the audio tools take, by extension. `.m4r` is an iPhone ringtone,
/// `.caf` what Apple's own apps record.
pub const EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "wav", "flac", "ogg", "oga", "opus", "aiff", "aif", "wma", "amr", "caf", "m4r"];

/// The longest ringtone an iPhone takes, in seconds.
pub const LONGEST_RINGTONE: f64 = 40.0;

/// The longest crossfade between two files, in seconds.
const LONGEST_CROSSFADE: f64 = 10.0;

/// How much a lossy file is to be squeezed: for MP3, VBR quality; for AAC
/// and Opus, a bitrate.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    High,
    Medium,
    Small,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Mp3,
    M4a,
    Opus,
    Flac,
    Alac,
    Wav,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Loudness {
    /// −16 LUFS, what Apple Podcasts and Spotify ask of speech.
    Podcast,
    /// −14 LUFS, where streaming services play music.
    Music,
    /// Quiet and loud passages of one voice brought together (`speechnorm`).
    Speech,
    /// Louder or quieter by `gain` decibels, kept from clipping.
    Gain,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Strength {
    Light,
    Medium,
    Strong,
}

/// One tool and its settings.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "tool", rename_all = "camelCase")]
pub enum AudioTask {
    Convert { format: Format, level: Level },
    /// From `start` to `end` seconds, faded in and out over so many seconds.
    Trim {
        start: f64,
        end: f64,
        #[serde(rename = "fadeIn")]
        fade_in: f64,
        #[serde(rename = "fadeOut")]
        fade_out: f64,
    },
    Loudness { mode: Loudness, gain: f64 },
    Denoise { strength: Strength },
    /// Every pause longer than `pause` seconds shortened to `pause` seconds.
    Silence { pause: f64 },
    /// Faster or slower by `factor`, the pitch kept.
    Speed { factor: f64 },
    /// An iPhone ringtone from `start` to `end`: AAC in `.m4r`, faded.
    Ringtone { start: f64, end: f64 },
}

/// What `loudnorm` measured in the first run.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub i: f64,
    pub tp: f64,
    pub lra: f64,
    pub thresh: f64,
    pub offset: f64,
}

/// The extension and encoder a result keeps when it stays in its own kind of
/// file: MP3 stays MP3, FLAC stays FLAC, and anything ffmpeg cannot write back
/// as it was (WMA, AMR, a video) becomes M4A.
fn own(ext: &str, codec: &str) -> (&'static str, Vec<String>) {
    match ext.to_lowercase().as_str() {
        "mp3" => ("mp3", encoder("mp3", Level::High)),
        "wav" => ("wav", encoder("wav", Level::High)),
        "aiff" | "aif" => ("aiff", args(&["-c:a", "pcm_s16be"])),
        "flac" => ("flac", encoder("flac", Level::High)),
        "opus" => ("opus", encoder("opus", Level::High)),
        "ogg" | "oga" => ("ogg", encoder("opus", Level::High)),
        _ if codec == "alac" => ("m4a", args(&["-c:a", "alac"])),
        _ => ("m4a", encoder("m4a", Level::High)),
    }
}

fn encoder(ext: &str, level: Level) -> Vec<String> {
    let pick = |high: &str, medium: &str, small: &str| match level {
        Level::High => high.to_string(),
        Level::Medium => medium.to_string(),
        Level::Small => small.to_string(),
    };
    match ext {
        "mp3" => vec!["-c:a".into(), "libmp3lame".into(), "-q:a".into(), pick("0", "2", "5")],
        "opus" | "ogg" => vec!["-c:a".into(), "libopus".into(), "-b:a".into(), pick("160k", "128k", "96k")],
        "flac" => args(&["-c:a", "flac"]),
        "wav" => args(&["-c:a", "pcm_s16le"]),
        _ => vec!["-c:a".into(), "aac".into(), "-b:a".into(), pick("256k", "192k", "128k")],
    }
}

/// Kilobits a second a lossy level makes, to tell whether a file is already
/// lighter than that (and so is copied, not made worse).
fn kbps(format: Format, level: Level) -> u64 {
    match (format, level) {
        (Format::Mp3, Level::High) => 245,
        (Format::Mp3, Level::Medium) => 190,
        (Format::Mp3, Level::Small) => 130,
        (Format::Opus, Level::High) => 160,
        (Format::Opus, Level::Medium) => 128,
        (Format::Opus, Level::Small) => 96,
        (_, Level::High) => 256,
        (_, Level::Medium) => 192,
        (_, Level::Small) => 128,
    }
}

/// The sound of the first track and nothing else.
fn sound_only(url: String) -> Vec<String> {
    let mut a = start();
    a.extend(["-i".into(), url, "-map".into(), "0:a:0".into(), "-vn".into(), "-sn".into(), "-dn".into()]);
    a
}

/// The end of a run that makes `out.<ext>`. An `.m4r` is an iPod-flavoured
/// MP4, which ffmpeg does not know by its extension.
fn out(mut a: Vec<String>, ext: &str) -> Vec<String> {
    match ext {
        "m4a" => finish(a, "mp4").into_iter().map(|x| if x == "out.mp4" { "out.m4a".into() } else { x }).collect(),
        "m4r" => {
            a.extend(args(&["-movflags", "+faststart", "-f", "ipod", "out.m4r"]));
            a
        }
        _ => {
            a.push(format!("out.{ext}"));
            a
        }
    }
}

fn fades(fade_in: f64, fade_out: f64, length: f64) -> Option<String> {
    let most = length / 2.0;
    let fin = if fade_in.is_finite() { fade_in.clamp(0.0, most) } else { 0.0 };
    let fout = if fade_out.is_finite() { fade_out.clamp(0.0, most) } else { 0.0 };
    let mut parts = Vec::new();
    if fin >= 0.05 {
        parts.push(format!("afade=t=in:st=0:d={fin:.2}"));
    }
    if fout >= 0.05 {
        parts.push(format!("afade=t=out:st={:.3}:d={fout:.2}", length - fout));
    }
    (!parts.is_empty()).then(|| parts.join(","))
}

/// The settings a target loudness asks of `loudnorm`.
fn target(mode: Loudness) -> &'static str {
    match mode {
        Loudness::Music => "I=-14:TP=-1:LRA=11",
        _ => "I=-16:TP=-1.5:LRA=11",
    }
}

/// Whether loudness to a target needs the measuring run first.
pub fn needs_measure(task: &AudioTask) -> bool {
    matches!(task, AudioTask::Loudness { mode: Loudness::Podcast | Loudness::Music, .. })
}

/// The measuring run: decodes the sound and prints what `loudnorm` found, as
/// JSON, at the end. Nothing is made. `info` level, which is where it prints.
pub fn measure(task: &AudioTask, input: &str, probe: &Probe) -> Result<Plan, PlanError> {
    let AudioTask::Loudness { mode, .. } = task else { return Err(PlanError::BadRange) };
    probe.audio.as_ref().ok_or(PlanError::NoAudio)?;
    let mut a = sound_only(input_url(input));
    if let Some(level) = a.iter().position(|x| x == "error") {
        a[level] = "info".into();
    }
    a.extend(["-af".into(), format!("loudnorm={}:print_format=json", target(*mode)), "-f".into(), "null".into(), "-".into()]);
    Ok(Plan::one(vec![Step { args: a, seconds: probe.duration.max(0.0) }], "json"))
}

/// What the measuring run printed: the last JSON object with `input_i` in it.
pub fn parse_measured(said: &str) -> Option<Measured> {
    let end = said.rfind('}')?;
    let begin = said[..end].rfind('{')?;
    let json: serde_json::Value = serde_json::from_str(&said[begin..=end]).ok()?;
    let get = |k: &str| json.get(k).and_then(|v| v.as_str()).and_then(|v| v.trim().parse::<f64>().ok());
    Some(Measured { i: get("input_i")?, tp: get("input_tp")?, lra: get("input_lra")?, thresh: get("input_thresh")?, offset: get("target_offset")? })
}

/// What to run for one file. `input` is its path where the room reads it,
/// `ext` its extension. `keep_picture` keeps a video's picture as it is (the
/// loudness tool for video); `measured` is the measuring run's result, for
/// loudness to a target.
pub fn plan(task: &AudioTask, input: &str, ext: &str, probe: &Probe, keep_picture: bool, measured: Option<&Measured>) -> Result<Plan, PlanError> {
    let audio = probe.audio.as_ref().ok_or(PlanError::NoAudio)?;
    let url = input_url(input);
    let duration = probe.duration.max(0.0);
    let (own_ext, own_encoder) = own(ext, &audio.codec);
    match *task {
        AudioTask::Convert { format, level } => {
            let (out_ext, same) = match format {
                Format::Mp3 => ("mp3", audio.codec == "mp3"),
                Format::M4a => ("m4a", audio.codec == "aac"),
                Format::Opus => ("opus", audio.codec == "opus"),
                Format::Flac => ("flac", audio.codec == "flac"),
                Format::Alac => ("m4a", audio.codec == "alac"),
                Format::Wav => ("wav", audio.codec == "pcm_s16le"),
            };
            let lossless = matches!(format, Format::Flac | Format::Alac | Format::Wav);
            // Already this kind, and no heavier than asked: copied, since
            // encoding a lossy file again only loses.
            let light = audio.bitrate.is_some_and(|b| b <= kbps(format, level) * 1000 + 8000);
            let mut a = sound_only(url);
            if same && (lossless || light) {
                a.extend(args(&["-c:a", "copy"]));
            } else if format == Format::Alac {
                a.extend(args(&["-c:a", "alac"]));
            } else {
                a.extend(encoder(out_ext, level));
            }
            Ok(Plan::one(vec![Step { args: out(a, out_ext), seconds: duration }], out_ext))
        }
        AudioTask::Trim { start: from, end, fade_in, fade_out } => {
            let (from, length) = range(from, end, duration)?;
            let mut a = start();
            a.extend(["-ss".into(), secs(from), "-i".into(), url, "-t".into(), secs(length)]);
            a.extend(args(&["-map", "0:a:0", "-vn", "-sn", "-dn"]));
            let shape = fades(fade_in, fade_out, length);
            // Without fades, a file ffmpeg writes back as it was is cut as it
            // is: an MP3 or AAC to within a frame, and nothing lost.
            let writes_back = matches!(
                (own_ext, audio.codec.as_str()),
                ("mp3", "mp3") | ("m4a", "aac" | "alac") | ("flac", "flac") | ("opus" | "ogg", "opus") | ("wav", "pcm_s16le")
            );
            match shape {
                None if writes_back => a.extend(args(&["-c:a", "copy"])),
                None => a.extend(own_encoder),
                Some(f) => {
                    a.extend(["-af".into(), f]);
                    a.extend(own_encoder);
                }
            }
            Ok(Plan::one(vec![Step { args: out(a, own_ext), seconds: length }], own_ext))
        }
        AudioTask::Loudness { mode, gain } => {
            let filter = match mode {
                Loudness::Podcast | Loudness::Music => {
                    let m = measured.ok_or(PlanError::BadRange)?;
                    if !m.i.is_finite() || m.i < -70.0 {
                        return Err(PlanError::Silent);
                    }
                    format!(
                        "loudnorm={}:measured_I={:.2}:measured_TP={:.2}:measured_LRA={:.2}:measured_thresh={:.2}:offset={:.2}:linear=true:print_format=none,aresample={}",
                        target(mode),
                        m.i,
                        m.tp,
                        m.lra,
                        m.thresh,
                        m.offset,
                        if audio.rate > 0 { audio.rate } else { 48000 },
                    )
                }
                Loudness::Speech => "speechnorm=e=6.25:r=0.00001:l=1".to_string(),
                Loudness::Gain => {
                    if !gain.is_finite() || gain.abs() < 0.1 {
                        return Err(PlanError::BadRange);
                    }
                    let db = gain.clamp(-30.0, 30.0);
                    // Louder is held under full scale, so peaks do not clip.
                    if db > 0.0 { format!("volume={db:.1}dB,alimiter=limit=0.89:level=0") } else { format!("volume={db:.1}dB") }
                }
            };
            if keep_picture && probe.video.is_some() {
                let vext = if VIDEO.contains(&ext.to_lowercase().as_str()) { ext.to_lowercase() } else { "mkv".into() };
                let mut a = start();
                a.extend(["-i".into(), url, "-map".into(), "0:v?".into(), "-map".into(), "0:a:0".into(), "-c:v".into(), "copy".into(), "-af".into(), filter]);
                a.extend(if vext == "webm" { args(&["-c:a", "libopus", "-b:a", "160k"]) } else { aac(192) });
                return Ok(Plan::one(vec![Step { args: finish(a, &vext), seconds: duration }], &vext));
            }
            let mut a = sound_only(url);
            a.extend(["-af".into(), filter]);
            a.extend(own_encoder);
            Ok(Plan::one(vec![Step { args: out(a, own_ext), seconds: duration }], own_ext))
        }
        AudioTask::Denoise { strength } => {
            // The hum under a voice first (below 80 Hz there is no speech),
            // then the steady hiss, tracked as it changes.
            let nr = match strength {
                Strength::Light => 6,
                Strength::Medium => 12,
                Strength::Strong => 20,
            };
            let mut a = sound_only(url);
            a.extend(["-af".into(), format!("highpass=f=80,afftdn=nr={nr}:nf=-40:tn=1")]);
            a.extend(own_encoder);
            Ok(Plan::one(vec![Step { args: out(a, own_ext), seconds: duration }], own_ext))
        }
        AudioTask::Silence { pause } => {
            if !pause.is_finite() || pause < 0.2 {
                return Err(PlanError::BadRange);
            }
            // ffmpeg lets the first `stop_duration` of a pause through and cuts
            // the rest, so every pause comes out at most `pause` long: shorter
            // ones are left alone, and speech keeps its breaths.
            let mut a = sound_only(url);
            a.extend(["-af".into(), format!("silenceremove=stop_periods=-1:stop_duration={pause:.2}:stop_threshold=-40dB")]);
            a.extend(own_encoder);
            Ok(Plan::one(vec![Step { args: out(a, own_ext), seconds: duration }], own_ext))
        }
        AudioTask::Speed { factor } => {
            if !factor.is_finite() || !(0.5..=2.0).contains(&factor) || (factor - 1.0).abs() < 0.01 {
                return Err(PlanError::BadRange);
            }
            let mut a = sound_only(url);
            a.extend(["-af".into(), format!("atempo={factor:.3}")]);
            a.extend(own_encoder);
            Ok(Plan::one(vec![Step { args: out(a, own_ext), seconds: duration / factor }], own_ext))
        }
        AudioTask::Ringtone { start: from, end } => {
            let (from, length) = range(from, end, duration)?;
            if length > LONGEST_RINGTONE + 0.05 {
                return Err(PlanError::TooLong);
            }
            let mut a = start();
            a.extend(["-ss".into(), secs(from), "-i".into(), url, "-t".into(), secs(length)]);
            a.extend(args(&["-map", "0:a:0", "-vn", "-sn", "-dn"]));
            if let Some(f) = fades(0.3, 1.0, length) {
                a.extend(["-af".into(), f]);
            }
            a.extend(aac(192));
            Ok(Plan::one(vec![Step { args: out(a, "m4r"), seconds: length }], "m4r"))
        }
    }
}

/// Files one after another, as the first one's kind of file. Joined as they
/// are when they are the same kind of sound and there is no crossfade;
/// otherwise each is brought to 48 kHz stereo and encoded again, each
/// running into the next over `crossfade` seconds.
pub fn plan_merge(inputs: &[(String, String, Probe)], crossfade: f64) -> Result<Plan, PlanError> {
    let Some((_, first_ext, first)) = inputs.first() else { return Err(PlanError::NoAudio) };
    let a0 = first.audio.clone().ok_or(PlanError::NoAudio)?;
    if inputs.iter().any(|(_, _, p)| p.audio.is_none()) {
        return Err(PlanError::NoAudio);
    }
    let shortest = inputs.iter().map(|(_, _, p)| p.duration).fold(f64::INFINITY, f64::min);
    let fade = if crossfade.is_finite() { crossfade.clamp(0.0, LONGEST_CROSSFADE).min(shortest / 2.0).max(0.0) } else { 0.0 };
    let fade = if fade < 0.05 { 0.0 } else { fade };
    let total: f64 = inputs.iter().map(|(_, _, p)| p.duration.max(0.0)).sum::<f64>() - fade * (inputs.len() - 1) as f64;
    let (own_ext, own_encoder) = own(first_ext, &a0.codec);
    let same = inputs.iter().all(|(_, ext, p)| {
        let a = p.audio.as_ref().expect("checked");
        a.codec == a0.codec && a.rate == a0.rate && a.channels == a0.channels && own(ext, &a.codec).0 == own_ext && p.video.is_none()
    });
    if same && fade == 0.0 {
        let list: String = inputs.iter().map(|(path, _, _)| concat_line(path)).collect();
        let mut a = start();
        a.extend(args(&["-f", "concat", "-safe", "0", "-i", "list.txt", "-map", "0:a", "-c", "copy"]));
        return Ok(Plan { steps: vec![Step { args: out(a, own_ext), seconds: total }], ext: own_ext.into(), made: Made::One, files: vec![("list.txt".into(), list)] });
    }
    let mut a = start();
    for (path, _, _) in inputs {
        a.extend(["-i".into(), input_url(path)]);
    }
    let mut graph = String::new();
    for i in 0..inputs.len() {
        graph.push_str(&format!("[{i}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}];"));
    }
    if fade > 0.0 {
        let mut last = "a0".to_string();
        for i in 1..inputs.len() {
            let next = if i + 1 == inputs.len() { "out".to_string() } else { format!("x{i}") };
            graph.push_str(&format!("[{last}][a{i}]acrossfade=d={fade:.2}[{next}];"));
            last = next;
        }
        graph.pop();
    } else {
        let joined: String = (0..inputs.len()).map(|i| format!("[a{i}]")).collect();
        graph.push_str(&format!("{joined}concat=n={}:v=0:a=1[out]", inputs.len()));
    }
    a.extend(["-filter_complex".into(), graph, "-map".into(), "[out]".into()]);
    a.extend(own_encoder);
    Ok(Plan::one(vec![Step { args: out(a, own_ext), seconds: total }], own_ext))
}

/// The loudest sample in each of `buckets` equal stretches of 16-bit mono
/// samples, from 0 to 1: a waveform to draw.
pub fn peaks(samples: &[u8], buckets: usize) -> Vec<f32> {
    let values: Vec<i16> = samples.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    if values.is_empty() || buckets == 0 {
        return Vec::new();
    }
    (0..buckets)
        .map(|k| {
            let from = k * values.len() / buckets;
            let to = ((k + 1) * values.len() / buckets).max(from + 1).min(values.len());
            values[from..to].iter().map(|v| v.unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0
        })
        .collect()
}

/// How many samples a second to decode for a waveform: about four million in
/// all, however long the file is.
pub fn peak_rate(duration: f64) -> u32 {
    if duration <= 0.0 {
        return 4000;
    }
    (4_000_000.0 / duration).clamp(50.0, 4000.0) as u32
}

/// What the waveform command returns.
#[derive(Debug, Clone, Serialize)]
pub struct Waveform {
    pub peaks: Vec<f32>,
}

#[cfg(target_os = "macos")]
pub use running::*;

#[cfg(target_os = "macos")]
mod running {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use super::{peak_rate, peaks, Waveform};
    use crate::syn::machine::cell::Limits;
    use crate::syn::machine::inside;
    use crate::syn::machine::rooms::Rooms;
    use crate::utilities::room::{blocking, borrow, quoted, tail, RoomError};
    use crate::utilities::video::{args, door_of, input_url, programs, start};

    /// A file's sound as a waveform of `buckets` peaks: decoded in a room to a
    /// few million mono samples, and those boiled down here.
    pub async fn waveform(rooms: &Arc<Rooms>, name: &str, input: &Path, duration: f64, buckets: usize) -> Result<Waveform, RoomError> {
        let (ffmpeg, _) = programs().ok_or(RoomError::Missing("ffmpeg"))?;
        let (door, real) = door_of(input)?;
        let room = borrow(rooms, name).await?;
        let cell = room.cell.clone().with_doors(vec![door]);
        let mut a = start();
        a.extend(["-i".into(), input_url(&real.to_string_lossy())]);
        a.extend(args(&["-map", "0:a:0", "-ac", "1", "-ar"]));
        a.push(peak_rate(duration).to_string());
        a.extend(args(&["-f", "s16le", "out.raw"]));
        let script = std::iter::once(quoted(&ffmpeg.to_string_lossy())).chain(a.iter().map(|x| quoted(x))).collect::<Vec<_>>().join(" ");
        let limits = Limits { wall: Duration::from_secs(300), cpu: 1200, ..Limits::default() };
        let watched = rooms.run(&cell, &script, limits).await.map_err(|e| RoomError::Io(e.to_string()))?;
        let cell2 = cell.clone();
        let read = blocking(move || inside::read(&cell2, Path::new("out.raw"), 64 << 20).map_err(|e| RoomError::Io(e.to_string()))).await;
        match read {
            Ok((bytes, false)) if !bytes.is_empty() => Ok(Waveform { peaks: peaks(&bytes, buckets.clamp(50, 4000)) }),
            _ => Err(RoomError::Failed(tail(&watched.ran.stderr))),
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod real {
    use super::*;
    use crate::syn::machine::rooms::Rooms;
    use crate::utilities::video::{parse_probe, probe, run, take};
    use std::path::Path;
    use std::process::Command;
    use std::sync::Arc;

    fn ffprobe(path: &Path) -> Probe {
        let out = Command::new("ffprobe").args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path).output().unwrap();
        parse_probe(&String::from_utf8_lossy(&out.stdout)).unwrap()
    }

    /// Integrated loudness of a file, by ffmpeg outside any room (the test's own check).
    fn lufs(path: &Path) -> f64 {
        let out = Command::new("ffmpeg").args(["-hide_banner", "-nostats", "-i"]).arg(path).args(["-af", "ebur128", "-f", "null", "-"]).output().unwrap();
        let text = String::from_utf8_lossy(&out.stderr);
        let at = text.rfind("I:").unwrap();
        text[at + 2..].split_whitespace().next().unwrap().parse().unwrap()
    }

    /// Every audio tool through ffmpeg in a sealed room, on a voice-like clip
    /// made here: a quiet tone, three seconds of silence, a tone, as MP3.
    #[tokio::test]
    #[ignore]
    async fn real_audio() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Arc::new(Rooms::at(&dir.path().join("Application Support/machine")).unwrap());
        rooms.ready().await.unwrap();
        let folder = dir.path().join("in/it's here");
        std::fs::create_dir_all(&folder).unwrap();
        let input = folder.join("memo.mp3");
        let made = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("sine=frequency=300:sample_rate=44100:duration=4,volume=0.5[a];anullsrc=r=44100:cl=mono,atrim=duration=3[b];sine=frequency=500:sample_rate=44100:duration=4,volume=0.5[c];[a][b][c]concat=n=3:v=0:a=1")
            .args(["-c:a", "libmp3lame", "-q:a", "2"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());
        let p = probe(&rooms, "a-probe", &input).await.unwrap();
        println!("probe: {p:?}, lufs {:.1}", lufs(&input));
        let real = input.canonicalize().unwrap().to_string_lossy().into_owned();

        let w = waveform(&rooms, "a-wave", &input, p.duration, 110).await.unwrap();
        // Loud, then the silence in the middle, then loud.
        assert!(w.peaks[5] > 0.03 && w.peaks[55] < 0.01 && w.peaks[100] > 0.03, "{:?}", w.peaks);

        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        let tasks = [
            ("m4a", AudioTask::Convert { format: Format::M4a, level: Level::Medium }),
            ("flac", AudioTask::Convert { format: Format::Flac, level: Level::High }),
            ("opus", AudioTask::Convert { format: Format::Opus, level: Level::Small }),
            ("cut", AudioTask::Trim { start: 1.0, end: 9.0, fade_in: 0.0, fade_out: 0.0 }),
            ("fade", AudioTask::Trim { start: 1.0, end: 9.0, fade_in: 1.0, fade_out: 2.0 }),
            ("podcast", AudioTask::Loudness { mode: Loudness::Podcast, gain: 0.0 }),
            ("speech", AudioTask::Loudness { mode: Loudness::Speech, gain: 0.0 }),
            ("gain", AudioTask::Loudness { mode: Loudness::Gain, gain: 6.0 }),
            ("denoise", AudioTask::Denoise { strength: Strength::Medium }),
            ("silence", AudioTask::Silence { pause: 1.0 }),
            ("speed", AudioTask::Speed { factor: 2.0 }),
            ("ring", AudioTask::Ringtone { start: 0.0, end: 8.0 }),
        ];
        for (name, task) in tasks {
            let started = std::time::Instant::now();
            let measured = if needs_measure(&task) {
                let m = measure(&task, &real, &p).unwrap();
                let (_, said) = run(&rooms, &format!("a-{name}m"), std::slice::from_ref(&input), &m, &|_| {}, &|| false).await.unwrap();
                Some(parse_measured(&said).unwrap_or_else(|| panic!("no measurement in {said}")))
            } else {
                None
            };
            let planned = plan(&task, &real, "mp3", &p, false, measured.as_ref()).unwrap();
            let (room, _) = run(&rooms, &format!("a-{name}"), std::slice::from_ref(&input), &planned, &|_| {}, &|| false)
                .await
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let (o, ext, label) = (out.clone(), planned.ext.clone(), name.to_string());
            let written = take(&room, &planned, move |_| o.join(format!("{label}.{ext}"))).await.unwrap();
            let got = ffprobe(&written[0]);
            let a = got.audio.clone().unwrap();
            println!("{name}: {} {:.2}s {} Hz, {:.1} LUFS, {:?}", a.codec, got.duration, a.rate, lufs(&written[0]), started.elapsed());
            match name {
                "cut" | "fade" => assert!((got.duration - 8.0).abs() < 0.1),
                "podcast" => assert!((lufs(&written[0]) + 16.0).abs() < 1.5),
                // 8 s of sound and the 3 s pause cut to 1 s.
                "silence" => assert!((got.duration - 9.0).abs() < 0.15, "{}", got.duration),
                "speed" => assert!((got.duration - 5.5).abs() < 0.2, "{}", got.duration),
                "ring" => assert!(written[0].extension().unwrap() == "m4r" && a.codec == "aac"),
                "flac" => assert_eq!(a.codec, "flac"),
                _ => {}
            }
        }

        let a = (real.clone(), "mp3".to_string(), p.clone());
        let joined = plan_merge(&[a.clone(), a.clone()], 2.0).unwrap();
        let (room, _) = run(&rooms, "a-join", std::slice::from_ref(&input), &joined, &|_| {}, &|| false).await.unwrap();
        let o = out.join("join.mp3");
        let written = take(&room, &joined, move |_| o.clone()).await.unwrap();
        let got = ffprobe(&written[0]);
        println!("join: {:.2}s", got.duration);
        assert!((got.duration - 20.0).abs() < 0.2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::video::parse_probe;

    fn song() -> Probe {
        parse_probe(
            r#"{"streams":[{"codec_type":"audio","codec_name":"mp3","channels":2,"sample_rate":"44100","bit_rate":"320000"},
            {"codec_type":"video","codec_name":"mjpeg","width":500,"height":500,"disposition":{"attached_pic":1}}],
            "format":{"format_name":"mp3","duration":"200.0","size":"8000000"}}"#,
        )
        .unwrap()
    }

    fn memo() -> Probe {
        parse_probe(r#"{"streams":[{"codec_type":"audio","codec_name":"aac","channels":1,"sample_rate":"48000","bit_rate":"64000"}],"format":{"duration":"60"}}"#).unwrap()
    }

    fn line(plan: &Plan) -> String {
        plan.steps.iter().map(|s| s.args.join(" ")).collect::<Vec<_>>().join(" || ")
    }

    #[test]
    fn convert_copies_what_is_already_light_enough_and_encodes_the_rest() {
        let p = plan(&AudioTask::Convert { format: Format::M4a, level: Level::Medium }, "/m.m4a", "m4a", &memo(), false, None).unwrap();
        assert!(line(&p).contains("-c:a copy") && line(&p).ends_with("-movflags +faststart out.m4a"), "{}", line(&p));
        // A 320k MP3 made a smaller MP3 is encoded, not copied.
        let p = plan(&AudioTask::Convert { format: Format::Mp3, level: Level::Small }, "/s.mp3", "mp3", &song(), false, None).unwrap();
        assert!(line(&p).contains("libmp3lame -q:a 5") && p.ext == "mp3");
        let p = plan(&AudioTask::Convert { format: Format::Alac, level: Level::High }, "/s.mp3", "mp3", &song(), false, None).unwrap();
        assert!(line(&p).contains("-c:a alac") && p.ext == "m4a");
        let p = plan(&AudioTask::Convert { format: Format::Opus, level: Level::Medium }, "/s.mp3", "mp3", &song(), false, None).unwrap();
        assert!(line(&p).contains("libopus -b:a 128k") && line(&p).ends_with("out.opus"));
        // The cover picture is not carried into a sound file.
        assert!(line(&p).contains("-map 0:a:0 -vn"));
    }

    #[test]
    fn a_cut_without_fades_is_copied_and_with_them_encoded_in_its_own_kind() {
        let p = plan(&AudioTask::Trim { start: 10.0, end: 40.0, fade_in: 0.0, fade_out: 0.0 }, "/s.mp3", "mp3", &song(), false, None).unwrap();
        assert!(line(&p).contains("-ss 10.000 -i file:/s.mp3 -t 30.000") && line(&p).contains("-c:a copy") && line(&p).ends_with("out.mp3"));
        let p = plan(&AudioTask::Trim { start: 10.0, end: 40.0, fade_in: 2.0, fade_out: 99.0 }, "/s.mp3", "mp3", &song(), false, None).unwrap();
        assert!(line(&p).contains("afade=t=in:st=0:d=2.00,afade=t=out:st=15.000:d=15.00") && line(&p).contains("libmp3lame"), "{}", line(&p));
        // WMA cannot be written back: it becomes M4A.
        let mut wma = memo();
        wma.audio.as_mut().unwrap().codec = "wmav2".into();
        let p = plan(&AudioTask::Trim { start: 0.0, end: 5.0, fade_in: 0.0, fade_out: 0.0 }, "/a.wma", "wma", &wma, false, None).unwrap();
        assert!(p.ext == "m4a" && line(&p).contains("-c:a aac"));
        assert_eq!(plan(&AudioTask::Trim { start: 5.0, end: 4.0, fade_in: 0.0, fade_out: 0.0 }, "/a", "mp3", &song(), false, None), Err(PlanError::BadRange));
    }

    #[test]
    fn loudness_to_a_target_takes_what_was_measured() {
        let task = AudioTask::Loudness { mode: Loudness::Podcast, gain: 0.0 };
        assert!(needs_measure(&task));
        let m = measure(&task, "/m.m4a", &memo()).unwrap();
        assert!(line(&m).contains("-loglevel info") && line(&m).contains("loudnorm=I=-16:TP=-1.5:LRA=11:print_format=json -f null -"), "{}", line(&m));
        let said = "[Parsed_loudnorm_0 @ 0x1]\n{\n\t\"input_i\" : \"-27.61\",\n\t\"input_tp\" : \"-4.47\",\n\t\"input_lra\" : \"18.06\",\n\t\"input_thresh\" : \"-39.20\",\n\t\"output_i\" : \"-16.58\",\n\t\"target_offset\" : \"0.58\"\n}\nsize=N/A time=00:01:00.00";
        let measured = parse_measured(said).unwrap();
        assert_eq!(measured, Measured { i: -27.61, tp: -4.47, lra: 18.06, thresh: -39.2, offset: 0.58 });
        let p = plan(&task, "/m.m4a", "m4a", &memo(), false, Some(&measured)).unwrap();
        assert!(line(&p).contains("measured_I=-27.61:measured_TP=-4.47:measured_LRA=18.06:measured_thresh=-39.20:offset=0.58:linear=true"), "{}", line(&p));
        assert!(line(&p).contains("aresample=48000"));
        let silent = Measured { i: f64::NEG_INFINITY, ..measured };
        assert_eq!(plan(&task, "/m.m4a", "m4a", &memo(), false, Some(&silent)), Err(PlanError::Silent));
        assert_eq!(parse_measured("no json here"), None);
    }

    #[test]
    fn loudness_for_a_video_keeps_the_picture() {
        let mut clip = memo();
        clip.video = parse_probe(r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":640,"height":360}],"format":{}}"#).unwrap().video;
        let p = plan(&AudioTask::Loudness { mode: Loudness::Speech, gain: 0.0 }, "/c.mov", "mov", &clip, true, None).unwrap();
        assert!(line(&p).contains("-map 0:v? -map 0:a:0 -c:v copy -af speechnorm") && line(&p).ends_with("out.mov") && p.ext == "mov", "{}", line(&p));
        // The same file in an audio tool makes a sound file.
        let p = plan(&AudioTask::Loudness { mode: Loudness::Speech, gain: 0.0 }, "/c.mov", "mov", &clip, false, None).unwrap();
        assert_eq!(p.ext, "m4a");
    }

    #[test]
    fn gain_is_held_under_full_scale_when_louder() {
        let up = plan(&AudioTask::Loudness { mode: Loudness::Gain, gain: 6.0 }, "/s.mp3", "mp3", &song(), false, None).unwrap();
        assert!(line(&up).contains("volume=6.0dB,alimiter=limit=0.89:level=0"));
        let down = plan(&AudioTask::Loudness { mode: Loudness::Gain, gain: -50.0 }, "/s.mp3", "mp3", &song(), false, None).unwrap();
        assert!(line(&down).contains("-af volume=-30.0dB -c:a"));
        assert_eq!(plan(&AudioTask::Loudness { mode: Loudness::Gain, gain: 0.0 }, "/s.mp3", "mp3", &song(), false, None), Err(PlanError::BadRange));
    }

    #[test]
    fn denoise_silence_speed_and_ringtone() {
        let p = plan(&AudioTask::Denoise { strength: Strength::Strong }, "/m.m4a", "m4a", &memo(), false, None).unwrap();
        assert!(line(&p).contains("highpass=f=80,afftdn=nr=20"));
        let p = plan(&AudioTask::Silence { pause: 1.0 }, "/m.m4a", "m4a", &memo(), false, None).unwrap();
        assert!(line(&p).ends_with("-af silenceremove=stop_periods=-1:stop_duration=1.00:stop_threshold=-40dB -c:a aac -b:a 256k -movflags +faststart out.m4a"), "{}", line(&p));
        let p = plan(&AudioTask::Speed { factor: 1.5 }, "/m.m4a", "m4a", &memo(), false, None).unwrap();
        assert!(line(&p).contains("atempo=1.500") && (p.steps[0].seconds - 40.0).abs() < 0.01);
        assert_eq!(plan(&AudioTask::Speed { factor: 3.0 }, "/m", "m4a", &memo(), false, None), Err(PlanError::BadRange));
        let p = plan(&AudioTask::Ringtone { start: 30.0, end: 60.0 }, "/s.mp3", "mp3", &song(), false, None).unwrap();
        assert!(line(&p).contains("-c:a aac -b:a 192k -movflags +faststart -f ipod out.m4r") && p.ext == "m4r", "{}", line(&p));
        assert_eq!(plan(&AudioTask::Ringtone { start: 0.0, end: 60.0 }, "/s", "mp3", &song(), false, None), Err(PlanError::TooLong));
        let mut quiet = memo();
        quiet.audio = None;
        assert_eq!(plan(&AudioTask::Speed { factor: 1.5 }, "/m", "m4a", &quiet, false, None), Err(PlanError::NoAudio));
    }

    #[test]
    fn files_that_match_are_joined_as_they_are_and_a_crossfade_chains() {
        let a = ("/a.mp3".to_string(), "mp3".to_string(), song());
        let p = plan_merge(&[a.clone(), a.clone()], 0.0).unwrap();
        assert!(line(&p).contains("-f concat -safe 0 -i list.txt -map 0:a -c copy") && p.files.len() == 1 && p.ext == "mp3");
        let b = ("/b.m4a".to_string(), "m4a".to_string(), memo());
        let p = plan_merge(&[a.clone(), b.clone(), a.clone()], 3.0).unwrap();
        let s = line(&p);
        assert!(s.contains("[a0][a1]acrossfade=d=3.00[x1];[x1][a2]acrossfade=d=3.00[out] -map [out]"), "{s}");
        assert!((p.steps[0].seconds - (200.0 + 60.0 + 200.0 - 6.0)).abs() < 0.01);
        let p = plan_merge(&[b.clone(), a], 0.0).unwrap();
        assert!(line(&p).contains("concat=n=2:v=0:a=1[out]") && p.ext == "m4a");
    }

    #[test]
    fn peaks_are_the_loudest_sample_of_each_stretch() {
        let samples: Vec<u8> = [0i16, 100, -16384, 5, 32767, -1, 0, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let p = peaks(&samples, 4);
        assert_eq!(p.len(), 4);
        assert!((p[1] - 0.5).abs() < 0.001 && (p[2] - 0.99997).abs() < 0.001 && p[3] == 0.0);
        assert_eq!(peak_rate(3600.0), 1111);
        assert_eq!(peak_rate(10.0), 4000);
    }

    #[test]
    fn tasks_come_from_the_screen_as_tagged_json() {
        let t: AudioTask = serde_json::from_str(r#"{"tool":"trim","start":1,"end":2,"fadeIn":0.5,"fadeOut":0}"#).unwrap();
        assert_eq!(t, AudioTask::Trim { start: 1.0, end: 2.0, fade_in: 0.5, fade_out: 0.0 });
        let t: AudioTask = serde_json::from_str(r#"{"tool":"loudness","mode":"podcast","gain":0}"#).unwrap();
        assert!(needs_measure(&t));
    }
}
