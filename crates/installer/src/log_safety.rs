use evohime_launcher::observed_command::CommandEvent;

/// Удаляет известные секреты только из строк дочернего процесса. Безопасное
/// представление команды формируется отдельно и по контракту секретов не
/// содержит.
pub fn redact_command_event(mut event: CommandEvent, secrets: &[&str]) -> CommandEvent {
    if let CommandEvent::Output { line, .. } = &mut event {
        for secret in secrets.iter().copied().filter(|secret| !secret.is_empty()) {
            *line = line.replace(secret, "<скрыто>");
        }
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_launcher::observed_command::CommandStream;

    #[test]
    fn redacts_known_secret_from_command_output() {
        let event = CommandEvent::Output {
            stream: CommandStream::Stderr,
            line: "connection password=generated-secret failed".to_string(),
        };

        let redacted = redact_command_event(event, &["generated-secret"]);
        assert!(matches!(
            redacted,
            CommandEvent::Output { line, .. }
                if line == "connection password=<скрыто> failed"
        ));
    }

    #[test]
    fn ignores_empty_secret_to_avoid_corrupting_every_character() {
        let event = CommandEvent::Output {
            stream: CommandStream::Stdout,
            line: "normal output".to_string(),
        };

        let redacted = redact_command_event(event.clone(), &[""]);
        assert_eq!(redacted, event);
    }
}
