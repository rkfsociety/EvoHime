//! Дедупликация распознанных высказываний.
//!
//! Телевизор, повтор одной фразы и «эхо» одного и того же предложения через
//! паузу дают одинаковый текст. Записывать его несколько раз означает
//! засорить и хранилище, и последующую экстракцию памяти, поэтому повтор
//! подавляется **до** отправки в Core: подавленный текст не покидает процесс
//! листенера вовсе.
//!
//! Подавленное считается счётчиком, а не выбрасывается молча: пользователь
//! должен видеть, что фраза была услышана и признана повтором.

use std::collections::VecDeque;

use unicode_normalization::UnicodeNormalization;

/// Порог near-dup по token-set ratio.
pub const DEDUP_NEAR_THRESHOLD: f32 = 0.9;
/// Сколько предыдущих высказываний сравнивается на near-dup.
pub const DEDUP_RECENT_DEPTH: usize = 5;

/// Решение по одному высказыванию.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Admission {
    Accepted,
    /// Точный повтор нормализованного текста в окне.
    SuppressedExact,
    /// Почти тот же набор слов: пересказ той же реплики в пределах окна.
    SuppressedNearDup,
}

impl Admission {
    pub const fn accepted(self) -> bool {
        matches!(self, Admission::Accepted)
    }
}

#[derive(Debug)]
struct Entry {
    normalized: String,
    tokens: Vec<String>,
    at_ms: u64,
}

/// Окно дедупликации: точное совпадение по всему окну, near-dup — против
/// последних [`DEDUP_RECENT_DEPTH`] записей.
#[derive(Debug)]
pub struct Deduplicator {
    window_ms: u32,
    recent: VecDeque<Entry>,
    suppressed: u64,
}

impl Deduplicator {
    pub fn new(window_ms: u32) -> Self {
        Self {
            window_ms,
            recent: VecDeque::new(),
            suppressed: 0,
        }
    }

    /// Сколько высказываний подавлено за сессию.
    pub const fn suppressed(&self) -> u64 {
        self.suppressed
    }

    pub fn reset(&mut self) {
        self.recent.clear();
    }

    /// Принимает высказывание либо объясняет, почему оно подавлено.
    pub fn admit(&mut self, text: &str, at_ms: u64) -> Admission {
        self.prune(at_ms);
        let normalized = normalize(text);
        if normalized.is_empty() {
            self.suppressed = self.suppressed.saturating_add(1);
            return Admission::SuppressedExact;
        }
        if self
            .recent
            .iter()
            .any(|entry| entry.normalized == normalized)
        {
            self.suppressed = self.suppressed.saturating_add(1);
            return Admission::SuppressedExact;
        }
        let tokens = tokenize(&normalized);
        let near = self
            .recent
            .iter()
            .rev()
            .take(DEDUP_RECENT_DEPTH)
            .any(|entry| token_set_ratio(&entry.tokens, &tokens) >= DEDUP_NEAR_THRESHOLD);
        if near {
            self.suppressed = self.suppressed.saturating_add(1);
            return Admission::SuppressedNearDup;
        }
        self.recent.push_back(Entry {
            normalized,
            tokens,
            at_ms,
        });
        Admission::Accepted
    }

    fn prune(&mut self, now_ms: u64) {
        let cutoff = now_ms.saturating_sub(u64::from(self.window_ms));
        while self
            .recent
            .front()
            .is_some_and(|entry| entry.at_ms < cutoff)
        {
            self.recent.pop_front();
        }
    }
}

/// NFKC, нижний регистр, без пунктуации, одиночные пробелы.
///
/// NFKC, а не NFC: движок иногда отдаёт совместимые формы (например, полноширинные
/// знаки), и без него «одна и та же» фраза различалась бы по байтам.
pub fn normalize(text: &str) -> String {
    let folded: String = text
        .nfkc()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect();
    folded
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize(normalized: &str) -> Vec<String> {
    let mut tokens: Vec<String> = normalized.split(' ').map(str::to_owned).collect();
    tokens.sort_unstable();
    tokens.dedup();
    tokens
}

/// Мера Сёренсена–Дайса на множествах слов: `2·|A∩B| / (|A|+|B|)`.
///
/// Именно множеств, а не последовательностей: перестановка слов в повторе
/// фразы — обычное дело для распознавания, а вот другой набор слов означает
/// другую реплику.
fn token_set_ratio(left: &[String], right: &[String]) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = right
        .iter()
        .filter(|token| left.binary_search(token).is_ok())
        .count();
    2.0 * intersection as f32 / (left.len() + right.len()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_folds_case_and_punctuation() {
        assert_eq!(normalize("Привет, мир!"), "привет мир");
        assert_eq!(normalize("  ПРИВЕТ   мир  "), "привет мир");
        assert_eq!(normalize("!!!"), "");
        // NFKC приводит совместимые формы к обычным.
        assert_eq!(normalize("ﬁle"), "file");
    }

    #[test]
    fn exact_repeat_inside_the_window_is_suppressed_and_counted() {
        let mut dedup = Deduplicator::new(60_000);
        assert_eq!(dedup.admit("позвони маме", 1_000), Admission::Accepted);
        assert_eq!(
            dedup.admit("Позвони маме!", 5_000),
            Admission::SuppressedExact
        );
        assert_eq!(dedup.suppressed(), 1);
    }

    #[test]
    fn repeat_after_the_window_is_accepted_again() {
        let mut dedup = Deduplicator::new(60_000);
        assert_eq!(dedup.admit("позвони маме", 1_000), Admission::Accepted);
        assert_eq!(dedup.admit("позвони маме", 120_000), Admission::Accepted);
        assert_eq!(dedup.suppressed(), 0);
    }

    #[test]
    fn near_duplicate_is_suppressed() {
        let mut dedup = Deduplicator::new(60_000);
        assert_eq!(
            dedup.admit("надо позвонить маме сегодня вечером", 1_000),
            Admission::Accepted
        );
        // Отличается одним словом из десяти — это тот же повтор.
        assert_eq!(
            dedup.admit("надо позвонить маме сегодня вечером обязательно", 2_000),
            Admission::SuppressedNearDup
        );
    }

    #[test]
    fn a_different_sentence_passes() {
        let mut dedup = Deduplicator::new(60_000);
        assert_eq!(dedup.admit("позвони маме", 1_000), Admission::Accepted);
        assert_eq!(
            dedup.admit("завтра встреча в десять", 2_000),
            Admission::Accepted
        );
        assert_eq!(dedup.suppressed(), 0);
    }

    /// Near-dup смотрит на пять последних записей, а не на всё окно: иначе
    /// длинный разговор превращался бы в квадратичное сравнение.
    #[test]
    fn near_duplicate_check_is_bounded_by_recent_depth() {
        let mut dedup = Deduplicator::new(600_000);
        dedup.admit("первая фраза про отчёт", 1_000);
        for index in 0..DEDUP_RECENT_DEPTH {
            dedup.admit(
                &format!("другая реплика номер {index}"),
                2_000 + index as u64,
            );
        }
        assert_eq!(
            dedup.admit("первая фраза про отчет", 9_000),
            Admission::Accepted
        );
    }

    #[test]
    fn empty_text_is_never_written() {
        let mut dedup = Deduplicator::new(60_000);
        assert_eq!(dedup.admit("   ...   ", 1_000), Admission::SuppressedExact);
        assert_eq!(dedup.suppressed(), 1);
    }

    #[test]
    fn ratio_is_symmetric_and_bounded() {
        let a = tokenize(&normalize("один два три"));
        let b = tokenize(&normalize("три два один"));
        assert!((token_set_ratio(&a, &b) - 1.0).abs() < f32::EPSILON);
        assert_eq!(token_set_ratio(&a, &[]), 0.0);
    }
}
