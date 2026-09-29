//! Safe: the secret manager, for the user and for Syn.
//!
//! The design is `docs/safe-2026-09-28.md`. What exists so far is its
//! foundation — the two parts every later piece stands on and that must never
//! change underneath a user's data:
//!
//! * [`crypto`] — the primitives and how they compose: Argon2id and a Secret
//!   Key into an Account Unlock Key, BLAKE3 subkeys, XChaCha20-Poly1305 with
//!   associated data, length padding.
//! * [`format`] — the bytes on disk: `keyset.safe` and `items/<id>.safe`.
//!
//! Both are pure. Nothing here reads a file, touches a keychain or knows about
//! Tauri, so every rule the design states can be pinned by a test that runs in
//! milliseconds — and is: `testdata/vectors.json` freezes the output of every
//! function for fixed inputs. A change that alters one byte of it is a change
//! to the format of data already sitting in users' vaults.
//!
//! # Why not SQLCipher
//!
//! Section 3 of the design has the table. The deciding reason is not
//! cryptographic: Synabit syncs files, and one binary database edited on two
//! devices is one conflict over the whole store. One file per item is what
//! makes two devices editing two different items not a conflict at all.
//!
//! # "Safe", not "vault"
//!
//! In this codebase the vault is the user's folder of notes, and the "vault
//! key" is the sync E2EE key. The Safe Key is neither and is never derived from
//! either — see [`crypto::derive_auk`] for where it comes from instead.

pub mod bridge;
pub mod clipboard;
pub mod crypto;
pub mod device;
pub mod egress;
pub mod exchange;
pub mod format;
pub mod generator;
pub mod guard;
pub mod health;
pub mod item;
pub mod keyset;
pub mod memory;
pub mod requests;
pub mod session;
#[cfg(desktop)]
pub mod ssh;
#[cfg(all(desktop, unix))]
pub mod ssh_agent;
pub mod store;
pub mod sync;
pub mod totp;

#[cfg(test)]
mod tests;
