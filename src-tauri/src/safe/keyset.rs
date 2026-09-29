//! Creating a Safe, opening it, and choosing how hard Argon2id should work.
//!
//! The first place in `safe/` that touches the disk, and only for one file:
//! `{vault}/Safe/keyset.safe`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::crypto::{self, CryptoError, KdfParams, Key, SecretKey};
use super::format::{FormatError, Keyset, KeysetHeader, OpenError};

pub const SAFE_DIR: &str = "Safe";
pub const KEYSET_FILE: &str = "keyset.safe";

/// Shorter than this and the master password is refused outright. The meter
/// on screen asks for more; this is only the floor under it.
pub const MIN_PASSWORD_CHARS: usize = 10;

pub fn safe_dir(vault: &Path) -> PathBuf {
    vault.join(SAFE_DIR)
}

pub fn keyset_path(vault: &Path) -> PathBuf {
    safe_dir(vault).join(KEYSET_FILE)
}

#[derive(Debug, thiserror::Error)]
pub enum KeysetError {
    #[error("this vault already has a Safe")]
    AlreadyExists,
    #[error("this vault has no Safe yet")]
    Missing,
    #[error("the master password needs at least {MIN_PASSWORD_CHARS} characters")]
    PasswordTooShort,
    /// A keyset that asks for less work than any build ever wrote. Refused
    /// rather than opened: somebody who can write the file could otherwise
    /// make the next unlock cheap to brute-force.
    #[error("keyset.safe asks for less protection than Synabit allows; it was not written by Synabit")]
    BelowFloor,
    #[error("keyset.safe is unreadable: {0}")]
    Format(#[from] FormatError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("could not {what} {path}: {source}")]
    Io { what: &'static str, path: String, source: std::io::Error },
}

impl KeysetError {
    fn io(what: &'static str, path: &Path, source: std::io::Error) -> Self {
        KeysetError::Io { what, path: path.display().to_string(), source }
    }
}

/// A Safe that has just been created: the keyset on disk, the key it guards,
/// and the Secret Key — the one moment the Secret Key exists outside the
/// keychain, so that the caller can store it and show it once.
pub struct Created {
    pub keyset: Keyset,
    pub safe_key: Key,
    pub secret_key: SecretKey,
}

/// Create a Safe in `vault`.
///
/// Writes `keyset.safe` and nothing else. Refuses if one is already there —
/// a second Safe written over the first is every item in it lost.
pub fn create(vault: &Path, password: &str, kdf: KdfParams) -> Result<Created, KeysetError> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(KeysetError::PasswordTooShort);
    }
    if !kdf.meets_floor() {
        return Err(KeysetError::BelowFloor);
    }
    let path = keyset_path(vault);
    if path.exists() {
        return Err(KeysetError::AlreadyExists);
    }

    let secret_key = SecretKey::random()?;
    let safe_key = Key::random()?;
    let header = KeysetHeader {
        safe_id: crypto::random_bytes()?,
        key_epoch: 1,
        keyset_revision: 1,
        kdf,
        kdf_salt: crypto::random_bytes()?,
    };
    let auk = crypto::derive_auk(password.as_bytes(), &header.kdf_salt, kdf, &secret_key)?;
    let keyset = Keyset::seal(header, &auk, &safe_key, crypto::random_bytes()?)?;

    let dir = safe_dir(vault);
    std::fs::create_dir_all(&dir).map_err(|e| KeysetError::io("create", &dir, e))?;
    let bytes = keyset.encode();
    write_new(&path, &bytes)?;
    super::sync::held(vault, "keyset", &bytes);
    Ok(Created { keyset, safe_key, secret_key })
}

/// Read `keyset.safe` without opening it — enough to know the Safe's id, and
/// so which Secret Key to ask the keychain for.
pub fn read(vault: &Path) -> Result<Keyset, KeysetError> {
    let path = keyset_path(vault);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(KeysetError::Missing),
        Err(e) => return Err(KeysetError::io("read", &path, e)),
    };
    Ok(Keyset::decode(&bytes)?)
}

/// Open a keyset with the master password and the Secret Key. Slow on purpose:
/// this is where Argon2id runs.
pub fn unlock(keyset: &Keyset, password: &str, secret_key: &SecretKey) -> Result<Key, KeysetError> {
    if !keyset.header.kdf.meets_floor() {
        return Err(KeysetError::BelowFloor);
    }
    let auk = crypto::derive_auk(password.as_bytes(), &keyset.header.kdf_salt, keyset.header.kdf, secret_key)?;
    Ok(keyset.open(&auk)?)
}

/// Seal the same Safe Key under a new master password, with a fresh salt, and
/// replace `keyset.safe`. No item is touched: they are sealed under the Safe
/// Key, which does not change.
pub fn change_password(
    vault: &Path,
    keyset: &Keyset,
    safe_key: &Key,
    new_password: &str,
    secret_key: &SecretKey,
    kdf: KdfParams,
) -> Result<Keyset, KeysetError> {
    if new_password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(KeysetError::PasswordTooShort);
    }
    if !kdf.meets_floor() {
        return Err(KeysetError::BelowFloor);
    }
    let header = KeysetHeader {
        kdf,
        kdf_salt: crypto::random_bytes()?,
        keyset_revision: keyset.header.keyset_revision + 1,
        ..keyset.header.clone()
    };
    let auk = crypto::derive_auk(new_password.as_bytes(), &header.kdf_salt, kdf, secret_key)?;
    let next = Keyset::seal(header, &auk, safe_key, crypto::random_bytes()?)?;
    let bytes = next.encode();
    super::store::write_atomic(&keyset_path(vault), &bytes).map_err(|e| KeysetError::io("write", &keyset_path(vault), e))?;
    super::sync::held(vault, "keyset", &bytes);
    super::sync::saw_keyset(vault, next.header.keyset_revision);
    Ok(next)
}

/// A keyset for a new Safe Key — the next epoch, the next revision, a fresh
/// salt — under the same password and Secret Key. Not written: see
/// [`write_rotated`], and `Unlocked::rotate_safe_key` for the order.
pub fn rotated(vault: &Path, keyset: &Keyset, new_key: &Key, password: &str, secret_key: &SecretKey) -> Result<Keyset, KeysetError> {
    let seen = super::sync::Seen::load(vault).keyset;
    let header = KeysetHeader {
        key_epoch: keyset.header.key_epoch.saturating_add(1),
        keyset_revision: keyset.header.keyset_revision.max(seen).saturating_add(1),
        kdf_salt: crypto::random_bytes()?,
        ..keyset.header.clone()
    };
    let auk = crypto::derive_auk(password.as_bytes(), &header.kdf_salt, header.kdf, secret_key)?;
    Ok(Keyset::seal(header, &auk, new_key, crypto::random_bytes()?)?)
}

pub fn write_rotated(vault: &Path, keyset: &Keyset) -> Result<(), KeysetError> {
    let bytes = keyset.encode();
    super::store::write_atomic(&keyset_path(vault), &bytes).map_err(|e| KeysetError::io("write", &keyset_path(vault), e))?;
    super::sync::held(vault, "keyset", &bytes);
    super::sync::saw_keyset(vault, keyset.header.keyset_revision);
    Ok(())
}

/// The same Safe Key under a new Secret Key: for a Secret Key that may have
/// been seen — an Emergency Kit left somewhere. Every other device then needs
/// the new words once.
pub fn change_secret_key(vault: &Path, keyset: &Keyset, safe_key: &Key, password: &str) -> Result<(Keyset, SecretKey), KeysetError> {
    let secret_key = SecretKey::random()?;
    let seen = super::sync::Seen::load(vault).keyset;
    let header = KeysetHeader {
        keyset_revision: keyset.header.keyset_revision.max(seen).saturating_add(1),
        kdf_salt: crypto::random_bytes()?,
        ..keyset.header.clone()
    };
    let auk = crypto::derive_auk(password.as_bytes(), &header.kdf_salt, header.kdf, &secret_key)?;
    let next = Keyset::seal(header, &auk, safe_key, crypto::random_bytes()?)?;
    write_rotated(vault, &next)?;
    Ok((next, secret_key))
}

/// Make `from` — an older keyset the user just opened with its password —
/// the Safe's keyset again, above `above` so that it replaces the current one
/// on every device. The Safe Key inside is the same; only its wrapping moves.
pub fn restore(
    vault: &Path,
    from: &Keyset,
    safe_key: &Key,
    password: &str,
    secret_key: &SecretKey,
    above: u64,
) -> Result<Keyset, KeysetError> {
    let header = KeysetHeader {
        kdf_salt: crypto::random_bytes()?,
        keyset_revision: above.max(from.header.keyset_revision).saturating_add(1),
        ..from.header.clone()
    };
    let auk = crypto::derive_auk(password.as_bytes(), &header.kdf_salt, header.kdf, secret_key)?;
    let next = Keyset::seal(header, &auk, safe_key, crypto::random_bytes()?)?;
    let bytes = next.encode();
    super::store::write_atomic(&keyset_path(vault), &bytes).map_err(|e| KeysetError::io("write", &keyset_path(vault), e))?;
    super::sync::held(vault, "keyset", &bytes);
    super::sync::saw_keyset(vault, next.header.keyset_revision);
    Ok(next)
}

/// Write a file that must not already exist, in one step: to a temporary name
/// beside it, then linked into place — which fails, rather than replacing,
/// if something appeared there meanwhile. The temporary name is new each
/// time, so one left by a crash does not block every later try.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), KeysetError> {
    use std::io::Write;
    let suffix = hex::encode(crypto::random_bytes::<4>()?);
    let tmp = path.with_extension(format!("safe.{suffix}.new"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp).map_err(|e| KeysetError::io("create", &tmp, e))?;
    let written = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    let placed = written.and_then(|_| std::fs::hard_link(&tmp, path));
    let _ = std::fs::remove_file(&tmp);
    match placed {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(KeysetError::AlreadyExists),
        Err(e) => Err(KeysetError::io("write", path, e)),
    }
}

// ─── calibration ─────────────────────────────────────────

/// How long one unlock should take on the machine that creates the Safe.
pub const TARGET: std::ops::RangeInclusive<Duration> = Duration::from_millis(500)..=Duration::from_millis(1000);
/// More passes than this and the machine is fast enough that the time is
/// better spent on memory, which is already at its start value.
const MAX_T: u32 = 12;

/// Argon2id parameters that take about [`TARGET`] on this machine.
///
/// Starts at [`KdfParams::STARTING`]. Too slow, or unable to allocate: halve
/// the memory, never below the floor. Too fast: add passes, which cost time
/// roughly in proportion. One Argon2id run per step, a handful of steps.
pub fn calibrate() -> KdfParams {
    calibrate_with(|params| {
        let started = Instant::now();
        let secret = SecretKey::from_bytes([0; 16]);
        crypto::derive_auk(b"calibration", &[0; 32], params, &secret).ok().map(|_| started.elapsed())
    })
}

/// [`calibrate`] with the measurement handed in, so the search can be tested
/// without spending seconds in Argon2id. `measure` returns `None` when the
/// parameters could not run at all — on a phone, 256 MiB may not be there.
pub fn calibrate_with(mut measure: impl FnMut(KdfParams) -> Option<Duration>) -> KdfParams {
    let mut params = KdfParams::STARTING;
    loop {
        match measure(params) {
            Some(took) if took <= *TARGET.end() => {
                if took >= *TARGET.start() || params.t >= MAX_T {
                    return params;
                }
                // Scale the passes to land in the middle of the target.
                let aim = (TARGET.start().as_secs_f64() + TARGET.end().as_secs_f64()) / 2.0;
                let per_pass = took.as_secs_f64().max(1e-6) / f64::from(params.t);
                let t = ((aim / per_pass).floor() as u32).clamp(params.t + 1, MAX_T);
                params.t = t;
                // One more measurement to confirm; if it overshoots, the branch
                // below walks back.
                if let Some(check) = measure(params) {
                    if check <= *TARGET.end() {
                        return params;
                    }
                }
                params.t = (params.t - 1).max(KdfParams::FLOOR.t);
                return params;
            }
            // Too slow or could not run: less memory, if there is any to give.
            _ if params.m_kib / 2 >= KdfParams::FLOOR.m_kib => params.m_kib /= 2,
            // At the floor already. Slow machines get the floor, not less.
            _ => return KdfParams { t: params.t.max(KdfParams::FLOOR.t), ..params },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model machine: time is linear in memory × passes.
    fn machine(ms_per_gib_pass: f64, max_m_kib: u32) -> impl FnMut(KdfParams) -> Option<Duration> {
        move |p: KdfParams| {
            if p.m_kib > max_m_kib {
                return None;
            }
            let gib = f64::from(p.m_kib) / (1024.0 * 1024.0);
            Some(Duration::from_secs_f64(ms_per_gib_pass * gib * f64::from(p.t) / 1000.0))
        }
    }

    #[test]
    fn a_fast_machine_gets_more_passes_within_the_target() {
        let p = calibrate_with(machine(400.0, u32::MAX));
        assert_eq!(p.m_kib, KdfParams::STARTING.m_kib);
        let took = machine(400.0, u32::MAX)(p).unwrap();
        assert!(TARGET.contains(&took) || p.t == MAX_T, "{p:?} took {took:?}");
    }

    #[test]
    fn a_slow_machine_gets_less_memory_but_never_below_the_floor() {
        let p = calibrate_with(machine(20_000.0, u32::MAX));
        assert_eq!(p, KdfParams { m_kib: KdfParams::FLOOR.m_kib, ..KdfParams::STARTING });
        assert!(p.meets_floor());
    }

    #[test]
    fn a_phone_that_cannot_allocate_the_start_value_steps_down() {
        let p = calibrate_with(machine(2_000.0, 128 * 1024));
        assert!(p.m_kib <= 128 * 1024 && p.meets_floor(), "{p:?}");
    }

    #[test]
    fn the_result_always_meets_the_floor() {
        for speed in [1.0, 50.0, 400.0, 2_000.0, 10_000.0, 100_000.0] {
            for max in [64 * 1024, 100 * 1024, u32::MAX] {
                assert!(calibrate_with(machine(speed, max)).meets_floor(), "speed {speed} max {max}");
            }
        }
    }

    #[test]
    fn create_refuses_weak_settings_and_a_second_safe() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(create(dir.path(), "short", KdfParams::FLOOR), Err(KeysetError::PasswordTooShort)));
        let weak = KdfParams { m_kib: 1024, t: 1, p: 1 };
        assert!(matches!(create(dir.path(), "long enough password", weak), Err(KeysetError::BelowFloor)));
        assert!(!keyset_path(dir.path()).exists(), "a refused create wrote something");
    }

    /// The whole cycle at the real floor — the one test that pays for a real
    /// Argon2id run at 64 MiB, because the floor is what users get on the
    /// slowest machine and it has to work.
    #[test]
    fn create_unlock_change_password_at_the_floor() {
        let dir = tempfile::tempdir().unwrap();
        let created = create(dir.path(), "correct horse battery", KdfParams::FLOOR).unwrap();
        assert!(matches!(
            create(dir.path(), "correct horse battery", KdfParams::FLOOR),
            Err(KeysetError::AlreadyExists)
        ));

        let on_disk = read(dir.path()).unwrap();
        assert_eq!(on_disk, created.keyset);
        let sk = SecretKey::from_bytes(*created.secret_key.as_bytes());
        let key = unlock(&on_disk, "correct horse battery", &sk).unwrap();
        assert_eq!(key.as_bytes(), created.safe_key.as_bytes());
        assert!(matches!(
            unlock(&on_disk, "wrong horse battery", &sk),
            Err(KeysetError::Open(OpenError::WrongPassword))
        ));

        let next = change_password(dir.path(), &on_disk, &key, "a new horse battery", &sk, KdfParams::FLOOR).unwrap();
        assert_eq!(next.header.keyset_revision, 2);
        assert_eq!(next.header.safe_id, on_disk.header.safe_id);
        let reread = read(dir.path()).unwrap();
        assert!(unlock(&reread, "correct horse battery", &sk).is_err());
        assert_eq!(unlock(&reread, "a new horse battery", &sk).unwrap().as_bytes(), created.safe_key.as_bytes());
    }

    #[test]
    fn a_keyset_below_the_floor_is_not_opened() {
        let weak = KdfParams { m_kib: 256, t: 1, p: 1 };
        let sk = SecretKey::from_bytes([1; 16]);
        let auk = crypto::derive_auk(b"pw", &[0; 32], weak, &sk).unwrap();
        let header = KeysetHeader { safe_id: [0; 16], key_epoch: 1, keyset_revision: 1, kdf: weak, kdf_salt: [0; 32] };
        let keyset = Keyset::seal(header, &auk, &Key::from_bytes([9; 32]), [0; 24]).unwrap();
        assert!(matches!(unlock(&keyset, "pw", &sk), Err(KeysetError::BelowFloor)));
    }

    /// Another device's keyset arrives — a password changed there, or one a
    /// peer forged. Ours is kept aside; the user who never changed the
    /// password opens that one and makes it current again, above the other.
    #[test]
    fn a_keyset_replaced_by_another_devices_can_be_put_back() {
        let a = tempfile::tempdir().unwrap();
        let created = create(a.path(), "correct horse battery", KdfParams::FLOOR).unwrap();
        let sk = SecretKey::from_bytes(*created.secret_key.as_bytes());
        let ours = std::fs::read(keyset_path(a.path())).unwrap();

        // The other device: same Safe, another password, a higher revision.
        let b = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(safe_dir(b.path())).unwrap();
        std::fs::write(keyset_path(b.path()), &ours).unwrap();
        let theirs = change_password(b.path(), &created.keyset, &created.safe_key, "someone elses pass", &sk, KdfParams::FLOOR).unwrap();

        let decision = super::super::sync::decide(a.path(), "Safe/keyset.safe", &theirs.encode());
        super::super::sync::apply(a.path(), "Safe/keyset.safe", &theirs.encode(), &decision).unwrap();
        assert!(unlock(&read(a.path()).unwrap(), "correct horse battery", &sk).is_err());
        let asides = super::super::sync::keyset_asides(a.path());
        assert_eq!(asides.len(), 1);

        let older = Keyset::decode(&std::fs::read(&asides[0]).unwrap()).unwrap();
        let key = unlock(&older, "correct horse battery", &sk).unwrap();
        let restored = restore(a.path(), &older, &key, "correct horse battery", &sk, theirs.header.keyset_revision).unwrap();
        assert!(restored.header.keyset_revision > theirs.header.keyset_revision, "it must replace theirs elsewhere too");
        assert_eq!(unlock(&read(a.path()).unwrap(), "correct horse battery", &sk).unwrap().as_bytes(), created.safe_key.as_bytes());
    }

    #[test]
    fn a_leftover_from_a_crashed_create_does_not_block_the_next() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(safe_dir(dir.path())).unwrap();
        std::fs::write(keyset_path(dir.path()).with_extension("safe.new"), b"half").unwrap();
        create(dir.path(), "correct horse battery", KdfParams::FLOOR).unwrap();
        let left: Vec<_> = std::fs::read_dir(safe_dir(dir.path())).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 2, "the keyset and the old leftover, no new one: {left:?}");
    }

    #[test]
    fn a_missing_keyset_is_missing_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(read(dir.path()), Err(KeysetError::Missing)));
    }
}
