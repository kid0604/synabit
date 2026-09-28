//! The format, held to the design and frozen.
//!
//! Two kinds of test live here. Most say what must be true of any correct
//! implementation — a wrong password is told apart from a damaged file, no
//! single flipped bit goes unnoticed, an item cannot be moved to another file.
//! The last two say what *this* implementation produced on the day the format
//! was fixed, and must go on producing: `testdata/vectors.json`.

use serde_json::{json, Value};

use super::crypto::{self, ctx, CryptoError, KdfParams, Key, SecretKey, PAD_BLOCK, PAD_MIN};
use super::format::{
    FormatError, ItemContent, ItemFile, ItemHeader, Keyset, KeysetHeader, OpenError, ITEM_BODY_OFFSET,
    ITEM_HEADER_LEN, KEYSET_AAD_LEN, KEYSET_LEN,
};

// ─── fixed inputs ────────────────────────────────────────
//
// Deliberately patterned bytes rather than random ones: a vector is easier to
// check by hand, and to recognise in a hex dump, when each input is one byte
// repeated.

const PASSWORD: &[u8] = b"correct horse battery staple";
const SALT: [u8; 32] = [0x11; 32];
const SECRET_KEY: [u8; 16] = [0x22; 16];
const SAFE_KEY: [u8; 32] = [0x33; 32];
const SAFE_ID: [u8; 16] = [0x44; 16];
const ITEM_KEY: [u8; 32] = [0x55; 32];
const ITEM_ID: [u8; 16] = [0x66; 16];
const WRAP_NONCE: [u8; 24] = [0x77; 24];
const ITEM_KEY_NONCE: [u8; 24] = [0x88; 24];
const BODY_NONCE: [u8; 24] = [0x99; 24];
const PLAINTEXT: &[u8] = br#"{"title":"GitHub","username":"anh","password":"hunter2"}"#;

/// Tiny, so the suite stays fast. Real Safes are held to `KdfParams::FLOOR`.
const TEST_KDF: KdfParams = KdfParams { m_kib: 256, t: 2, p: 1 };

fn auk() -> Key {
    crypto::derive_auk(PASSWORD, &SALT, TEST_KDF, &SecretKey::from_bytes(SECRET_KEY)).expect("derive")
}

fn keyset_header() -> KeysetHeader {
    KeysetHeader { safe_id: SAFE_ID, key_epoch: 1, keyset_revision: 7, kdf: TEST_KDF, kdf_salt: SALT }
}

fn keyset() -> Keyset {
    Keyset::seal(keyset_header(), &auk(), &Key::from_bytes(SAFE_KEY), WRAP_NONCE).expect("seal keyset")
}

fn item_header(tombstone: bool) -> ItemHeader {
    ItemHeader { safe_id: SAFE_ID, item_id: ITEM_ID, revision: 3, key_epoch: 1, tombstone }
}

fn item() -> ItemFile {
    ItemFile::seal(
        item_header(false),
        &Key::from_bytes(SAFE_KEY),
        &Key::from_bytes(ITEM_KEY),
        ITEM_KEY_NONCE,
        BODY_NONCE,
        PLAINTEXT,
    )
    .expect("seal item")
}

fn tombstone() -> ItemFile {
    ItemFile::seal(
        item_header(true),
        &Key::from_bytes(SAFE_KEY),
        &Key::from_bytes(ITEM_KEY),
        ITEM_KEY_NONCE,
        BODY_NONCE,
        &[],
    )
    .expect("seal tombstone")
}

fn body(content: ItemContent) -> Vec<u8> {
    match content {
        ItemContent::Body(b) => b.to_vec(),
        ItemContent::Tombstone => panic!("expected a body, got a tombstone"),
    }
}

// ─── the keyset ──────────────────────────────────────────

#[test]
fn the_layout_constants_match_the_design() {
    // Section 5.2 and 5.3 of the design draw these offsets. If a field is
    // added, the drawing changes first.
    assert_eq!(KEYSET_AAD_LEN, 110);
    assert_eq!(KEYSET_LEN, 182);
    assert_eq!(ITEM_HEADER_LEN, 56);
    assert_eq!(ITEM_BODY_OFFSET, 152);
    assert_eq!(keyset().encode().len(), KEYSET_LEN);
}

#[test]
fn a_keyset_round_trips_and_opens_with_the_right_password() {
    let decoded = Keyset::decode(&keyset().encode()).expect("decode");
    assert_eq!(decoded, keyset());
    let safe_key = decoded.open(&auk()).expect("open");
    assert_eq!(safe_key.as_bytes(), &SAFE_KEY);
}

#[test]
fn a_wrong_password_is_reported_as_one() {
    let wrong = crypto::derive_auk(b"correct horse battery stapler", &SALT, TEST_KDF, &SecretKey::from_bytes(SECRET_KEY))
        .expect("derive");
    assert_eq!(keyset().open(&wrong).unwrap_err(), OpenError::WrongPassword);
}

/// The right password on a device without the right Secret Key opens nothing.
/// This is the property that makes a copied `keyset.safe` worthless alone.
#[test]
fn the_right_password_without_the_secret_key_opens_nothing() {
    let mut other = SECRET_KEY;
    other[0] ^= 1;
    let without = crypto::derive_auk(PASSWORD, &SALT, TEST_KDF, &SecretKey::from_bytes(other)).expect("derive");
    assert_eq!(keyset().open(&without).unwrap_err(), OpenError::WrongPassword);
}

/// Every single-bit change to `keyset.safe` is caught: by decoding, by the key
/// check, or by the AEAD. None of them yields a key.
///
/// The interesting ones are the Argon2id parameters. Lowering them would let an
/// attacker who can write the file make the next unlock cheaper to brute-force;
/// they are in the AAD, so the unwrap fails instead.
#[test]
fn no_bit_of_a_keyset_can_be_changed_unnoticed() {
    let bytes = keyset().encode();
    let auk = auk();
    for byte in 0..bytes.len() {
        for bit in 0..8 {
            let mut tampered = bytes.clone();
            tampered[byte] ^= 1 << bit;
            if let Ok(parsed) = Keyset::decode(&tampered) {
                assert!(parsed.open(&auk).is_err(), "flipping bit {bit} of byte {byte} went unnoticed");
            }
        }
    }
}

#[test]
fn a_matching_key_check_with_a_failed_unwrap_is_corruption_not_a_wrong_password() {
    let mut damaged = keyset();
    damaged.wrapped_safe_key[0] ^= 1;
    assert_eq!(damaged.open(&auk()).unwrap_err(), OpenError::Corrupt);
}

#[test]
fn keyset_decoding_rejects_what_no_build_wrote() {
    let good = keyset().encode();

    assert_eq!(Keyset::decode(b"nope").unwrap_err(), FormatError::BadMagic);
    assert_eq!(Keyset::decode(&good[..good.len() - 1]).unwrap_err(), FormatError::BadLength { found: KEYSET_LEN - 1 });

    let mut newer = good.clone();
    newer[4] = 2;
    assert_eq!(Keyset::decode(&newer).unwrap_err(), FormatError::UnsupportedVersion(2));

    let mut flagged = good.clone();
    flagged[6] = 1;
    assert_eq!(Keyset::decode(&flagged).unwrap_err(), FormatError::UnknownFlags(1));

    let mut other_kdf = good.clone();
    other_kdf[36] = 2;
    assert_eq!(Keyset::decode(&other_kdf).unwrap_err(), FormatError::UnknownKdf(2));

    let mut no_lanes = good.clone();
    no_lanes[45] = 0;
    assert_eq!(Keyset::decode(&no_lanes).unwrap_err(), FormatError::BadKdfParams);
}

// ─── items ───────────────────────────────────────────────

#[test]
fn an_item_round_trips() {
    let decoded = ItemFile::decode(&item().encode()).expect("decode");
    assert_eq!(decoded, item());
    let opened = decoded.open(&Key::from_bytes(SAFE_KEY), &SAFE_ID, &ITEM_ID).expect("open");
    assert_eq!(body(opened), PLAINTEXT);
}

#[test]
fn no_bit_of_an_item_can_be_changed_unnoticed() {
    let bytes = item().encode();
    let safe_key = Key::from_bytes(SAFE_KEY);
    for byte in 0..bytes.len() {
        for bit in 0..8 {
            let mut tampered = bytes.clone();
            tampered[byte] ^= 1 << bit;
            if let Ok(parsed) = ItemFile::decode(&tampered) {
                assert!(
                    parsed.open(&safe_key, &SAFE_ID, &ITEM_ID).is_err(),
                    "flipping bit {bit} of byte {byte} went unnoticed"
                );
            }
        }
    }
}

/// A sync peer copies item A's file over item B's. The reader finds A's
/// header where B was expected and refuses before decrypting anything.
#[test]
fn an_item_moved_to_another_file_is_refused() {
    let mut elsewhere = ITEM_ID;
    elsewhere[0] ^= 1;
    let err = item().open(&Key::from_bytes(SAFE_KEY), &SAFE_ID, &elsewhere).unwrap_err();
    assert_eq!(err, OpenError::Misplaced);
}

/// And one that has had its header rewritten to match is caught by the AEAD,
/// because the header is the associated data.
#[test]
fn an_item_with_a_rewritten_header_does_not_decrypt() {
    let mut forged = item();
    forged.header.item_id[0] ^= 1;
    let expected = forged.header.item_id;
    assert_eq!(forged.open(&Key::from_bytes(SAFE_KEY), &SAFE_ID, &expected).unwrap_err(), OpenError::Corrupt);

    // The rollback case: an old revision relabelled as a new one.
    let mut replayed = item();
    replayed.header.revision += 1;
    assert_eq!(replayed.open(&Key::from_bytes(SAFE_KEY), &SAFE_ID, &ITEM_ID).unwrap_err(), OpenError::Corrupt);
}

#[test]
fn an_item_from_another_safe_is_refused() {
    let mut other = SAFE_ID;
    other[0] ^= 1;
    assert_eq!(item().open(&Key::from_bytes(SAFE_KEY), &other, &ITEM_ID).unwrap_err(), OpenError::WrongSafe);
}

#[test]
fn an_item_does_not_open_under_another_safe_key() {
    let mut other = SAFE_KEY;
    other[0] ^= 1;
    assert_eq!(item().open(&Key::from_bytes(other), &SAFE_ID, &ITEM_ID).unwrap_err(), OpenError::Corrupt);
}

#[test]
fn a_tombstone_round_trips_and_cannot_carry_a_body() {
    let decoded = ItemFile::decode(&tombstone().encode()).expect("decode");
    assert_eq!(decoded.encode().len(), ITEM_BODY_OFFSET + crypto::TAG_LEN);
    assert_eq!(decoded.open(&Key::from_bytes(SAFE_KEY), &SAFE_ID, &ITEM_ID).unwrap(), ItemContent::Tombstone);

    let refused = ItemFile::seal(
        item_header(true),
        &Key::from_bytes(SAFE_KEY),
        &Key::from_bytes(ITEM_KEY),
        ITEM_KEY_NONCE,
        BODY_NONCE,
        b"not empty",
    );
    assert_eq!(refused.unwrap_err(), CryptoError::Seal);
}

/// Setting the tombstone flag on a live item cannot delete it: the flag is in
/// the header, and the header is the AAD.
#[test]
fn a_live_item_cannot_be_turned_into_a_tombstone() {
    let mut bytes = item().encode();
    bytes[6] |= 1;
    let parsed = ItemFile::decode(&bytes).expect("still parses");
    assert!(parsed.header.tombstone);
    assert_eq!(parsed.open(&Key::from_bytes(SAFE_KEY), &SAFE_ID, &ITEM_ID).unwrap_err(), OpenError::Corrupt);
}

#[test]
fn item_decoding_rejects_what_no_build_wrote() {
    let good = item().encode();
    assert_eq!(ItemFile::decode(b"SFK1").unwrap_err(), FormatError::BadMagic);
    assert_eq!(
        ItemFile::decode(&good[..ITEM_BODY_OFFSET]).unwrap_err(),
        FormatError::BadLength { found: ITEM_BODY_OFFSET }
    );
    let mut flagged = good.clone();
    flagged[6] = 2;
    assert_eq!(ItemFile::decode(&flagged).unwrap_err(), FormatError::UnknownFlags(2));
    let mut reserved = good.clone();
    reserved[52] = 1;
    assert_eq!(ItemFile::decode(&reserved).unwrap_err(), FormatError::ReservedSet);
}

// ─── padding ─────────────────────────────────────────────

#[test]
fn padding_hides_length_in_steps_of_a_block() {
    for len in 0..=2_000 {
        let body = vec![0xAB; len];
        let padded = crypto::pad(&body).unwrap();
        assert!(padded.len().is_multiple_of(PAD_BLOCK), "length {len}");
        assert!(padded.len() >= PAD_MIN, "length {len}");
        assert!(padded.len() - (len + 4) < PAD_BLOCK || padded.len() == PAD_MIN, "length {len} over-padded");
        assert_eq!(crypto::unpad(&padded).unwrap(), &body[..], "length {len}");
    }
    // One password and a paragraph of notes are the same size on disk.
    assert_eq!(crypto::pad(b"hunter2").unwrap().len(), crypto::pad(&[b'x'; 500]).unwrap().len());
    assert_eq!(crypto::pad(&[0; 508]).unwrap().len(), 512);
    assert_eq!(crypto::pad(&[0; 509]).unwrap().len(), 768);
}

#[test]
fn unpadding_accepts_exactly_one_encoding() {
    let good = crypto::pad(b"hunter2").unwrap();

    let mut stray = good.to_vec();
    *stray.last_mut().unwrap() = 1;
    assert_eq!(crypto::unpad(&stray), Err(CryptoError::Padding));

    let mut overlong = good.to_vec();
    overlong[..4].copy_from_slice(&(good.len() as u32).to_le_bytes());
    assert_eq!(crypto::unpad(&overlong), Err(CryptoError::Padding));

    // Valid content, but padded further than `pad` would have.
    let mut extra = good.to_vec();
    extra.extend_from_slice(&[0; PAD_BLOCK]);
    assert_eq!(crypto::unpad(&extra), Err(CryptoError::Padding));

    assert_eq!(crypto::unpad(&good[..PAD_MIN - 1]), Err(CryptoError::Padding));
    assert_eq!(crypto::unpad(&[]), Err(CryptoError::Padding));
}

// ─── keys ────────────────────────────────────────────────

#[test]
fn every_context_string_is_distinct_and_dated() {
    let mut seen = std::collections::HashSet::new();
    for c in ctx::ALL {
        assert!(seen.insert(c), "{c} is used twice");
        assert!(c.starts_with("synabit safe 2026-09 "), "{c} does not follow the convention");
    }
}

#[test]
fn subkeys_for_different_purposes_differ() {
    let root = Key::from_bytes(SAFE_KEY);
    let mut seen = std::collections::HashSet::new();
    for c in ctx::ALL {
        assert!(seen.insert(*crypto::subkey(c, &root).as_bytes()), "{c} collides with another purpose");
    }
    assert!(!seen.contains(&SAFE_KEY), "a subkey equals its root");
}

#[test]
fn kdf_parameters_below_the_floor_are_recognised() {
    assert!(KdfParams::STARTING.meets_floor());
    assert!(KdfParams::FLOOR.meets_floor());
    assert!(!KdfParams { m_kib: 64 * 1024 - 1, ..KdfParams::FLOOR }.meets_floor());
    assert!(!KdfParams { t: 2, ..KdfParams::FLOOR }.meets_floor());
    assert!(!TEST_KDF.meets_floor());
}

#[test]
fn keys_do_not_print() {
    let key = Key::from_bytes(SAFE_KEY);
    let printed = format!("{key:?} {:?}", SecretKey::from_bytes(SECRET_KEY));
    assert!(!printed.contains("51"), "a key byte leaked into Debug output: {printed}");
    assert!(printed.contains("redacted"));
}

#[test]
fn random_keys_are_random() {
    let a = Key::random().unwrap();
    let b = Key::random().unwrap();
    assert_ne!(a.as_bytes(), b.as_bytes());
    assert_ne!(a.as_bytes(), &[0; 32]);
}

// ─── frozen vectors ──────────────────────────────────────

fn vectors_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/safe/testdata/vectors.json")
}

fn computed_vectors() -> Value {
    let auk = auk();
    let safe_key = Key::from_bytes(SAFE_KEY);
    let subkeys: serde_json::Map<String, Value> =
        ctx::ALL.iter().map(|c| (c.to_string(), json!(hex::encode(crypto::subkey(c, &safe_key).as_bytes())))).collect();

    json!({
        "about": "Frozen outputs of src/safe for fixed inputs. See safe::tests. \
                  Changing any value here changes the format of data in users' vaults.",
        "inputs": {
            "password": String::from_utf8(PASSWORD.to_vec()).unwrap(),
            "kdf_salt": hex::encode(SALT),
            "kdf": { "m_kib": TEST_KDF.m_kib, "t": TEST_KDF.t, "p": TEST_KDF.p },
            "secret_key": hex::encode(SECRET_KEY),
            "safe_key": hex::encode(SAFE_KEY),
            "safe_id": hex::encode(SAFE_ID),
            "item_key": hex::encode(ITEM_KEY),
            "item_id": hex::encode(ITEM_ID),
            "wrap_nonce": hex::encode(WRAP_NONCE),
            "item_key_nonce": hex::encode(ITEM_KEY_NONCE),
            "body_nonce": hex::encode(BODY_NONCE),
            "plaintext": String::from_utf8(PLAINTEXT.to_vec()).unwrap(),
        },
        "auk": hex::encode(auk.as_bytes()),
        "key_check": hex::encode(crypto::key_check(&auk)),
        "subkeys_of_safe_key": subkeys,
        "keyset_safe": hex::encode(keyset().encode()),
        "item_safe": hex::encode(item().encode()),
        "tombstone_safe": hex::encode(tombstone().encode()),
    })
}

/// Everything this module computes, for fixed inputs, is what it computed
/// when the format was fixed.
///
/// Re-blessing is deliberate, and means a format change:
///
///   SAFE_BLESS_VECTORS=1 cargo test --lib safe::tests::frozen
///
/// Not "write the file if it is missing", for the reason
/// `the_system_prompt_matches_its_snapshot` gives: that blesses silently on
/// the one run that should have caught the change.
#[test]
fn frozen_vectors_are_what_this_build_computes() {
    let computed = computed_vectors();
    let path = vectors_path();

    if std::env::var_os("SAFE_BLESS_VECTORS").is_some() {
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("testdata dir");
        let mut text = serde_json::to_string_pretty(&computed).expect("serialise");
        text.push('\n');
        std::fs::write(&path, text).expect("write vectors");
        eprintln!("blessed {}", path.display());
        return;
    }

    let frozen: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{} is missing ({e}); it is the definition of the Safe format", path.display())
    }))
    .expect("vectors are JSON");
    assert_eq!(computed, frozen, "src/safe no longer produces its frozen vectors — this is a format change");
}

/// The frozen *files* still open — read from the JSON, not recomputed, so this
/// holds even if sealing and opening changed together in a way that kept them
/// agreeing with each other but not with what users already have on disk.
#[test]
fn frozen_files_still_open() {
    let frozen: Value =
        serde_json::from_str(&std::fs::read_to_string(vectors_path()).expect("vectors")).expect("vectors are JSON");
    let bytes = |name: &str| hex::decode(frozen[name].as_str().expect(name)).expect("hex");

    let keyset = Keyset::decode(&bytes("keyset_safe")).expect("decode keyset");
    let derived = crypto::derive_auk(
        PASSWORD,
        &keyset.header.kdf_salt,
        keyset.header.kdf,
        &SecretKey::from_bytes(SECRET_KEY),
    )
    .expect("derive");
    let safe_key = keyset.open(&derived).expect("the frozen keyset opens");
    assert_eq!(safe_key.as_bytes(), &SAFE_KEY);

    let item = ItemFile::decode(&bytes("item_safe")).expect("decode item");
    assert_eq!(body(item.open(&safe_key, &SAFE_ID, &ITEM_ID).expect("the frozen item opens")), PLAINTEXT);

    let gone = ItemFile::decode(&bytes("tombstone_safe")).expect("decode tombstone");
    assert_eq!(gone.open(&safe_key, &SAFE_ID, &ITEM_ID).unwrap(), ItemContent::Tombstone);
}

// ─── untrusted bytes ─────────────────────────────────────
//
// Everything below reads bytes another device chose: a sync peer, a copied
// file, a pasted link. None of it may panic — a panic on the sync path is a
// device that stops syncing, and on the unlock path a Safe that cannot open.
// Not cargo-fuzz (that wants nightly), but the same idea on every run: many
// random inputs, and every valid file damaged in every way a byte can be.

fn random_inputs(count: usize, max_len: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|i| {
            let len = i % max_len;
            let mut bytes = vec![0u8; len];
            use rand::RngCore;
            rand::rng().fill_bytes(&mut bytes);
            // Half of them start with a real magic, so the parser gets past
            // the first check and into the fields.
            if i % 2 == 0 && len >= 4 {
                bytes[..4].copy_from_slice(if i % 4 == 0 { b"SFK1" } else { b"SFI1" });
                if len >= 6 {
                    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
                }
            }
            bytes
        })
        .collect()
}

#[test]
fn parsers_never_panic_on_random_bytes() {
    let safe_key = Key::from_bytes(SAFE_KEY);
    for bytes in random_inputs(4_000, 700) {
        if let Ok(k) = Keyset::decode(&bytes) {
            let _ = k.open(&auk());
        }
        if let Ok(f) = ItemFile::decode(&bytes) {
            let _ = f.open(&safe_key, &SAFE_ID, &ITEM_ID);
        }
        let _ = crypto::unpad(&bytes);
        let _ = super::item::ItemBody::from_json(&bytes);
        let _ = super::totp::Totp::parse(&String::from_utf8_lossy(&bytes));
    }
}

/// Truncated at every length, and each byte set to each of a few values that
/// break parsers — the edges of the length fields, zero, all ones.
#[test]
fn damaged_files_never_panic() {
    let safe_key = Key::from_bytes(SAFE_KEY);
    for valid in [keyset().encode(), item().encode(), tombstone().encode()] {
        for len in 0..valid.len() {
            let _ = Keyset::decode(&valid[..len]).map(|k| k.open(&auk()).is_ok());
            let _ = ItemFile::decode(&valid[..len]).map(|f| f.open(&safe_key, &SAFE_ID, &ITEM_ID).is_ok());
        }
        for at in 0..valid.len() {
            for value in [0x00, 0x01, 0x7f, 0x80, 0xff] {
                let mut bytes = valid.clone();
                bytes[at] = value;
                if let Ok(k) = Keyset::decode(&bytes) {
                    let _ = k.open(&auk());
                }
                if let Ok(f) = ItemFile::decode(&bytes) {
                    let _ = f.open(&safe_key, &SAFE_ID, &ITEM_ID);
                }
            }
        }
    }
}

/// The sync decision reads a peer's bytes while the Safe is locked.
#[test]
fn the_sync_decision_never_panics_on_what_a_peer_sends() {
    let dir = tempfile::tempdir().unwrap();
    super::store::write_atomic(&dir.path().join("Safe/keyset.safe"), &keyset().encode()).unwrap();
    let path = format!("Safe/items/{}.safe", hex::encode(ITEM_ID));
    for bytes in random_inputs(1_000, 400) {
        let _ = super::sync::decide(dir.path(), &path, &bytes);
        let _ = super::sync::decide(dir.path(), "Safe/keyset.safe", &bytes);
    }
}
