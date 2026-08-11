pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/evohime.desktop.v1.rs"));
}
pub mod transport;

pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl ProtocolVersion {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub const fn is_compatible(self, peer: Self) -> bool {
        self.major == peer.major
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("frame is truncated")]
    Truncated,
    #[error("frame exceeds the {MAX_FRAME_BYTES} byte limit")]
    TooLarge,
    #[error("frame length prefix does not match payload")]
    LengthMismatch,
    #[error("IPC I/O error: {0}")]
    Io(String),
}

pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }

    let length = u32::try_from(payload.len()).map_err(|_| FrameError::TooLarge)?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

pub fn decode_frame(frame: &[u8]) -> Result<&[u8], FrameError> {
    if frame.len() < 4 {
        return Err(FrameError::Truncated);
    }

    let length = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    if frame.len() < 4 + length {
        return Err(FrameError::Truncated);
    }
    if frame.len() != 4 + length {
        return Err(FrameError::LengthMismatch);
    }

    Ok(&frame[4..])
}

#[cfg(test)]
mod tests {
    use crate::generated;
    use super::{
        decode_frame, encode_frame, transport, FrameError, ProtocolVersion, MAX_FRAME_BYTES,
    };

    #[test]
    fn accepts_additive_minor_versions() {
        assert!(ProtocolVersion::new(1, 0).is_compatible(ProtocolVersion::new(1, 1)));
        assert!(ProtocolVersion::new(1, 1).is_compatible(ProtocolVersion::new(1, 0)));
    }

    #[test]
    fn rejects_major_version_mismatch() {
        assert!(!ProtocolVersion::new(1, 0).is_compatible(ProtocolVersion::new(2, 0)));
    }

    #[test]
    fn rejects_malformed_frame() {
        assert!(matches!(
            decode_frame(&[1, 2, 3]),
            Err(FrameError::Truncated)
        ));
    }

    #[test]
    fn round_trips_a_bounded_frame() {
        let frame = encode_frame(b"hello").expect("small frame");
        assert_eq!(decode_frame(&frame).expect("valid frame"), b"hello");
    }

    #[test]
    fn rejects_oversized_frame() {
        let payload = vec![0; MAX_FRAME_BYTES + 1];
        assert_eq!(encode_frame(&payload), Err(FrameError::TooLarge));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut frame = encode_frame(b"hello").expect("small frame");
        frame.push(0);
        assert_eq!(decode_frame(&frame), Err(FrameError::LengthMismatch));
    }

    #[tokio::test]
    async fn async_transport_round_trips_a_frame() {
        let (mut writer, mut reader) = tokio::io::duplex(64);
        let write = tokio::spawn(async move {
            transport::write_frame(&mut writer, b"hello")
                .await
                .expect("write frame");
        });

        let payload = transport::read_frame(&mut reader)
            .await
            .expect("read frame");
        write.await.expect("writer task");
        assert_eq!(payload, b"hello");
    }

    #[test]
    fn legacy_command_envelope_decodes_after_additive_fields() {
        use prost::Message;

        let legacy = generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: "legacy-request".into(),
            client_id: String::new(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 7 },
            )),
        };
        let decoded = generated::CommandEnvelope::decode(legacy.encode_to_vec().as_slice())
            .expect("legacy envelope decodes");
        assert_eq!(decoded.request_id, "legacy-request");
        assert_eq!(decoded.session_epoch, 0);
        assert!(matches!(
            decoded.command,
            Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 7 }
            ))
        ));
    }

    #[test]
    fn unknown_additive_field_is_ignored() {
        use prost::Message;

        let mut encoded = generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: "request".into(),
            client_id: String::new(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: None,
        }
        .encode_to_vec();
        encoded.extend_from_slice(&[0x98, 0x06, 0x01]);
        let decoded = generated::CommandEnvelope::decode(encoded.as_slice())
            .expect("unknown field is ignored");
        assert_eq!(decoded.request_id, "request");
    }
}
