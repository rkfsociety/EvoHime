//! Единый формат времени и правила retention для ambient-домена.
//!
//! Этот модуль — контракт, а не реализация хранилища: SQL остаётся в
//! `evohime_local_storage::ambient_store`, а файлы политики — в ядре. Здесь
//! живёт только то, что обязано совпадать у всех участников домена: формат
//! меток времени, сроки хранения и адресация журнальных строк.

use crate::{policy::MAX_RETENTION_DAYS, AmbientLogEvent, DEFAULT_RETENTION_DAYS};

/// Retention текста транскриптов, в сутках. Потолок берётся из контракта,
/// чтобы значение не разъехалось с валидацией политики.
pub const RETENTION_DAYS_ENV: &str = "EVOHIME_AMBIENT_RETENTION_DAYS";

/// Метаданные эпизода живут дольше текста: список «когда слушали» нужен
/// пользователю, чтобы было что удалять, но и он не вечен.
pub const EPISODE_METADATA_RETENTION_DAYS: u32 = 30;
/// Tombstone — след удаления, а не вечная запись.
pub const TOMBSTONE_RETENTION_DAYS: u32 = 30;
/// Собственный срок ambient-строк в `events`. У самой таблицы retention нет
/// вообще, поэтому без этого срока хронология слушания жила бы вечно.
pub const EVENT_RETENTION_DAYS: u32 = 30;

/// Интервал фонового purge.
pub const PURGE_INTERVAL_SECONDS: u64 = 60 * 60;

/// `task_id` ambient-событий, у которых нет эпизода.
///
/// Соглашение «`task_id` ambient-события — это `episode_id`» существует ради
/// удаления: иначе вычищать журнал пришлось бы сканом BLOB-payload, а так
/// оно идёт по существующему индексу `idx_events_task_sequence`.
pub const SESSION_TASK_ID: &str = "ambient-session";

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// Retention текста из окружения: по умолчанию 7 суток, потолок 90.
/// Мусор в переменной — это не «слушать вечно», а дефолт.
pub fn retention_days_from_env() -> u32 {
    parse_retention_days(std::env::var(RETENTION_DAYS_ENV).ok().as_deref())
}

/// Разбирает значение retention из окружения; мусор и ноль означают дефолт.
pub fn parse_retention_days(raw: Option<&str>) -> u32 {
    raw.and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|days| *days > 0)
        .map(|days| days.min(MAX_RETENTION_DAYS))
        .unwrap_or(DEFAULT_RETENTION_DAYS)
}

/// Единый формат времени ambient-строк: тот же, что SQLite пишет в
/// `events.created_at`, поэтому лексикографическое сравнение совпадает с
/// хронологическим и окно forget можно применять к обеим таблицам сразу.
pub fn timestamp_ms(unix_ms: u64) -> String {
    let clamped = i64::try_from(unix_ms).unwrap_or(i64::MAX);
    chrono::DateTime::from_timestamp_millis(clamped)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Момент истечения: `now + days`.
pub fn expires_at(now_ms: u64, days: u32) -> String {
    timestamp_ms(now_ms.saturating_add(u64::from(days) * DAY_MS))
}

/// Граница retention: `now - days`.
pub fn cutoff_at(now_ms: u64, days: u32) -> String {
    timestamp_ms(now_ms.saturating_sub(u64::from(days) * DAY_MS))
}

/// Локальная минута суток пользователя: вход «тихих часов» политики.
pub fn local_minute_of_day() -> u32 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    now.hour() * 60 + now.minute()
}

/// `task_id`, под которым ambient-событие попадает в `events`.
pub fn event_task_id(event: &AmbientLogEvent) -> String {
    match event {
        AmbientLogEvent::Transcript { episode_id, .. } => episode_id.to_string(),
        // Предложение адресуется своему эпизоду по той же причине: удаление
        // источника обязано унести и строку `ambient.proposal`, а ищется она
        // по индексу `idx_events_task_sequence`, а не сканом payload.
        AmbientLogEvent::Proposal {
            episode_id: Some(episode_id),
            ..
        } => episode_id.to_string(),
        _ => SESSION_TASK_ID.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_falls_back_to_the_default_instead_of_listening_forever() {
        assert_eq!(parse_retention_days(None), DEFAULT_RETENTION_DAYS);
        assert_eq!(parse_retention_days(Some("  ")), DEFAULT_RETENTION_DAYS);
        assert_eq!(parse_retention_days(Some("ноль")), DEFAULT_RETENTION_DAYS);
        assert_eq!(parse_retention_days(Some("0")), DEFAULT_RETENTION_DAYS);
        assert_eq!(parse_retention_days(Some("-3")), DEFAULT_RETENTION_DAYS);
        assert_eq!(parse_retention_days(Some("365")), MAX_RETENTION_DAYS);
        assert_eq!(parse_retention_days(Some("14")), 14);
    }

    #[test]
    fn timestamps_sort_the_same_way_the_clock_moves() {
        let earlier = timestamp_ms(1_000);
        let later = timestamp_ms(2_000);
        assert!(earlier < later);
        assert_eq!(expires_at(0, 7), timestamp_ms(7 * 24 * 60 * 60 * 1000));
        assert_eq!(cutoff_at(7 * 24 * 60 * 60 * 1000, 7), timestamp_ms(0));
    }

    #[test]
    fn only_transcript_events_are_addressed_to_an_episode() {
        use crate::{
            state::ListeningState, EpisodeId, ProposalId, ProposalKind, ProposalState, SubjectKey,
        };
        let transcript = AmbientLogEvent::Transcript {
            episode_id: EpisodeId::new("ep-1").expect("episode id"),
            started_at_ms: 0,
            utterance_count: 0,
            extraction_state: crate::ExtractionState::Pending,
        };
        let proposal = AmbientLogEvent::Proposal {
            proposal_id: ProposalId::new("prop-1").expect("proposal id"),
            episode_id: Some(EpisodeId::new("ep-2").expect("episode id")),
            kind: ProposalKind::Reminder,
            subject_key: SubjectKey::new("subject").expect("subject key"),
            proposal_state: ProposalState::Proposed,
        };
        let error = AmbientLogEvent::Error {
            code: crate::AmbientErrorCode::StorageFailed,
            state: ListeningState::Listening,
        };
        assert_eq!(event_task_id(&transcript), "ep-1");
        assert_eq!(event_task_id(&proposal), "ep-2");
        assert_eq!(event_task_id(&error), SESSION_TASK_ID);
    }
}
