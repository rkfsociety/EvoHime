pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/evohime.listener.rs"));
}
use prost::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("frame is too large")]
    TooLarge,
    #[error("truncated frame")]
    Truncated,
    #[error("I/O: {0}")]
    Io(String),
    #[error("protobuf: {0}")]
    Protobuf(#[from] prost::DecodeError),
}
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<generated::Envelope, FrameError> {
    let mut len = [0; 4];
    reader
        .read_exact(&mut len)
        .await
        .map_err(|_| FrameError::Truncated)?;
    let size = u32::from_le_bytes(len) as usize;
    if size > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    };
    let mut data = vec![0; size];
    reader
        .read_exact(&mut data)
        .await
        .map_err(|_| FrameError::Truncated)?;
    Ok(generated::Envelope::decode(data.as_slice())?)
}
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &generated::Envelope,
) -> Result<(), FrameError> {
    let data = message.encode_to_vec();
    if data.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    };
    writer
        .write_all(&(data.len() as u32).to_le_bytes())
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
    writer
        .write_all(&data)
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
    Ok(())
}
pub fn envelope(payload: generated::envelope::Payload) -> generated::Envelope {
    generated::Envelope {
        payload: Some(payload),
    }
}
