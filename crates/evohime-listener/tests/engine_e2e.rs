//! Прогон настоящего движка распознавания.
//!
//! Тест по умолчанию не выполняется: CI обязан быть зелёным без DLL и без
//! модели, а они весят сотни мегабайт и не лежат в репозитории. Включается он
//! так же, как `EVOHIME_UPDATE_E2E` у обновления, — переменной окружения, но
//! на стороне Rust: ранний выход вместо `#[ignore]`, чтобы включение не
//! требовало ещё и отдельного флага `--ignored`.
//!
//! ```text
//! $env:EVOHIME_LISTENER_ENGINE_E2E='1'
//! $env:EVOHIME_LISTENER_TOOLS_DIR='C:\...\tools\listener'
//! $env:EVOHIME_LISTENER_ENGINE_FIXTURE='C:\...\utterance-16k-mono.wav'
//! cargo test -p evohime-listener --test engine_e2e
//! ```
//!
//! Без переменных остаётся детерминированный путь на `FixtureEngine`: он
//! проверяет тот же контракт движка, не требуя ни файлов, ни микрофона.

use evohime_listener::{EngineError, EngineUnavailable, FixtureEngine, SpeechEngine};

const ENABLE: &str = "EVOHIME_LISTENER_ENGINE_E2E";
const FIXTURE: &str = "EVOHIME_LISTENER_ENGINE_FIXTURE";

#[test]
fn fixture_engine_satisfies_the_contract_without_any_runtime() {
    let mut engine = FixtureEngine::new(["одна фраза".to_owned()]);
    let recognition = engine.recognize(&[0.1; 16_000]).expect("fixture answers");
    assert_eq!(recognition.text, "одна фраза");
    assert_eq!(recognition.language, "und");
    assert_eq!(engine.version(), "fixture-v1");
    assert_eq!(
        engine.recognize(&[0.1; 16_000]),
        Err(EngineError::FixtureExhausted)
    );
}

#[cfg(windows)]
#[test]
fn real_engine_transcribes_a_fixture_when_enabled() {
    use evohime_listener::engine::whisper_dll::WhisperDllEngine;
    use evohime_listener::tools_dir;

    if std::env::var(ENABLE).ok().as_deref() != Some("1") {
        return;
    }
    let runtime = tools_dir::resolve(&tools_dir::ProcessEnv)
        .expect("EVOHIME_LISTENER_TOOLS_DIR must point at a verified runtime");
    let mut engine = WhisperDllEngine::open(&runtime).expect("whisper runtime loads");
    assert!(engine.version().starts_with(&runtime.manifest.version));

    let fixture = std::env::var_os(FIXTURE)
        .unwrap_or_else(|| panic!("{FIXTURE} must point at a 16 kHz mono WAV"));
    let samples = read_wav_mono_16k(std::path::Path::new(&fixture));
    let recognition = engine.recognize(&samples).expect("engine transcribes");
    assert!(
        !recognition.text.trim().is_empty(),
        "engine returned an empty transcript"
    );
    assert!(recognition.duration_ms > 0);
}

/// Отсутствие рантайма — это код, а не паника и не пустая строка.
#[cfg(windows)]
#[test]
fn a_missing_runtime_reports_a_code() {
    use evohime_listener::tools_dir;
    let missing = std::env::temp_dir().join("evohime-listener-e2e-missing");
    let _ = std::fs::remove_dir_all(&missing);
    assert_eq!(
        tools_dir::resolve_in(&[missing]).err(),
        Some(EngineUnavailable::ToolsDirMissing)
    );
}

/// Минимальный разбор PCM-WAV: тесту нужен именно 16 кГц моно, любой другой
/// формат — ошибка фикстуры, а не повод молча ресемплить.
#[cfg(windows)]
fn read_wav_mono_16k(path: &std::path::Path) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("fixture is readable");
    assert!(bytes.len() > 44, "fixture is not a WAV file");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
    let sample_rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
    assert_eq!(channels, 1, "fixture must be mono");
    assert_eq!(sample_rate, 16_000, "fixture must be 16 kHz");
    assert_eq!(bits, 16, "fixture must be 16-bit PCM");
    bytes[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_le_bytes(*pair) as f32 / i16::MAX as f32)
        .collect()
}
