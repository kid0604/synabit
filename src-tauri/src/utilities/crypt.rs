//! PDF passwords: opening a protected PDF, and protecting one.
//!
//! `lopdf` decrypts only the oldest scheme (RC4, revisions 2 and 3), and it
//! cannot read a protected file whose objects live in object streams — which
//! is how nearly every PDF written this decade is laid out. It parses those
//! streams while loading, before anybody can hand it a key, so their objects
//! are dropped as unreadable. This module does the Standard security handler
//! itself (ISO 32000-2 §7.6.4) and hooks into the load, so the bytes are
//! decrypted before `lopdf` tries to read them:
//!
//! - **Opening:** RC4 40–128 bit (V1, V2), AES-128 (V4, `AESV2`) and AES-256
//!   (V5, R5 and R6). With the user password, or the owner password, or with
//!   none for a file that only restricts printing and copying — which opens in
//!   every reader without asking, so refusing it here would only be in the way.
//! - **Protecting:** AES-256, revision 6, the one scheme PDF 2.0 keeps.
//!
//! The ciphers are `aes` (already used by Safe) with CBC written out, MD5 and
//! SHA-2 — every one already in the build.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use md5::{Digest as _, Md5};
use sha2::{Sha256, Sha384, Sha512};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CryptError {
    /// A password is needed, or the one given is not it.
    #[error("the password is not right")]
    WrongPassword,
    #[error("this kind of protection is not supported: {0}")]
    Unsupported(String),
    #[error("not a readable PDF: {0}")]
    Unreadable(String),
}

/// The 32 bytes every password is padded with (Algorithm 2, step a).
const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08, 0x2E, 0x2E, 0x00,
    0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Method {
    Identity,
    Rc4,
    Aes128,
    Aes256,
}

/// What it takes to decrypt a file: the key, and which cipher strings and
/// streams use.
#[derive(Debug, Clone)]
struct Keys {
    key: Vec<u8>,
    strings: Method,
    streams: Method,
    encrypt_metadata: bool,
    /// The encryption dictionary's own object, which is never encrypted.
    dict_id: Option<ObjectId>,
}

// ── Primitives ────────────────────────────────────────────────────────────

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|&b| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            b ^ s[(s[i as usize].wrapping_add(s[j as usize])) as usize]
        })
        .collect()
}

fn md5(parts: &[&[u8]]) -> [u8; 16] {
    let mut h = Md5::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn sha256(parts: &[&[u8]]) -> Vec<u8> {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().to_vec()
}

/// AES-CBC without padding, for keys of 16 or 32 bytes. Callers check the
/// key's length ([`aes_key_ok`]); a file can claim any.
fn cbc_encrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    let mut prev: [u8; 16] = iv.try_into().expect("16-byte IV");
    let mut out = Vec::with_capacity(data.len());
    macro_rules! run {
        ($cipher:ty) => {{
            let c = <$cipher>::new(GenericArray::from_slice(key));
            for chunk in data.chunks(16) {
                let mut block = [0u8; 16];
                for k in 0..16 {
                    block[k] = chunk[k] ^ prev[k];
                }
                let mut b = GenericArray::from(block);
                c.encrypt_block(&mut b);
                prev = b.into();
                out.extend_from_slice(&prev);
            }
        }};
    }
    match key.len() {
        16 => run!(aes::Aes128),
        _ => run!(aes::Aes256),
    }
    out
}

fn aes_key_ok(key: &[u8]) -> bool {
    key.len() == 16 || key.len() == 32
}

fn cbc_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    let mut prev: [u8; 16] = iv.try_into().expect("16-byte IV");
    let mut out = Vec::with_capacity(data.len());
    macro_rules! run {
        ($cipher:ty) => {{
            let c = <$cipher>::new(GenericArray::from_slice(key));
            for chunk in data.chunks_exact(16) {
                let mut b = GenericArray::clone_from_slice(chunk);
                c.decrypt_block(&mut b);
                for k in 0..16 {
                    out.push(b[k] ^ prev[k]);
                }
                prev = chunk.try_into().unwrap();
            }
        }};
    }
    match key.len() {
        16 => run!(aes::Aes128),
        _ => run!(aes::Aes256),
    }
    out
}

/// A password as the AES-256 revisions take it: UTF-8, at most 127 bytes.
/// (SASLprep is skipped: it changes nothing for the passwords people type.)
fn utf8_password(password: &str) -> &[u8] {
    let bytes = password.as_bytes();
    let mut end = bytes.len().min(127);
    while !password.is_char_boundary(end) {
        end -= 1;
    }
    &bytes[..end]
}

/// A password as RC4 and AES-128 take it: Latin-1, padded to 32 bytes.
fn padded_password(password: &str) -> [u8; 32] {
    let latin: Vec<u8> = password.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
    let mut out = [0u8; 32];
    let n = latin.len().min(32);
    out[..n].copy_from_slice(&latin[..n]);
    out[n..].copy_from_slice(&PAD[..32 - n]);
    out
}

/// Algorithm 2.B: the hash revision 6 uses for passwords.
fn hash_r6(password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    let mut k = sha256(&[password, salt, udata]);
    let mut round = 0u32;
    loop {
        let mut k1 = Vec::with_capacity(64 * (password.len() + k.len() + udata.len()));
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            k1.extend_from_slice(udata);
        }
        let e = cbc_encrypt(&k[..16], &k[16..32], &k1);
        let sum: u32 = e[..16].iter().map(|&b| b as u32).sum();
        k = match sum % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        round += 1;
        if round >= 64 && (*e.last().unwrap() as u32) <= round - 32 {
            break;
        }
    }
    k.truncate(32);
    k
}

fn hash_for(revision: i64, password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    if revision >= 6 { hash_r6(password, salt, udata) } else { sha256(&[password, salt, udata]) }
}

// ── Reading the encryption dictionary ─────────────────────────────────────

fn bytes_of(dict: &Dictionary, key: &[u8]) -> Vec<u8> {
    dict.get(key).and_then(Object::as_str).map(<[u8]>::to_vec).unwrap_or_default()
}

fn int_of(dict: &Dictionary, key: &[u8], default: i64) -> i64 {
    dict.get(key).and_then(Object::as_i64).unwrap_or(default)
}

fn first_id(doc: &Document) -> Vec<u8> {
    doc.trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .ok()
        .and_then(|a| a.first())
        .and_then(|o| o.as_str().ok())
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

fn encryption_dict(doc: &Document) -> Option<(Dictionary, Option<ObjectId>)> {
    match doc.trailer.get(b"Encrypt").ok()? {
        Object::Reference(id) => Some((doc.get_dictionary(*id).ok()?.clone(), Some(*id))),
        Object::Dictionary(d) => Some((d.clone(), None)),
        _ => None,
    }
}

fn crypt_filter_method(dict: &Dictionary, name_key: &[u8]) -> Result<Method, CryptError> {
    let name = dict.get(name_key).and_then(Object::as_name).unwrap_or(b"Identity");
    if name == b"Identity" {
        return Ok(Method::Identity);
    }
    let cfm = dict
        .get(b"CF")
        .and_then(Object::as_dict)
        .and_then(|cf| cf.get(name))
        .and_then(Object::as_dict)
        .and_then(|f| f.get(b"CFM"))
        .and_then(Object::as_name)
        .unwrap_or(b"None");
    match cfm {
        b"V2" => Ok(Method::Rc4),
        b"AESV2" => Ok(Method::Aes128),
        b"AESV3" => Ok(Method::Aes256),
        b"None" => Ok(Method::Identity),
        other => Err(CryptError::Unsupported(String::from_utf8_lossy(other).into())),
    }
}

/// The file key's length in bytes for V1–V4. For V4 it is the crypt filter's
/// `/Length` (bytes or, in some writers, bits), defaulting to 128 bits as
/// pdf.js does; the top-level `/Length` belongs to V2 and V3.
fn key_length(dict: &Dictionary, v: i64) -> usize {
    let bits_or_bytes = |n: i64| if n > 16 { n as usize / 8 } else { n as usize };
    let length = match v {
        1 => 5,
        4 => {
            let name = dict.get(b"StmF").and_then(Object::as_name).unwrap_or(b"StdCF");
            dict.get(b"CF")
                .and_then(Object::as_dict)
                .and_then(|cf| cf.get(name))
                .and_then(Object::as_dict)
                .and_then(|f| f.get(b"Length"))
                .and_then(Object::as_i64)
                .map(bits_or_bytes)
                .unwrap_or(16)
        }
        _ => int_of(dict, b"Length", 40) as usize / 8,
    };
    length.clamp(5, 16)
}

/// The file key for RC4 and AES-128 (Algorithm 2), from a padded user password.
fn legacy_key(dict: &Dictionary, id0: &[u8], padded: &[u8; 32], revision: i64, length: usize) -> Vec<u8> {
    let o = bytes_of(dict, b"O");
    let p = (int_of(dict, b"P", 0) as i32).to_le_bytes();
    let mut parts: Vec<&[u8]> = vec![padded, &o[..o.len().min(32)], &p, id0];
    let metadata_off = [0xFFu8; 4];
    let encrypt_metadata = dict.get(b"EncryptMetadata").and_then(Object::as_bool).unwrap_or(true);
    if revision >= 4 && !encrypt_metadata {
        parts.push(&metadata_off);
    }
    let mut key = md5(&parts).to_vec();
    if revision >= 3 {
        for _ in 0..50 {
            key = md5(&[&key[..length]]).to_vec();
        }
    }
    key.truncate(length);
    key
}

/// Whether `key` is what the user password makes (Algorithms 4 and 5).
fn user_key_matches(dict: &Dictionary, id0: &[u8], key: &[u8], revision: i64) -> bool {
    let u = bytes_of(dict, b"U");
    if revision == 2 {
        return rc4(key, &PAD) == u;
    }
    let mut x = rc4(key, &md5(&[&PAD, id0]));
    for i in 1..=19u8 {
        let k: Vec<u8> = key.iter().map(|b| b ^ i).collect();
        x = rc4(&k, &x);
    }
    u.len() >= 16 && x[..16] == u[..16]
}

/// The user password an owner password unlocks (Algorithm 7).
fn user_from_owner(dict: &Dictionary, owner: &str, revision: i64, length: usize) -> [u8; 32] {
    let mut h = md5(&[&padded_password(owner)]).to_vec();
    if revision >= 3 {
        for _ in 0..50 {
            h = md5(&[&h]).to_vec();
        }
    }
    let key = &h[..length];
    let mut x = bytes_of(dict, b"O");
    if revision == 2 {
        x = rc4(key, &x);
    } else {
        for i in (0..=19u8).rev() {
            let k: Vec<u8> = key.iter().map(|b| b ^ i).collect();
            x = rc4(&k, &x);
        }
    }
    let mut out = [0u8; 32];
    let n = x.len().min(32);
    out[..n].copy_from_slice(&x[..n]);
    out
}

fn keys_for(doc: &Document, password: &str) -> Result<Option<Keys>, CryptError> {
    let Some((dict, dict_id)) = encryption_dict(doc) else { return Ok(None) };
    let filter = dict.get(b"Filter").and_then(Object::as_name).unwrap_or(b"Standard");
    if filter != b"Standard" {
        return Err(CryptError::Unsupported(format!("security handler {}", String::from_utf8_lossy(filter))));
    }
    let v = int_of(&dict, b"V", 0);
    let revision = int_of(&dict, b"R", 0);
    let encrypt_metadata = dict.get(b"EncryptMetadata").and_then(Object::as_bool).unwrap_or(true);

    if v == 5 {
        let u = bytes_of(&dict, b"U");
        let o = bytes_of(&dict, b"O");
        if u.len() < 48 || o.len() < 48 {
            return Err(CryptError::Unreadable("a damaged encryption dictionary".into()));
        }
        let pw = utf8_password(password);
        let wrapped = |k: Vec<u8>, sealed: Vec<u8>| -> Result<Vec<u8>, CryptError> {
            if sealed.len() != 32 {
                return Err(CryptError::Unreadable("a damaged encryption key".into()));
            }
            Ok(cbc_decrypt(&k, &[0; 16], &sealed))
        };
        let key = if hash_for(revision, pw, &u[32..40], &[])[..32] == u[..32] {
            wrapped(hash_for(revision, pw, &u[40..48], &[]), bytes_of(&dict, b"UE"))?
        } else if hash_for(revision, pw, &o[32..40], &u[..48])[..32] == o[..32] {
            wrapped(hash_for(revision, pw, &o[40..48], &u[..48]), bytes_of(&dict, b"OE"))?
        } else {
            return Err(CryptError::WrongPassword);
        };
        if key.len() != 32 {
            return Err(CryptError::Unreadable("a damaged encryption key".into()));
        }
        return Ok(Some(Keys {
            key,
            strings: crypt_filter_method(&dict, b"StrF")?,
            streams: crypt_filter_method(&dict, b"StmF")?,
            encrypt_metadata,
            dict_id,
        }));
    }

    if !(1..=4).contains(&v) || !(2..=4).contains(&revision) {
        return Err(CryptError::Unsupported(format!("V{v} R{revision}")));
    }
    let length = key_length(&dict, v);
    let (strings, streams) = if v == 4 {
        (crypt_filter_method(&dict, b"StrF")?, crypt_filter_method(&dict, b"StmF")?)
    } else {
        (Method::Rc4, Method::Rc4)
    };
    let id0 = first_id(doc);
    let as_user = legacy_key(&dict, &id0, &padded_password(password), revision, length);
    let key = if user_key_matches(&dict, &id0, &as_user, revision) {
        as_user
    } else {
        let user = user_from_owner(&dict, password, revision, length);
        let as_owner = legacy_key(&dict, &id0, &user, revision, length);
        if !user_key_matches(&dict, &id0, &as_owner, revision) {
            return Err(CryptError::WrongPassword);
        }
        as_owner
    };
    Ok(Some(Keys { key, strings, streams, encrypt_metadata, dict_id }))
}

// ── Decrypting objects ────────────────────────────────────────────────────

fn object_key(keys: &Keys, id: ObjectId, method: Method) -> Vec<u8> {
    if method == Method::Aes256 {
        return keys.key.clone();
    }
    let num = id.0.to_le_bytes();
    let gen = id.1.to_le_bytes();
    let salt: &[u8] = if method == Method::Aes128 { b"sAlT" } else { b"" };
    let h = md5(&[&keys.key, &num[..3], &gen, salt]);
    h[..(keys.key.len() + 5).min(16)].to_vec()
}

fn decrypt_bytes(keys: &Keys, id: ObjectId, method: Method, data: &[u8]) -> Vec<u8> {
    match method {
        Method::Identity => data.to_vec(),
        Method::Rc4 => rc4(&object_key(keys, id, method), data),
        Method::Aes128 | Method::Aes256 => {
            let key = object_key(keys, id, method);
            if data.len() < 32 || !data.len().is_multiple_of(16) || !aes_key_ok(&key) {
                // An IV and nothing after it is an empty string; a key of a
                // length AES does not have decrypts nothing.
                return Vec::new();
            }
            let mut out = cbc_decrypt(&key, &data[..16], &data[16..]);
            if let Some(&pad) = out.last() {
                let pad = pad as usize;
                if (1..=16).contains(&pad) && out.len() >= pad && out[out.len() - pad..].iter().all(|&b| b as usize == pad) {
                    out.truncate(out.len() - pad);
                }
            }
            out
        }
    }
}

fn decrypt_strings(keys: &Keys, id: ObjectId, object: &mut Object) {
    match object {
        Object::String(bytes, _) => *bytes = decrypt_bytes(keys, id, keys.strings, bytes),
        Object::Array(items) => items.iter_mut().for_each(|o| decrypt_strings(keys, id, o)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, o)| decrypt_strings(keys, id, o)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, o)| decrypt_strings(keys, id, o)),
        _ => {}
    }
}

/// Decrypt one object as it came off the disk. Objects read out of an object
/// stream are not touched: the stream they came in was decrypted whole.
fn decrypt_object(keys: &Keys, id: ObjectId, object: &mut Object) {
    if Some(id) == keys.dict_id {
        return;
    }
    if let Object::Stream(stream) = object {
        let kind = stream.dict.get(b"Type").and_then(Object::as_name).ok();
        if kind == Some(b"XRef") {
            return;
        }
        if kind == Some(b"Metadata") && !keys.encrypt_metadata {
            return;
        }
        // An empty stream whose `/Length` lives in another object is filled in
        // by `lopdf` after this runs; it is decrypted then (`load`), and its
        // dictionary must keep pointing at that length.
        if !stream.content.is_empty() {
            let content = decrypt_bytes(keys, id, keys.streams, &stream.content);
            stream.set_content(content);
        }
    }
    decrypt_strings(keys, id, object);
}

/// What the load hook needs, set for the one load that uses it at a time.
struct Hook {
    keys: Keys,
    /// Objects stored on their own in the file, by the cross-reference table:
    /// the only ones encrypted one by one. Everything else the hook sees came
    /// out of an object stream, which was decrypted whole. Going by the table
    /// rather than by what object streams list keeps an object that a later
    /// update stored on its own from being skipped (`lopdf` keeps that one).
    plain: HashSet<ObjectId>,
}

fn hook() -> &'static Mutex<Option<Hook>> {
    static HOOK: OnceLock<Mutex<Option<Hook>>> = OnceLock::new();
    HOOK.get_or_init(|| Mutex::new(None))
}

/// Loads that use the hook, one at a time: `lopdf` takes a plain function, not
/// a closure, so the key has to be somewhere it can reach.
fn hooked_loads() -> &'static Mutex<()> {
    static LOADS: OnceLock<Mutex<()>> = OnceLock::new();
    LOADS.get_or_init(|| Mutex::new(()))
}

/// `lopdf` calls this twice over: for each object in the file, where it keeps
/// the object it passed in and ignores what comes back, and for each object
/// read out of an object stream, where it keeps only what comes back.
fn on_load(id: ObjectId, object: &mut Object) -> Option<(ObjectId, Object)> {
    let mut guard = hook().lock().unwrap_or_else(|e| e.into_inner());
    let Some(h) = guard.as_mut() else { return Some((id, std::mem::replace(object, Object::Null))) };
    if !h.plain.contains(&id) {
        // From an object stream: plain already, and kept from what comes back.
        return Some((id, std::mem::replace(object, Object::Null)));
    }
    decrypt_object(&h.keys, id, object);
    // Ignored for an object in the file, which is decrypted in place. (An
    // object stream's stale copy of a plain object is decrypted here too,
    // wrongly, and dropped: `lopdf` keeps the plain one.)
    Some((id, Object::Null))
}

/// Whether a PDF asks for a password before it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Lock {
    /// Not protected.
    None,
    /// Protected, but opens without a password (an owner password only).
    Restrictions,
    /// Needs a password to open.
    Password,
}

/// Open a PDF, decrypted. `password` is tried as the user password and then as
/// the owner password; empty opens a file that only has an owner password.
pub fn load(path: &Path, password: &str) -> Result<Document, CryptError> {
    let first = Document::load(path).map_err(|e| CryptError::Unreadable(e.to_string()))?;
    let Some(keys) = keys_for(&first, password)? else { return Ok(first) };
    let plain: HashSet<ObjectId> = first
        .reference_table
        .entries
        .iter()
        .filter_map(|(n, e)| match e {
            lopdf::xref::XrefEntry::Normal { generation, .. } => Some((*n, *generation)),
            _ => None,
        })
        .collect();
    // Streams `lopdf` fills in after the hook has run: their length is
    // another object's. Known from the first load, where they came back empty.
    let late: Vec<ObjectId> = first
        .objects
        .iter()
        .filter(|(id, o)| plain.contains(id) && matches!(o, Object::Stream(s) if s.content.is_empty() && matches!(s.dict.get(b"Length"), Ok(Object::Reference(_)))))
        .map(|(id, _)| *id)
        .collect();
    drop(first);

    let mut doc = {
        let _one_at_a_time = hooked_loads().lock().unwrap_or_else(|e| e.into_inner());
        // Cleared however the load ends, a panic in `lopdf` included, so a
        // key never outlives its file.
        struct Clear;
        impl Drop for Clear {
            fn drop(&mut self) {
                *hook().lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
        }
        let _clear = Clear;
        *hook().lock().unwrap_or_else(|e| e.into_inner()) = Some(Hook { keys: keys.clone(), plain });
        Document::load_filtered(path, on_load).map_err(|e| CryptError::Unreadable(e.to_string()))?
    };
    for id in late {
        if let Ok(Object::Stream(stream)) = doc.get_object_mut(id) {
            if !stream.content.is_empty() && stream.dict.get(b"Type").and_then(Object::as_name).ok() != Some(b"XRef") {
                let content = decrypt_bytes(&keys, id, keys.streams, &stream.content);
                stream.set_content(content);
            }
        }
    }

    if let Ok(Object::Reference(id)) = doc.trailer.get(b"Encrypt") {
        let id = *id;
        doc.objects.remove(&id);
    }
    doc.trailer.remove(b"Encrypt");
    Ok(doc)
}

/// Whether a file asks for a password, without decrypting it.
pub fn lock_of(path: &Path) -> Result<Lock, CryptError> {
    let doc = Document::load(path).map_err(|e| CryptError::Unreadable(e.to_string()))?;
    Ok(match keys_for(&doc, "") {
        Ok(None) => Lock::None,
        Ok(Some(_)) => Lock::Restrictions,
        Err(CryptError::WrongPassword) => Lock::Password,
        Err(e) => return Err(e),
    })
}

// ── Protecting ────────────────────────────────────────────────────────────

fn random<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut b = [0u8; N];
    rand::rng().fill_bytes(&mut b);
    b
}

fn encrypt_bytes(key: &[u8], data: &[u8]) -> Vec<u8> {
    let iv: [u8; 16] = random();
    let pad = 16 - data.len() % 16;
    let mut padded = data.to_vec();
    padded.extend(std::iter::repeat_n(pad as u8, pad));
    let mut out = iv.to_vec();
    out.extend(cbc_encrypt(key, &iv, &padded));
    out
}

fn encrypt_strings(key: &[u8], object: &mut Object) {
    match object {
        Object::String(bytes, format) => {
            *bytes = encrypt_bytes(key, bytes);
            // Ciphertext is binary; written as a literal it needs escaping
            // that some readers get wrong.
            *format = StringFormat::Hexadecimal;
        }
        Object::Array(items) => items.iter_mut().for_each(|o| encrypt_strings(key, o)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, o)| encrypt_strings(key, o)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, o)| encrypt_strings(key, o)),
        _ => {}
    }
}

/// Protect a document with a password to open it (AES-256, revision 6). The
/// owner password, which lifts restrictions, is random: nothing is
/// restricted, so nobody needs it.
pub fn protect(doc: &mut Document, password: &str) -> Result<(), CryptError> {
    if password.is_empty() {
        return Err(CryptError::WrongPassword);
    }
    let file_key: [u8; 32] = random();
    let user = utf8_password(password);
    let owner_secret: [u8; 32] = random();
    let owner = owner_secret.as_slice();

    let (uvs, uks): ([u8; 8], [u8; 8]) = (random(), random());
    let mut u = hash_r6(user, &uvs, &[]);
    u.extend_from_slice(&uvs);
    u.extend_from_slice(&uks);
    let ue = cbc_encrypt(&hash_r6(user, &uks, &[]), &[0; 16], &file_key);

    let (ovs, oks): ([u8; 8], [u8; 8]) = (random(), random());
    let mut o = hash_r6(owner, &ovs, &u);
    o.extend_from_slice(&ovs);
    o.extend_from_slice(&oks);
    let oe = cbc_encrypt(&hash_r6(owner, &oks, &u), &[0; 16], &file_key);

    // Every permission granted: -4 is all bits set but the two reserved ones.
    let p: i32 = -4;
    let mut perms = [0u8; 16];
    perms[..4].copy_from_slice(&p.to_le_bytes());
    perms[4..8].copy_from_slice(&[0xFF; 4]);
    perms[8] = b'T';
    perms[9..12].copy_from_slice(b"adb");
    perms[12..].copy_from_slice(&random::<4>());
    let perms = cbc_encrypt(&file_key, &[0; 16], &perms);

    // Everything that is not the trailer gets encrypted, before the
    // encryption dictionary itself is added.
    for (_, object) in doc.objects.iter_mut() {
        if let Object::Stream(stream) = object {
            if stream.dict.type_is(b"XRef") {
                continue;
            }
            let content = encrypt_bytes(&file_key, &stream.content);
            stream.set_content(content);
        }
        encrypt_strings(&file_key, object);
    }

    let hex = |bytes: Vec<u8>| Object::String(bytes, StringFormat::Hexadecimal);
    let mut std_cf = Dictionary::new();
    std_cf.set("AuthEvent", Object::Name(b"DocOpen".to_vec()));
    std_cf.set("CFM", Object::Name(b"AESV3".to_vec()));
    std_cf.set("Length", 32);
    let mut cf = Dictionary::new();
    cf.set("StdCF", std_cf);
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"Standard".to_vec()));
    dict.set("V", 5);
    dict.set("R", 6);
    dict.set("Length", 256);
    dict.set("CF", cf);
    dict.set("StmF", Object::Name(b"StdCF".to_vec()));
    dict.set("StrF", Object::Name(b"StdCF".to_vec()));
    dict.set("O", hex(o));
    dict.set("U", hex(u));
    dict.set("OE", hex(oe));
    dict.set("UE", hex(ue));
    dict.set("P", p as i64);
    dict.set("Perms", hex(perms));
    let id = doc.add_object(dict);
    doc.trailer.set("Encrypt", Object::Reference(id));
    // Encrypted files must say who they are; a new ID is as good as any.
    if doc.trailer.get(b"ID").is_err() {
        let fresh: [u8; 16] = random();
        doc.trailer.set("ID", vec![hex(fresh.to_vec()), hex(fresh.to_vec())]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::pdf::tests::{labels, sample};

    fn saved(doc: &mut Document) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        crate::utilities::pdf::save(doc, file.path()).unwrap();
        file
    }

    /// Real files, against another implementation: opens `UTILITIES_LOCKED`
    /// with `UTILITIES_PASSWORD` and writes it out unlocked, and protects it
    /// again with the same password, both into `UTILITIES_OUT`.
    #[test]
    #[ignore]
    fn locked_files() {
        let locked = std::path::PathBuf::from(std::env::var("UTILITIES_LOCKED").unwrap());
        let password = std::env::var("UTILITIES_PASSWORD").unwrap();
        let out = std::path::PathBuf::from(std::env::var("UTILITIES_OUT").unwrap());
        println!("lock: {:?}", lock_of(&locked).unwrap());
        assert_eq!(load(&locked, "wrong").err(), Some(CryptError::WrongPassword));
        let mut doc = load(&locked, &password).unwrap();
        println!("pages: {}", doc.get_pages().len());
        crate::utilities::pdf::save(&mut doc, &out.join("unlocked.pdf")).unwrap();
        let new_password = std::env::var("UTILITIES_NEW_PASSWORD").unwrap_or(password);
        protect(&mut doc, &new_password).unwrap();
        crate::utilities::pdf::save(&mut doc, &out.join("protected.pdf")).unwrap();
        let again = load(&out.join("protected.pdf"), &new_password).unwrap();
        assert_eq!(again.get_pages().len(), doc.get_pages().len());
    }

    #[test]
    fn rc4_matches_a_known_vector() {
        // RFC 6229, key 0x0102030405, first bytes of keystream.
        let out = rc4(&[1, 2, 3, 4, 5], &[0; 8]);
        assert_eq!(out, [0xb2, 0x39, 0x63, 0x05, 0xf0, 0x3d, 0xc0, 0x27]);
    }

    #[test]
    fn aes_cbc_matches_a_known_vector() {
        // NIST SP 800-38A F.2.5, CBC-AES256.Encrypt, first block.
        let key = hex::decode("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4").unwrap();
        let iv = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let plain = hex::decode("6bc1bee22e409f96e93d7e117393172a").unwrap();
        let out = cbc_encrypt(&key, &iv, &plain);
        assert_eq!(hex::encode(&out), "f58c4c04d6e5f1ba779eabfb5f7bfbd6");
        assert_eq!(cbc_decrypt(&key, &iv, &out), plain);
    }

    #[test]
    fn a_protected_file_opens_with_its_password_and_not_without() {
        let mut doc = sample(3, "a");
        protect(&mut doc, "mật khẩu").unwrap();
        let file = saved(&mut doc);
        assert_eq!(lock_of(file.path()).unwrap(), Lock::Password);
        assert_eq!(load(file.path(), "").err(), Some(CryptError::WrongPassword));
        assert_eq!(load(file.path(), "mat khau").err(), Some(CryptError::WrongPassword));
        let opened = load(file.path(), "mật khẩu").unwrap();
        assert_eq!(labels(&opened), ["a-1", "a-2", "a-3"]);
        assert!(opened.trailer.get(b"Encrypt").is_err());
    }

    #[test]
    fn an_unprotected_file_opens_as_it_is() {
        let file = saved(&mut sample(2, "a"));
        assert_eq!(lock_of(file.path()).unwrap(), Lock::None);
        assert_eq!(labels(&load(file.path(), "").unwrap()), ["a-1", "a-2"]);
    }

    #[test]
    fn padding_a_password_follows_the_standard() {
        let p = padded_password("ab");
        assert_eq!(&p[..2], b"ab");
        assert_eq!(&p[2..], &PAD[..30]);
        assert_eq!(utf8_password(&"é".repeat(100)).len(), 126, "cut on a character boundary");
    }
}
