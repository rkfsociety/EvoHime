use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{encode_frame, FrameError, MAX_FRAME_BYTES};

pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>, FrameError> {
    let mut length_bytes = [0_u8; 4];
    reader
        .read_exact(&mut length_bytes)
        .await
        .map_err(|_| FrameError::Truncated)?;
    let length = u32::from_le_bytes(length_bytes) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }

    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|_| FrameError::Truncated)?;
    Ok(payload)
}

pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> Result<(), FrameError> {
    let frame = encode_frame(payload)?;
    writer
        .write_all(&frame)
        .await
        .map_err(|error| FrameError::Io(error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| FrameError::Io(error.to_string()))?;
    Ok(())
}
