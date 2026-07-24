//! Токен сессии Launcher'а (раздел XV плана): случайный UUID v4,
//! генерируется заново при каждом запуске, живёт только в памяти
//! (никогда не пишется на диск). Передаётся дочерним процессам
//! (`server.exe`, `worker.py`) через переменную окружения
//! `EVOHIME_LOCAL_TOKEN` и требуется заголовком `Authorization: Bearer`
//! на всех локальных управляющих эндпоинтах — `127.0.0.1` не значит
//! "доверенный источник": любой сайт, открытый в браузере пользователя,
//! может дёрнуть `fetch('http://127.0.0.1:3001/...')` без каких-либо
//! дополнительных разрешений.

/// Генерирует новый случайный токен сессии.
pub fn generate_session_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Сравнивает предъявленный токен с ожидаемым в постоянное время
/// (независимо от длины совпадающего префикса) — тот же подход, что и в
/// `evohime-server::auth::tokens_equal`, чтобы не давать оракул по времени
/// ответа для локального REST-эндпоинта статуса.
pub fn tokens_equal(expected: &str, presented: &str) -> bool {
    if expected.len() != presented.len() {
        return false;
    }
    expected
        .bytes()
        .zip(presented.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_well_formed_uuid_v4_tokens() {
        let token = generate_session_token();
        let parsed = uuid::Uuid::parse_str(&token).expect("should be a valid UUID");
        assert_eq!(parsed.get_version_num(), 4);
    }

    #[test]
    fn generates_distinct_tokens_across_calls() {
        let a = generate_session_token();
        let b = generate_session_token();
        assert_ne!(a, b);
    }

    #[test]
    fn tokens_equal_matches_identical_strings() {
        assert!(tokens_equal("abc123", "abc123"));
    }

    #[test]
    fn tokens_equal_rejects_mismatched_strings() {
        assert!(!tokens_equal("abc123", "abc124"));
        assert!(!tokens_equal("abc123", "abc12"));
        assert!(!tokens_equal("abc123", ""));
    }
}
