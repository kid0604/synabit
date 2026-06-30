//! Synabit Mailbox wire protocol types (client-side).
//!
//! These types mirror the server's `protocol.rs` exactly. Both sides must
//! agree on the serialization format (postcard, length-prefixed).
//!
//! The protocol runs over QUIC bidirectional streams with ALPN `b"synabit/mailbox/1"`.

use serde::{Deserialize, Serialize};

/// ALPN identifier for the Synabit Mailbox protocol.
pub const MAILBOX_ALPN: &[u8] = b"synabit/mailbox/1";

/// Maximum framed message size (16 MiB).
pub const MAX_MESSAGE_SIZE: u32 = 128 * 1024 * 1024;


// ---------------------------------------------------------------------------
// Request types (client → server)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub enum MailboxRequest {
    Auth {
        vault_hash: [u8; 32],
        mailbox_token: [u8; 32],
        device_id: String,
    },
    Push {
        doc_hash: [u8; 32],
        encrypted_payload: Vec<u8>,
        payload_hash: [u8; 32],
    },
    Pull {
        since_seq: u64,
    },
    Ack {
        up_to_seq: u64,
    },
    PushAsset {
        asset_hash: [u8; 32],
        encrypted_data: Vec<u8>,
    },
    PullAsset {
        asset_hash: [u8; 32],
    },
    PushDelete {
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

#[derive(Debug, Serialize, Deserialize)]
pub enum MailboxResponse {
    AuthOk,
    AuthFailed { reason: String },
    PushOk { seq: u64 },
    PullResult { entries: Vec<MailboxEntry> },
    AckOk,
    AssetOk,
    AssetData { data: Vec<u8> },
    AssetNotFound,
    DeleteOk { seq: u64 },
    Error { message: String },
    QuotaExceeded {
        current_bytes: u64,
        limit_bytes: u64,
    },

    /// Device revocation recorded.
    RevokeOk,
    /// Mailbox token rotated successfully.
    TokenRotated,
}

// ---------------------------------------------------------------------------
// Mailbox entry (inside PullResult)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MailboxEntry {
    pub seq: u64,
    pub doc_hash: [u8; 32],
    pub source_device: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub timestamp: i64,
    pub is_delete: bool,
}

// ---------------------------------------------------------------------------
// Length-prefixed framing helpers (postcard over QUIC streams)
// ---------------------------------------------------------------------------

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Write a length-prefixed postcard message to a QUIC send stream.
pub async fn write_message<W, T>(writer: &mut W, msg: &T) -> Result<(), String>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let payload =
        postcard::to_stdvec(msg).map_err(|e| format!("serialize error: {}", e))?;
    let len = u32::try_from(payload.len())
        .map_err(|_| "message too large".to_string())?;
    writer
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| format!("write len error: {}", e))?;
    writer
        .write_all(&payload)
        .await
        .map_err(|e| format!("write payload error: {}", e))?;
    Ok(())
}

/// Read a length-prefixed postcard message from a QUIC recv stream.
/// Returns `Ok(None)` on clean EOF.
pub async fn read_message<R, T>(reader: &mut R) -> Result<Option<T>, String>
where
    R: AsyncReadExt + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(format!("read len error: {}", e)),
    }

    let len = u32::from_be_bytes(len_buf);
    if len > MAX_MESSAGE_SIZE {
        return Err(format!("message too large: {} bytes", len));
    }

    let mut buf = vec![0u8; len as usize];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("read payload error: {}", e))?;
    postcard::from_bytes(&buf).map(Some).map_err(|e| format!("deserialize error: {}", e))
}
