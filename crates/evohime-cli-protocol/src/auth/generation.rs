use evohime_desktop_ipc::generated;

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
