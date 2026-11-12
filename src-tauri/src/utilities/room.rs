//! Converting documents in a sealed room (`syn::machine`), macOS 14 and later.
//!
//! LibreOffice and pdf2docx are large programs that parse files from anywhere:
//! a Word file somebody sent, a PDF off the web. Run with the app's rights, a
//! crafted file that broke one of them would have the person's whole account.
//! In a room it has a folder with one file in it, no network, and nothing of
//! the person's — the same kernel rules Syn's commands run under, used here
//! for the opposite reason: not to keep a model in, but a document.
//!
//! No model is involved and nothing here is somebody's words being run. The
//! commands are fixed; the only thing that varies is a file, always copied in
//! under a name this module chose.
//!
//! - **LibreOffice** is not shipped (it is 800 MB). It is found where its
//!   installer puts it, and the room is given its bundle to read and run and
//!   nothing else of `/Applications`, plus the pipe and the one system service
//!   it cannot start without (`Cell::for_libreoffice`, measured).
//! - **pdf2docx** comes as an environment (`machine::env`), installed once when
//!   the person asks, through the room that reaches the package registry and
//!   nothing else.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::syn::machine::cell::Limits;
use crate::syn::machine::env;
use crate::syn::machine::inside;
use crate::syn::machine::rooms::Rooms;

/// Where LibreOffice's installer and Homebrew's cask put it.
pub const LIBREOFFICE: &str = "/Applications/LibreOffice.app";

/// The packages PDF → Word needs. pdf2docx brings PyMuPDF and python-docx.
pub const PDF2DOCX: &[&str] = &["pdf2docx"];

/// The largest result read back out of a room.
const MOST_OUT: u64 = 1 << 30;

#[derive(Debug, thiserror::Error)]
pub enum RoomError {
    /// A program is missing: `libreoffice` or `pdf2docx`.
    #[error("needs {0}")]
    Missing(&'static str),
    #[error("the room is not available: {0}")]
    Unavailable(String),
    /// The program ran and made nothing; its last words, for the log.
    #[error("the conversion failed")]
    Failed(String),
    #[error("{0}")]
    Io(String),
}

/// LibreOffice's program, if it is installed.
pub fn libreoffice() -> Option<PathBuf> {
    let program = Path::new(LIBREOFFICE).join("Contents/MacOS/soffice");
    program.is_file().then_some(program)
}

fn limits(minutes: u64) -> Limits {
    Limits { wall: Duration::from_secs(minutes * 60), cpu: minutes * 120, ..Limits::default() }
}

/// A room of its own for one conversion, emptied and removed afterwards.
struct Borrowed<'a> {
    rooms: &'a Rooms,
    cell: crate::syn::machine::cell::Cell,
}

impl Drop for Borrowed<'_> {
    fn drop(&mut self) {
        let _ = self.rooms.clear(&self.cell);
        let _ = std::fs::remove_dir_all(self.cell.room());
    }
}

async fn borrow<'a>(rooms: &'a Rooms, name: &str) -> Result<Borrowed<'a>, RoomError> {
    let cell = rooms.open(name).await.map_err(|e| RoomError::Unavailable(e.to_string()))?;
    Ok(Borrowed { rooms, cell })
}

/// The extension a file keeps in the room: what LibreOffice reads the format by.
fn safe_extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| (1..=5).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "bin".into())
}

/// One argument for `/bin/sh`, in single quotes, with any quote in it closed,
/// escaped and reopened.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(12)..].join("\n")
}

/// A Word, Excel, PowerPoint or OpenDocument file as a PDF. `name` is the
/// room's, unique to this call.
pub async fn office_to_pdf(rooms: &Rooms, name: &str, input: &Path) -> Result<Vec<u8>, RoomError> {
    libreoffice().ok_or(RoomError::Missing("libreoffice"))?;
    let room = borrow(rooms, name).await?;
    let cell = room.cell.clone().for_libreoffice(PathBuf::from(LIBREOFFICE));
    let file = format!("in.{}", safe_extension(input));
    let bytes = std::fs::read(input).map_err(|e| RoomError::Io(e.to_string()))?;
    inside::write(&cell, Path::new(&file), &bytes).map_err(|e| RoomError::Io(e.to_string()))?;
    // Its settings in the room, not in ~/Library; a fresh profile each time.
    // A URL, percent-encoded, and quoted for the shell: the room lives under
    // "Application Support", and a space would split the argument in two.
    let profile = url::Url::from_file_path(cell.work().join("lo"))
        .map_err(|_| RoomError::Io("the room's path is not absolute".into()))?;
    let script = format!(
        "{} {} --headless --norestore --nologo --convert-to pdf --outdir out {}",
        quoted(&format!("{LIBREOFFICE}/Contents/MacOS/soffice")),
        quoted(&format!("-env:UserInstallation={profile}")),
        quoted(&file),
    );
    let watched = rooms.run(&cell, &script, limits(5)).await.map_err(|e| RoomError::Io(e.to_string()))?;
    match inside::read(&cell, Path::new("out/in.pdf"), MOST_OUT) {
        Ok((pdf, false)) if pdf.starts_with(b"%PDF") => Ok(pdf),
        _ => Err(failed(&watched)),
    }
}

fn failed(watched: &crate::syn::machine::watch::Watched) -> RoomError {
    RoomError::Failed(format!(
        "exit {:?}{}{}\n{}",
        watched.ran.code,
        if watched.ran.timed_out { ", timed out" } else { "" },
        watched.over.as_ref().map(|o| format!(", over {o:?}")).unwrap_or_default(),
        tail(&format!("{}\n{}", watched.ran.stdout, watched.ran.stderr))
    ))
}

/// The environment with pdf2docx, if it has been installed.
pub fn pdf2docx_env(root: &Path, id: Option<&str>) -> Option<PathBuf> {
    env::bin_of(root, id?)
}

/// Install pdf2docx: the one step that needs the network, through the room
/// that reaches only the package registry. Returns the environment's id.
pub async fn install_pdf2docx(rooms: &Rooms, root: &Path, name: &str) -> Result<String, RoomError> {
    let packages: Vec<String> = PDF2DOCX.iter().map(|p| p.to_string()).collect();
    env::make(rooms, root, name, &packages).await.map(|made| made.id).map_err(|e| RoomError::Failed(e.to_string()))
}

/// The script pdf2docx runs: fixed, with the file names it is given.
const PDF2DOCX_SCRIPT: &str = "import sys\nfrom pdf2docx import Converter\n\
c = Converter('in.pdf')\nc.convert('out.docx', multi_processing=False)\nc.close()\n";

/// A PDF as a Word file, by pdf2docx in a room. `bin` is the environment's.
pub async fn pdf_to_docx(rooms: &Rooms, name: &str, bin: &Path, input: &Path) -> Result<Vec<u8>, RoomError> {
    let room = borrow(rooms, name).await?;
    let cell = &room.cell;
    let bytes = std::fs::read(input).map_err(|e| RoomError::Io(e.to_string()))?;
    inside::write(cell, Path::new("in.pdf"), &bytes).map_err(|e| RoomError::Io(e.to_string()))?;
    inside::write(cell, Path::new("convert.py"), PDF2DOCX_SCRIPT.as_bytes()).map_err(|e| RoomError::Io(e.to_string()))?;
    let script = format!("{} -I convert.py", quoted(&bin.join("python3").to_string_lossy()));
    let watched = rooms.run(cell, &script, limits(10)).await.map_err(|e| RoomError::Io(e.to_string()))?;
    match inside::read(cell, Path::new("out.docx"), MOST_OUT) {
        Ok((docx, false)) if docx.starts_with(b"PK") => Ok(docx),
        _ => Err(failed(&watched)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_survive_spaces_and_quotes() {
        assert_eq!(quoted("/Users/a/Library/Application Support/x"), "'/Users/a/Library/Application Support/x'");
        assert_eq!(quoted("it's"), "'it'\\''s'");
        let url = url::Url::from_file_path("/Users/a/Library/Application Support/lo").unwrap();
        assert_eq!(url.as_str(), "file:///Users/a/Library/Application%20Support/lo");
    }

    #[test]
    fn a_file_keeps_only_a_plain_extension() {
        assert_eq!(safe_extension(Path::new("a/Báo cáo.DOCX")), "docx");
        assert_eq!(safe_extension(Path::new("x.do'cx")), "bin");
        assert_eq!(safe_extension(Path::new("noext")), "bin");
    }

    /// LibreOffice in a sealed room, for real: `UTILITIES_OFFICE` is a document,
    /// written as a PDF beside it. Runs the room's self-check first.
    #[tokio::test]
    #[ignore]
    async fn real_office() {
        let input = PathBuf::from(std::env::var("UTILITIES_OFFICE").unwrap());
        let dir = tempfile::tempdir().unwrap();
        // A space in the path, as under ~/Library/Application Support.
        let rooms = Rooms::at(&dir.path().join("Application Support/machine")).unwrap();
        let started = std::time::Instant::now();
        rooms.ready().await.unwrap();
        println!("self-check: {:?}", started.elapsed());
        let started = std::time::Instant::now();
        match office_to_pdf(&rooms, "office-test", &input).await {
            Ok(pdf) => {
                println!("converted in {:?}, {} bytes", started.elapsed(), pdf.len());
                std::fs::write(input.with_extension("room.pdf"), pdf).unwrap();
            }
            Err(e) => panic!("{e}: {e:?}"),
        }
    }

    /// pdf2docx in a sealed room: installs it (network, through the
    /// registry-only room) into `UTILITIES_MACHINE` if it is not there, then
    /// converts `UTILITIES_PDF` beside itself.
    #[tokio::test]
    #[ignore]
    async fn real_pdf2docx() {
        let input = PathBuf::from(std::env::var("UTILITIES_PDF").unwrap());
        let root = PathBuf::from(std::env::var("UTILITIES_MACHINE").unwrap());
        let rooms = Rooms::at(&root).unwrap();
        let root = rooms.root().to_path_buf();
        let id_file = root.join("pdf2docx-env");
        let id = match std::fs::read_to_string(&id_file) {
            Ok(id) => id,
            Err(_) => {
                let started = std::time::Instant::now();
                let id = install_pdf2docx(&rooms, &root, "pdf2docx-install").await.unwrap();
                println!("installed in {:?}", started.elapsed());
                std::fs::write(&id_file, &id).unwrap();
                id
            }
        };
        let bin = pdf2docx_env(&root, Some(&id)).expect("the environment");
        let started = std::time::Instant::now();
        let docx = pdf_to_docx(&rooms, "pdf2docx-test", &bin, &input).await.unwrap();
        println!("converted in {:?}, {} bytes", started.elapsed(), docx.len());
        std::fs::write(input.with_extension("room.docx"), docx).unwrap();
    }
}

#[cfg(test)]
mod probe {
    use super::*;

    /// A one-off command in a room that can run LibreOffice, for finding out
    /// what it needs: `UTILITIES_SCRIPT`.
    #[tokio::test]
    #[ignore]
    async fn probe() {
        let script = std::env::var("UTILITIES_SCRIPT").unwrap();
        let rooms = Rooms::at(Path::new(&std::env::var("UTILITIES_MACHINE").unwrap())).unwrap();
        let room = borrow(&rooms, "probe").await.unwrap();
        let mut cell = room.cell.clone().for_libreoffice(PathBuf::from(LIBREOFFICE));
        if let Ok(extra) = std::env::var("UTILITIES_EXTRA") {
            let text = cell.profile.args()[1].to_string_lossy().into_owned();
            cell.profile.rules = Some(format!("{text}\n{extra}\n"));
        }
        let w = rooms.run(&cell, &script, limits(2)).await.unwrap();
        println!("code {:?}\n--- out\n{}\n--- err\n{}", w.ran.code, w.ran.stdout, w.ran.stderr);
    }
}
