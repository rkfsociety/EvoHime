//! Движки распознавания и их общий контракт.
//!
//! Движок отдаёт одно распознанное высказывание целиком; никакого потока
//! промежуточных гипотез наружу нет. Отсутствие движка — явное состояние с
//! кодом, а не тишина: `EngineUnavailable` доходит до Core и до UI.

mod dedup;
mod ladder;
#[cfg(windows)]
pub mod whisper_dll;

pub use dedup::{Admission, Deduplicator, DEDUP_NEAR_THRESHOLD, DEDUP_RECENT_DEPTH};
pub use ladder::{LadderAction, RtfLadder, RTF_BREACH_STREAK, RTF_THRESHOLD};

use serde::Deserialize;

/// Ступень лестницы моделей. Порядок — от тяжёлой к лёгкой.
///
/// `Ord` идёт по объявлению, поэтому `Small < Base < Tiny`: сравнение
/// означает «тяжелее», а не «лучше», и на это опирается выбор стартовой
/// ступени.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRung {
    Small,
    Base,
    Tiny,
}

impl ModelRung {
    /// Лестница целиком, от тяжёлой ступени к лёгкой.
    pub const LADDER: [ModelRung; 3] = [ModelRung::Small, ModelRung::Base, ModelRung::Tiny];

    pub const fn as_str(self) -> &'static str {
        match self {
            ModelRung::Small => "small",
            ModelRung::Base => "base",
            ModelRung::Tiny => "tiny",
        }
    }

    /// Следующая, более лёгкая ступень. `None` у `Tiny`: ниже деградация.
    pub const fn next_lower(self) -> Option<ModelRung> {
        match self {
            ModelRung::Small => Some(ModelRung::Base),
            ModelRung::Base => Some(ModelRung::Tiny),
            ModelRung::Tiny => None,
        }
    }
}

/// Почему движок недоступен. Закрытый набор: renderer показывает известный код
/// и никогда не выдаёт отказ за успех.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EngineUnavailable {
    ToolsDirMissing,
    ManifestMissing,
    ManifestInvalid,
    ManifestPathEscapes,
    FileMissing,
    SizeMismatch,
    HashMismatch,
    /// В каталоге рантайма лежит DLL, которой нет в манифесте. Загрузчик
    /// Windows подхватил бы её как зависимость мимо проверки хеша.
    UnexpectedFile,
    SignatureMissing,
    SignatureUntrusted,
    AbiUnsupported,
    LoadFailed,
    ModelLoadFailed,
}

impl EngineUnavailable {
    pub const ALL: [EngineUnavailable; 13] = [
        EngineUnavailable::ToolsDirMissing,
        EngineUnavailable::ManifestMissing,
        EngineUnavailable::ManifestInvalid,
        EngineUnavailable::ManifestPathEscapes,
        EngineUnavailable::FileMissing,
        EngineUnavailable::SizeMismatch,
        EngineUnavailable::HashMismatch,
        EngineUnavailable::UnexpectedFile,
        EngineUnavailable::SignatureMissing,
        EngineUnavailable::SignatureUntrusted,
        EngineUnavailable::AbiUnsupported,
        EngineUnavailable::LoadFailed,
        EngineUnavailable::ModelLoadFailed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            EngineUnavailable::ToolsDirMissing => "tools_dir_missing",
            EngineUnavailable::ManifestMissing => "manifest_missing",
            EngineUnavailable::ManifestInvalid => "manifest_invalid",
            EngineUnavailable::ManifestPathEscapes => "manifest_path_escapes",
            EngineUnavailable::FileMissing => "file_missing",
            EngineUnavailable::SizeMismatch => "size_mismatch",
            EngineUnavailable::HashMismatch => "hash_mismatch",
            EngineUnavailable::UnexpectedFile => "unexpected_file",
            EngineUnavailable::SignatureMissing => "signature_missing",
            EngineUnavailable::SignatureUntrusted => "signature_untrusted",
            EngineUnavailable::AbiUnsupported => "abi_unsupported",
            EngineUnavailable::LoadFailed => "load_failed",
            EngineUnavailable::ModelLoadFailed => "model_load_failed",
        }
    }
}

impl std::fmt::Display for EngineUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Ошибка распознавания одного сегмента.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineError {
    /// Движка нет: причина уже известна и постоянна для сессии.
    Unavailable(EngineUnavailable),
    /// Движок есть, но конкретный вызов не удался. Сессия продолжается.
    RecognizeFailed,
    /// Фикстура кончилась — только тестовый путь.
    FixtureExhausted,
}

impl EngineError {
    /// Стабильный код для журналов и UI.
    pub fn as_str(&self) -> &'static str {
        match self {
            EngineError::Unavailable(code) => code.as_str(),
            EngineError::RecognizeFailed => "recognize_failed",
            EngineError::FixtureExhausted => "fixture_exhausted",
        }
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Одно распознанное высказывание.
///
/// Язык и длительность считает движок: угадывать их в Core значило бы писать
/// в хранилище заведомо неверные `language = "und"` и `duration_ms = 0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Recognition {
    pub text: String,
    /// Код языка от движка либо `und`, когда он его не сообщил.
    pub language: String,
    /// Длительность распознанной речи по временным меткам сегментов.
    pub duration_ms: u32,
}

impl Recognition {
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            text: value.into(),
            language: "und".into(),
            duration_ms: 0,
        }
    }
}

pub trait SpeechEngine: Send {
    fn recognize(&mut self, samples: &[f32]) -> Result<Recognition, EngineError>;
    /// Opaque-токен версии: он уходит в `ambient.engine` и в метаданные
    /// эпизода, поэтому свободного текста в нём быть не может.
    fn version(&self) -> &str;
    /// Текущая ступень лестницы, если движок вообще ей управляет.
    fn rung(&self) -> Option<ModelRung> {
        None
    }
    /// Переключение на более лёгкую ступень. `false` — движок так не умеет
    /// либо нужной модели нет; вызывающий тогда деградирует, а не молчит.
    fn switch_rung(&mut self, _rung: ModelRung) -> bool {
        false
    }
}

/// Движка нет. Каждое распознавание — честный отказ с сохранённой причиной.
#[derive(Debug)]
pub struct NullEngine {
    reason: EngineUnavailable,
}

impl NullEngine {
    pub fn new(reason: EngineUnavailable) -> Self {
        Self { reason }
    }

    pub fn reason(&self) -> EngineUnavailable {
        self.reason
    }
}

impl Default for NullEngine {
    fn default() -> Self {
        Self::new(EngineUnavailable::ToolsDirMissing)
    }
}

impl SpeechEngine for NullEngine {
    fn recognize(&mut self, _samples: &[f32]) -> Result<Recognition, EngineError> {
        Err(EngineError::Unavailable(self.reason))
    }

    fn version(&self) -> &str {
        "null"
    }
}

/// Фикстурный движок для детерминированных тестов: каждый непустой сегмент
/// отдаёт следующую строку, не читая WAV и не создавая временные файлы.
pub struct FixtureEngine {
    outputs: std::collections::VecDeque<String>,
}

impl FixtureEngine {
    pub fn new(outputs: impl IntoIterator<Item = String>) -> Self {
        Self {
            outputs: outputs.into_iter().collect(),
        }
    }
}

impl SpeechEngine for FixtureEngine {
    fn recognize(&mut self, _samples: &[f32]) -> Result<Recognition, EngineError> {
        self.outputs
            .pop_front()
            .map(Recognition::text)
            .ok_or(EngineError::FixtureExhausted)
    }

    fn version(&self) -> &str {
        "fixture-v1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_only_goes_down() {
        assert_eq!(ModelRung::Small.next_lower(), Some(ModelRung::Base));
        assert_eq!(ModelRung::Base.next_lower(), Some(ModelRung::Tiny));
        assert_eq!(ModelRung::Tiny.next_lower(), None);
        assert!(ModelRung::Small < ModelRung::Tiny);
    }

    #[test]
    fn unavailable_codes_are_unique_and_stable() {
        let mut seen: Vec<&str> = EngineUnavailable::ALL.iter().map(|c| c.as_str()).collect();
        let total = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total);
    }

    #[test]
    fn null_engine_keeps_its_reason() {
        let mut engine = NullEngine::new(EngineUnavailable::HashMismatch);
        assert_eq!(
            engine.recognize(&[0.0]),
            Err(EngineError::Unavailable(EngineUnavailable::HashMismatch))
        );
        assert_eq!(engine.version(), "null");
    }

    #[test]
    fn rung_names_match_the_manifest_spelling() {
        for (rung, name) in [
            (ModelRung::Small, "\"small\""),
            (ModelRung::Base, "\"base\""),
            (ModelRung::Tiny, "\"tiny\""),
        ] {
            let parsed: ModelRung = serde_json::from_str(name).unwrap();
            assert_eq!(parsed, rung);
            assert_eq!(format!("\"{}\"", rung.as_str()), name);
        }
    }
}
