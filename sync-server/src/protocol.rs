//! Wire protocol types for the Synabit Mailbox protocol.
//!
//! All messages are serialized with `postcard` (compact binary) and framed with
//! a 4-byte big-endian length prefix over QUIC bidirectional streams.
//!
//! The server NEVER interprets the `encrypted_payload` — it is opaque ciphertext
//! produced by the client's E2EE layer.

use serde::{Deserialize, Serialize};

/// ALPN identifier for the Synabit Mailbox protocol.
/// Used during QUIC/TLS handshake to route connections to our handler.
pub const MAILBOX_ALPN: &[u8] = b"synabit/mailbox/1";

/// Maximum size of a single framed message (16 MiB).
/// Prevents a malicious peer from forcing unbounded memory allocation.
pub const MAX_MESSAGE_SIZE: u32 = 128 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Trash metadata (synced soft-delete)
// ---------------------------------------------------------------------------

/// Metadata for a trashed document, synced across devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashMetaEntry {
    pub doc_hash: [u8; 32],
    pub original_path_encrypted: Vec<u8>,
    pub deleted_at: u64,
    pub deleted_by_device: String,
    pub is_purged: bool,
}

// ---------------------------------------------------------------------------
// Request types (client → server)
// ---------------------------------------------------------------------------

/// Requests sent from a client device to the mailbox server.
#[derive(Debug, Serialize, Deserialize)]
pub struct PushBatchItem {
    pub doc_hash: [u8; 32],
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MailboxRequest {
    /// Initial handshake to negotiate protocol version.
    Hello {
        version: u32,
    },
    /// Authenticate this connection for a specific vault.
    /// Must be the first message on every stream.
    Auth {
        /// BLAKE3 hash of the vault identifier.
        vault_hash: [u8; 32],
        /// `blake3::derive_key("synabit-mailbox-v1", &e2ee_key)` — proves
        /// knowledge of the vault's E2EE key without revealing it.
        mailbox_token: [u8; 32],
        /// Opaque device identifier (chosen by the client).
        device_id: String,
    },

    /// Push a new encrypted CRDT document snapshot.
    Push {
        /// BLAKE3 hash of the logical document.
        doc_hash: [u8; 32],
        /// The encrypted payload (opaque to the server).
        encrypted_payload: Vec<u8>,
        /// BLAKE3 hash of `encrypted_payload` for integrity verification.
        payload_hash: [u8; 32],
    },
    /// Batch push multiple documents
    PushBatch {
        items: Vec<PushBatchItem>,
    },

    /// Pull all mailbox entries with `seq > since_seq`.
    Pull {
        /// Sequence number cursor — only entries after this are returned.
        since_seq: u64,
    },

    /// Acknowledge that this device has processed entries up to `up_to_seq`.
    /// Once ALL registered devices ACK an entry, the server may garbage-collect it.
    Ack {
        up_to_seq: u64,
    },

    /// Push an encrypted binary asset (image, attachment, etc.).
    PushAsset {
        /// Content-hash of the plaintext asset (chosen by client).
        asset_hash: [u8; 32],
        /// Encrypted asset data.
        encrypted_data: Vec<u8>,
    },

    /// Pull a previously stored asset by its hash.
    PullAsset {
        asset_hash: [u8; 32],
    },

    /// Mark a document as deleted (tombstone).
    PushDelete {
        doc_hash: [u8; 32],
    },

    /// Push encrypted trash metadata entries for synced soft-delete.
    PushTrashMeta {
        entries: Vec<TrashMetaEntry>,
    },

    /// Pull all trash metadata for the vault.
    PullTrashMeta,

    /// Application-level keepalive ping.
    Ping,

    /// Notify server that a document has been restored from trash.
    PushRestore {
        doc_hash: [u8; 32],
    },

    /// Revoke a device — server deletes its cursor so it can no longer ACK.
    RevokeDevice {
        device_id: String,
    },

    /// Rotate the vault's mailbox token after an epoch increment.
    RotateToken {
        new_mailbox_token: Vec<u8>,
    },
}

// ---------------------------------------------------------------------------
// Response types (server → client)
// ---------------------------------------------------------------------------

/// Responses sent from the mailbox server back to the client.
#[derive(Debug, Serialize, Deserialize)]
pub enum MailboxResponse {
    /// Server push notification: new data is available for this vault.
    NotifyNewData {
        /// The sequence number that triggered this notification.
        trigger_seq: u64,
    },
    /// Handshake successful.
    HelloOk {
        server_version: u32,
        max_bytes: u64,
    },
    /// Authentication succeeded.
    AuthOk,

    /// Authentication failed.
    AuthFailed { reason: String },

    /// Document push accepted; returns the assigned sequence number.
    PushOk { seq: u64 },
    PushBatchOk { max_seq: u64 },

    /// Result of a Pull request — a batch of entries since the requested cursor.
    PullResult { entries: Vec<MailboxEntry> },

    /// ACK recorded successfully.
    AckOk,

    /// Asset stored successfully.
    AssetOk,

    /// Requested asset data.
    AssetData { data: Vec<u8> },

    /// Requested asset was not found.
    AssetNotFound,

    /// Delete tombstone recorded; returns the assigned sequence number.
    DeleteOk { seq: u64 },

    /// Generic error.
    Error { message: String },

    /// Application-level keepalive pong.
    Pong,

    /// Storage quota exceeded — the vault has used all its allocated space.
    QuotaExceeded {
        current_bytes: u64,
        limit_bytes: u64,
    },

    /// Result of a PullTrashMeta request — all trash metadata for the vault.
    TrashMetaResult {
        entries: Vec<TrashMetaEntry>,
    },

    /// Confirmation that a document was restored from trash.
    RestoreOk {
        seq: u64,
    },

    /// Device revocation recorded.
    RevokeOk,

    /// Mailbox token rotated successfully.
    TokenRotated,
}

// ---------------------------------------------------------------------------
// Mailbox entry (returned inside PullResult)
// ---------------------------------------------------------------------------

/// A single mailbox entry representing either a document update or a deletion.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MailboxEntry {
    /// Monotonically increasing per-vault sequence number.
    pub seq: u64,
    /// BLAKE3 hash of the logical document.
    pub doc_hash: [u8; 32],
    /// Device that pushed this entry.
    pub source_device: String,
    /// Encrypted CRDT payload (empty for deletes).
    pub encrypted_payload: Vec<u8>,
    /// BLAKE3 hash of the encrypted payload.
    pub payload_hash: [u8; 32],
    /// Unix timestamp (seconds) when the server received this entry.
    pub timestamp: i64,
    /// Whether this entry represents a deletion tombstone.
    pub is_delete: bool,
}

// ---------------------------------------------------------------------------
// Length-prefixed framing helpers
// ---------------------------------------------------------------------------

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Write a length-prefixed postcard-encoded message to an async writer.
pub async fn write_message<W, T>(writer: &mut W, msg: &T) -> anyhow::Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let payload = postcard::to_stdvec(msg)?;
    let len = u32::try_from(payload.len())
        .map_err(|_| anyhow::anyhow!("message too large to frame"))?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&payload).await?;
    Ok(())
}

/// Read a length-prefixed postcard-encoded message from an async reader.
///
/// Returns `Ok(None)` when the stream is cleanly closed (EOF on the length prefix).
pub async fn read_message<R, T>(reader: &mut R) -> anyhow::Result<Option<T>>
where
    R: AsyncReadExt + Unpin,
    T: for<'de> Deserialize<'de>,
{
    // Read 4-byte big-endian length prefix.
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_n) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }

    let len = u32::from_be_bytes(len_buf);
    if len > MAX_MESSAGE_SIZE {
        anyhow::bail!(
            "message size {} exceeds maximum {}",
            len,
            MAX_MESSAGE_SIZE
        );
    }

    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf).await?;
    let msg = postcard::from_bytes(&buf)?;
    Ok(Some(msg))
}
