//! Opening a KeePass database (`.kdbx`, format 4) with its password, so it
//! can be imported without exporting it to plaintext XML first.
//!
//! What comes out is the database's XML with every protected value decrypted
//! and marked the way KeePass's own XML export marks it, for the XML importer
//! beside this to read. It never touches the disk.
//!
//! Read: AES-256 or ChaCha20 outer encryption; Argon2d, Argon2id or AES-KDF;
//! gzip or none; the ChaCha20 inner stream. Refused, each with its reason:
//! format 3.1 and older, Twofish, key files, and Salsa20 inner streams.
//!
//! The format, from KeePass's documentation (keepass.info/help/kb/kdbx_4.html):
//!
//! ```text
//! signature 03 d9 a2 9a 67 fb 4b b5 · version (minor u16, major u16)
//! header fields: id u8, length u32, data — until id 0
//! SHA-256(header) · HMAC-SHA-256(header)
//! blocks: HMAC(32) · length i32 · data — until a block of length 0
//! ```
//!
//! Everything is checked before anything is decrypted: the header's hash for
//! damage, its HMAC for the password, and each block's HMAC for tampering.

use std::io::Read;

use sha2::{Digest, Sha256, Sha512};
use zeroize::Zeroizing;

use super::exchange::ExchangeError;
use super::totp::{hmac, Algorithm};

pub const SIGNATURE: [u8; 8] = [0x03, 0xd9, 0xa2, 0x9a, 0x67, 0xfb, 0x4b, 0xb5];

const AES256: [u8; 16] = hex16("31c1f2e6bf714350be5805216afc5aff");
const CHACHA20: [u8; 16] = hex16("d6038a2b8b6f4cb5a524339a31dbb59a");
const TWOFISH: [u8; 16] = hex16("ad68f29f576f4bb9a36ad47af965346c");
const ARGON2D: [u8; 16] = hex16("ef636ddf8c29444b91f7a9a403e30a0c");
const ARGON2ID: [u8; 16] = hex16("9e298b1956db4773b23dfc3ec6f0a1e6");
const AES_KDF: [u8; 16] = hex16("c9d9f39a628a4460bf740d08c18a4fea");

/// A database asking for more than this to open it is not one to open here.
const MAX_ARGON2_BYTES: u64 = 2 << 30;
/// What decompressing may produce: a bomb stops here.
const MAX_XML: u64 = 256 << 20;

const fn hex16(s: &str) -> [u8; 16] {
    let b = s.as_bytes();
    let mut out = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = (nibble(b[2 * i]) << 4) | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        _ => c - b'a' + 10,
    }
}

fn damaged(what: &str) -> ExchangeError {
    ExchangeError::Damaged(format!("KeePass database: {what}"))
}

fn unsupported(what: &str) -> ExchangeError {
    ExchangeError::Unsupported(what.to_string())
}

/// A cursor that fails instead of panicking at the end.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ExchangeError> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.bytes.len()).ok_or_else(|| damaged("it ends early"))?;
        let s = &self.bytes[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, ExchangeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, ExchangeError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap_or_default()))
    }
    fn u32(&mut self) -> Result<u32, ExchangeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap_or_default()))
    }
    fn i32(&mut self) -> Result<i32, ExchangeError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap_or_default()))
    }
}

/// What the KDF parameters say, as KeePass's "variant dictionary" holds them.
#[derive(Default)]
struct Kdf {
    uuid: Vec<u8>,
    salt: Vec<u8>,
    parallelism: u32,
    memory: u64,
    iterations: u64,
    version: u32,
    rounds: u64,
}

fn variant_dictionary(bytes: &[u8]) -> Result<Kdf, ExchangeError> {
    let mut c = Cursor { bytes, at: 0 };
    if c.u16()? >> 8 != 1 {
        return Err(unsupported("its key derivation settings are in a version this Safe cannot read"));
    }
    let mut kdf = Kdf::default();
    loop {
        let kind = c.u8()?;
        if kind == 0 {
            return Ok(kdf);
        }
        let name_len = usize::try_from(c.i32()?).map_err(|_| damaged("a negative length"))?;
        let name = c.take(name_len)?;
        let value_len = usize::try_from(c.i32()?).map_err(|_| damaged("a negative length"))?;
        let value = c.take(value_len)?;
        let number = || -> u64 {
            let mut b = [0u8; 8];
            let n = value.len().min(8);
            b[..n].copy_from_slice(&value[..n]);
            u64::from_le_bytes(b)
        };
        match name {
            b"$UUID" => kdf.uuid = value.to_vec(),
            b"S" => kdf.salt = value.to_vec(),
            b"P" => kdf.parallelism = number() as u32,
            b"M" => kdf.memory = number(),
            b"I" => kdf.iterations = number(),
            b"V" => kdf.version = number() as u32,
            b"R" => kdf.rounds = number(),
            _ => {}
        }
    }
}

fn transform(kdf: &Kdf, composite: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>, ExchangeError> {
    let mut out = Zeroizing::new([0u8; 32]);
    let uuid: [u8; 16] = kdf.uuid.as_slice().try_into().map_err(|_| damaged("no key derivation named"))?;
    if uuid == ARGON2D || uuid == ARGON2ID {
        if kdf.memory > MAX_ARGON2_BYTES {
            return Err(unsupported("it asks for more than 2 GiB of memory to open"));
        }
        let algorithm = if uuid == ARGON2D { argon2::Algorithm::Argon2d } else { argon2::Algorithm::Argon2id };
        let version = if kdf.version == 0x10 { argon2::Version::V0x10 } else { argon2::Version::V0x13 };
        let params = argon2::Params::new(
            (kdf.memory / 1024) as u32,
            u32::try_from(kdf.iterations).map_err(|_| damaged("too many Argon2 passes"))?,
            kdf.parallelism,
            Some(32),
        )
        .map_err(|e| damaged(&format!("Argon2 settings: {e}")))?;
        argon2::Argon2::new(algorithm, version, params)
            .hash_password_into(composite, &kdf.salt, out.as_mut())
            .map_err(|e| damaged(&format!("Argon2: {e}")))?;
    } else if uuid == AES_KDF {
        use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
        let seed: [u8; 32] = kdf.salt.as_slice().try_into().map_err(|_| damaged("the AES-KDF seed is not 32 bytes"))?;
        let aes = aes::Aes256::new(GenericArray::from_slice(&seed));
        let mut key = Zeroizing::new(*composite);
        for _ in 0..kdf.rounds {
            for block in key.chunks_exact_mut(16) {
                aes.encrypt_block(GenericArray::from_mut_slice(block));
            }
        }
        out.copy_from_slice(&Sha256::digest(key.as_ref()));
    } else {
        return Err(unsupported("its key derivation is not Argon2 or AES-KDF"));
    }
    Ok(out)
}

fn block_key(base: &[u8], index: u64) -> Zeroizing<Vec<u8>> {
    let mut h = Sha512::new();
    h.update(index.to_le_bytes());
    h.update(base);
    Zeroizing::new(h.finalize().to_vec())
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Open a KDBX 4 database with its password: its XML, protected values
/// decrypted, and how many attachments it had (they are not imported).
pub fn open(bytes: &[u8], password: &str) -> Result<(Zeroizing<String>, usize), ExchangeError> {
    let mut c = Cursor { bytes, at: 0 };
    if c.take(8)? != SIGNATURE {
        return Err(ExchangeError::Unknown);
    }
    let _minor = c.u16()?;
    let major = c.u16()?;
    if major < 4 {
        return Err(unsupported(
            "it is in KeePass's older format (3.1). In KeePass, open it, choose Argon2 under Database Settings → Security and save it; or export it as KeePass XML",
        ));
    }
    if major > 4 {
        return Err(unsupported("it is in a KeePass format newer than 4"));
    }

    let (mut cipher, mut compressed, mut seed, mut iv, mut kdf) = (None, false, None, None, None);
    loop {
        let id = c.u8()?;
        let len = c.u32()? as usize;
        let data = c.take(len)?;
        match id {
            0 => break,
            2 => cipher = Some(data.to_vec()),
            3 => compressed = data.first() == Some(&1),
            4 => seed = Some(data.to_vec()),
            7 => iv = Some(data.to_vec()),
            11 => kdf = Some(variant_dictionary(data)?),
            _ => {}
        }
    }
    let header = &bytes[..c.at];
    let hash = c.take(32)?;
    let header_hmac = c.take(32)?;
    if !same(&Sha256::digest(header), hash) {
        return Err(damaged("its header does not match its checksum"));
    }

    let cipher: [u8; 16] = cipher.as_deref().and_then(|c| c.try_into().ok()).ok_or_else(|| damaged("no cipher named"))?;
    if cipher == TWOFISH {
        return Err(unsupported("it is encrypted with Twofish. In KeePass, choose AES or ChaCha20 under Database Settings → Security and save it"));
    }
    if cipher != AES256 && cipher != CHACHA20 {
        return Err(unsupported("its cipher is not AES-256 or ChaCha20"));
    }
    let seed = seed.ok_or_else(|| damaged("no master seed"))?;
    let iv = iv.ok_or_else(|| damaged("no IV"))?;
    let kdf = kdf.ok_or_else(|| damaged("no key derivation settings"))?;

    // The composite key: SHA-256 of each part, then of them together. A key
    // file would be a second part; this reads password-only databases.
    let mut composite = Zeroizing::new([0u8; 32]);
    composite.copy_from_slice(&Sha256::digest(Sha256::digest(password.as_bytes())));
    let transformed = transform(&kdf, &composite)?;

    let mut h = Sha256::new();
    h.update(&seed);
    h.update(transformed.as_ref());
    let mut master = Zeroizing::new([0u8; 32]);
    master.copy_from_slice(&h.finalize());
    let mut h = Sha512::new();
    h.update(&seed);
    h.update(transformed.as_ref());
    h.update([1u8]);
    let hmac_base = Zeroizing::new(h.finalize().to_vec());

    if !same(&hmac(Algorithm::Sha256, &block_key(&hmac_base, u64::MAX), header), header_hmac) {
        return Err(ExchangeError::WrongKdbxPassword);
    }

    let mut payload = Zeroizing::new(Vec::new());
    for index in 0u64.. {
        let tag = c.take(32)?;
        let len = c.i32()?;
        let data = c.take(usize::try_from(len).map_err(|_| damaged("a negative block length"))?)?;
        let mut signed = Vec::with_capacity(12 + data.len());
        signed.extend_from_slice(&index.to_le_bytes());
        signed.extend_from_slice(&len.to_le_bytes());
        signed.extend_from_slice(data);
        if !same(&hmac(Algorithm::Sha256, &block_key(&hmac_base, index), &signed), tag) {
            return Err(damaged("a block was altered"));
        }
        if data.is_empty() {
            break;
        }
        payload.extend_from_slice(data);
    }

    if cipher == AES256 {
        use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
        let iv: [u8; 16] = iv.as_slice().try_into().map_err(|_| damaged("the AES IV is not 16 bytes"))?;
        if payload.is_empty() || payload.len() % 16 != 0 {
            return Err(damaged("its encrypted part is not whole AES blocks"));
        }
        // CBC: each block, decrypted, XORed with the ciphertext before it.
        let aes = aes::Aes256::new(GenericArray::from_slice(master.as_ref()));
        let mut previous = iv;
        for block in payload.chunks_exact_mut(16) {
            let this: [u8; 16] = block.try_into().unwrap_or_default();
            aes.decrypt_block(GenericArray::from_mut_slice(block));
            block.iter_mut().zip(previous).for_each(|(b, p)| *b ^= p);
            previous = this;
        }
        let pad = *payload.last().unwrap_or(&0) as usize;
        if pad == 0 || pad > 16 || !payload[payload.len() - pad..].iter().all(|b| *b as usize == pad) {
            return Err(damaged("its padding is wrong"));
        }
        let keep = payload.len() - pad;
        payload.truncate(keep);
    } else {
        use chacha20::cipher::{KeyIvInit, StreamCipher};
        let nonce: [u8; 12] = iv.as_slice().try_into().map_err(|_| damaged("the ChaCha20 nonce is not 12 bytes"))?;
        chacha20::ChaCha20::new(master.as_ref().into(), &nonce.into()).apply_keystream(&mut payload);
    }

    let inner = if compressed {
        let mut out = Zeroizing::new(Vec::new());
        flate2::read::GzDecoder::new(payload.as_slice())
            .take(MAX_XML)
            .read_to_end(&mut out)
            .map_err(|e| damaged(&format!("it does not decompress: {e}")))?;
        out
    } else {
        payload
    };

    // The inner header: the stream that protects values, and attachments.
    let mut c = Cursor { bytes: &inner, at: 0 };
    let (mut stream_id, mut stream_key, mut attachments) = (0u32, None, 0usize);
    loop {
        let id = c.u8()?;
        let len = c.u32()? as usize;
        let data = c.take(len)?;
        match id {
            0 => break,
            1 => stream_id = u32::from_le_bytes(data.try_into().map_err(|_| damaged("the inner stream id"))?),
            2 => stream_key = Some(Zeroizing::new(data.to_vec())),
            3 => attachments += 1,
            _ => {}
        }
    }
    if stream_id != 3 {
        return Err(unsupported("its protected values use an inner stream other than ChaCha20"));
    }
    let key = Sha512::digest(stream_key.ok_or_else(|| damaged("no inner stream key"))?.as_slice());
    let key = Zeroizing::new(key.to_vec());
    let xml = std::str::from_utf8(&inner[c.at..]).map_err(|_| damaged("its XML is not UTF-8"))?;
    Ok((unprotect(xml, &key[..32], &key[32..44])?, attachments))
}

/// The XML with each `Protected="True"` value decrypted, in document order —
/// the stream runs through them one after another, history and all — and
/// marked `ProtectInMemory="True"` as KeePass's XML export marks them.
fn unprotect(xml: &str, key: &[u8], nonce: &[u8]) -> Result<Zeroizing<String>, ExchangeError> {
    use base64::Engine;
    use chacha20::cipher::{KeyIvInit, StreamCipher};
    use quick_xml::events::{BytesStart, BytesText, Event};
    let mut stream = chacha20::ChaCha20::new(key.into(), nonce.into());
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut writer = quick_xml::Writer::new(Vec::new());
    let mut protected = false;
    loop {
        let event = reader.read_event().map_err(|e| damaged(&e.to_string()))?;
        let event = match event {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == b"Value" => {
                protected = e.attributes().flatten().any(|a| a.key.as_ref() == b"Protected" && a.value.as_ref() == b"True");
                if protected {
                    Event::Start(BytesStart::new("Value").with_attributes([("ProtectInMemory", "True")]))
                } else {
                    Event::Start(e)
                }
            }
            Event::Text(t) if protected => {
                let raw = t.unescape().map_err(|e| damaged(&e.to_string()))?;
                let mut value = Zeroizing::new(
                    base64::engine::general_purpose::STANDARD.decode(raw.trim()).map_err(|_| damaged("a protected value is not base64"))?,
                );
                stream.apply_keystream(&mut value);
                let text = Zeroizing::new(String::from_utf8(std::mem::take(&mut *value)).map_err(|_| damaged("a protected value is not text"))?);
                writer.write_event(Event::Text(BytesText::new(&text))).map_err(|e| damaged(&e.to_string()))?;
                continue;
            }
            Event::Empty(e) if e.name().as_ref() == b"Value" => {
                // An empty protected value takes nothing from the stream.
                if e.attributes().flatten().any(|a| a.key.as_ref() == b"Protected") {
                    Event::Empty(BytesStart::new("Value").with_attributes([("ProtectInMemory", "True")]))
                } else {
                    Event::Empty(e)
                }
            }
            Event::End(e) => {
                if e.name().as_ref() == b"Value" {
                    protected = false;
                }
                Event::End(e)
            }
            other => other,
        };
        writer.write_event(event).map_err(|e| damaged(&e.to_string()))?;
    }
    let out = Zeroizing::new(writer.into_inner());
    String::from_utf8(out.to_vec()).map(Zeroizing::new).map_err(|_| damaged("its XML is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AES_ARGON2D: &[u8] = include_bytes!("testdata/kdbx/aes-argon2d.kdbx");
    const CHACHA_ARGON2ID: &[u8] = include_bytes!("testdata/kdbx/chacha-argon2id.kdbx");
    const AES_AESKDF: &[u8] = include_bytes!("testdata/kdbx/aes-aeskdf.kdbx");

    #[test]
    fn real_databases_open_with_their_password() {
        for (name, db) in [("aes/argon2d", AES_ARGON2D), ("chacha20/argon2id", CHACHA_ARGON2ID)] {
            let (xml, attachments) = open(db, "correct horse").unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(xml.contains("<KeePassFile"), "{name}");
            assert!(xml.contains(">kdbx-canary-3<"), "{name}: the current password");
            assert!(xml.contains(">kdbx-canary-2<"), "{name}: a protected custom field");
            assert!(xml.contains(">mật-khẩu-ünïcode<"), "{name}: non-ASCII survives");
            assert!(!xml.contains("Protected=\"True\""), "{name}: every protected value was opened");
            assert_eq!(attachments, 0);
        }
        let (xml, _) = open(AES_AESKDF, "correct horse").unwrap();
        assert!(xml.contains(">kdbx-canary-aeskdf<"));
    }

    #[test]
    fn a_wrong_password_is_said_as_such() {
        for db in [AES_ARGON2D, CHACHA_ARGON2ID, AES_AESKDF] {
            assert!(matches!(open(db, "wrong horse"), Err(ExchangeError::WrongKdbxPassword)));
        }
    }

    #[test]
    fn a_flipped_byte_anywhere_is_caught() {
        // In the header (its checksum), and in the encrypted blocks (their HMAC).
        for at in [20, AES_ARGON2D.len() / 2, AES_ARGON2D.len() - 40] {
            let mut bad = AES_ARGON2D.to_vec();
            bad[at] ^= 1;
            match open(&bad, "correct horse") {
                Err(ExchangeError::Damaged(_)) | Err(ExchangeError::WrongKdbxPassword) | Err(ExchangeError::Unsupported(_)) => {}
                other => panic!("byte {at}: {:?}", other.map(|_| ())),
            }
        }
    }

    #[test]
    fn older_formats_and_junk_are_refused_with_reasons() {
        let mut v3 = SIGNATURE.to_vec();
        v3.extend_from_slice(&[1, 0, 3, 0]);
        assert!(matches!(open(&v3, "x"), Err(ExchangeError::Unsupported(r)) if r.contains("3.1")));
        for junk in [&b""[..], &SIGNATURE[..], b"not a database at all"] {
            assert!(open(junk, "x").is_err());
        }
        for cut in (0..AES_ARGON2D.len()).step_by(97) {
            let _ = open(&AES_ARGON2D[..cut], "correct horse");
        }
    }
}
