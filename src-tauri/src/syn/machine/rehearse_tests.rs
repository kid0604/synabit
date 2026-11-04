//! Rehearsing, proposing, applying and undoing, on real folders and a real room
//! (design §9.2, plan M5): a case-only rename, two Unicode forms of one name,
//! two files with the same bytes, links, a folder that changes between the card
//! and the apply, an app that dies half way, and a drive of its own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use sha2::{Digest, Sha256};

use super::apply::{self, Standing, Trash};
use super::cell::{Cell, Limits};
use super::doors::Door;
use super::rehearse::{self, Class, Proposal, What};
use super::rooms::Rooms;

static ROOMS: LazyLock<(PathBuf, Arc<Rooms>)> = LazyLock::new(|| {
    let dir = tempfile::Builder::new().prefix("rh-rooms").tempdir().unwrap().keep();
    let rooms = Rooms::at(&dir.join("machine")).unwrap();
    (dir, Arc::new(rooms))
});

fn rooms() -> Arc<Rooms> {
    ROOMS.1.clone()
}

struct Place {
    _dir: tempfile::TempDir,
    folder: PathBuf,
    trash: PathBuf,
    elsewhere: PathBuf,
}

/// A folder with these files, a Trash beside it, and a place it must never reach.
fn place(name: &str, files: &[(&str, &[u8])]) -> Place {
    let dir = tempfile::Builder::new().prefix("rh-").tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let folder = root.join(name);
    for (path, bytes) in files {
        let at = folder.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, bytes).unwrap();
    }
    std::fs::create_dir_all(&folder).unwrap();
    let trash = root.join("Trash");
    std::fs::create_dir_all(&trash).unwrap();
    let elsewhere = root.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("secret.txt"), "not yours").unwrap();
    Place { _dir: dir, folder, trash, elsewhere }
}

fn door(name: &str) -> Door {
    Door { id: "0123456789abcdef".into(), name: name.into(), path: String::new(), opened: chrono::Utc::now(), away: false, hidden: 0 }
}

/// Every file under a folder, by path, with its hash; folders as `/`.
fn snapshot(folder: &Path) -> BTreeMap<String, String> {
    walkdir::WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.depth() > 0)
        .map(|e| {
            let path = e.path().strip_prefix(folder).unwrap().display().to_string();
            let what = if e.file_type().is_dir() {
                "/".to_string()
            } else if e.file_type().is_symlink() {
                format!("-> {}", std::fs::read_link(e.path()).unwrap().display())
            } else {
                hex::encode(Sha256::digest(std::fs::read(e.path()).unwrap()))
            };
            (path, what)
        })
        .collect()
}

fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

async fn cell(room: &str) -> Cell {
    rooms().open(room).await.unwrap()
}

async fn run(cell: &Cell, script: &str) {
    let ran = rooms().run(cell, script, Limits::default()).await.unwrap();
    assert!(ran.ran.ok(), "{ran:?}");
}

/// Rehearse `place`, run `script` in the copy, and propose.
async fn proposed(room: &str, place: &Place, script: &str) -> (Cell, Proposal) {
    let cell = cell(room).await;
    let name = place.folder.file_name().unwrap().to_str().unwrap().to_string();
    let rehearsed = rehearse::rehearse(&rooms(), &cell, room, &door(&name), &place.folder, None).unwrap();
    run(&cell, &format!("cd '{}' && {script}", rehearsed.at)).await;
    let proposal = rehearse::propose(&rooms(), &cell, &rehearsed.rehearsal, "/vault", Some("conv")).unwrap();
    (cell, proposal)
}

fn every(proposal: &Proposal) -> Vec<usize> {
    proposal.ops.iter().map(|o| o.n).collect()
}

fn applied(proposal: &Proposal, approved: &[usize], trash: &Path) -> apply::Outcome {
    let rooms = rooms();
    apply::apply(rooms.root(), proposal, |n| rehearse::staged_file(&rooms, &proposal.id, n), approved, &Trash::At(trash.to_path_buf())).unwrap()
}

fn op<'a>(proposal: &'a Proposal, what: What, path: &str) -> &'a rehearse::Op {
    proposal
        .ops
        .iter()
        .find(|o| o.what == what && (o.from.as_deref() == Some(path) || o.to.as_deref() == Some(path)))
        .unwrap_or_else(|| panic!("no {what:?} of {path}: {:#?}", proposal.ops))
}

const PDF: &[u8] = b"%PDF-1.7 a lease";
const JPG: &[u8] = b"\xFF\xD8\xFF\xE0 a photo";

/// The whole way, and back: tidied as the room tidied its copy, with a case-only
/// rename among it (A17), then undone to the byte and to the name.
#[tokio::test]
async fn a_folder_is_tidied_as_the_copy_was_and_put_back_exactly() {
    let place = place("Downloads", &[("lease.pdf", PDF), ("photo.jpg", JPG), ("notes.txt", b"call the bank"), ("old.txt", b"old"), ("sub/deep.txt", b"deep")]);
    let before = snapshot(&place.folder);
    let original_ino = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(place.folder.join("lease.pdf")).unwrap());

    let (_cell, proposal) = proposed(
        "rh-tidy",
        &place,
        "mkdir Docs Images && mv lease.pdf Docs/ && mv photo.jpg Images/ && rm old.txt && mv notes.txt Notes.txt && printf 'made here' > summary.md && mv sub/deep.txt . && rmdir sub",
    )
    .await;
    assert_eq!(op(&proposal, What::Move, "lease.pdf").to.as_deref(), Some("Docs/lease.pdf"));
    assert_eq!(op(&proposal, What::Rename, "notes.txt").to.as_deref(), Some("Notes.txt"), "a case-only rename is a rename");
    assert_eq!(op(&proposal, What::Trash, "old.txt").class, Class::Data);
    assert_eq!(op(&proposal, What::New, "summary.md").class, Class::Data);
    op(&proposal, What::NewFolder, "Docs");
    op(&proposal, What::RemoveFolder, "sub");
    assert!(!proposal.ops.iter().any(|o| o.what == What::Trash && o.from.as_deref() != Some("old.txt")), "{:#?}", proposal.ops);
    assert!(proposal.read_outside);

    let outcome = applied(&proposal, &every(&proposal), &place.trash);
    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    let moved = std::fs::metadata(place.folder.join("Docs/lease.pdf")).unwrap();
    assert_eq!(std::os::unix::fs::MetadataExt::ino(&moved), original_ino, "moved, not copied");
    assert!(names_in(&place.folder).contains(&"Notes.txt".to_string()), "{:?}", names_in(&place.folder));
    assert_eq!(std::fs::read(place.folder.join("summary.md")).unwrap(), b"made here");
    assert_eq!(names_in(&place.trash).len(), 2, "old.txt and the emptied folder: {:?}", names_in(&place.trash));
    assert!(!names_in(&place.folder).iter().any(|n| n.starts_with(".synabit")), "no staging folder left");
    let marked = std::process::Command::new("/usr/bin/xattr").arg("-p").arg("com.apple.quarantine").arg(place.folder.join("summary.md")).output().unwrap();
    assert!(marked.status.success(), "a file made in the room carries the quarantine mark");

    let batch = apply::batches(rooms().root()).into_iter().find(|b| b.id == proposal.id).unwrap();
    assert_eq!(batch.standing, Standing::Finished);
    assert!(batch.can_undo);

    let undone = apply::undo(rooms().root(), &proposal.id, &Trash::At(place.trash.clone())).unwrap();
    assert!(undone.left.is_empty(), "{:?}", undone.left);
    assert_eq!(snapshot(&place.folder), before, "every file back, byte for byte");
    assert!(names_in(&place.folder).contains(&"notes.txt".to_string()), "and its name as it was: {:?}", names_in(&place.folder));
    assert_eq!(names_in(&place.trash), vec!["summary.md".to_string()], "what the batch made is thrown away, not deleted");
    let batch = apply::batches(rooms().root()).into_iter().find(|b| b.id == proposal.id).unwrap();
    assert_eq!(batch.standing, Standing::Undone);
}

/// §9.1: only documents are chosen beforehand. A script, a name with no
/// extension, a renamed `.command`, a PDF that is not one, an executable.
#[tokio::test]
async fn only_what_is_plainly_a_document_is_data() {
    let place = place("Mixed", &[("a.txt", b"text"), ("fake.pdf", b"#!/bin/sh\necho hi"), ("tool.txt", b"x")]);
    std::fs::set_permissions(place.folder.join("tool.txt"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let (_cell, proposal) = proposed(
        "rh-kinds",
        &place,
        "printf 'echo hi' > run.sh && printf x > README && mv a.txt a.command && mkdir In && mv fake.pdf In/ && mv tool.txt In/ && mkdir Thing.app",
    )
    .await;
    let class = |what, path| op(&proposal, what, path).class;
    assert_eq!(class(What::New, "run.sh"), Class::Unknown);
    assert_eq!(class(What::New, "README"), Class::Unknown);
    assert_eq!(class(What::Rename, "a.txt"), Class::Unknown, "renamed into a program");
    assert_eq!(op(&proposal, What::Move, "fake.pdf").why, Some(rehearse::Why::ContentDoesNotMatch));
    assert_eq!(op(&proposal, What::Move, "tool.txt").why, Some(rehearse::Why::Executable));
    assert_eq!(class(What::NewFolder, "Thing.app"), Class::Unknown);
    assert_eq!(class(What::NewFolder, "In"), Class::Data);
}

/// A16: the room changes the file again after the card was made. What goes into
/// the folder is what the card was made from.
#[tokio::test]
async fn what_is_applied_is_what_was_shown() {
    let place = place("After", &[("a.txt", b"a")]);
    let (cell, proposal) = proposed("rh-a16", &place, "printf 'as shown' > report.txt").await;
    let copy = cell.work().join("After");
    run(&cell, &format!("printf 'swapped later' > '{}/report.txt'", copy.display())).await;
    let outcome = applied(&proposal, &every(&proposal), &place.trash);
    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    assert_eq!(std::fs::read(place.folder.join("report.txt")).unwrap(), b"as shown");
}

/// A18 and its kin: between the card and the apply, a file is swapped for a link
/// to something else, a folder for a link to another, another file is edited,
/// and a name the room freed is taken. None of them is followed or written over;
/// the rest is applied.
#[tokio::test]
async fn a_folder_that_changes_after_the_card_is_not_trusted() {
    let place = place(
        "Changing",
        &[("swap.pdf", PDF), ("sub/inner.txt", b"inner"), ("edited.txt", b"before"), ("taken.txt", b"t"), ("fine.txt", b"fine")],
    );
    let (_cell, proposal) = proposed(
        "rh-a18",
        &place,
        "mkdir Out && mv swap.pdf Out/ && mv sub/inner.txt Out/ && mv edited.txt Out/ && mv taken.txt Out/ && mv fine.txt Out/",
    )
    .await;

    std::fs::remove_file(place.folder.join("swap.pdf")).unwrap();
    std::os::unix::fs::symlink(place.elsewhere.join("secret.txt"), place.folder.join("swap.pdf")).unwrap();
    std::fs::rename(place.folder.join("sub"), place.folder.join("sub-real")).unwrap();
    std::fs::create_dir_all(place.elsewhere.join("sub")).unwrap();
    std::fs::write(place.elsewhere.join("sub/inner.txt"), "theirs").unwrap();
    std::os::unix::fs::symlink(place.elsewhere.join("sub"), place.folder.join("sub")).unwrap();
    std::fs::write(place.folder.join("edited.txt"), "edited after").unwrap();
    std::fs::create_dir_all(place.folder.join("Out")).unwrap();
    std::fs::write(place.folder.join("Out/taken.txt"), "someone else's").unwrap();

    let outcome = applied(&proposal, &every(&proposal), &place.trash);
    let skipped: Vec<usize> = outcome.skipped.iter().map(|(n, _)| *n).collect();
    for path in ["swap.pdf", "sub/inner.txt", "edited.txt", "taken.txt"] {
        assert!(skipped.contains(&op(&proposal, What::Move, path).n), "{path}: {:?}", outcome.skipped);
    }
    assert_eq!(std::fs::read_to_string(place.folder.join("Out/fine.txt")).unwrap(), "fine");
    assert_eq!(std::fs::read_to_string(place.elsewhere.join("secret.txt")).unwrap(), "not yours");
    assert_eq!(std::fs::read_to_string(place.elsewhere.join("sub/inner.txt")).unwrap(), "theirs");
    assert!(place.folder.join("swap.pdf").is_symlink(), "the link is left where it is");
    assert_eq!(std::fs::read_to_string(place.folder.join("edited.txt")).unwrap(), "edited after");
    assert_eq!(std::fs::read_to_string(place.folder.join("taken.txt")).unwrap(), "t", "put back, not written over");
    assert_eq!(std::fs::read_to_string(place.folder.join("Out/taken.txt")).unwrap(), "someone else's");
    assert!(names_in(&place.trash).is_empty());
}

/// Two files with the same bytes are two files: the one the room removed goes,
/// the other stays, and nothing is read as a move.
#[tokio::test]
async fn files_with_the_same_bytes_are_told_apart() {
    let place = place("Twins", &[("one.txt", b"same"), ("two.txt", b"same")]);
    let (_cell, proposal) = proposed("rh-twins", &place, "rm two.txt").await;
    assert_eq!(proposal.ops.len(), 1, "{:#?}", proposal.ops);
    assert_eq!(op(&proposal, What::Trash, "two.txt").what, What::Trash);
    applied(&proposal, &every(&proposal), &place.trash);
    assert_eq!(names_in(&place.folder), vec!["one.txt".to_string()]);
}

/// A name in one Unicode form renamed to the other is the same file renamed,
/// never one gone and one new.
#[tokio::test]
async fn two_unicode_forms_of_a_name_are_one_file() {
    use unicode_normalization::UnicodeNormalization;
    let decomposed: String = "Tài liệu.txt".nfd().collect();
    let composed: String = "Tài liệu.txt".nfc().collect();
    let place = place("Forms", &[(decomposed.as_str(), b"vn")]);
    let (_cell, proposal) = proposed("rh-forms", &place, &format!("mv '{decomposed}' tmp && mv tmp '{composed}'")).await;
    assert!(!proposal.ops.iter().any(|o| matches!(o.what, What::Trash | What::New)), "{:#?}", proposal.ops);
    let outcome = applied(&proposal, &every(&proposal), &place.trash);
    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    assert_eq!(std::fs::read(place.folder.join(&composed)).unwrap(), b"vn");
}

/// What the room never saw is never gone: names the backstop hides, links, and
/// with them a folder of code.
#[tokio::test]
async fn what_the_room_never_saw_is_never_thrown_away() {
    let place = place("Unseen", &[(".env", b"KEY=1"), ("a.txt", b"a")]);
    std::os::unix::fs::symlink(place.elsewhere.join("secret.txt"), place.folder.join("link.txt")).unwrap();
    let (cell, proposal) = proposed("rh-unseen", &place, "ls -a > ../listing.txt; rm -f a.txt").await;
    let listing = std::fs::read_to_string(cell.work().join("listing.txt")).unwrap();
    assert!(!listing.contains(".env") && !listing.contains("link.txt"), "{listing}");
    assert_eq!(proposal.ops.len(), 1, "only a.txt: {:#?}", proposal.ops);
    applied(&proposal, &every(&proposal), &place.trash);
    assert!(place.folder.join(".env").exists() && place.folder.join("link.txt").is_symlink());

    let code = self::place("Code", &[(".git/HEAD", b"ref: refs/heads/main"), ("main.rs", b"fn main() {}")]);
    let refused = rehearse::rehearse(&rooms(), &cell, "rh-unseen", &door("Code"), &code.folder, None);
    assert!(matches!(refused, Err(rehearse::Refused::Code)), "{refused:?}");
    let refused = rehearse::rehearse(&rooms(), &cell, "rh-unseen", &door("Code"), &code.folder, Some(".git"));
    assert!(refused.is_err());
}

/// Only what was approved, and nothing that cannot be.
#[tokio::test]
async fn only_the_approved_ops_are_applied() {
    let place = place("Some", &[("a.txt", b"a"), ("b.txt", b"b")]);
    let (_cell, proposal) = proposed("rh-some", &place, "rm a.txt b.txt").await;
    let a = op(&proposal, What::Trash, "a.txt").n;
    let outcome = applied(&proposal, &[a], &place.trash);
    assert_eq!(outcome.done, vec![a]);
    assert_eq!(names_in(&place.folder), vec!["b.txt".to_string()]);
}

/// The room puts a link to someone else's folder where its copy was, and then
/// where its whole work folder was. Ending the rehearsal removes the links,
/// never what they point at.
#[tokio::test]
async fn ending_a_rehearsal_never_follows_what_the_room_left() {
    for trick in ["rm -rf Ends; ln -s '{to}' Ends", "cd ..; mkdir keep; mv work/Ends keep/; rm -rf work; ln -s '{to}' work"] {
        let place = place("Ends", &[("a.txt", b"a")]);
        let (cell, mut proposal) = proposed("rh-ends", &place, "rm a.txt").await;
        run(&cell, &trick.replace("{to}", &place.elsewhere.display().to_string())).await;
        rehearse::settle(&rooms(), Some(&cell), &mut proposal, rehearse::State::Declined).unwrap();
        assert_eq!(std::fs::read_to_string(place.elsewhere.join("secret.txt")).unwrap(), "not yours", "{trick}");
        assert_eq!(names_in(&place.folder), vec!["a.txt".to_string()], "declined: nothing changed");
    }
}

// ── An app that dies half way ───────────────────────────────────────────

const CHILD: &str = "SYNABIT_APPLY_CHILD";

/// Run only as the child of the test below, which kills it part way through.
#[test]
#[ignore]
fn child_applies_and_dies() {
    let Ok(args) = std::env::var(CHILD) else { return };
    let mut parts = args.split('\n');
    let (machine, id, trash, die) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
    let rooms = Rooms::at(Path::new(machine)).unwrap();
    let proposal = rehearse::load_proposal(&rooms, id).unwrap();
    apply::DIE_AFTER.store(die.parse().unwrap(), std::sync::atomic::Ordering::SeqCst);
    let _ = apply::apply(rooms.root(), &proposal, |n| rehearse::staged_file(&rooms, id, n), &every(&proposal), &Trash::At(trash.into()));
    panic!("it should have died");
}

/// The app is killed in the middle of applying, with files out of their places.
/// The next launch reads the journal, calls the batch interrupted, and undo puts
/// every file back as it was.
#[tokio::test]
async fn a_batch_cut_short_by_a_crash_is_put_back() {
    let place = place(
        "Crash",
        &[("a.pdf", PDF), ("b.pdf", PDF), ("c.txt", b"c"), ("d.txt", b"d"), ("e.txt", b"e")],
    );
    let before = snapshot(&place.folder);
    let (_cell, proposal) = proposed("rh-crash", &place, "mkdir Docs && mv a.pdf b.pdf Docs/ && rm c.txt && mv d.txt D.txt && printf n > new.txt").await;
    // Dies after the folder, the new file and two files taken out: mid-way.
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "syn::machine::rehearse_tests::child_applies_and_dies", "--ignored", "--nocapture"])
        .env(CHILD, format!("{}\n{}\n{}\n4", rooms().root().display(), proposal.id, place.trash.display()))
        .output()
        .unwrap();
    assert_eq!(child.status.code(), Some(9), "{}", String::from_utf8_lossy(&child.stderr));
    assert!(names_in(&place.folder).iter().any(|n| n.starts_with(".synabit-applying")), "files were out of their places");

    let batch = apply::batches(rooms().root()).into_iter().find(|b| b.id == proposal.id).unwrap();
    assert_eq!(batch.standing, Standing::Interrupted);
    let undone = apply::undo(rooms().root(), &proposal.id, &Trash::At(place.trash.clone())).unwrap();
    assert!(undone.left.is_empty(), "{:?}", undone.left);
    assert_eq!(snapshot(&place.folder), before);
}

// ── A drive of its own ──────────────────────────────────────────────────

struct Image {
    mount: PathBuf,
    _dir: tempfile::TempDir,
}

impl Image {
    fn attach(fs: &str) -> Self {
        let dir = tempfile::Builder::new().prefix("rh-img").tempdir().unwrap();
        let image = dir.path().join("drive.dmg");
        let made = std::process::Command::new("/usr/bin/hdiutil")
            .args(["create", "-quiet", "-size", "64m", "-fs", fs, "-volname", "SynDrive"])
            .arg(&image)
            .status()
            .unwrap();
        assert!(made.success());
        let mount = dir.path().canonicalize().unwrap().join("mnt");
        std::fs::create_dir_all(&mount).unwrap();
        let attached = std::process::Command::new("/usr/bin/hdiutil")
            .args(["attach", "-quiet", "-nobrowse", "-mountpoint"])
            .arg(&mount)
            .arg(&image)
            .status()
            .unwrap();
        assert!(attached.success());
        Self { mount, _dir: dir }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        let _ = std::process::Command::new("/usr/bin/hdiutil").args(["detach", "-quiet", "-force"]).arg(&self.mount).status();
    }
}

/// On another volume: the copy is a copy, not a clone, the Trash is that
/// volume's own, and a volume that tells case apart keeps two names the room's
/// volume could hold only as one — the second is left as it is.
#[tokio::test]
async fn a_folder_on_another_drive_is_tidied_and_put_back() {
    for fs in ["APFS", "Case-sensitive APFS"] {
        let image = Image::attach(fs);
        let folder = image.mount.join("Inbox");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("scan.pdf"), PDF).unwrap();
        std::fs::write(folder.join("note.txt"), b"lower").unwrap();
        let sensitive = fs.starts_with("Case");
        if sensitive {
            std::fs::write(folder.join("NOTE.txt"), b"upper").unwrap();
        }
        let before = snapshot(&folder);
        let place = Place { _dir: tempfile::tempdir().unwrap(), folder: folder.clone(), trash: PathBuf::new(), elsewhere: PathBuf::new() };
        let room = if sensitive { "rh-drive-cs" } else { "rh-drive" };
        let (_cell, proposal) = proposed(room, &place, "mkdir Docs && mv scan.pdf Docs/ && rm -f note.txt NOTE.txt").await;
        // Whichever of the two names the room's copy holds, only that one goes.
        assert_eq!(proposal.ops.iter().filter(|o| o.what == What::Trash).count(), 1, "{fs}: {:#?}", proposal.ops);

        let rooms = rooms();
        let system = Trash::System { home: PathBuf::from(std::env::var("HOME").unwrap()) };
        let outcome = apply::apply(rooms.root(), &proposal, |n| rehearse::staged_file(&rooms, &proposal.id, n), &every(&proposal), &system).unwrap();
        assert!(outcome.skipped.is_empty(), "{fs}: {:?}", outcome.skipped);
        assert!(folder.join("Docs/scan.pdf").exists());
        // SAFETY: a plain query.
        let uid = unsafe { libc::getuid() };
        let trash = image.mount.join(".Trashes").join(uid.to_string());
        assert_eq!(names_in(&trash).len(), 1, "{fs}: the drive's own Trash");
        if sensitive {
            assert_eq!(names_in(&folder).iter().filter(|n| n.eq_ignore_ascii_case("note.txt")).count(), 1, "{fs}: {:?}", names_in(&folder));
        }

        let undone = apply::undo(rooms.root(), &proposal.id, &system).unwrap();
        assert!(undone.left.is_empty(), "{fs}: {:?}", undone.left);
        assert_eq!(snapshot(&folder), before, "{fs}");
    }
}
