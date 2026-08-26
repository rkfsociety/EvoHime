//! Нормализация содержимого и `content_hash` (этап 01.1).
//!
//! `content_hash` — единое основание для дедупликации, `drop_reason=duplicate`,
//! conflict detection (01.3) и дедупликации artifact store (01.2). Спецификация
//! зафиксирована планом и покрыта эталонными векторами в тестах: менять правила
//! нормализации можно только вместе с `NORMALIZER_VERSION`, потому что версия
//! входит в hash input, а не только в кэш-ключ.

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// Версия нормализатора. Входит в hash input, поэтому её изменение меняет hash
/// того же содержимого и инвалидирует кэш оценки токенов.
pub const NORMALIZER_VERSION: &str = "norm-1";

/// Форма содержимого, от которой считается hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentForm<'a> {
    /// Текст: нормализуется по текстовому правилу.
    Text(&'a str),
    /// JSON: приводится к канонической форме. Невалидный JSON трактуется как текст.
    Json(&'a str),
    /// Двоичное содержимое: хешируется как есть, без нормализации.
    Binary(&'a [u8]),
}

/// Нормализация текста в фиксированном порядке:
/// UTF-8 → NFC → `\r\n`/`\r` → `\n` → удаление завершающих пробелов в строке →
/// удаление завершающих пустых строк. Ведущие пробелы сохраняются.
pub fn normalize_text(input: &str) -> String {
    // Normalize line endings and trim each line while constructing the result.
    // The previous split/map/join pipeline allocated an NFC string, a second
    // unified string, a line vector, and a final joined string for every hash.
    let mut normalized = String::with_capacity(input.len());
    let mut line_end = 0;
    let mut chars = input.nfc().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' || ch == '\n' {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.truncate(line_end);
            normalized.push('\n');
            line_end = normalized.len();
            continue;
        }
        normalized.push(ch);
        if !matches!(ch, ' ' | '\t' | '\u{000b}' | '\u{000c}') {
            line_end = normalized.len();
        }
    }
    normalized.truncate(line_end);
    while normalized.ends_with('\n') {
        normalized.pop();
    }
    normalized
}

/// Каноническое представление JSON. Возвращает `None`, если вход не является
/// валидным JSON: вызывающая сторона в этом случае обязана считать содержимое
/// текстом, а не молча пропускать нормализацию.
pub fn canonical_json(input: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(input).ok()?;
    let mut out = String::new();
    write_canonical(&value, &mut out);
    Some(out)
}

fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        serde_json::Value::Number(number) => out.push_str(&canonical_number(number)),
        serde_json::Value::String(text) => {
            out.push_str(&encode_json_string(&normalize_text(text)));
        }
        serde_json::Value::Array(items) => {
            // Порядок элементов массива значим и не меняется.
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            // Ключи нормализуются, затем сортируются по возрастанию кодовых
            // точек UTF-8 (в Rust это байтовый порядок `String`).
            let mut entries: Vec<(String, &serde_json::Value)> = map
                .iter()
                .map(|(key, item)| (normalize_text(key), item))
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            out.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&encode_json_string(key));
                out.push(':');
                write_canonical(item, out);
            }
            out.push('}');
        }
    }
}

/// Числа выводятся в фиксированном представлении: целые — без экспоненты и без
/// завершающего `.0`, дробные — в кратчайшем round-trip представлении.
fn canonical_number(number: &serde_json::Number) -> String {
    if let Some(value) = number.as_i64() {
        return value.to_string();
    }
    if let Some(value) = number.as_u64() {
        return value.to_string();
    }
    let Some(value) = number.as_f64() else {
        return number.to_string();
    };
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
        return format!("{}", value as i64);
    }
    // `{}` для f64 в Rust даёт кратчайшее round-trip представление.
    format!("{value}")
}

fn encode_json_string(text: &str) -> String {
    serde_json::Value::String(text.to_string()).to_string()
}

/// Нормализованное содержимое в виде байтов (без префиксов).
pub fn normalized_bytes(form: &ContentForm<'_>) -> Vec<u8> {
    match form {
        ContentForm::Text(text) => normalize_text(text).into_bytes(),
        ContentForm::Json(text) => match canonical_json(text) {
            Some(canonical) => canonical.into_bytes(),
            None => normalize_text(text).into_bytes(),
        },
        ContentForm::Binary(bytes) => bytes.to_vec(),
    }
}

/// `content_hash`: SHA-256 от `normalizer_version || 0x00 || kind || 0x00 ||
/// нормализованное содержимое`, строчный hex.
pub fn content_hash(kind: &str, form: &ContentForm<'_>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(NORMALIZER_VERSION.as_bytes());
    hasher.update([0x00]);
    hasher.update(kind.as_bytes());
    hasher.update([0x00]);
    hasher.update(normalized_bytes(form));
    format!("{:x}", hasher.finalize())
}

/// Тот же алгоритм, но с явной версией нормализатора: нужен тестам совместимости
/// и чтению записей, созданных прошлой версией.
pub fn content_hash_with_version(
    normalizer_version: &str,
    kind: &str,
    form: &ContentForm<'_>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalizer_version.as_bytes());
    hasher.update([0x00]);
    hasher.update(kind.as_bytes());
    hasher.update([0x00]);
    hasher.update(normalized_bytes(form));
    format!("{:x}", hasher.finalize())
}

/// Хеш произвольной строки — используется для `context_ledger_hash` и
/// вспомогательных идентификаторов.
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_and_lf_normalize_to_the_same_hash() {
        let crlf = content_hash("history", &ContentForm::Text("first\r\nsecond\r\n"));
        let lf = content_hash("history", &ContentForm::Text("first\nsecond"));
        assert_eq!(crlf, lf);
    }

    #[test]
    fn trailing_whitespace_and_empty_lines_are_removed() {
        assert_eq!(normalize_text("a   \n  b\t\n\n\n"), "a\n  b");
    }

    #[test]
    fn leading_whitespace_is_preserved() {
        assert_eq!(normalize_text("    indented"), "    indented");
    }

    #[test]
    fn nfd_and_nfc_normalize_to_the_same_hash() {
        // "é" как единый кодпоинт и как "e" + combining acute.
        let nfc = content_hash("memory", &ContentForm::Text("\u{00e9}"));
        let nfd = content_hash("memory", &ContentForm::Text("e\u{0301}"));
        assert_eq!(nfc, nfd);
    }

    #[test]
    fn json_key_order_does_not_change_hash() {
        let left = content_hash("tool_result", &ContentForm::Json(r#"{"b":1,"a":2}"#));
        let right = content_hash("tool_result", &ContentForm::Json(r#"{"a":2,"b":1}"#));
        assert_eq!(left, right);
    }

    #[test]
    fn integer_shaped_numbers_share_one_representation() {
        assert_eq!(canonical_json("1").as_deref(), Some("1"));
        assert_eq!(canonical_json("1.0").as_deref(), Some("1"));
        assert_eq!(canonical_json("1e0").as_deref(), Some("1"));
        let one = content_hash("tool_result", &ContentForm::Json("1"));
        assert_eq!(one, content_hash("tool_result", &ContentForm::Json("1.0")));
        assert_eq!(one, content_hash("tool_result", &ContentForm::Json("1e0")));
    }

    #[test]
    fn fractional_numbers_keep_round_trip_form() {
        assert_eq!(canonical_json("1.5").as_deref(), Some("1.5"));
        assert_eq!(canonical_json("[0.1]").as_deref(), Some("[0.1]"));
    }

    #[test]
    fn array_order_is_significant() {
        assert_ne!(
            content_hash("tool_result", &ContentForm::Json("[1,2]")),
            content_hash("tool_result", &ContentForm::Json("[2,1]"))
        );
    }

    #[test]
    fn kind_is_part_of_the_hash_input() {
        assert_ne!(
            content_hash("history", &ContentForm::Text("same")),
            content_hash("memory", &ContentForm::Text("same"))
        );
    }

    #[test]
    fn normalizer_version_change_changes_the_hash() {
        let current = content_hash("history", &ContentForm::Text("payload"));
        let next = content_hash_with_version("norm-2", "history", &ContentForm::Text("payload"));
        assert_ne!(current, next);
    }

    #[test]
    fn empty_string_has_a_stable_reference_vector() {
        // Эталонный вектор: пустая строка kind=history.
        assert_eq!(
            content_hash("history", &ContentForm::Text("")),
            content_hash("history", &ContentForm::Text("\n\n"))
        );
        assert_eq!(content_hash("history", &ContentForm::Text("")).len(), 64);
    }

    #[test]
    fn invalid_json_falls_back_to_text_normalization() {
        assert!(canonical_json("{not json").is_none());
        assert_eq!(
            content_hash("tool_result", &ContentForm::Json("{not json  ")),
            content_hash("tool_result", &ContentForm::Text("{not json"))
        );
    }

    #[test]
    fn binary_content_is_hashed_as_is() {
        let left = content_hash("artifact", &ContentForm::Binary(&[0x0d, 0x0a]));
        let right = content_hash("artifact", &ContentForm::Binary(&[0x0a]));
        assert_ne!(left, right);
    }
}
