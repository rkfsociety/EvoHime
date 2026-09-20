use evohime_desktop_ipc::{generated, session};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn ready_generation(event: &generated::EventEnvelope) -> Result<(String, u64), String> {
    if !matches!(
        &event.event,
        Some(generated::event_envelope::Event::Ready(_))
    ) {
        return Err("authentication_failed: Core did not become ready".into());
    }
    if event.core_instance_id.is_empty() || event.session_epoch == 0 {
        return Err("authentication_failed: Core generation is invalid".into());
    }
    Ok((event.core_instance_id.clone(), event.session_epoch))
}

pub(crate) fn validate_event_generation(
    event: &generated::EventEnvelope,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), String> {
    if core_instance_id.is_empty() {
        return Ok(());
    }
    if event.core_instance_id != core_instance_id || event.session_epoch != session_epoch {
        return Err("protocol_error: Core generation changed".into());
    }
    Ok(())
}

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
