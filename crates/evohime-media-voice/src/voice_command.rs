//! Разбор голосовой команды, адресованной Еве.
//!
//! Разбор детерминированный и без модели: «Ева, открой хром» обязано работать
//! в офлайне и на той же секунде, а не после round-trip к провайдеру. Модель
//! здесь не нужна ещё и потому, что решение всё равно ограничено каталогом
//! приложений — угадывать нечего, есть только сопоставление с allow-list.
//!
//! Два правила, из которых состоит вся безопасность разбора:
//!
//! - **обращение обязательно.** Без имени в начале фразы команды нет: рядом с
//!   микрофоном разговаривают друг с другом, и «открой окно» в разговоре двух
//!   людей не должно ничего запускать;
//! - **цель — не путь.** Из фразы достаётся только текст названия, а во что он
//!   превратится, решает каталог. Пути, аргументов и командной строки в разборе
//!   нет как понятия.

use serde::{Deserialize, Serialize};

/// Как к Еве обращаются вслух.
///
/// Варианты — это то, что реально выдаёт распознавание на одно и то же имя, а
/// не список прозвищ: whisper на русском слышит «Ева» и «Эва» примерно поровну.
pub const WAKE_WORDS: &[&str] = &["ева", "эва"];

/// Глаголы открытия.
const OPEN_VERBS: &[&str] = &[
    "открой",
    "открои",
    "открыть",
    "запусти",
    "запустить",
    "включи",
    "включить",
];

/// Слова, которые могут стоять между обращением и глаголом.
const BRIDGE_WORDS: &[&str] = &["а", "ну", "слушай", "пожалуйста", "давай", "ка"];

/// Служебные слова вокруг названия: «открой мне пожалуйста приложение хром» и
/// «хром» обязаны попасть в одну запись каталога.
const FILLER_WORDS: &[&str] = &[
    "пожалуйста",
    "мне",
    "давай",
    "ка",
    "приложение",
    "программу",
    "программа",
    "приложуху",
    "app",
];

/// Сколько слов допускается между обращением и глаголом.
const MAX_BRIDGE_WORDS: usize = 2;

/// Потолок длины цели: название приложения — это одно-три слова, а не остаток
/// разговора.
const MAX_TARGET_WORDS: usize = 4;
const MAX_TARGET_CHARS: usize = 64;

/// Что услышано после разбора голосовой команды.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceIntent {
    /// Тип распознанного намерения (например, `"open_app"`).
    pub kind: String,
    /// Название приложения так, как его произнесли. Каталог разбирается с ним
    /// сам — здесь оно остаётся текстом.
    pub target: String,
}

/// Приводит сказанное к сравнимому виду: нижний регистр, `ё` как `е`,
/// всё, что не буква и не цифра, — разделитель.
pub fn normalize(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.trim().chars() {
        let character = match character {
            'ё' | 'Ё' => 'е',
            other => other,
        };
        if character.is_alphanumeric() {
            if pending_space && !result.is_empty() {
                result.push(' ');
            }
            pending_space = false;
            for lower in character.to_lowercase() {
                result.push(lower);
            }
        } else {
            pending_space = true;
        }
    }
    result
}

/// Убирает служебные слова из уже нормализованного запроса.
pub fn strip_filler_words(normalized: &str) -> String {
    normalized
        .split(' ')
        .filter(|word| !word.is_empty() && !FILLER_WORDS.contains(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Разбирает высказывание. `None` — обычная фраза, а не команда.
pub fn parse(text: &str) -> Option<VoiceIntent> {
    let normalized = normalize(text);
    if normalized.is_empty() {
        return None;
    }
    let words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
    let wake = words.iter().position(|word| WAKE_WORDS.contains(word))?;
    let mut index = wake + 1;
    let mut bridged = 0;
    while index < words.len() && BRIDGE_WORDS.contains(&words[index]) {
        bridged += 1;
        if bridged > MAX_BRIDGE_WORDS {
            return None;
        }
        index += 1;
    }
    if index >= words.len() || !OPEN_VERBS.contains(&words[index]) {
        return None;
    }
    // Служебные слова убираются здесь же: «открой-ка мне блокнот» и «открой
    // блокнот» — одна и та же команда, и различать их дальше по конвейеру
    // незачем.
    let target = strip_filler_words(&words[index + 1..].join(" "));
    if target.is_empty()
        || target.split(' ').count() > MAX_TARGET_WORDS
        || target.chars().count() > MAX_TARGET_CHARS
    {
        return None;
    }
    Some(VoiceIntent {
        kind: "open_app".to_owned(),
        target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(target: &str) -> Option<VoiceIntent> {
        Some(VoiceIntent {
            kind: "open_app".to_owned(),
            target: target.to_owned(),
        })
    }

    #[test]
    fn a_direct_address_with_a_verb_is_a_command() {
        assert_eq!(parse("Ева, открой хром"), open("хром"));
        assert_eq!(parse("эва запусти телеграм"), open("телеграм"));
        assert_eq!(
            parse("Ева, включи Visual Studio Code"),
            open("visual studio code")
        );
        assert_eq!(parse("Ева, а открой-ка блокнот"), open("блокнот"));
    }

    #[test]
    fn bridge_words_between_the_name_and_the_verb_are_allowed() {
        assert_eq!(parse("Ева, пожалуйста открой блокнот"), open("блокнот"));
        assert_eq!(parse("Ева слушай давай открой почту"), open("почту"));
    }

    #[test]
    fn a_phrase_without_the_address_is_not_a_command() {
        assert_eq!(parse("открой окно, душно"), None);
        assert_eq!(parse("надо открыть хром и посмотреть"), None);
    }

    #[test]
    fn the_address_must_come_before_the_verb() {
        assert_eq!(parse("открой хром, Ева"), None);
    }

    #[test]
    fn an_address_without_a_verb_is_not_a_command() {
        assert_eq!(parse("Ева, привет"), None);
        assert_eq!(parse("Ева"), None);
        assert_eq!(parse("Ева, открой"), None);
    }

    #[test]
    fn a_target_longer_than_a_name_is_refused() {
        assert_eq!(
            parse("Ева, открой мне пожалуйста тот самый документ который мы вчера обсуждали"),
            None
        );
    }

    #[test]
    fn too_many_words_between_the_address_and_the_verb_break_the_link() {
        assert_eq!(parse("Ева ну давай ка слушай открой хром"), None);
    }

    #[test]
    fn empty_and_noise_input_is_not_a_command() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("   ...   "), None);
    }
}
