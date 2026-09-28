//! The bytes of `keyset.safe` and `items/<id>.safe`.
//!
//! Fixed-offset binary, little-endian, no self-description. Section 5 of
//! `docs/safe-2026-09-28.md` draws both layouts; the constants here are those
//! drawings, and the tests hold each offset to them.
//!
//! # The one rule both formats follow
//!
//! **Everything that is not ciphertext is associated data.** The header of an
//! item — which Safe it belongs to, which item it is, which revision — is bound
//! into both the wrapped item key and the body. So is every Argon2id parameter
//! in the keyset. Nothing in cleartext can be edited without the next
//! decryption failing, which is what stops a sync server from moving an item
//! into another file, rolling a revision number back, or quietly lowering the
//! cost of guessing the password.

use zeroize::Zeroizing;

use super::crypto::{
    self, ctx, CryptoError, KdfParams, Key, KEY_LEN, NONCE_LEN, SALT_LEN, TAG_LEN, WRAPPED_KEY_LEN,
};

pub const KEYSET_MAGIC: [u8; 4] = *b"SFK1";
pub const ITEM_MAGIC: [u8; 4] = *b"SFI1";
pub const FORMAT_VERSION: u16 = 1;
/// The only KDF there is. A byte rather than an assumption, so that a second
/// one can be added without a new format version.
pub const KDF_ARGON2ID: u8 = 1;

/// An item that has been deleted for good. It keeps its file, header and a
/// higher revision so that a device that was offline cannot bring it back.
pub const FLAG_TOMBSTONE: u16 = 1;

/// Bytes `0..110` of `keyset.safe`: everything before the wrap nonce.
pub const KEYSET_AAD_LEN: usize = 110;
pub const KEYSET_LEN: usize = KEYSET_AAD_LEN + NONCE_LEN + WRAPPED_KEY_LEN;
/// Bytes `0..56` of an item file, and the associated data of both its layers.
pub const ITEM_HEADER_LEN: usize = 56;
/// Where the sealed body starts.
pub const ITEM_BODY_OFFSET: usize = ITEM_HEADER_LEN + NONCE_LEN + WRAPPED_KEY_LEN + NONCE_LEN;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FormatError {
    #[error("not a Safe file of this kind")]
    BadMagic,
    #[error("written by a newer format ({0}) than this build reads")]
    UnsupportedVersion(u16),
    #[error("carries flags this build does not know ({0:#06x})")]
    UnknownFlags(u16),
    #[error("uses a key derivation this build does not know ({0})")]
    UnknownKdf(u8),
    #[error("Argon2id parameters that could never have been written")]
    BadKdfParams,
    #[error("is {found} bytes long, which no valid file is")]
    BadLength { found: usize },
    #[error("has reserved bytes set")]
    ReservedSet,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OpenError {
    /// The master password or the Secret Key is wrong. Known before any
    /// decryption is attempted — see [`crypto::key_check`].
    #[error("the master password or Secret Key is wrong")]
    WrongPassword,
    /// The key was right and the data still did not decrypt: it was altered or
    /// damaged. Never a reason to rewrite the file.
    #[error("the file is damaged or was altered")]
    Corrupt,
    #[error("the file belongs to a different Safe")]
    WrongSafe,
    /// An item file whose header names a different item than its file name
    /// does. Either a sync peer moved it or something renamed it.
    #[error("the file holds a different item than its name says")]
    Misplaced,
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

// ─── keyset.safe ─────────────────────────────────────────

/// The cleartext part of `keyset.safe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysetHeader {
    pub safe_id: [u8; 16],
    /// Raised by "rotate every key"; items carry the epoch they were sealed in.
    pub key_epoch: u32,
    /// Raised by every rewrite of the keyset: a password change, a
    /// recalibration, a rotation.
    pub keyset_revision: u64,
    pub kdf: KdfParams,
    pub kdf_salt: [u8; SALT_LEN],
}

/// `keyset.safe`, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyset {
    pub header: KeysetHeader,
    pub key_check: [u8; KEY_LEN],
    pub wrap_nonce: [u8; NONCE_LEN],
    pub wrapped_safe_key: [u8; WRAPPED_KEY_LEN],
}

impl Keyset {
    /// Seal `safe_key` under an AUK. The key check is computed here rather than
    /// passed in, so a keyset whose check disagrees with its wrap cannot be
    /// built.
    pub fn seal(header: KeysetHeader, auk: &Key, safe_key: &Key, wrap_nonce: [u8; NONCE_LEN]) -> Result<Self, CryptoError> {
        let key_check = crypto::key_check(auk);
        let aad = encode_keyset_aad(&header, &key_check);
        let kek = crypto::subkey(ctx::WRAP_SAFE_KEY, auk);
        let wrapped_safe_key = crypto::wrap_key(&kek, &wrap_nonce, &aad, safe_key)?;
        Ok(Self { header, key_check, wrap_nonce, wrapped_safe_key })
    }

    /// The Safe Key, given the AUK.
    ///
    /// The key check is compared first, in constant time, and only a match goes
    /// on to decrypt. A match followed by a failed decryption can only mean the
    /// file was changed — the AAD covers every parameter — so the two failures
    /// are reported as the different things they are.
    pub fn open(&self, auk: &Key) -> Result<Key, OpenError> {
        if !crypto::key_check_matches(auk, &self.key_check) {
            return Err(OpenError::WrongPassword);
        }
        let aad = encode_keyset_aad(&self.header, &self.key_check);
        let kek = crypto::subkey(ctx::WRAP_SAFE_KEY, auk);
        crypto::unwrap_key(&kek, &self.wrap_nonce, &aad, &self.wrapped_safe_key).map_err(|_| OpenError::Corrupt)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(KEYSET_LEN);
        out.extend_from_slice(&encode_keyset_aad(&self.header, &self.key_check));
        out.extend_from_slice(&self.wrap_nonce);
        out.extend_from_slice(&self.wrapped_safe_key);
        debug_assert_eq!(out.len(), KEYSET_LEN);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() < 4 || bytes[..4] != KEYSET_MAGIC {
            return Err(FormatError::BadMagic);
        }
        if bytes.len() != KEYSET_LEN {
            return Err(FormatError::BadLength { found: bytes.len() });
        }
        let mut r = Reader::new(bytes, 4);
        let version = r.u16();
        if version != FORMAT_VERSION {
            return Err(FormatError::UnsupportedVersion(version));
        }
        let flags = r.u16();
        if flags != 0 {
            return Err(FormatError::UnknownFlags(flags));
        }
        let safe_id = r.array();
        let key_epoch = r.u32();
        let keyset_revision = r.u64();
        let kdf_alg = r.u8();
        if kdf_alg != KDF_ARGON2ID {
            return Err(FormatError::UnknownKdf(kdf_alg));
        }
        let kdf = KdfParams { m_kib: r.u32(), t: r.u32(), p: r.u8() };
        // What Argon2id itself would refuse: no lanes, no passes, less than
        // eight blocks per lane. Weak-but-valid is for the caller to judge
        // against `KdfParams::FLOOR`; this only rejects the impossible.
        if kdf.p == 0 || kdf.t == 0 || kdf.m_kib < 8 * u32::from(kdf.p) {
            return Err(FormatError::BadKdfParams);
        }
        let kdf_salt = r.array();
        let key_check = r.array();
        let wrap_nonce = r.array();
        let wrapped_safe_key = r.array();
        debug_assert_eq!(r.at, KEYSET_LEN);

        Ok(Self {
            header: KeysetHeader { safe_id, key_epoch, keyset_revision, kdf, kdf_salt },
            key_check,
            wrap_nonce,
            wrapped_safe_key,
        })
    }
}

fn encode_keyset_aad(header: &KeysetHeader, key_check: &[u8; KEY_LEN]) -> [u8; KEYSET_AAD_LEN] {
    let mut w = Writer::<KEYSET_AAD_LEN>::new();
    w.bytes(&KEYSET_MAGIC);
    w.bytes(&FORMAT_VERSION.to_le_bytes());
    w.bytes(&0u16.to_le_bytes());
    w.bytes(&header.safe_id);
    w.bytes(&header.key_epoch.to_le_bytes());
    w.bytes(&header.keyset_revision.to_le_bytes());
    w.bytes(&[KDF_ARGON2ID]);
    w.bytes(&header.kdf.m_kib.to_le_bytes());
    w.bytes(&header.kdf.t.to_le_bytes());
    w.bytes(&[header.kdf.p]);
    w.bytes(&header.kdf_salt);
    w.bytes(key_check);
    w.finish()
}

// ─── items/<id>.safe ─────────────────────────────────────

/// The cleartext header of an item file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemHeader {
    pub safe_id: [u8; 16],
    pub item_id: [u8; 16],
    /// Raised by every write. A device refuses a revision lower than the
    /// highest it has seen for the item, which is what stops a sync peer
    /// replaying an old password.
    pub revision: u64,
    pub key_epoch: u32,
    pub tombstone: bool,
}

impl ItemHeader {
    pub fn encode(&self) -> [u8; ITEM_HEADER_LEN] {
        let flags = if self.tombstone { FLAG_TOMBSTONE } else { 0 };
        let mut w = Writer::<ITEM_HEADER_LEN>::new();
        w.bytes(&ITEM_MAGIC);
        w.bytes(&FORMAT_VERSION.to_le_bytes());
        w.bytes(&flags.to_le_bytes());
        w.bytes(&self.safe_id);
        w.bytes(&self.item_id);
        w.bytes(&self.revision.to_le_bytes());
        w.bytes(&self.key_epoch.to_le_bytes());
        w.bytes(&[0u8; 4]);
        w.finish()
    }
}

/// An item file, parsed but not decrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFile {
    pub header: ItemHeader,
    pub item_key_nonce: [u8; NONCE_LEN],
    pub wrapped_item_key: [u8; WRAPPED_KEY_LEN],
    pub body_nonce: [u8; NONCE_LEN],
    /// Padded plaintext sealed under the item key, tag included.
    pub body: Vec<u8>,
}

/// What an item file holds, once opened.
#[derive(Debug, PartialEq, Eq)]
pub enum ItemContent {
    Tombstone,
    /// The serialised item, unpadded, in a buffer that wipes itself.
    Body(Zeroizing<Vec<u8>>),
}

impl ItemFile {
    /// Seal an item.
    ///
    /// `plaintext` is the serialised item; it is padded here. A tombstone
    /// carries no plaintext and no padding — its body is the tag of an empty
    /// message, so it too cannot be forged without the keys.
    pub fn seal(
        header: ItemHeader,
        safe_key: &Key,
        item_key: &Key,
        item_key_nonce: [u8; NONCE_LEN],
        body_nonce: [u8; NONCE_LEN],
        plaintext: &[u8],
    ) -> Result<Self, CryptoError> {
        let aad = header.encode();
        let kek = crypto::subkey(ctx::WRAP_ITEM_KEY, safe_key);
        let wrapped_item_key = crypto::wrap_key(&kek, &item_key_nonce, &aad, item_key)?;
        let body = if header.tombstone {
            if !plaintext.is_empty() {
                return Err(CryptoError::Seal);
            }
            crypto::seal(item_key, &body_nonce, &aad, &[])?
        } else {
            crypto::seal(item_key, &body_nonce, &aad, &crypto::pad(plaintext)?)?
        };
        Ok(Self { header, item_key_nonce, wrapped_item_key, body_nonce, body })
    }

    /// Open an item that was found at `items/<expected_item_id>.safe` of the
    /// Safe `expected_safe_id`.
    ///
    /// Both identities are checked against the header before any decryption,
    /// and the header is the AAD, so a header edited to match is caught by the
    /// decryption instead.
    pub fn open(&self, safe_key: &Key, expected_safe_id: &[u8; 16], expected_item_id: &[u8; 16]) -> Result<ItemContent, OpenError> {
        if &self.header.safe_id != expected_safe_id {
            return Err(OpenError::WrongSafe);
        }
        if &self.header.item_id != expected_item_id {
            return Err(OpenError::Misplaced);
        }
        let aad = self.header.encode();
        let kek = crypto::subkey(ctx::WRAP_ITEM_KEY, safe_key);
        let item_key = crypto::unwrap_key(&kek, &self.item_key_nonce, &aad, &self.wrapped_item_key)
            .map_err(|_| OpenError::Corrupt)?;
        let opened = crypto::open(&item_key, &self.body_nonce, &aad, &self.body).map_err(|_| OpenError::Corrupt)?;

        if self.header.tombstone {
            return if opened.is_empty() { Ok(ItemContent::Tombstone) } else { Err(OpenError::Corrupt) };
        }
        let body = crypto::unpad(&opened).map_err(|_| OpenError::Corrupt)?;
        Ok(ItemContent::Body(Zeroizing::new(body.to_vec())))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ITEM_BODY_OFFSET + self.body.len());
        out.extend_from_slice(&self.header.encode());
        out.extend_from_slice(&self.item_key_nonce);
        out.extend_from_slice(&self.wrapped_item_key);
        out.extend_from_slice(&self.body_nonce);
        out.extend_from_slice(&self.body);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() < 4 || bytes[..4] != ITEM_MAGIC {
            return Err(FormatError::BadMagic);
        }
        // The shortest valid file is a tombstone: header, keys, and a bare tag.
        if bytes.len() < ITEM_BODY_OFFSET + TAG_LEN {
            return Err(FormatError::BadLength { found: bytes.len() });
        }
        let mut r = Reader::new(bytes, 4);
        let version = r.u16();
        if version != FORMAT_VERSION {
            return Err(FormatError::UnsupportedVersion(version));
        }
        let flags = r.u16();
        if flags & !FLAG_TOMBSTONE != 0 {
            return Err(FormatError::UnknownFlags(flags));
        }
        let safe_id = r.array();
        let item_id = r.array();
        let revision = r.u64();
        let key_epoch = r.u32();
        let reserved: [u8; 4] = r.array();
        if reserved != [0; 4] {
            return Err(FormatError::ReservedSet);
        }
        let item_key_nonce = r.array();
        let wrapped_item_key = r.array();
        let body_nonce = r.array();
        debug_assert_eq!(r.at, ITEM_BODY_OFFSET);

        Ok(Self {
            header: ItemHeader { safe_id, item_id, revision, key_epoch, tombstone: flags & FLAG_TOMBSTONE != 0 },
            item_key_nonce,
            wrapped_item_key,
            body_nonce,
            body: bytes[ITEM_BODY_OFFSET..].to_vec(),
        })
    }
}

// ─── fixed-width reading and writing ─────────────────────

/// Reads fields in order from a buffer whose length the caller has already
/// checked, so none of these can run off the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], at: usize) -> Self {
        Self { bytes, at }
    }

    fn array<const N: usize>(&mut self) -> [u8; N] {
        let out = self.bytes[self.at..self.at + N].try_into().expect("length checked by the caller");
        self.at += N;
        out
    }

    fn u8(&mut self) -> u8 {
        self.array::<1>()[0]
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.array())
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.array())
    }

    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.array())
    }
}

/// Writes fields in order into a buffer of exactly `N` bytes, and panics in
/// `finish` if they did not fill it — a layout constant out of step with the
/// fields is a bug to find in the first test run, not a file to write.
struct Writer<const N: usize> {
    out: [u8; N],
    at: usize,
}

impl<const N: usize> Writer<N> {
    fn new() -> Self {
        Self { out: [0; N], at: 0 }
    }

    fn bytes(&mut self, b: &[u8]) {
        self.out[self.at..self.at + b.len()].copy_from_slice(b);
        self.at += b.len();
    }

    fn finish(self) -> [u8; N] {
        assert_eq!(self.at, N, "a Safe header's fields do not add up to its length");
        self.out
    }
}
