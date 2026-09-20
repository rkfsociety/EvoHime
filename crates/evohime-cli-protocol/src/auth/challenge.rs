use evohime_desktop_ipc::{generated, session};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn challenge_nonce(event: &generated::EventEnvelope) -> Result<String, String> {
    let Some(generated::event_envelope::Event::AuthChallenge(challenge)) = &event.event else {
        return Err("authentication_failed: challenge missing".into());
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok());
    if challenge.expires_at_ms == 0
        || now_ms.is_none()
        || now_ms.is_some_and(|now| challenge.expires_at_ms <= now)
        || challenge.nonce.len() != session::NONCE_BYTES * 2
        || !challenge.nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("authentication_failed: challenge invalid".into());
    }
    Ok(challenge.nonce.clone())
}
