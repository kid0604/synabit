//! The Utilities screen's way into `crate::utilities`.
//!
//! # Paths
//!
//! The screen never hands this module a path to read or write.
//!
//! - **In:** a file reaches a tool by being picked in a dialog Rust opens
//!   ([`utility_pick_inputs`]) or by its bytes being handed over
//!   ([`utility_stage`]); either way it is copied into the workspace and named
//!   by an id from then on.
//! - **Out:** results stay in the workspace until saved to a path the person
//!   just picked (`app_shell::gate::CHOSEN_ARGS` holds [`utility_save`],
//!   [`utility_save_zip`] and [`utility_save_all`] to that), or into the open
//!   vault's `assets/`.
//!
//! # Long work
//!
//! A run is started with a job id from [`utility_new_job`], so the screen can
//! stop it with [`utility_cancel`] while it is going. Progress arrives as the
//! `utility-progress` event.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::utilities::images::{self, Layout};
use crate::utilities::{crypt, metadata, stamp};
use crate::utilities::pdf::{self, PdfError, Pick};
use crate::utilities::workspace::{clean_name, unused_path, Input, Job, Output, Workspace, KEEP_FOR};

fn workspace(app: &tauri::AppHandle) -> AppResult<Workspace> {
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::General(format!("no cache folder: {e}")))?
        .join("utilities");
    Ok(Workspace::new(root))
}

/// Runs that may be stopped, by job id.
fn stops() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static STOPS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    STOPS.get_or_init(Default::default)
}

/// The flag a run checks between pages. Registered when the job is made
/// (`utility_new_job`), so a Cancel pressed before the run reaches Rust still
/// counts; made here if it was not.
fn stop_flag(job: &str) -> Arc<AtomicBool> {
    stops()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(job.to_string())
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

/// A run's bookkeeping. Made first thing, so a job string that is not an id
/// is refused before any work; and on the way out — a `?`, a panic, success —
/// the stop flag is forgotten and, unless it finished, the half-made job is
/// thrown away.
struct Running {
    job: String,
    root: PathBuf,
    finished: bool,
    flag: Arc<AtomicBool>,
}

impl Running {
    fn start(app: &tauri::AppHandle, job: &str) -> AppResult<Self> {
        if !crate::utilities::workspace::is_id(job) {
            return Err(AppError::InvalidPath(format!("'{job}' is not a job")));
        }
        let root = app
            .path()
            .app_cache_dir()
            .map_err(|e| AppError::General(format!("no cache folder: {e}")))?
            .join("utilities");
        Ok(Self { job: job.to_string(), root, finished: false, flag: stop_flag(job) })
    }

    fn stopped(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// The run's answer, with the job kept only if it is a success.
    fn finish(mut self, result: AppResult<Job>) -> AppResult<Job> {
        self.finished = result.is_ok();
        result
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        forget_stop(&self.job);
        if !self.finished {
            let _ = Workspace::new(self.root.clone()).discard(&self.job);
        }
    }
}

fn forget_stop(job: &str) {
    stops().lock().unwrap_or_else(|e| e.into_inner()).remove(job);
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    job: &'a str,
    done: usize,
    total: usize,
}

/// What went wrong, in words for the person, with a code the screen can act on.
fn pdf_error(name: &str, e: PdfError) -> AppError {
    match e {
        PdfError::Locked => AppError::General(format!("locked: “{name}” is protected by a password")),
        PdfError::Unreadable(why) => AppError::General(format!("unreadable: “{name}” could not be read as a PDF ({why})")),
        PdfError::Cancelled => AppError::General("cancelled".into()),
        other => AppError::General(other.to_string()),
    }
}

/// The dialog filters for the kinds of file a tool takes.
fn filters(kinds: &[String]) -> Vec<crate::app_shell::dialogs::Filter> {
    use crate::app_shell::dialogs::Filter;
    kinds
        .iter()
        .filter_map(|k| match k.as_str() {
            "pdf" => Some(Filter::new("PDF", &["pdf"])),
            "office" => Some(Filter::new(
                "Office",
                &["docx", "doc", "odt", "rtf", "xlsx", "xls", "ods", "csv", "pptx", "ppt", "odp"],
            )),
            "image" => Some(Filter::new(
                "Images",
                &["jpg", "jpeg", "jfif", "png", "heic", "heif", "webp", "gif", "bmp", "tif", "tiff"],
            )),
            _ => None,
        })
        .collect()
}

/// Ask for files to work on, and take a copy of each. Empty when the dialog
/// was closed.
#[tauri::command]
pub async fn utility_pick_inputs(app: tauri::AppHandle, kinds: Vec<String>, multiple: bool) -> AppResult<Vec<Input>> {
    use tauri_plugin_dialog::DialogExt;
    let mut builder = app.dialog().file();
    #[cfg(desktop)]
    if let Some(window) = crate::app_shell::app_window(&app) {
        builder = builder.set_parent(&window);
    }
    for filter in filters(&kinds) {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        builder = builder.add_filter(&filter.name, &extensions);
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    if multiple {
        builder.pick_files(move |picked| {
            let _ = tx.send(picked.unwrap_or_default());
        });
    } else {
        builder.pick_file(move |picked| {
            let _ = tx.send(picked.into_iter().collect());
        });
    }
    let picked = rx.await.unwrap_or_default();
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        picked
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .map(|path| ws.add_file(&path))
            .collect::<AppResult<Vec<_>>>()
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

fn header(request: &tauri::ipc::Request<'_>, name: &str) -> AppResult<String> {
    let raw = request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::General(format!("missing {name}")))?;
    Ok(urlencoding::decode(raw).map_err(|e| AppError::General(e.to_string()))?.into_owned())
}

fn body(request: &tauri::ipc::Request<'_>) -> AppResult<Vec<u8>> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        _ => Err(AppError::General("expected the file's bytes".into())),
    }
}

/// Keep a file the WebView holds the bytes of — dropped on the window, or a
/// picture it made into a JPEG. The name comes in the `x-utility-name`
/// header, percent-encoded.
#[tauri::command]
pub async fn utility_stage(app: tauri::AppHandle, request: tauri::ipc::Request<'_>) -> AppResult<Input> {
    let name = header(&request, "x-utility-name")?;
    let bytes = body(&request)?;
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || ws.add_bytes(&name, &bytes))
        .await
        .map_err(|e| AppError::General(e.to_string()))?
}

/// Start a run. Its id is what [`utility_cancel`] stops.
#[tauri::command]
pub fn utility_new_job(app: tauri::AppHandle, tool: String) -> AppResult<Job> {
    let job = workspace(&app)?.new_job(&tool)?;
    stop_flag(&job.id);
    Ok(job)
}

/// Stop a run. Harmless for one that has finished.
#[tauri::command]
pub fn utility_cancel(job: String) {
    if let Some(flag) = stops().lock().unwrap_or_else(|e| e.into_inner()).get(&job) {
        flag.store(true, Ordering::SeqCst);
    }
}

/// One file a PDF run makes: its name and the pages that go in it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfOutput {
    pub name: String,
    pub picks: Vec<Pick>,
}

/// Make new PDFs from pages of the inputs. `picks[].source` indexes `inputs`.
/// Merge, split, organize and rotate all come here (see `utilities::pdf`).
#[tauri::command]
pub async fn utility_assemble(
    app: tauri::AppHandle,
    job: String,
    inputs: Vec<String>,
    outputs: Vec<PdfOutput>,
) -> AppResult<Job> {
    let run = Running::start(&app, &job)?;
    let ws = workspace(&app)?;
    let flag = run.flag.clone();
    let emitter = app.clone();
    let id = job.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> AppResult<Job> {
        let mut sources = Vec::with_capacity(inputs.len());
        for input in &inputs {
            let path = ws.input_path(input)?;
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            sources.push(pdf::open(&path).map_err(|e| pdf_error(&name, e))?);
        }
        let total: usize = outputs
            .iter()
            .flat_map(|o| &o.picks)
            .map(|p| match &p.pages {
                Some(list) => list.len(),
                None => sources.get(p.source).map(|d| pdf::page_count(d) as usize).unwrap_or(0),
            })
            .sum();
        let mut before = 0;
        for output in &outputs {
            let mut doc = pdf::assemble(&sources, &output.picks, |done, _| {
                let _ = emitter.emit("utility-progress", Progress { job: &id, done: before + done, total });
                !flag.load(Ordering::SeqCst)
            })
            .map_err(|e| pdf_error(&output.name, e))?;
            before += pdf::page_count(&doc) as usize;
            let path = ws.output_path(&id, &output.name)?;
            pdf::save(&mut doc, &path).map_err(|e| pdf_error(&output.name, e))?;
            ws.add_output(&id, &path)?;
        }
        ws.note_origin(&id, &inputs)
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))
    .and_then(|r| r);
    // A failed or stopped run leaves nothing half-made behind (`Running`).
    run.finish(result)
}

/// Make a PDF of the inputs, one page each. Every input must be a JPEG: the
/// screen turns other pictures into one first.
#[tauri::command]
pub async fn utility_images_to_pdf(
    app: tauri::AppHandle,
    job: String,
    inputs: Vec<String>,
    layout: Layout,
    name: String,
) -> AppResult<Job> {
    let run = Running::start(&app, &job)?;
    let ws = workspace(&app)?;
    let flag = run.flag.clone();
    let emitter = app.clone();
    let id = job.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> AppResult<Job> {
        let mut pictures = Vec::with_capacity(inputs.len());
        for input in &inputs {
            let path = ws.input_path(input)?;
            let bytes = std::fs::read(&path)?;
            let jpeg = images::read_jpeg(&bytes).map_err(|e| {
                AppError::General(format!("“{}”: {e}", path.file_name().unwrap_or_default().to_string_lossy()))
            })?;
            pictures.push((jpeg, bytes));
        }
        let mut doc = images::jpegs_to_pdf(&pictures, layout, |done, total| {
            let _ = emitter.emit("utility-progress", Progress { job: &id, done, total });
            !flag.load(Ordering::SeqCst)
        })
        .map_err(|e| pdf_error(&name, e))?;
        let path = ws.output_path(&id, &name)?;
        pdf::save(&mut doc, &path).map_err(|e| pdf_error(&name, e))?;
        ws.add_output(&id, &path)?;
        ws.note_origin(&id, &inputs)
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))
    .and_then(|r| r);
    // A failed or stopped run leaves nothing half-made behind (`Running`).
    run.finish(result)
}

// ── Vision (macOS) ────────────────────────────────────────────────────────

/// What this machine's Vision can do, or nothing off macOS. Asked once: the
/// answer does not change while the app runs.
#[tauri::command]
pub fn utility_vision_status() -> Option<serde_json::Value> {
    #[cfg(target_os = "macos")]
    {
        static ABILITIES: OnceLock<serde_json::Value> = OnceLock::new();
        Some(ABILITIES.get_or_init(|| serde_json::to_value(crate::utilities::vision::abilities()).unwrap_or_default()).clone())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// `W×H` from the `x-utility-size` header.
#[cfg(target_os = "macos")]
fn picture_size(request: &tauri::ipc::Request<'_>) -> AppResult<(u32, u32)> {
    let size = header(request, "x-utility-size")?;
    let (w, h) = size.split_once('x').ok_or_else(|| AppError::General("bad size".into()))?;
    Ok((w.parse().map_err(|_| AppError::General("bad width".into()))?, h.parse().map_err(|_| AppError::General("bad height".into()))?))
}

#[cfg(target_os = "macos")]
fn vision_error(e: crate::utilities::vision::VisionError) -> AppError {
    use crate::utilities::vision::VisionError;
    match e {
        VisionError::NoSubject => AppError::General("no-subject".into()),
        VisionError::Unavailable => AppError::UnsupportedCapability("vision".into()),
        other => AppError::General(other.to_string()),
    }
}

/// The subject of a picture: its RGBA pixels in, one mask byte a pixel out.
#[tauri::command]
pub async fn utility_subject_mask(request: tauri::ipc::Request<'_>) -> AppResult<tauri::ipc::Response> {
    #[cfg(target_os = "macos")]
    {
        let (w, h) = picture_size(&request)?;
        let rgba = body(&request)?;
        let mask = tauri::async_runtime::spawn_blocking(move || crate::utilities::vision::subject_mask(w, h, rgba))
            .await
            .map_err(|e| AppError::General(e.to_string()))?
            .map_err(vision_error)?;
        Ok(tauri::ipc::Response::new(mask))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        Err(AppError::UnsupportedCapability("vision".into()))
    }
}

/// The text in a picture: RGBA in; lines out, in reading order. Languages,
/// most likely first, in the `x-utility-languages` header, comma-separated.
#[tauri::command]
pub async fn utility_recognize(request: tauri::ipc::Request<'_>) -> AppResult<Vec<crate::utilities::searchable::Line>> {
    #[cfg(target_os = "macos")]
    {
        let (w, h) = picture_size(&request)?;
        let languages: Vec<String> = header(&request, "x-utility-languages")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        let rgba = body(&request)?;
        tauri::async_runtime::spawn_blocking(move || crate::utilities::vision::recognize_text(w, h, rgba, &languages))
            .await
            .map_err(|e| AppError::General(e.to_string()))?
            .map_err(vision_error)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        Err(AppError::UnsupportedCapability("vision".into()))
    }
}

/// A file to make searchable, and the text found on its pages.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchableFile {
    pub input: String,
    pub name: String,
    pub pages: Vec<crate::utilities::searchable::PageLines>,
}

/// Lay the text found on each page over it, invisible, so the scan can be
/// searched and copied from.
#[tauri::command]
pub async fn utility_searchable(app: tauri::AppHandle, job: String, files: Vec<SearchableFile>) -> AppResult<Job> {
    let id = job.clone();
    let pages: HashMap<String, Vec<crate::utilities::searchable::PageLines>> =
        files.iter().map(|f| (f.input.clone(), f.pages.clone())).collect();
    let per: Vec<PerFile> = files.into_iter().map(|f| PerFile { input: f.input, name: f.name }).collect();
    per_file(
        &app,
        job,
        per,
        Box::new(move |ws, file, _| {
            let lines = pages.get(&file.input).cloned().unwrap_or_default();
            pdf_step(ws, &id, file, "", |doc| crate::utilities::searchable::add_text_layer(doc, &lines))
        }),
    )
    .await
}

// ── Compress PDF ──────────────────────────────────────────────────────────

/// What can be shrunk in a PDF, and the longest side of its largest page in
/// points, for working out how many pixels a picture needs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfPictures {
    pub pictures: Vec<crate::utilities::compress::Picture>,
    pub page_long_side: f32,
}

#[tauri::command]
pub async fn utility_pdf_pictures(app: tauri::AppHandle, input: String) -> AppResult<PdfPictures> {
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = ws.input_path(&input)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let doc = pdf::open(&path).map_err(|e| pdf_error(&name, e))?;
        let page_long_side = doc
            .get_pages()
            .values()
            .filter_map(|id| doc.get_dictionary(*id).ok())
            .filter_map(|d| d.get(b"MediaBox").and_then(lopdf::Object::as_array).ok().cloned())
            .filter_map(|b| {
                let n: Vec<f32> = b.iter().filter_map(|o| o.as_float().ok()).collect();
                (n.len() == 4).then(|| (n[2] - n[0]).abs().max((n[3] - n[1]).abs()))
            })
            .fold(842.0f32, f32::max);
        Ok(PdfPictures { pictures: crate::utilities::compress::pictures(&doc), page_long_side })
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

/// One picture's bytes, as `compress::picture_bytes` gives them.
#[tauri::command]
pub async fn utility_pdf_picture(app: tauri::AppHandle, input: String, id: String) -> AppResult<tauri::ipc::Response> {
    let ws = workspace(&app)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<u8>> {
        let path = ws.input_path(&input)?;
        let doc = pdf::open(&path).map_err(|e| pdf_error("", e))?;
        let object = crate::utilities::compress::parse_id(&id).ok_or_else(|| AppError::General("bad picture id".into()))?;
        crate::utilities::compress::picture_bytes(&doc, object).map_err(|e| pdf_error("", e))
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// A smaller JPEG for one picture, kept until [`utility_compress`] rebuilds the
/// PDF. Headers: `x-utility-job`, `x-utility-input`, `x-utility-id` ("12 0").
#[tauri::command]
pub async fn utility_compress_put(app: tauri::AppHandle, request: tauri::ipc::Request<'_>) -> AppResult<()> {
    let job = header(&request, "x-utility-job")?;
    let input = header(&request, "x-utility-input")?;
    let id = header(&request, "x-utility-id")?;
    let object = crate::utilities::compress::parse_id(&id).ok_or_else(|| AppError::General("bad picture id".into()))?;
    let bytes = body(&request)?;
    let dir = workspace(&app)?.pending_dir(&job, &input)?;
    std::fs::write(dir.join(format!("{}_{}.jpg", object.0, object.1)), bytes)?;
    Ok(())
}

/// Rebuild each PDF with the pictures put for it. A file that comes out no
/// smaller is left out; if none does, the run says so.
#[tauri::command]
pub async fn utility_compress(app: tauri::AppHandle, job: String, files: Vec<PerFile>) -> AppResult<Job> {
    let id = job.clone();
    per_file(
        &app,
        job,
        files,
        Box::new(move |ws, file, _| {
            let path = ws.input_path(&file.input)?;
            let before = std::fs::metadata(&path)?.len();
            let pending = ws.pending_dir(&id, &file.input)?;
            let out = pdf_step(ws, &id, file, "", |doc| {
                for entry in std::fs::read_dir(&pending).map_err(|e| PdfError::Write(e.to_string()))?.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let Some((a, b)) = name.trim_end_matches(".jpg").split_once('_') else { continue };
                    let (Ok(a), Ok(b)) = (a.parse(), b.parse()) else { continue };
                    let jpeg = std::fs::read(entry.path()).map_err(|e| PdfError::Write(e.to_string()))?;
                    crate::utilities::compress::replace(doc, (a, b), jpeg)?;
                }
                // Deflate whatever was stored plain, and write without the
                // incremental leftovers a file collects.
                doc.compress();
                Ok(())
            })?;
            let _ = std::fs::remove_dir_all(&pending);
            let after = std::fs::metadata(&out)?.len();
            if after >= before {
                let _ = std::fs::remove_file(&out);
                return Err(AppError::General(format!("{LEFT_OUT}: “{}”", path.file_name().unwrap_or_default().to_string_lossy())));
            }
            Ok(out)
        }),
    )
    .await
}

// ── Office and Word, in a room (macOS) ────────────────────────────────────

/// What the room-backed tools need, and whether it is here.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OfficeStatus {
    /// macOS 14 or later, where a room can be had.
    pub room: bool,
    pub libreoffice: bool,
    pub pdf2docx: bool,
}

#[cfg(target_os = "macos")]
fn machine(app: &tauri::AppHandle) -> AppResult<Arc<crate::syn::machine::rooms::Rooms>> {
    app.try_state::<Arc<crate::syn::machine::rooms::Rooms>>()
        .map(|r| r.inner().clone())
        .ok_or_else(|| AppError::UnsupportedCapability("room".into()))
}

/// Where the id of the pdf2docx environment is kept.
#[cfg(target_os = "macos")]
fn pdf2docx_id_file(rooms: &crate::syn::machine::rooms::Rooms) -> PathBuf {
    rooms.root().join("utilities-pdf2docx")
}

#[cfg(target_os = "macos")]
fn pdf2docx_bin(rooms: &crate::syn::machine::rooms::Rooms) -> Option<PathBuf> {
    let id = std::fs::read_to_string(pdf2docx_id_file(rooms)).ok()?;
    crate::utilities::room::pdf2docx_env(rooms.root(), Some(id.trim()))
}

#[tauri::command]
pub fn utility_office_status(app: tauri::AppHandle) -> OfficeStatus {
    #[cfg(target_os = "macos")]
    {
        let major = tauri_plugin_os::version().to_string().split('.').next().and_then(|m| m.parse::<u32>().ok()).unwrap_or(0);
        let rooms = machine(&app).ok();
        OfficeStatus {
            room: major >= 14 && rooms.is_some(),
            libreoffice: crate::utilities::room::libreoffice().is_some(),
            pdf2docx: rooms.as_deref().and_then(pdf2docx_bin).is_some(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        OfficeStatus::default()
    }
}

#[cfg(target_os = "macos")]
fn room_error(name: &str, e: crate::utilities::room::RoomError) -> AppError {
    use crate::utilities::room::RoomError;
    match e {
        RoomError::Missing(what) => AppError::General(format!("missing-{what}")),
        RoomError::Unavailable(why) => AppError::General(format!("no-room: {why}")),
        RoomError::Failed(said) => {
            log::warn!("[utilities] conversion of {name} failed: {said}");
            AppError::General(format!("unreadable: “{name}” could not be converted"))
        }
        RoomError::Io(e) => AppError::General(e),
    }
}

/// A room's name for one file of one run.
/// `job` is an id (`Running::start` checked), so slicing it is safe.
#[cfg(target_os = "macos")]
fn room_name(job: &str, index: usize) -> String {
    format!("u-{}-{index}", &job[..16])
}

/// Each Word, Excel, PowerPoint or OpenDocument file as a PDF, by LibreOffice
/// in a sealed room.
#[tauri::command]
pub async fn utility_office_to_pdf(app: tauri::AppHandle, job: String, files: Vec<PerFile>) -> AppResult<Job> {
    #[cfg(target_os = "macos")]
    {
        let rooms = machine(&app)?;
        let run = Running::start(&app, &job)?;
        let ws = workspace(&app)?;
        let total = files.len();
        let mut failure = None;
        for (i, file) in files.iter().enumerate() {
            if run.stopped() {
                failure = Some(AppError::General("cancelled".into()));
                break;
            }
            let _ = app.emit("utility-progress", Progress { job: &job, done: i, total });
            let path = ws.input_path(&file.input)?;
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            match crate::utilities::room::office_to_pdf(&rooms, &room_name(&job, i), &path).await {
                Ok(pdf) => {
                    let out = ws.output_path(&job, &file.name)?;
                    std::fs::write(&out, pdf)?;
                    ws.add_output(&job, &out)?;
                }
                Err(e) => {
                    failure = Some(room_error(&name, e));
                    break;
                }
            }
        }
        if let Some(e) = failure {
            return run.finish(Err(e));
        }
        let _ = app.emit("utility-progress", Progress { job: &job, done: total, total });
        let inputs: Vec<String> = files.iter().map(|f| f.input.clone()).collect();
        run.finish(ws.note_origin(&job, &inputs))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, job, files);
        Err(AppError::UnsupportedCapability("room".into()))
    }
}

/// Install pdf2docx, once: through the room that reaches the package registry
/// and nothing else. Minutes, the first time.
#[tauri::command]
pub async fn utility_install_pdf2docx(app: tauri::AppHandle) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        let rooms = machine(&app)?;
        let root = rooms.root().to_path_buf();
        let id = crate::utilities::room::install_pdf2docx(&rooms, &root, "utilities-install")
            .await
            .map_err(|e| AppError::General(format!("install-failed: {e}")))?;
        std::fs::write(pdf2docx_id_file(&rooms), id)?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err(AppError::UnsupportedCapability("room".into()))
    }
}

/// Each PDF as a Word file, by pdf2docx in a sealed room.
#[tauri::command]
pub async fn utility_pdf_to_word(app: tauri::AppHandle, job: String, files: Vec<PerFile>) -> AppResult<Job> {
    #[cfg(target_os = "macos")]
    {
        let rooms = machine(&app)?;
        let bin = pdf2docx_bin(&rooms).ok_or_else(|| AppError::General("missing-pdf2docx".into()))?;
        let run = Running::start(&app, &job)?;
        let ws = workspace(&app)?;
        let total = files.len();
        let mut failure = None;
        for (i, file) in files.iter().enumerate() {
            if run.stopped() {
                failure = Some(AppError::General("cancelled".into()));
                break;
            }
            let _ = app.emit("utility-progress", Progress { job: &job, done: i, total });
            let path = ws.input_path(&file.input)?;
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            // A PDF that needs a password, or does not open, is said as such
            // before a room is spent on it.
            if let Err(e) = pdf::open(&path) {
                failure = Some(pdf_error(&name, e));
                break;
            }
            match crate::utilities::room::pdf_to_docx(&rooms, &room_name(&job, i), &bin, &path).await {
                Ok(docx) => {
                    let out = ws.output_path(&job, &file.name)?;
                    std::fs::write(&out, docx)?;
                    ws.add_output(&job, &out)?;
                }
                Err(e) => {
                    failure = Some(room_error(&name, e));
                    break;
                }
            }
        }
        if let Some(e) = failure {
            return run.finish(Err(e));
        }
        let _ = app.emit("utility-progress", Progress { job: &job, done: total, total });
        let inputs: Vec<String> = files.iter().map(|f| f.input.clone()).collect();
        run.finish(ws.note_origin(&job, &inputs))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, job, files);
        Err(AppError::UnsupportedCapability("room".into()))
    }
}

// ── One output per input ──────────────────────────────────────────────────

/// One input and the name its result gets.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerFile {
    pub input: String,
    pub name: String,
}

/// What a work function's error starts with when it could not improve a file
/// and leaves it out rather than failing the run.
const LEFT_OUT: &str = "not-smaller";

/// The work of a run that makes one file from each input, given the
/// workspace and a progress callback that answers `false` once stopped.
type PerFileWork = dyn FnMut(&Workspace, &PerFile, &mut dyn FnMut(usize, usize) -> bool) -> AppResult<PathBuf> + Send;

/// Run `work` over each file in its own thread, with stopping, progress by
/// file, and nothing left behind if it fails.
async fn per_file(app: &tauri::AppHandle, job: String, files: Vec<PerFile>, mut work: Box<PerFileWork>) -> AppResult<Job> {
    let run = Running::start(app, &job)?;
    let ws = workspace(app)?;
    let flag = run.flag.clone();
    let emitter = app.clone();
    let id = job.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> AppResult<Job> {
        let total = files.len();
        // The first file left out, for the message if every file is.
        let mut left_out: Option<String> = None;
        for (i, file) in files.iter().enumerate() {
            // In hundredths of a file, so a long file's pages move the bar.
            let mut progress = |done: usize, of: usize| {
                let within = if of == 0 { 0 } else { done.min(of) * 100 / of };
                let _ = emitter.emit("utility-progress", Progress { job: &id, done: i * 100 + within, total: total * 100 });
                !flag.load(Ordering::SeqCst)
            };
            if !progress(0, 0) {
                return Err(AppError::General("cancelled".into()));
            }
            match work(&ws, file, &mut progress) {
                Ok(path) => {
                    ws.add_output(&id, &path)?;
                }
                // A file a tool could not improve is left out, not the reason
                // the others are thrown away (`utility_compress`).
                Err(AppError::General(why)) if why.starts_with(LEFT_OUT) => {
                    left_out.get_or_insert(why);
                }
                Err(e) => return Err(e),
            }
        }
        if let Some(why) = left_out {
            if ws.job(&id)?.outputs.is_empty() {
                return Err(AppError::General(why));
            }
        }
        let _ = emitter.emit("utility-progress", Progress { job: &id, done: total * 100, total: total * 100 });
        let inputs: Vec<String> = files.iter().map(|f| f.input.clone()).collect();
        ws.note_origin(&id, &inputs)
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))
    .and_then(|r| r);
    run.finish(result)
}

/// Open the input as a PDF, change it, and save it under the file's name.
fn pdf_step(
    ws: &Workspace,
    job: &str,
    file: &PerFile,
    password: &str,
    change: impl FnOnce(&mut lopdf::Document) -> Result<(), PdfError>,
) -> AppResult<PathBuf> {
    let path = ws.input_path(&file.input)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let mut doc = pdf::open_with(&path, password).map_err(|e| pdf_error(&name, e))?;
    change(&mut doc).map_err(|e| pdf_error(&name, e))?;
    let out = ws.output_path(job, &file.name)?;
    pdf::save(&mut doc, &out).map_err(|e| pdf_error(&file.name, e))?;
    Ok(out)
}

/// What a watermark run needs besides the pixels, which come as the body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatermarkArgs {
    job: String,
    files: Vec<PerFile>,
    width: u32,
    height: u32,
    layout: stamp::WatermarkLayout,
    /// 1-based pages; empty for all.
    #[serde(default)]
    pages: Vec<u32>,
    #[serde(default)]
    except_first: bool,
}

/// Stamp a picture the WebView drew — text or a logo — on the pages of each
/// input. The pixels are the body (RGBA); the rest is the `x-utility-args`
/// header, percent-encoded JSON.
#[tauri::command]
pub async fn utility_watermark(app: tauri::AppHandle, request: tauri::ipc::Request<'_>) -> AppResult<Job> {
    let args: WatermarkArgs = serde_json::from_str(&header(&request, "x-utility-args")?)?;
    let picture = stamp::Picture { width: args.width, height: args.height, rgba: body(&request)? };
    let job = args.job.clone();
    let (layout, pages, except_first) = (args.layout, args.pages, args.except_first);
    per_file(
        &app,
        args.job,
        args.files,
        Box::new(move |ws, file, progress| {
            pdf_step(ws, &job, file, "", |doc| {
                let pages = if except_first && pages.is_empty() {
                    (2..=pdf::page_count(doc)).collect()
                } else {
                    pages.clone()
                };
                if except_first && pages.is_empty() {
                    return Ok(());
                }
                stamp::watermark(doc, &picture, &layout, &pages, &mut *progress)
            })
        }),
    )
    .await
}

/// Number the pages of each input.
#[tauri::command]
pub async fn utility_number_pages(app: tauri::AppHandle, job: String, files: Vec<PerFile>, numbering: stamp::Numbering) -> AppResult<Job> {
    let id = job.clone();
    per_file(
        &app,
        job,
        files,
        Box::new(move |ws, file, progress| pdf_step(ws, &id, file, "", |doc| stamp::number_pages(doc, &numbering, &mut *progress))),
    )
    .await
}

/// Give each input a password to open it.
#[tauri::command]
pub async fn utility_protect(app: tauri::AppHandle, job: String, files: Vec<PerFile>, password: String) -> AppResult<Job> {
    if password.is_empty() {
        return Err(AppError::General("a password is needed".into()));
    }
    let id = job.clone();
    per_file(
        &app,
        job,
        files,
        Box::new(move |ws, file, _| {
            pdf_step(ws, &id, file, "", |doc| {
                crypt::protect(doc, &password).map_err(|e| PdfError::Write(e.to_string()))
            })
        }),
    )
    .await
}

/// Take the password off each input, with the password (or none, for a file
/// that only restricts printing and copying).
#[tauri::command]
pub async fn utility_unlock(app: tauri::AppHandle, job: String, files: Vec<PerFile>, password: String) -> AppResult<Job> {
    let id = job.clone();
    per_file(&app, job, files, Box::new(move |ws, file, _| pdf_step(ws, &id, file, &password, |_| Ok(())))).await
}

/// Take EXIF, XMP, IPTC and comments out of JPEGs and PNGs, without
/// re-encoding them. Other formats are the WebView's to redraw.
#[tauri::command]
pub async fn utility_strip_metadata(app: tauri::AppHandle, job: String, files: Vec<PerFile>) -> AppResult<Job> {
    let id = job.clone();
    per_file(
        &app,
        job,
        files,
        Box::new(move |ws, file, _| {
            let path = ws.input_path(&file.input)?;
            let (clean, _) = metadata::strip(&std::fs::read(&path)?).map_err(|e| {
                AppError::General(format!("“{}”: {e}", path.file_name().unwrap_or_default().to_string_lossy()))
            })?;
            let out = ws.output_path(&id, &file.name)?;
            std::fs::write(&out, clean)?;
            Ok(out)
        }),
    )
    .await
}

/// Whether each input asks for a password to open.
#[tauri::command]
pub async fn utility_lock_status(app: tauri::AppHandle, inputs: Vec<String>) -> AppResult<Vec<crypt::Lock>> {
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        inputs
            .iter()
            .map(|i| {
                let path = ws.input_path(i)?;
                crypt::lock_of(&path).map_err(|e| AppError::General(e.to_string()))
            })
            .collect()
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

/// Take copies of files Files shows — in the vault or a folder added as a
/// source, and nowhere else.
#[tauri::command]
pub async fn utility_add_from_vault(
    app: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    paths: Vec<String>,
) -> AppResult<Vec<Input>> {
    let roots = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        crate::commands::files::allowed_roots(&db, &vault_path)
    };
    let vault = canonical(Path::new(&vault_path));
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let refs: Vec<&str> = roots.iter().map(String::as_str).collect();
        paths
            .iter()
            .map(|p| {
                let path = Path::new(p);
                crate::path_utils::enforce_within_roots(path, &refs)?;
                if !path.is_file() {
                    return Err(AppError::InvalidPath(format!("“{p}” is not a file")));
                }
                // Only a file inside the vault gets "save beside": a source
                // folder is the person's own, and Utilities does not write there.
                // Copied from the resolved path that was checked, so a link
                // swapped in afterwards is not followed somewhere else.
                let real = canonical(path);
                crate::path_utils::enforce_within_roots(&real, &refs)?;
                if real.starts_with(&vault) { ws.add_vault_file(&real) } else { ws.add_file(&real) }
            })
            .collect()
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Save an output into the vault folder its first input came from, and index
/// it. Returns its path inside the vault.
#[tauri::command]
pub async fn utility_save_beside(
    app: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    job: String,
    output: String,
    name: String,
) -> AppResult<String> {
    let ws = workspace(&app)?;
    let record = ws.job(&job)?;
    let Some(dir) = record.origin_dir else {
        return Err(AppError::InvalidPath("This result did not come from the vault".into()));
    };
    let vault = canonical(Path::new(&vault_path));
    let dir = canonical(Path::new(&dir));
    if !dir.starts_with(&vault) || !dir.is_dir() {
        return Err(AppError::InvalidPath("The folder the original was in is not in the open vault any more".into()));
    }
    let from = ws.output_file(&job, &output)?;
    let to = unused_path(&dir, &clean_name(&name));
    copy_out(&from, &to)?;
    ws.mark_saved(&job, &to)?;
    crate::commands::files::index_written_file(state.inner(), &to)?;
    Ok(to.strip_prefix(&vault).unwrap_or(&to).to_string_lossy().into_owned())
}

/// Add a file the WebView made to a run — a page drawn as a picture. The job
/// and the name come in the `x-utility-job` and `x-utility-name` headers.
#[tauri::command]
pub async fn utility_put_output(app: tauri::AppHandle, request: tauri::ipc::Request<'_>) -> AppResult<Output> {
    let job = header(&request, "x-utility-job")?;
    let name = header(&request, "x-utility-name")?;
    let bytes = body(&request)?;
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = ws.output_path(&job, &name)?;
        std::fs::write(&path, &bytes)?;
        ws.add_output(&job, &path)
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

/// A run's record as it stands.
#[tauri::command]
pub fn utility_job(app: tauri::AppHandle, job: String) -> AppResult<Job> {
    workspace(&app)?.job(&job)
}

/// Results waiting to be saved, newest first. Clears out what is past keeping.
#[tauri::command]
pub async fn utility_results(app: tauri::AppHandle) -> AppResult<Vec<Job>> {
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        ws.sweep(KEEP_FOR);
        ws.unsaved()
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))
}

/// Throw a result away.
#[tauri::command]
pub fn utility_discard(app: tauri::AppHandle, job: String) -> AppResult<()> {
    forget_stop(&job);
    workspace(&app)?.discard(&job)
}

fn copy_out(from: &Path, to: &Path) -> AppResult<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(from, to)?;
    Ok(())
}

/// Save one output where the person just chose (`gate::CHOSEN_ARGS`).
#[tauri::command]
pub async fn utility_save(app: tauri::AppHandle, job: String, output: String, destination: String) -> AppResult<Job> {
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let from = ws.output_file(&job, &output)?;
        let to = PathBuf::from(&destination);
        copy_out(&from, &to)?;
        ws.mark_saved(&job, &to)
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

/// Save every output into a folder the person just chose, next to whatever
/// is there already (`gate::CHOSEN_ARGS`).
#[tauri::command]
pub async fn utility_save_all(app: tauri::AppHandle, job: String, folder: String) -> AppResult<Job> {
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let record = ws.job(&job)?;
        let folder = PathBuf::from(&folder);
        let mut last = folder.clone();
        for output in &record.outputs {
            let from = ws.output_file(&job, &output.name)?;
            last = unused_path(&folder, &clean_name(&output.name));
            copy_out(&from, &last)?;
        }
        ws.mark_saved(&job, &last)
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

/// Save every output in one .zip where the person just chose
/// (`gate::CHOSEN_ARGS`).
#[tauri::command]
pub async fn utility_save_zip(app: tauri::AppHandle, job: String, destination: String) -> AppResult<Job> {
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let record = ws.job(&job)?;
        // Every output is there before the chosen file is touched: a failure
        // must not leave an empty .zip where the person said to save.
        let froms: Vec<(String, PathBuf)> =
            record.outputs.iter().map(|o| ws.output_file(&job, &o.name).map(|p| (o.name.clone(), p))).collect::<AppResult<_>>()?;
        let to = PathBuf::from(&destination);
        let file = std::fs::File::create(&to)?;
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
        // Stored, not deflated: PDFs and pictures are compressed already.
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, from) in &froms {
            zip.start_file(clean_name(name), options).map_err(|e| AppError::General(e.to_string()))?;
            std::io::copy(&mut std::fs::File::open(from)?, &mut zip)?;
        }
        zip.finish().map_err(|e| AppError::General(e.to_string()))?;
        ws.mark_saved(&job, &to)
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

/// Put an output into the open vault's `assets/`, under `name`, and index it
/// so Files shows it at once. Returns the path inside the vault.
#[tauri::command]
pub async fn utility_save_to_vault(
    app: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    job: String,
    output: String,
    name: String,
) -> AppResult<String> {
    let ws = workspace(&app)?;
    let from = ws.output_file(&job, &output)?;
    let assets = Path::new(&vault_path).join("assets");
    std::fs::create_dir_all(&assets)?;
    let to = unused_path(&assets, &clean_name(&name));
    copy_out(&from, &to)?;
    ws.mark_saved(&job, &to)?;
    crate::commands::files::index_written_file(state.inner(), &to)?;
    Ok(to.strip_prefix(&vault_path).unwrap_or(&to).to_string_lossy().into_owned())
}

/// Hand a result to the next tool without saving it first: it becomes an
/// input like any other, a copy of its own.
#[tauri::command]
pub async fn utility_input_from_output(app: tauri::AppHandle, job: String, output: String) -> AppResult<Input> {
    let ws = workspace(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let from = ws.output_file(&job, &output)?;
        ws.add_file(&from)
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))?
}

/// Open an output in the app the system uses for its kind.
#[cfg(desktop)]
#[tauri::command]
pub fn utility_open(app: tauri::AppHandle, job: String, output: String) -> AppResult<()> {
    // Only the kinds of file a tool makes. Outputs can be written by the page
    // (`utility_put_output`), and opening a `.command` or `.app` would run it.
    const OPENS: &[&str] = &["pdf", "docx", "txt", "jpg", "jpeg", "png", "webp"];
    let ext = Path::new(&output).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !OPENS.contains(&ext.as_str()) {
        return Err(AppError::InvalidPath(format!("'{output}' is not a kind of file Utilities opens")));
    }
    let path = workspace(&app)?.output_file(&job, &output)?;
    opener::open(&path).map_err(|e| AppError::General(e.to_string()))
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn utility_open(_app: tauri::AppHandle, _job: String, _output: String) -> AppResult<()> {
    Err(AppError::UnsupportedCapability("Opening files is not available here".into()))
}

/// Show where a result was last saved, in the system's file manager. The
/// place comes from the job's own record, which only Rust writes.
#[cfg(desktop)]
#[tauri::command]
pub fn utility_reveal_saved(app: tauri::AppHandle, job: String) -> AppResult<()> {
    let record = workspace(&app)?.job(&job)?;
    let Some(saved) = record.saved_to else {
        return Err(AppError::InvalidPath("This result has not been saved".into()));
    };
    // As `files::reveal_in_file_manager` does it; `opener::reveal` would
    // bring D-Bus into the build for Linux.
    use std::process::Command;
    #[cfg(target_os = "macos")]
    Command::new("open").args(["-R", &saved]).spawn()?;
    #[cfg(target_os = "windows")]
    Command::new("explorer").args(["/select,", &saved]).spawn()?;
    #[cfg(target_os = "linux")]
    Command::new("xdg-open").arg(Path::new(&saved).parent().unwrap_or(Path::new("/"))).spawn()?;
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn utility_reveal_saved(_app: tauri::AppHandle, _job: String) -> AppResult<()> {
    Err(AppError::UnsupportedCapability("There is no file manager here".into()))
}
