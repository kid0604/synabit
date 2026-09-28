//! The primitives, and the only ways Safe combines them.
//!
//! Three algorithms, all already in this binary for sync and app lock:
//!
//! * **XChaCha20-Poly1305** for every encryption. The 192-bit nonce is what
//!   lets every device pick nonces at random with no counter to coordinate;
//!   ChaCha20 is constant-time in software, so an Android phone without AES
//!   instructions is exactly as fast and as safe as a laptop.
//! * **Argon2id** (RFC 9106) for the one secret a person chooses.
//! * **BLAKE3** for everything derived: `derive_key` with a distinct context
//!   string per purpose, so no key is ever used for two things.
//!
//! There is no public-key cryptography here. Every key is symmetric and 256
//! bits wide, which is what makes the store quantum-resistant without having to
//! choose a post-quantum algorithm: Grover's algorithm halves the exponent, and
//! 2¹²⁸ is still out of reach.
//!
//! Every function takes its nonces and keys as arguments rather than drawing
//! them itself. That is what makes the frozen vectors in `testdata/` possible,
//! and it puts the one call that must be random — [`random_bytes`] — where a
//! reader can see it.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use zeroize::{Zeroize, Zeroizing};

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const TAG_LEN: usize = 16;
pub const SALT_LEN: usize = 32;
pub const SECRET_KEY_LEN: usize = 16;
/// A 32-byte key sealed under another: the key plus the Poly1305 tag.
pub const WRAPPED_KEY_LEN: usize = KEY_LEN + TAG_LEN;

/// Padded plaintext is a multiple of this. See [`pad`].
pub const PAD_BLOCK: usize = 256;
/// And never shorter than this, so that an item holding one short password
/// looks like an item holding a paragraph of notes.
pub const PAD_MIN: usize = 512;

/// The context strings handed to BLAKE3's `derive_key`, one per purpose.
///
/// BLAKE3 asks for context strings that are hardcoded, globally unique and
/// application-specific, and suggests a date for exactly this reason: **a string
/// here is never edited once it has shipped.** A new purpose, or a new version
/// of an old one, gets a new string. Changing one silently turns every key
/// derived from it into a different key, and every item sealed with the old one
/// into noise.
pub mod ctx {
    pub const AUK: &str = "synabit safe 2026-09 account unlock key";
    pub const KEY_CHECK: &str = "synabit safe 2026-09 key check";
    pub const WRAP_SAFE_KEY: &str = "synabit safe 2026-09 wrap safe key";
    pub const WRAP_ITEM_KEY: &str = "synabit safe 2026-09 wrap item key";
    pub const FINGERPRINT: &str = "synabit safe 2026-09 leak guard fingerprint";
    pub const AUDIT: &str = "synabit safe 2026-09 audit log";
    pub const LOCAL: &str = "synabit safe 2026-09 device local state";
    pub const EXPORT: &str = "synabit safe 2026-09 export archive";
    pub const BIOMETRIC: &str = "synabit safe 2026-09 biometric wrap";

    /// Every one of them, for the tests that pin them.
    pub const ALL: [&str; 9] =
        [AUK, KEY_CHECK, WRAP_SAFE_KEY, WRAP_ITEM_KEY, FINGERPRINT, AUDIT, LOCAL, EXPORT, BIOMETRIC];
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("the system random number generator failed")]
    Rng,
    #[error("Argon2id refused its parameters: {0}")]
    Kdf(String),
    #[error("encryption failed")]
    Seal,
    /// Wrong key or altered data. AEAD cannot say which, and neither can this.
    #[error("decryption failed: the key is wrong or the data was altered")]
    Open,
    #[error("the padded plaintext is malformed")]
    Padding,
}

/// A 256-bit key, in a page kept out of swap and wiped when dropped.
///
/// The bytes live at one address for the key's whole life (see
/// `safe::memory`), so moving a `Key` moves a handle, not a copy of the
/// secret. Not `Clone` — a copy is a second place the key has to be wiped
/// from. `Debug` prints nothing of the key, so a key that ends up in a
/// `log::debug!("{:?}", …)` costs a line of log and not the store.
pub struct Key(super::memory::Slot);

impl Key {
    /// Take `bytes` into protected memory and wipe the copy handed in.
    pub fn from_bytes(mut bytes: [u8; KEY_LEN]) -> Self {
        let key = Self(super::memory::Slot::new(&bytes));
        bytes.zeroize();
        key
    }

    /// A fresh key from the operating system's random number generator.
    pub fn random() -> Result<Self, CryptoError> {
        random_bytes().map(Self::from_bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        self.0.bytes()
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(‹redacted›)")
    }
}

/// The Secret Key: 128 random bits generated on the device that created the
/// Safe, and the second thing — with the master password — that opening it
/// takes.
///
/// Its job is to make a stolen blob worthless on its own. Without it, a copy of
/// `keyset.safe` taken from a sync server or a git remote is an offline
/// password-guessing problem, and people choose guessable passwords. With it,
/// the attacker is searching 2¹²⁸ no matter what the password was.
pub struct SecretKey([u8; SECRET_KEY_LEN]);

impl SecretKey {
    pub fn from_bytes(bytes: [u8; SECRET_KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn random() -> Result<Self, CryptoError> {
        random_bytes().map(Self)
    }

    pub fn as_bytes(&self) -> &[u8; SECRET_KEY_LEN] {
        &self.0
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(‹redacted›)")
    }
}

/// Argon2id's cost, as stored in `keyset.safe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory, in KiB.
    pub m_kib: u32,
    /// Passes over that memory.
    pub t: u32,
    /// Lanes.
    pub p: u8,
}

impl KdfParams {
    /// Where calibration starts: 256 MiB, three passes, four lanes.
    pub const STARTING: Self = Self { m_kib: 256 * 1024, t: 3, p: 4 };

    /// RFC 9106's second recommended setting, for memory-constrained machines,
    /// and the least a Safe is ever created or opened with.
    ///
    /// The app lock's PIN uses 19 MiB (`commands::app_lock`). That sits behind
    /// the OS keychain; this guards a file that may be copied off to be attacked
    /// offline, so it is held to more.
    pub const FLOOR: Self = Self { m_kib: 64 * 1024, t: 3, p: 1 };

    pub fn meets_floor(&self) -> bool {
        self.m_kib >= Self::FLOOR.m_kib && self.t >= Self::FLOOR.t && self.p >= Self::FLOOR.p
    }
}

/// `N` bytes from the operating system's random number generator.
///
/// The OS generator directly, not the thread-local one sync uses for nonces:
/// what this makes lives for years, and the direct path is the one with nothing
/// in between to reason about.
pub fn random_bytes<const N: usize>() -> Result<[u8; N], CryptoError> {
    use rand::TryRngCore;
    let mut bytes = [0u8; N];
    rand::rngs::OsRng.try_fill_bytes(&mut bytes).map_err(|_| CryptoError::Rng)?;
    Ok(bytes)
}

/// The Account Unlock Key, from the master password and the Secret Key.
///
/// ```text
/// P   = Argon2id(password, salt, m, t, p) → 32 bytes
/// AUK = BLAKE3.derive_key(ctx::AUK, P ‖ SecretKey)
/// ```
///
/// Argon2id is the slow part and runs on the password alone; the Secret Key is
/// mixed in afterwards by BLAKE3. Either one missing and the AUK is unreachable.
///
/// This does not enforce [`KdfParams::FLOOR`]: the frozen vectors use tiny
/// parameters so the test suite stays fast. Creating or opening a real Safe is
/// where the floor is checked.
pub fn derive_auk(
    password: &[u8],
    salt: &[u8; SALT_LEN],
    params: KdfParams,
    secret_key: &SecretKey,
) -> Result<Key, CryptoError> {
    let argon_params = Params::new(params.m_kib, params.t, u32::from(params.p), Some(KEY_LEN))
        .map_err(|e| CryptoError::Kdf(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);

    let mut stretched = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(password, salt, stretched.as_mut())
        .map_err(|e| CryptoError::Kdf(e.to_string()))?;

    let mut material = Zeroizing::new([0u8; KEY_LEN + SECRET_KEY_LEN]);
    material[..KEY_LEN].copy_from_slice(stretched.as_ref());
    material[KEY_LEN..].copy_from_slice(secret_key.as_bytes());
    Ok(Key::from_bytes(blake3::derive_key(ctx::AUK, material.as_ref())))
}

/// A key for one purpose, derived from a key for another.
pub fn subkey(context: &'static str, key: &Key) -> Key {
    Key::from_bytes(blake3::derive_key(context, key.as_bytes()))
}

/// What `keyset.safe` stores to recognise the right AUK without decrypting
/// anything.
///
/// # Why it exists: key commitment
///
/// Poly1305 does not commit to its key — a ciphertext can be built that
/// decrypts validly under many keys. When those keys come from passwords, that
/// becomes a *partitioning oracle* (Len, Grubbs, Ristenpart, USENIX Security
/// 2021): one attempt tests many password guesses at once. Checking this value
/// first, and only then decrypting, closes it where keys come from passwords.
/// Item keys are random, so nothing below the keyset needs the same.
///
/// It also tells a wrong password apart from a damaged file, which a failed
/// AEAD alone cannot.
pub fn key_check(auk: &Key) -> [u8; KEY_LEN] {
    blake3::derive_key(ctx::KEY_CHECK, auk.as_bytes())
}

/// Whether `auk` is the key `stored` was made from. Constant-time, because
/// `blake3::Hash` compares in constant time.
pub fn key_check_matches(auk: &Key, stored: &[u8; KEY_LEN]) -> bool {
    blake3::Hash::from(key_check(auk)) == blake3::Hash::from(*stored)
}

/// XChaCha20-Poly1305: `plaintext` sealed under `key`, bound to `aad`.
///
/// The output is ciphertext followed by the 16-byte tag, the same length as the
/// plaintext plus [`TAG_LEN`].
pub fn seal(key: &Key, nonce: &[u8; NONCE_LEN], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    XChaCha20Poly1305::new(key.as_bytes().into())
        .encrypt(XNonce::from_slice(nonce), Payload { msg: plaintext, aad })
        .map_err(|_| CryptoError::Seal)
}

/// The inverse of [`seal`]. The plaintext comes back in a buffer that wipes
/// itself.
pub fn open(key: &Key, nonce: &[u8; NONCE_LEN], aad: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    XChaCha20Poly1305::new(key.as_bytes().into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: sealed, aad })
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::Open)
}

/// One key sealed under another.
pub fn wrap_key(kek: &Key, nonce: &[u8; NONCE_LEN], aad: &[u8], key: &Key) -> Result<[u8; WRAPPED_KEY_LEN], CryptoError> {
    let sealed = seal(kek, nonce, aad, key.as_bytes())?;
    sealed.try_into().map_err(|_| CryptoError::Seal)
}

pub fn unwrap_key(kek: &Key, nonce: &[u8; NONCE_LEN], aad: &[u8], wrapped: &[u8; WRAPPED_KEY_LEN]) -> Result<Key, CryptoError> {
    let opened = open(kek, nonce, aad, wrapped)?;
    let bytes: [u8; KEY_LEN] = opened.as_slice().try_into().map_err(|_| CryptoError::Open)?;
    Ok(Key::from_bytes(bytes))
}

/// `body`, prefixed with its length and padded with zeros to a multiple of
/// [`PAD_BLOCK`], at least [`PAD_MIN`].
///
/// ```text
/// u32_le(len(body)) ‖ body ‖ 0x00 …
/// ```
///
/// Ciphertext is exactly as long as plaintext, so without this the size of an
/// item file tells anyone holding it how long the password inside is. With it,
/// every item holding under half a kilobyte is the same size.
pub fn pad(body: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let len = u32::try_from(body.len()).map_err(|_| CryptoError::Padding)?;
    let unpadded = 4 + body.len();
    let total = unpadded.div_ceil(PAD_BLOCK).max(1) * PAD_BLOCK;
    let total = total.max(PAD_MIN);

    let mut out = Zeroizing::new(Vec::with_capacity(total));
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(body);
    out.resize(total, 0);
    Ok(out)
}

/// The inverse of [`pad`].
///
/// Strict: the length must be one [`pad`] could have produced and every byte
/// of padding must be zero. The input has already passed AEAD, so this is not
/// defending against tampering — it is refusing to accept two encodings of one
/// item, which is how parsers drift apart.
pub fn unpad(padded: &[u8]) -> Result<&[u8], CryptoError> {
    if padded.len() < PAD_MIN || !padded.len().is_multiple_of(PAD_BLOCK) {
        return Err(CryptoError::Padding);
    }
    let len = u32::from_le_bytes(padded[..4].try_into().expect("four bytes")) as usize;
    let end = 4usize.checked_add(len).filter(|&end| end <= padded.len()).ok_or(CryptoError::Padding)?;
    // The shortest padding `pad` would have chosen for this body.
    let expected_total = end.div_ceil(PAD_BLOCK).max(1) * PAD_BLOCK;
    if padded.len() != expected_total.max(PAD_MIN) || padded[end..].iter().any(|&b| b != 0) {
        return Err(CryptoError::Padding);
    }
    Ok(&padded[4..end])
}
