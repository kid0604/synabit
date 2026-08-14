//! Synabit Mailbox wire protocol types (client-side).
//!
//! These types mirror the server's `protocol.rs` exactly. Both sides must
//! agree on the serialization format (postcard, length-prefixed).
//!
//! The protocol runs over QUIC bidirectional streams with ALPN `b"synabit/mailbox/1"`.

use serde::{Deserialize, Serialize};

pub use synabit_protocol::{
    BatchResultItem, Capability, MailboxEntry, MailboxEntryV3, MailboxRequest, MailboxResponse,
    PullPageResult, PushBatchItem, ServerHello, SyncEntryKind, SyncMode, SyncPlan, TrashMetaEntry,
    MAILBOX_ALPN, MAX_MESSAGE_SIZE,
};

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
    let payload = postcard::to_stdvec(msg).map_err(|e| format!("serialize error: {}", e))?;
    let len = u32::try_from(payload.len()).map_err(|_| "message too large".to_string())?;
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

    match postcard::take_from_bytes::<T>(&buf) {
        Ok((val, remainder)) => {
            if !remainder.is_empty() {
                Err(format!(
                    "deserialize error: trailing {} unconsumed bytes inside frame",
                    remainder.len()
                ))
            } else {
                Ok(Some(val))
            }
        }
        Err(e) => Err(format!("deserialize error: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_message_rejects_trailing_postcard_bytes() {
        let msg = synabit_protocol::MailboxRequest::Pull { since_seq: 10 };
        let mut payload = postcard::to_stdvec(&msg).unwrap();

        // Create valid framed buffer
        let valid_len = payload.len() as u32;
        let mut framed_valid = valid_len.to_be_bytes().to_vec();
        framed_valid.extend_from_slice(&payload);

        let mut cursor_valid = std::io::Cursor::new(framed_valid);
        let res_valid: Option<synabit_protocol::MailboxRequest> =
            read_message(&mut cursor_valid).await.unwrap();
        assert!(res_valid.is_some());

        // Append trailing garbage bytes inside the frame payload
        payload.extend_from_slice(b"GARBAGE_BYTES_INSIDE_FRAME");
        let invalid_len = payload.len() as u32;
        let mut framed_invalid = invalid_len.to_be_bytes().to_vec();
        framed_invalid.extend_from_slice(&payload);

        let mut cursor_invalid = std::io::Cursor::new(framed_invalid);
        let res_invalid: Result<Option<synabit_protocol::MailboxRequest>, String> =
            read_message(&mut cursor_invalid).await;
        assert!(
            res_invalid.is_err(),
            "Frame with trailing bytes must be rejected"
        );
        assert!(res_invalid.unwrap_err().contains("trailing"));
    }
}
