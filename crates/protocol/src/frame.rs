//! Length-prefixed JSON framing.
//!
//! Every control exchange is a `u32` big-endian byte count followed by a JSON
//! body. Once a stream finishes its control exchange it carries raw bytes with
//! no further framing, so these helpers are used only at the head of a stream.

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Upper bound on a single control frame. Control messages are small; anything
/// larger is a desynchronised stream rather than a legitimate message.
pub const MAX_FRAME_LEN: u32 = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed frame payload: {0}")]
    Json(#[from] serde_json::Error),
    #[error("frame of {0} bytes exceeds the {MAX_FRAME_LEN} byte limit")]
    TooLarge(u32),
}

/// Serialise `msg` as a single length-prefixed frame.
///
/// The length prefix and body are written in one `write_all` so a frame is never
/// split across writes — a reader on the other side always sees either a whole
/// prefix or nothing.
pub async fn write_frame<W, T>(w: &mut W, msg: &T) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let body = serde_json::to_vec(msg)?;
    let len = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge(u32::MAX))?;
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(len));
    }

    let mut buf = Vec::with_capacity(4 + body.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&body);
    w.write_all(&buf).await?;
    w.flush().await?;
    Ok(())
}

/// Read a single length-prefixed frame and deserialise it.
pub async fn read_frame<R, T>(r: &mut R) -> Result<T, FrameError>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(len));
    }

    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientHello, ControlToClient};

    #[tokio::test]
    async fn round_trips_a_message() {
        let hello = ClientHello {
            publisher_id: "acct-123".into(),
            client_version: "0.1.0".into(),
            country: Some("se".into()),
            ..Default::default()
        };

        let mut buf = Vec::new();
        write_frame(&mut buf, &hello).await.unwrap();

        let decoded: ClientHello = read_frame(&mut buf.as_slice()).await.unwrap();
        assert_eq!(decoded.publisher_id, "acct-123");
        assert_eq!(decoded.country.as_deref(), Some("se"));
    }

    #[tokio::test]
    async fn round_trips_consecutive_frames() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &ControlToClient::Ping { nonce: 1 })
            .await
            .unwrap();
        write_frame(&mut buf, &ControlToClient::Ping { nonce: 2 })
            .await
            .unwrap();

        let mut cursor = buf.as_slice();
        let first: ControlToClient = read_frame(&mut cursor).await.unwrap();
        let second: ControlToClient = read_frame(&mut cursor).await.unwrap();
        assert!(matches!(first, ControlToClient::Ping { nonce: 1 }));
        assert!(matches!(second, ControlToClient::Ping { nonce: 2 }));
    }

    #[tokio::test]
    async fn rejects_an_oversized_length_prefix() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(MAX_FRAME_LEN + 1).to_be_bytes());
        let err = read_frame::<_, ClientHello>(&mut buf.as_slice())
            .await
            .unwrap_err();
        assert!(matches!(err, FrameError::TooLarge(_)));
    }
}
