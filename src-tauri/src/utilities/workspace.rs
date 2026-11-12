//! Where Utilities keeps files while it works on them.
//!
//! Everything lives under the app's cache folder, `utilities/`, in two places:
//!
//! - `in/<id>/<name>`: a copy of each file the person gave a tool. The screen
//!   names inputs by `id`, never by path, so a page cannot point a tool at a
//!   file nobody handed it. Copying costs nothing on APFS: `std::fs::copy`
//!   clones the file there, so a 300 MB PDF is not written twice.
//! - `jobs/<id>/`: what a run made, with `job.json` beside it saying which tool
//!   made it, when, and whether it was saved. Nothing reaches the person's
//!   files until they choose where (`commands::utilities`).
//!
//! Not in the vault, so sync never carries it, and the cache folder is already
//! in the asset protocol's scope (`tauri.conf.json`), which is how the WebView
//! shows a page or a picture from here. Anything older than [`KEEP_FOR`] is
//! removed by [`Workspace::sweep`].

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// How long a result waits to be saved. The screen says so beside each one.
pub const KEEP_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// A file handed to a tool.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    pub id: String,
    pub name: String,
    pub size: u64,
    /// Where the copy is, for the WebView to read through the asset protocol.
    pub path: String,
}

/// A file a run made.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub name: String,
    pub size: u64,
    pub path: String,
}

/// A run and what it made.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub tool: String,
    pub created_ms: u64,
    pub outputs: Vec<Output>,
    /// Saved somewhere at least once; the sidebar lists only those that are not.
    pub saved: bool,
    /// Where it was last saved to, for "Show in Finder". Written by Rust only.
    #[serde(default)]
    pub saved_to: Option<String>,
    /// The vault folder the first input came from, when it came from the
    /// vault: where "Save beside the original" puts the result.
    #[serde(default)]
    pub origin_dir: Option<String>,
}

pub struct Workspace {
    root: PathBuf,
}

/// A fresh id: 32 lowercase hex digits.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Whether an id is one [`new_id`] could have made, so it can be put in a path.
pub fn is_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A file name that is only a file name: no folders, no control characters,
/// nothing a file system refuses, not too long, never empty.
pub fn clean_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    let mut cleaned: String = last
        .chars()
        .map(|c| if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    cleaned = cleaned.trim().trim_start_matches('.').to_string();
    // At most 200 bytes, keeping the extension: file systems count bytes
    // (255 on APFS and ext4), and a Vietnamese letter can be three or more,
    // with room left for a " (2)" when a name is taken.
    const MOST: usize = 200;
    if cleaned.len() > MOST {
        let (stem, ext) = match cleaned.rfind('.') {
            Some(i) if cleaned.len() - i <= 10 => (cleaned[..i].to_string(), cleaned[i..].to_string()),
            _ => (cleaned.clone(), String::new()),
        };
        let mut end = MOST.saturating_sub(ext.len()).min(stem.len());
        while !stem.is_char_boundary(end) {
            end -= 1;
        }
        cleaned = stem[..end].to_string() + ext.as_str();
    }
    if cleaned.is_empty() { "file".into() } else { cleaned }
}

/// `name` in `dir`, or `name (2)`, `name (3)`… if that is taken.
pub fn unused_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn inputs(&self) -> PathBuf {
        self.root.join("in")
    }

    fn jobs(&self) -> PathBuf {
        self.root.join("jobs")
    }

    fn input_dir(&self, id: &str) -> AppResult<PathBuf> {
        if !is_id(id) {
            return Err(AppError::InvalidPath(format!("'{id}' is not an input")));
        }
        Ok(self.inputs().join(id))
    }

    fn job_dir(&self, id: &str) -> AppResult<PathBuf> {
        if !is_id(id) {
            return Err(AppError::InvalidPath(format!("'{id}' is not a job")));
        }
        Ok(self.jobs().join(id))
    }

    /// Take a copy of a file the person picked.
    pub fn add_file(&self, source: &Path) -> AppResult<Input> {
        let name = clean_name(&source.file_name().unwrap_or_default().to_string_lossy());
        let id = new_id();
        let dir = self.input_dir(&id)?;
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join(&name);
        std::fs::copy(source, &dest)?;
        Ok(Input { id, name, size: file_size(&dest), path: dest.to_string_lossy().into_owned() })
    }

    /// Take a copy of a file in the vault, remembering which folder it is in.
    pub fn add_vault_file(&self, source: &Path) -> AppResult<Input> {
        let input = self.add_file(source)?;
        if let Some(dir) = source.parent() {
            std::fs::write(self.inputs().join(format!("{}.origin", input.id)), dir.to_string_lossy().as_bytes())?;
        }
        Ok(input)
    }

    /// The vault folder an input came from, if it came from one.
    pub fn origin_of(&self, id: &str) -> Option<PathBuf> {
        if !is_id(id) {
            return None;
        }
        std::fs::read_to_string(self.inputs().join(format!("{id}.origin"))).ok().map(PathBuf::from)
    }

    /// Record on a job where its first vault input came from.
    pub fn note_origin(&self, job: &str, inputs: &[String]) -> AppResult<Job> {
        let mut record = self.job(job)?;
        if record.origin_dir.is_none() {
            record.origin_dir = inputs.iter().find_map(|i| self.origin_of(i)).map(|p| p.to_string_lossy().into_owned());
            self.write_job(&record)?;
        }
        Ok(record)
    }

    /// Keep bytes the WebView holds: a file dropped on the window, or a
    /// picture it turned into a JPEG.
    pub fn add_bytes(&self, name: &str, bytes: &[u8]) -> AppResult<Input> {
        let name = clean_name(name);
        let id = new_id();
        let dir = self.input_dir(&id)?;
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join(&name);
        std::fs::write(&dest, bytes)?;
        Ok(Input { id, name, size: bytes.len() as u64, path: dest.to_string_lossy().into_owned() })
    }

    /// The file behind an input id.
    pub fn input_path(&self, id: &str) -> AppResult<PathBuf> {
        let dir = self.input_dir(id)?;
        let mut files = std::fs::read_dir(&dir)
            .map_err(|_| AppError::InvalidPath("That file is no longer here. Add it again.".into()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file());
        files.next().ok_or_else(|| AppError::InvalidPath("That file is no longer here. Add it again.".into()))
    }

    /// Start a run: a folder for its outputs and its record.
    pub fn new_job(&self, tool: &str) -> AppResult<Job> {
        let id = new_id();
        std::fs::create_dir_all(self.job_dir(&id)?)?;
        let job = Job {
            id,
            tool: tool.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(32).collect(),
            created_ms: now_ms(),
            outputs: Vec::new(),
            saved: false,
            saved_to: None,
            origin_dir: None,
        };
        self.write_job(&job)?;
        Ok(job)
    }

    pub fn job(&self, id: &str) -> AppResult<Job> {
        let path = self.job_dir(id)?.join("job.json");
        let text = std::fs::read_to_string(&path)
            .map_err(|_| AppError::InvalidPath("That result is no longer here.".into()))?;
        Ok(serde_json::from_str(&text)?)
    }

    fn write_job(&self, job: &Job) -> AppResult<()> {
        let path = self.job_dir(&job.id)?.join("job.json");
        std::fs::write(path, serde_json::to_vec_pretty(job)?)?;
        Ok(())
    }

    /// A path in a job's folder to write an output to, under a name not yet used.
    pub fn output_path(&self, job: &str, name: &str) -> AppResult<PathBuf> {
        let dir = self.job_dir(job)?;
        Ok(unused_path(&dir, &clean_name(name)))
    }

    /// Where a replacement picture for a PDF waits until the PDF is rebuilt
    /// (`utilities::compress`): one folder per input, inside the job's.
    pub fn pending_dir(&self, job: &str, input: &str) -> AppResult<PathBuf> {
        if !is_id(input) {
            return Err(AppError::InvalidPath(format!("'{input}' is not an input")));
        }
        let dir = self.job_dir(job)?.join(".pending").join(input);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Record a written output against its job.
    pub fn add_output(&self, job: &str, path: &Path) -> AppResult<Output> {
        let mut record = self.job(job)?;
        let output = Output {
            name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            size: file_size(path),
            path: path.to_string_lossy().into_owned(),
        };
        record.outputs.push(output.clone());
        self.write_job(&record)?;
        Ok(output)
    }

    /// The file behind one of a job's outputs, by its name in the record.
    pub fn output_file(&self, job: &str, name: &str) -> AppResult<PathBuf> {
        let record = self.job(job)?;
        if !record.outputs.iter().any(|o| o.name == name) {
            return Err(AppError::InvalidPath(format!("'{name}' is not one of this result's files")));
        }
        let path = self.job_dir(job)?.join(clean_name(name));
        if path.is_file() { Ok(path) } else { Err(AppError::InvalidPath("That result is no longer here.".into())) }
    }

    pub fn mark_saved(&self, job: &str, to: &Path) -> AppResult<Job> {
        let mut record = self.job(job)?;
        record.saved = true;
        record.saved_to = Some(to.to_string_lossy().into_owned());
        self.write_job(&record)?;
        Ok(record)
    }

    pub fn discard(&self, job: &str) -> AppResult<()> {
        let dir = self.job_dir(job)?;
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    /// Results with something to save, newest first.
    pub fn unsaved(&self) -> Vec<Job> {
        let Ok(entries) = std::fs::read_dir(self.jobs()) else { return Vec::new() };
        let mut jobs: Vec<Job> = entries
            .flatten()
            .filter_map(|e| self.job(&e.file_name().to_string_lossy()).ok())
            .filter(|j| !j.saved && !j.outputs.is_empty())
            .collect();
        jobs.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        jobs
    }

    /// Remove inputs and jobs older than `age`, and runs that made nothing.
    pub fn sweep(&self, age: Duration) {
        let cutoff = SystemTime::now().checked_sub(age).unwrap_or(SystemTime::UNIX_EPOCH);
        for dir in [self.inputs(), self.jobs()] {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let old = entry.metadata().and_then(|m| m.modified()).map(|t| t < cutoff).unwrap_or(true);
                if old {
                    let path = entry.path();
                    let _ = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_cannot_climb_out() {
        assert_eq!(clean_name("../../.ssh/id_ed25519"), "id_ed25519");
        assert_eq!(clean_name("C:\\Users\\a\\b.pdf"), "b.pdf");
        assert_eq!(clean_name(".hidden"), "hidden");
        assert_eq!(clean_name(""), "file");
        assert_eq!(clean_name("a\u{0}b:c.pdf"), "a_b_c.pdf");
        assert_eq!(clean_name("Hợp đồng thuê nhà.pdf"), "Hợp đồng thuê nhà.pdf");
    }

    #[test]
    fn a_long_name_keeps_its_extension() {
        let long = "a".repeat(400) + ".pdf";
        let cleaned = clean_name(&long);
        assert!(cleaned.ends_with(".pdf"));
        assert!(cleaned.len() <= 200);
        let vietnamese = clean_name(&("Hợp đồng ".repeat(60) + ".pdf"));
        assert!(vietnamese.len() <= 200 && vietnamese.ends_with(".pdf"));
    }

    #[test]
    fn ids_are_only_ids() {
        assert!(is_id(&new_id()));
        assert!(!is_id("../jobs"));
        assert!(!is_id(&"A".repeat(32)));
        assert!(!is_id(""));
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), b"").unwrap();
        std::fs::write(dir.path().join("a (2).pdf"), b"").unwrap();
        assert_eq!(unused_path(dir.path(), "a.pdf"), dir.path().join("a (3).pdf"));
        assert_eq!(unused_path(dir.path(), "b.pdf"), dir.path().join("b.pdf"));
    }

    #[test]
    fn inputs_are_found_by_id_alone() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path().to_path_buf());
        let input = ws.add_bytes("x.pdf", b"%PDF").unwrap();
        assert_eq!(ws.input_path(&input.id).unwrap(), PathBuf::from(&input.path));
        assert!(ws.input_path("../in").is_err());
        assert!(ws.input_path(&new_id()).is_err());
    }

    #[test]
    fn a_job_lists_until_saved() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path().to_path_buf());
        let job = ws.new_job("merge").unwrap();
        assert!(ws.unsaved().is_empty(), "nothing made yet");
        let path = ws.output_path(&job.id, "out.pdf").unwrap();
        std::fs::write(&path, b"%PDF").unwrap();
        ws.add_output(&job.id, &path).unwrap();
        assert_eq!(ws.unsaved().len(), 1);
        assert!(ws.output_file(&job.id, "out.pdf").is_ok());
        assert!(ws.output_file(&job.id, "job.json").is_err(), "only outputs, not the record");
        ws.mark_saved(&job.id, Path::new("/tmp/out.pdf")).unwrap();
        assert!(ws.unsaved().is_empty());
    }

    #[test]
    fn a_vault_file_remembers_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path().join("ws"));
        let folder = dir.path().join("vault/Hợp đồng");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("a.pdf"), b"%PDF").unwrap();
        let input = ws.add_vault_file(&folder.join("a.pdf")).unwrap();
        let other = ws.add_bytes("b.pdf", b"%PDF").unwrap();
        assert_eq!(ws.input_path(&input.id).unwrap().file_name().unwrap(), "a.pdf", "the note is not the input");
        let job = ws.new_job("merge").unwrap();
        let job = ws.note_origin(&job.id, &[other.id, input.id]).unwrap();
        assert_eq!(job.origin_dir.as_deref(), Some(folder.to_string_lossy().as_ref()));
    }

    #[test]
    fn sweeping_removes_the_old() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path().to_path_buf());
        let input = ws.add_bytes("x.pdf", b"%PDF").unwrap();
        ws.sweep(Duration::from_secs(3600));
        assert!(ws.input_path(&input.id).is_ok(), "an hour old is kept");
        ws.sweep(Duration::ZERO);
        assert!(ws.input_path(&input.id).is_err());
    }
}
