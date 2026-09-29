//! Copying a value to the clipboard from Rust, and taking it back off.
//!
//! # Why Rust and not `navigator.clipboard`
//!
//! A string in the WebView cannot be wiped — JavaScript strings are immutable
//! and live until the garbage collector decides otherwise. Copying from Rust
//! means the value of a password the user only wanted to paste never enters
//! the WebView at all.
//!
//! # What the flags do
//!
//! Each platform has a way to say "this is a password": macOS clipboard
//! managers honour `org.nspasteboard.ConcealedType`, Windows keeps the entry
//! out of its clipboard history, cloud clipboard and monitoring apps, KDE's
//! Klipper honours `x-kde-passwordManagerHint`. `exclude_from_history` in
//! `arboard` sets whichever one applies. None of them is a guarantee — a
//! clipboard manager is free to ignore them — which is why the value is also
//! taken back off after a while.
//!
//! # Taking it back off only if it is still ours
//!
//! If the user copied something else in the meantime, clearing the clipboard
//! would destroy *their* copy. So the value copied is remembered as a keyed
//! hash, under a key made fresh for this run, and the clipboard is cleared only
//! if what is on it still hashes the same. The hash is keyed so that a memory
//! dump taken in those thirty seconds does not hold a fast, unsalted hash of a
//! password.

use std::sync::Mutex;

use super::item::SecretString;
use super::session::SafeError;

#[derive(Default)]
pub struct SafeClipboard {
    #[cfg(desktop)]
    inner: Mutex<Desktop>,
    #[cfg(mobile)]
    _unused: Mutex<()>,
}

#[cfg(desktop)]
#[derive(Default)]
struct Desktop {
    /// Kept for the life of the app: on Linux, what is copied stays pasteable
    /// only while the `Clipboard` that set it is alive.
    clipboard: Option<arboard::Clipboard>,
    /// Raised by every copy, so a clear scheduled by an earlier copy never
    /// clears a later one.
    generation: u64,
    copied: Option<[u8; 32]>,
    key: Option<[u8; 32]>,
}

#[cfg(desktop)]
impl Desktop {
    fn clipboard(&mut self) -> Result<&mut arboard::Clipboard, SafeError> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(|e| SafeError::Clipboard(e.to_string()))?);
        }
        Ok(self.clipboard.as_mut().expect("just set"))
    }

    fn fingerprint(&mut self, text: &str) -> Result<[u8; 32], SafeError> {
        if self.key.is_none() {
            self.key = Some(super::crypto::random_bytes().map_err(|e| SafeError::Failed(e.to_string()))?);
        }
        Ok(*blake3::keyed_hash(self.key.as_ref().expect("just set"), text.as_bytes()).as_bytes())
    }
}

impl SafeClipboard {
    /// Put `value` on the clipboard, marked as a secret. Returns the generation
    /// to hand to [`Self::clear_if_unchanged`].
    #[cfg(desktop)]
    pub fn copy(&self, value: &SecretString) -> Result<u64, SafeError> {
        let mut d = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let fingerprint = d.fingerprint(value.expose())?;
        let set = d.clipboard()?.set();
        #[cfg(target_os = "macos")]
        let set = {
            use arboard::SetExtApple;
            set.exclude_from_history()
        };
        #[cfg(target_os = "windows")]
        let set = {
            use arboard::SetExtWindows;
            set.exclude_from_history().exclude_from_cloud().exclude_from_monitoring()
        };
        #[cfg(target_os = "linux")]
        let set = {
            use arboard::SetExtLinux;
            set.exclude_from_history()
        };
        set.text(value.expose()).map_err(|e| SafeError::Clipboard(e.to_string()))?;
        d.generation += 1;
        d.copied = Some(fingerprint);
        Ok(d.generation)
    }

    /// Clear the clipboard if it still holds what copy number `generation` put
    /// there. Anything the user copied since is left alone.
    #[cfg(desktop)]
    pub fn clear_if_unchanged(&self, generation: u64) -> bool {
        let mut d = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if d.generation != generation {
            return false;
        }
        let Some(copied) = d.copied else { return false };
        let current = match d.clipboard().and_then(|c| c.get_text().map_err(|e| SafeError::Clipboard(e.to_string()))) {
            Ok(text) => zeroize::Zeroizing::new(text),
            Err(_) => return false,
        };
        let still_ours = d.fingerprint(&current).is_ok_and(|f| blake3::Hash::from(f) == blake3::Hash::from(copied));
        d.copied = None;
        if still_ours {
            if let Ok(c) = d.clipboard() {
                return c.clear().is_ok();
            }
        }
        false
    }

    /// Clear the clipboard now if it still holds the last thing copied from
    /// the Safe: the Safe locked, or the app is quitting — a clear scheduled
    /// for later would not run.
    #[cfg(desktop)]
    pub fn clear_now(&self) -> bool {
        let generation = self.inner.lock().unwrap_or_else(|p| p.into_inner()).generation;
        self.clear_if_unchanged(generation)
    }

    #[cfg(mobile)]
    pub fn clear_now(&self) -> bool {
        false
    }

    /// Phones reach the clipboard through Android's own service, with a flag
    /// of its own. That arrives with Safe on Android, in P2.
    #[cfg(mobile)]
    pub fn copy(&self, _value: &SecretString) -> Result<u64, SafeError> {
        Err(SafeError::Clipboard("copying from Safe is not available on this device yet".into()))
    }

    #[cfg(mobile)]
    pub fn clear_if_unchanged(&self, _generation: u64) -> bool {
        false
    }
}
