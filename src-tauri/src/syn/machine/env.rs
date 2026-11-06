//! An environment the rooms use: a Python and the packages somebody approved,
//! made once and read-only after.
//!
//! A sealed room cannot install anything: it has no network. So packages are
//! installed elsewhere, in a room that reaches the registry and nothing else
//! ([`Rooms::with_network`]), with no model in it: Rust runs `uv`, one program at
//! a time, with arguments it wrote. The list was on the card the person
//! approved, and is resolved into a lockfile with a hash for every file, which
//! is what is installed. The result is copied into `tools/<id>/`, named by that
//! lockfile, and made read-only; a sealed room uses it when a command names it.
//!
//! `uv` itself comes from astral's release, the version and its hash written
//! here: a blank Mac has no Python, and nothing is run that does not match.
//! `uv` then fetches its Python the same way, by hashes of its own.
//!
//! A package can still be malicious. Installing one runs its code in the room
//! with the network, which holds nothing of the person's; using it runs its code
//! in a sealed room, which has no way out. What is left is a wrong answer, said
//! in the design (§16).

use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::cell::{Cell, Limits};
use super::proxy;
use super::rooms::{Rooms, Unavailable};
use crate::syn::packages::PYTHON;

pub const UV_VERSION: &str = "0.13.2";

/// The release archives' SHA-256, as astral publishes them beside each one,
/// checked against GitHub's digest and a download on 2026-11-03. The newest
/// release then two weeks old: 0.13.7 had come out that morning.
const UV_ARCHIVES: [(&str, &str); 2] = [
    ("aarch64", "4a5d878cd0db3a57d79eec638c275e71c394e10533ae54c77b33d940866f53b7"),
    ("x86_64", "08f0cb0d63a012ffa07659d1872030c1110fccdd665d49bc3683b0ca262b4a62"),
];

/// How long each step may take. Python is about 50 MB.
const STEP: Limits = Limits {
    wall: std::time::Duration::from_secs(600),
    cpu: 3600,
    output: 1 << 20,
    file: 4 << 30,
    background: false,
};

/// An environment, made.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Made {
    /// What a command names to use it: the lockfile's hash, shortened.
    pub id: String,
    pub python: String,
    /// Every package in it, dependencies included, as `name==version`.
    pub packages: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Failed {
    #[error("{0}")]
    Room(#[from] Unavailable),
    #[error("uv could not be fetched: {0}")]
    Fetch(String),
    /// What the installer said, its last lines: a registry's words.
    #[error("{step} failed")]
    Step { step: &'static str, said: String },
    #[error("{0}")]
    Other(String),
}

fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    }
}

fn hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// `uv`, fetched once and checked against the hash written above. The binary
/// is checked again against what was unpacked each time it is used.
pub async fn uv(root: &Path) -> Result<PathBuf, Failed> {
    let folder = root.join("bootstrap").join(format!("uv-{UV_VERSION}"));
    let binary = folder.join("uv");
    let known = folder.join("uv.sha256");
    if let (Ok(bytes), Ok(want)) = (std::fs::read(&binary), std::fs::read_to_string(&known)) {
        if hex(&bytes) == want.trim() {
            return Ok(binary);
        }
    }
    let (_, want) = UV_ARCHIVES.iter().find(|(a, _)| *a == arch()).ok_or_else(|| Failed::Fetch("no uv for this processor".into()))?;
    let name = format!("uv-{}-apple-darwin", arch());
    let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{name}.tar.gz");
    let archive = reqwest::get(&url)
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| Failed::Fetch(e.to_string()))?
        .bytes()
        .await
        .map_err(|e| Failed::Fetch(e.to_string()))?;
    if hex(&archive) != *want {
        return Err(Failed::Fetch(format!("the archive from {url} is not the one this app was made with")));
    }

    let unpack = root.join("bootstrap").join(format!(".unpack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&unpack);
    std::fs::create_dir_all(&unpack).map_err(|e| Failed::Fetch(e.to_string()))?;
    std::fs::write(unpack.join("uv.tar.gz"), &archive).map_err(|e| Failed::Fetch(e.to_string()))?;
    let untar = tokio::process::Command::new("/usr/bin/tar")
        .args(["-xzf", "uv.tar.gz", "--no-same-owner", "--no-same-permissions"])
        .current_dir(&unpack)
        .status()
        .await
        .map_err(|e| Failed::Fetch(e.to_string()))?;
    let found = unpack.join(&name).join("uv");
    let meta = std::fs::symlink_metadata(&found).map_err(|e| Failed::Fetch(e.to_string()))?;
    if !untar.success() || !meta.file_type().is_file() {
        let _ = std::fs::remove_dir_all(&unpack);
        return Err(Failed::Fetch("the archive did not hold uv".into()));
    }
    let bytes = std::fs::read(&found).map_err(|e| Failed::Fetch(e.to_string()))?;
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).map_err(|e| Failed::Fetch(e.to_string()))?;
    std::fs::write(&binary, &bytes).map_err(|e| Failed::Fetch(e.to_string()))?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o555)).map_err(|e| Failed::Fetch(e.to_string()))?;
    std::fs::write(&known, hex(&bytes)).map_err(|e| Failed::Fetch(e.to_string()))?;
    let _ = std::fs::remove_dir_all(&unpack);
    Ok(binary)
}

/// The environment `id` names, if there is one: its `bin` folder.
pub fn bin_of(root: &Path, id: &str) -> Option<PathBuf> {
    if id.len() != 16 || !id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return None;
    }
    let bin = root.join("tools").join(id).join("bin");
    bin.join("python3").exists().then_some(bin)
}

/// `name==version` from each line of a lockfile that has one.
fn pinned(lock: &str) -> Vec<String> {
    lock.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && !l.starts_with("--"))
        .filter_map(|l| l.split_whitespace().next())
        .filter(|l| l.contains("=="))
        .map(|l| l.trim_end_matches('\\').to_string())
        .collect()
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(15)..].join("\n")
}

/// The Python `uv python install` left in `folder`: the folder itself, not the
/// link uv puts beside it under the shorter name.
pub(super) fn python_in(folder: &Path) -> Option<String> {
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .find(|name| name.starts_with(&format!("cpython-{PYTHON}.")))
}

/// Resolves, installs and keeps `packages`. `name` is the room's, unique to
/// this call. Nothing of the person's is in that room, and the registry is all
/// it can reach.
pub async fn make(rooms: &Rooms, root: &Path, name: &str, packages: &[String]) -> Result<Made, Failed> {
    let uv_binary = uv(root).await?;
    let requirements = format!("{}\n", packages.join("\n"));

    let built = rooms
        .with_network(name, proxy::python_registries(), |cell: Cell| async move {
            let tools = cell.profile.tools.clone();
            std::fs::copy(&uv_binary, tools.join("uv")).map_err(|e| Failed::Other(e.to_string()))?;
            let uv = tools.join("uv");
            let work = cell.work();
            std::fs::write(work.join("requirements.in"), &requirements).map_err(|e| Failed::Other(e.to_string()))?;
            let port = match cell.profile.reach {
                super::profile::Reach::Proxy(port) => port,
                super::profile::Reach::Closed => return Err(Failed::Other("the room has no way to the registry".into())),
            };
            let proxy = std::ffi::OsString::from(format!("http://127.0.0.1:{port}"));
            let env: Vec<(&str, std::ffi::OsString)> = vec![
                ("PATH", "/usr/bin:/bin".into()),
                ("HTTPS_PROXY", proxy.clone()),
                ("https_proxy", proxy),
                ("UV_NO_CONFIG", "1".into()),
                ("UV_CACHE_DIR", work.join(".cache").into()),
                ("UV_PYTHON_INSTALL_DIR", work.join("python").into()),
                ("UV_PYTHON_PREFERENCE", "only-managed".into()),
            ];
            let step = |step: &'static str, args: Vec<std::ffi::OsString>| {
                let (cell, uv, env) = (cell.clone(), uv.clone(), env.clone());
                async move {
                    let ran = cell.exec(&uv, &args, &env, STEP).await.map_err(|e| Failed::Other(e.to_string()))?;
                    if ran.ok() {
                        Ok(())
                    } else {
                        Err(Failed::Step { step, said: tail(&format!("{}\n{}", ran.stdout, ran.stderr)) })
                    }
                }
            };
            step("installing Python", ["python", "install", PYTHON].map(Into::into).to_vec()).await?;
            let python_dir = python_in(&work.join("python")).ok_or_else(|| Failed::Other("uv left no Python".into()))?;
            let python: std::ffi::OsString = work.join("python").join(&python_dir).join("bin").join("python3").into();
            let mut compile: Vec<std::ffi::OsString> = ["pip", "compile", "requirements.in", "--generate-hashes", "--no-header", "-o", "requirements.lock", "--python"]
                .map(Into::into)
                .to_vec();
            compile.push(python.clone());
            step("resolving the packages", compile).await?;
            let mut install: Vec<std::ffi::OsString> = ["pip", "install", "--require-hashes", "--break-system-packages", "-r", "requirements.lock", "--python"]
                .map(Into::into)
                .to_vec();
            install.push(python);
            step("installing the packages", install).await?;
            let (lock, _) = super::inside::read(&cell, Path::new("requirements.lock"), 1 << 20).map_err(|e| Failed::Other(e.to_string()))?;
            Ok((python_dir, String::from_utf8_lossy(&lock).to_string()))
        })
        .await;
    let place = rooms.prep_place(name);
    let kept = match built {
        Ok(Ok((python_dir, lock))) => keep(root, &place.join("room/work/python").join(python_dir), &lock).await,
        Ok(Err(failed)) => Err(failed),
        Err(unavailable) => Err(Failed::Room(unavailable)),
    };
    let _ = std::fs::remove_dir_all(&place);
    kept
}

/// Copies what the room made into `tools/<id>`, after the room has ended. A link
/// is copied as a link, never followed, and every file ends up read-only.
async fn keep(root: &Path, made: &Path, lock: &str) -> Result<Made, Failed> {
    let id = hex(format!("{PYTHON}\n{}\n{lock}", arch()).as_bytes())[..16].to_string();
    let packages = pinned(lock);
    let out = Made { id: id.clone(), python: PYTHON.to_string(), packages };
    let target = root.join("tools").join(&id);
    if bin_of(root, &id).is_some() {
        return Ok(out);
    }
    let meta = std::fs::symlink_metadata(made).map_err(|e| Failed::Other(e.to_string()))?;
    if !meta.file_type().is_dir() {
        return Err(Failed::Other("the room did not leave a Python folder".into()));
    }
    let partial = root.join("tools").join(format!(".{id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(root.join("tools")).map_err(|e| Failed::Other(e.to_string()))?;
    let copied = tokio::process::Command::new("/bin/cp")
        .arg("-cRP")
        .arg(made)
        .arg(&partial)
        .status()
        .await
        .map_err(|e| Failed::Other(e.to_string()))?;
    let sealed = tokio::process::Command::new("/bin/chmod")
        .args(["-R", "a-w,u-s,g-s"])
        .arg(&partial)
        .status()
        .await
        .map_err(|e| Failed::Other(e.to_string()))?;
    let real = std::fs::symlink_metadata(&partial).is_ok_and(|m| m.file_type().is_dir());
    if !copied.success() || !sealed.success() || !real || !partial.join("bin/python3").exists() {
        let _ = make_writable_and_remove(&partial).await;
        return Err(Failed::Other("the environment could not be kept".into()));
    }
    // The lockfile beside it, for whoever looks: what is in there, exactly.
    let _ = tokio::process::Command::new("/bin/chmod").args(["u+w"]).arg(&partial).status().await;
    let _ = std::fs::write(partial.join("synabit.lock"), lock);
    // Sealed only once it has its name: macOS 15 refuses to rename a folder
    // that cannot be written, and 26 does not, so the order is the only one
    // that works on both.
    if let Err(e) = std::fs::rename(&partial, &target) {
        let _ = make_writable_and_remove(&partial).await;
        return Err(Failed::Other(e.to_string()));
    }
    let _ = tokio::process::Command::new("/bin/chmod").args(["a-w"]).arg(&target).status().await;
    Ok(out)
}

async fn make_writable_and_remove(folder: &Path) -> std::io::Result<()> {
    let _ = tokio::process::Command::new("/bin/chmod").args(["-R", "u+w"]).arg(folder).status().await;
    std::fs::remove_dir_all(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lockfile_says_what_is_in_it() {
        let lock = "idna==3.10 \\\n    --hash=sha256:abc\nnumpy==2.5.4 \\\n    --hash=sha256:def\n# via x\n";
        assert_eq!(pinned(lock), ["idna==3.10", "numpy==2.5.4"]);
    }

    #[test]
    fn an_environment_is_named_by_its_hash_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["", "../x", "ABCDEF0123456789", "0123456789abcde", "0123456789abcdefg", "0123456789abcdeg"] {
            assert!(bin_of(dir.path(), bad).is_none(), "{bad}");
        }
        let bin = dir.path().join("tools/0123456789abcdef/bin");
        std::fs::create_dir_all(&bin).unwrap();
        assert!(bin_of(dir.path(), "0123456789abcdef").is_none(), "no python in it");
        std::fs::write(bin.join("python3"), "").unwrap();
        assert_eq!(bin_of(dir.path(), "0123456789abcdef"), Some(bin));
    }

    /// A package whose build runs hostile code, as an install script does. It runs
    /// in the room with the network, and reaches nothing: not a file outside, not
    /// the person's keys, not the shared tools, not the network but through the
    /// port, and through the port not a host off the list. Run by hand: it
    /// fetches uv and a Python first.
    #[tokio::test]
    #[ignore = "fetches uv and Python; run by hand"]
    async fn a_package_whose_install_attacks_gets_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Application Support/machine");
        let rooms = Rooms::at(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let bait = dir.path().canonicalize().unwrap().join("bait.txt");
        std::fs::write(&bait, "the person's").unwrap();
        let outside = dir.path().canonicalize().unwrap().join("written-by-a-package");
        std::fs::create_dir_all(root.join("tools")).unwrap();
        let home = std::env::var("HOME").unwrap();
        let uv_binary = uv(&root).await.unwrap();
        let (outside_in, root_in) = (outside.clone(), root.clone());

        let found = rooms
            .with_network("env-evil", proxy::python_registries(), |cell: Cell| async move {
                let (outside, root) = (outside_in, root_in);
                let tools = cell.profile.tools.clone();
                std::fs::copy(&uv_binary, tools.join("uv")).unwrap();
                let work = cell.work();
                let port = match cell.profile.reach {
                    super::super::profile::Reach::Proxy(port) => port,
                    super::super::profile::Reach::Closed => 0,
                };
                let package = work.join("evil");
                std::fs::create_dir_all(&package).unwrap();
                std::fs::write(
                    package.join("pyproject.toml"),
                    "[build-system]\nrequires = []\nbuild-backend = \"evil_backend\"\nbackend-path = [\".\"]\n[project]\nname = \"evil\"\nversion = \"0.1\"\n",
                )
                .unwrap();
                let backend = format!(
                    r#"import os, socket, json
def _try(f):
    try:
        f()
        return "LEAKED"
    except Exception as e:
        return "held: " + type(e).__name__
def _direct():
    socket.create_connection(("1.1.1.1", 443), timeout=5).close()
def _proxied():
    s = socket.create_connection(("127.0.0.1", {port}), timeout=5)
    s.sendall(b"CONNECT evil.example:443 HTTP/1.1\r\n\r\n")
    r = s.recv(64)
    s.close()
    if b" 200 " not in r:
        raise PermissionError(r.decode())
def get_requires_for_build_wheel(config_settings=None):
    return []
def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    out = {{
        "bait": _try(lambda: open({bait:?}).read()),
        "ssh": _try(lambda: os.listdir({ssh:?})),
        "write": _try(lambda: open({outside:?}, "w").write("x")),
        "tools": _try(lambda: os.listdir({tools:?})),
        "direct": _try(_direct),
        "proxy": _try(_proxied),
    }}
    open({found:?}, "w").write(json.dumps(out))
    raise RuntimeError("not building")
"#,
                    bait = bait.display().to_string(),
                    ssh = format!("{home}/.ssh"),
                    outside = outside.display().to_string(),
                    tools = root.join("tools").display().to_string(),
                    found = work.join("found.json").display().to_string(),
                );
                std::fs::write(package.join("evil_backend.py"), backend).unwrap();
                let proxy = std::ffi::OsString::from(format!("http://127.0.0.1:{port}"));
                let env: Vec<(&str, std::ffi::OsString)> = vec![
                    ("PATH", "/usr/bin:/bin".into()),
                    ("HTTPS_PROXY", proxy),
                    ("UV_NO_CONFIG", "1".into()),
                    ("UV_CACHE_DIR", work.join(".cache").into()),
                    ("UV_PYTHON_INSTALL_DIR", work.join("python").into()),
                    ("UV_PYTHON_PREFERENCE", "only-managed".into()),
                ];
                let uv = tools.join("uv");
                let args = |a: &[&str]| a.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
                let got = cell.exec(&uv, &args(&["python", "install", PYTHON]), &env, STEP).await.unwrap();
                assert!(got.ok(), "{got:?}");
                let python = work.join("python").join(python_in(&work.join("python")).unwrap()).join("bin/python3");
                let mut install = args(&["pip", "install", "--break-system-packages", "--python"]);
                install.push(python.into());
                install.push("./evil".into());
                let tried = cell.exec(&uv, &install, &env, STEP).await.unwrap();
                assert!(!tried.ok(), "{tried:?}");
                std::fs::read_to_string(work.join("found.json")).unwrap_or_else(|_| format!("no findings: {tried:?}"))
            })
            .await
            .unwrap();
        let found: serde_json::Value = serde_json::from_str(&found).unwrap_or_else(|_| panic!("{found}"));
        for what in ["bait", "ssh", "write", "tools", "direct", "proxy"] {
            let said = found[what].as_str().unwrap_or_default();
            assert!(said.starts_with("held"), "{what}: {said} ({found})");
        }
        assert!(!outside.exists());
        let audit = std::fs::read_to_string(proxy::audit_file(&root)).unwrap();
        assert!(audit.contains("evil.example") && audit.contains("not a host this room may reach"), "{audit}");
    }

    /// The whole way, from nothing: uv fetched and checked, Python and a
    /// package installed through the proxy, the environment kept read-only, and
    /// a sealed room using it with no network at all. Run by hand: it reaches
    /// GitHub and PyPI.
    #[tokio::test]
    #[ignore = "fetches uv, Python and a package; run by hand"]
    async fn an_environment_is_made_from_nothing_and_used_in_a_sealed_room() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Application Support/machine");
        let rooms = Rooms::at(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let made = make(&rooms, &root, "env-test", &["idna".to_string()]).await;
        let made = match made {
            Ok(made) => made,
            Err(Failed::Step { step, said }) => panic!("{step}: {said}"),
            Err(other) => panic!("{other}"),
        };
        assert!(made.packages.iter().any(|p| p.starts_with("idna==")), "{made:?}");
        let bin = bin_of(&root, &made.id).expect("kept");
        assert!(std::fs::write(bin.join("x"), "").is_err(), "it is read-only");

        let cell = rooms.open("user").await.unwrap();
        let script = format!("PATH='{}':\"$PATH\"; python3 -c 'import idna, sys; print(idna.encode(\"bücher.example\").decode(), sys.version.split()[0])'", bin.display());
        let ran = rooms.run(&cell, &script, Limits::default()).await.unwrap().ran;
        assert!(ran.ok() && ran.stdout.contains("xn--bcher-kva.example"), "{ran:?}");
        let tried = rooms.run(&cell, &format!("PATH='{}':\"$PATH\"; python3 -m pip install requests", bin.display()), Limits::default()).await.unwrap().ran;
        assert!(!tried.ok(), "a sealed room installed something: {tried:?}");
        let _ = make_writable_and_remove(&root.join("tools")).await;
    }
}
