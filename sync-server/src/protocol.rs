//! Wire protocol types for the Synabit Mailbox protocol.
//!
//! Re-exports the shared types from `synabit_protocol`.

pub use synabit_protocol::*;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Write a length-prefixed postcard-encoded message to an async writer.
pub async fn write_message<W, T>(writer: &mut W, msg: &T) -> anyhow::Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let payload = postcard::to_stdvec(msg)?;
    let len =
        u32::try_from(payload.len()).map_err(|_| anyhow::anyhow!("message too large to frame"))?;
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
        anyhow::bail!("message size {} exceeds maximum {}", len, MAX_MESSAGE_SIZE);
    }

    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf).await?;
    match postcard::take_from_bytes::<T>(&buf) {
        Ok((msg, remainder)) => {
            if !remainder.is_empty() {
                anyhow::bail!(
                    "deserialize error: trailing {} unconsumed bytes inside frame",
                    remainder.len()
                );
            }
            Ok(Some(msg))
        }
        Err(e) => Err(e.into()),
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
        let res_invalid: anyhow::Result<Option<synabit_protocol::MailboxRequest>> =
            read_message(&mut cursor_invalid).await;
        assert!(
            res_invalid.is_err(),
            "Frame with trailing bytes must be rejected"
        );
        assert!(res_invalid.unwrap_err().to_string().contains("trailing"));
    }
}
