//! Процесс листенера. Состояние и сырые PCM остаются в этом процессе; Core
//! получает только bounded распознанные высказывания и typed state events.

pub mod authenticode;
pub mod engine;
pub mod tools_dir;

pub use engine::{
    Admission, Deduplicator, EngineError, EngineUnavailable, FixtureEngine, LadderAction,
    ModelRung, NullEngine, Recognition, RtfLadder, SpeechEngine,
};

use evohime_listener_audio::{EnergyVad, Segmenter};
use evohime_listener_contract::{AmbientLimits, AmbientPolicy, ListeningReason, ListeningState};
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::watch;

#[derive(Clone, Debug)]
pub struct ForegroundWindow {
    pub process_name: String,
    pub title: String,
}

#[cfg(windows)]
pub fn foreground_window() -> ForegroundWindow {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let mut title = [0u16; 512];
        let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        let mut path = [0u16; 1024];
        let mut size = path.len() as u32;
        let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !process_handle.is_null() {
            let _ = QueryFullProcessImageNameW(process_handle, 0, path.as_mut_ptr(), &mut size);
            let _ = CloseHandle(process_handle);
        }
        let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
        let process = String::from_utf16_lossy(&path[..size as usize])
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_string();
        ForegroundWindow {
            process_name: process,
            title,
        }
    }
}

#[cfg(not(windows))]
pub fn foreground_window() -> ForegroundWindow {
    ForegroundWindow {
        process_name: String::new(),
        title: String::new(),
    }
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    fn inner(p: &[u8], v: &[u8]) -> bool {
        match p.split_first() {
            None => v.is_empty(),
            Some((b'*', rest)) => inner(rest, v) || (!v.is_empty() && inner(p, &v[1..])),
            Some((b'?', rest)) => !v.is_empty() && inner(rest, &v[1..]),
            Some((head, rest)) => v.first() == Some(head) && inner(rest, &v[1..]),
        }
    }
    inner(pattern.as_bytes(), value.as_bytes())
}

pub fn blocked(policy: &AmbientPolicy, window: &ForegroundWindow) -> bool {
    policy
        .process_blocklist
        .iter()
        .any(|p| glob_matches(p, &window.process_name))
        || policy
            .window_title_blocklist
            .iter()
            .any(|p| glob_matches(p, &window.title))
}

pub fn contains_stop_word(text: &str) -> bool {
    text.split(|ch: char| !ch.is_alphanumeric()).any(|word| {
        matches!(
            word.to_lowercase().as_str(),
            "стоп" | "остановись" | "evohime"
        )
    })
}

/// Одно принятое высказывание в том виде, в каком оно уходит в Core.
#[derive(Clone, Debug, PartialEq)]
pub struct Utterance {
    pub episode_id: String,
    pub sequence: u32,
    pub text: String,
    pub language: String,
    pub duration_ms: u32,
    pub started_at_ms: u64,
    pub continued: bool,
}

/// Изменение состояния движка, которое обязано дойти до пользователя.
///
/// Копится здесь, а не логируется на месте: у процесса листенера нет своего
/// канала в UI, публикацией занимается владелец соединения.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineNotice {
    /// Лестница шагнула вниз: распознавание не укладывалось в бюджет.
    RungChanged(ModelRung),
    /// Лестница исчерпана — слушание остановлено политикой.
    Degraded,
    /// Движок недоступен: причина постоянна для всей сессии.
    Unavailable(EngineUnavailable),
}

pub struct ListenerRuntime {
    pub policy: AmbientPolicy,
    pub state: ListeningState,
    /// Включено ли слушание вообще. Отличается от `policy.paused`: выключение
    /// — это `Stopped`, пауза — `PausedByUser`, и пользователь видит разные
    /// строки. Процесс поднимается выключенным: микрофон не открывается,
    /// пока Core не попросит явно.
    pub enabled: bool,
    /// Выбранное устройство захвата; пустая строка — «устройство системы по
    /// умолчанию».
    pub device_id: String,
    /// Причина последнего объявленного состояния.
    ///
    /// Хранится рядом с состоянием, потому что при новом соединении Core
    /// обязан узнать не только «что сейчас», но и «почему»: сам переход к
    /// этому моменту уже произошёл и второй раз не случится.
    pub last_reason: ListeningReason,
    engine: Box<dyn SpeechEngine>,
    segmenter: Segmenter,
    vad: EnergyVad,
    dedup: Deduplicator,
    ladder: RtfLadder,
    notices: Vec<EngineNotice>,
    sequence: u32,
    last_episode: String,
    state_tx: watch::Sender<ListeningState>,
}

impl ListenerRuntime {
    pub fn new(
        policy: AmbientPolicy,
        engine: Box<dyn SpeechEngine>,
        state_tx: watch::Sender<ListeningState>,
    ) -> Self {
        let limits = AmbientLimits::DEFAULT;
        let rung = engine.rung().unwrap_or(ModelRung::Small);
        Self {
            policy,
            state: ListeningState::Stopped,
            enabled: false,
            device_id: String::new(),
            last_reason: ListeningReason::UserRequest,
            engine,
            segmenter: Segmenter::new(limits, 16_000),
            vad: EnergyVad::default(),
            dedup: Deduplicator::new(limits.dedup_window_ms),
            ladder: RtfLadder::new(rung),
            notices: Vec::new(),
            sequence: 0,
            last_episode: String::new(),
            state_tx,
        }
    }

    pub fn engine_version(&self) -> &str {
        self.engine.version()
    }

    /// Сколько повторов подавлено за сессию.
    pub fn suppressed(&self) -> u64 {
        self.dedup.suppressed()
    }

    /// Забирает накопленные уведомления о движке.
    pub fn take_notices(&mut self) -> Vec<EngineNotice> {
        std::mem::take(&mut self.notices)
    }

    pub fn set_state(&mut self, next: ListeningState) -> Result<(), String> {
        self.state = self.state.transition(next).map_err(|e| e.to_string())?;
        let _ = self.state_tx.send(self.state);
        Ok(())
    }

    /// Состояние, которого требует текущая конфигурация.
    ///
    /// Функция чистая: часы и здоровье движка передаёт вызывающий. Порядок
    /// проверок и есть приоритет причин — сначала то, что делает захват
    /// невозможным, потом то, что его запрещает.
    pub fn desired_state(
        &self,
        minute_of_day: u32,
        engine_ready: bool,
    ) -> (ListeningState, ListeningReason) {
        if !engine_ready {
            return (
                ListeningState::EngineUnavailable,
                ListeningReason::EngineUnavailable,
            );
        }
        if !self.enabled {
            return (ListeningState::Stopped, ListeningReason::UserRequest);
        }
        if self.policy.paused {
            return (ListeningState::PausedByUser, ListeningReason::UserRequest);
        }
        if self.policy.is_quiet_at(minute_of_day) {
            return (ListeningState::PausedByPolicy, ListeningReason::QuietHours);
        }
        (ListeningState::Listening, ListeningReason::UserRequest)
    }

    pub fn reset_buffers(&mut self) {
        self.segmenter.reset();
        self.dedup.reset();
    }

    /// Обрабатывает один кадр 16 кГц моно.
    ///
    /// Часы передаёт вызывающий: рантайм остаётся детерминированным, а тест
    /// не зависит от системного времени.
    pub fn process_frame(&mut self, frame: &[f32], now_ms: u64) -> Vec<Utterance> {
        if !self.state.is_capturing()
            || self.policy.paused
            || blocked(&self.policy, &foreground_window())
        {
            return Vec::new();
        }
        let decision = self.vad.decide(frame);
        let segments = self.segmenter.push_frame(frame, decision);
        let mut accepted = Vec::new();
        let mut stop_word = false;
        for segment in segments {
            let speech_ms = samples_to_ms(segment.samples.len());
            let started = Instant::now();
            let recognized = self.engine.recognize(&segment.samples);
            let elapsed_ms = started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
            let recognition = match recognized {
                Ok(recognition) => recognition,
                Err(EngineError::Unavailable(reason)) => {
                    self.notices.push(EngineNotice::Unavailable(reason));
                    continue;
                }
                Err(_) => continue,
            };
            if recognition.text.trim().is_empty() {
                continue;
            }
            if contains_stop_word(&recognition.text) {
                // Стоп-слово не записывается: пользователь просил замолчать, а
                // не сохранить эту фразу.
                stop_word = true;
                break;
            }
            self.observe_budget(speech_ms, elapsed_ms);
            if !self.dedup.admit(&recognition.text, now_ms).accepted() {
                continue;
            }
            if segment.episode_id != self.last_episode {
                self.last_episode = segment.episode_id.clone();
                self.sequence = 0;
            }
            let sequence = self.sequence;
            self.sequence = self.sequence.saturating_add(1);
            let duration_ms = if recognition.duration_ms == 0 {
                speech_ms
            } else {
                recognition.duration_ms
            };
            accepted.push(Utterance {
                episode_id: segment.episode_id,
                sequence,
                text: recognition.text,
                language: recognition.language,
                duration_ms,
                started_at_ms: now_ms,
                continued: segment.continued,
            });
        }
        if stop_word {
            self.segmenter.reset();
            self.dedup.reset();
            let _ = self.set_state(ListeningState::PausedByUser);
            return Vec::new();
        }
        accepted
    }

    /// Учитывает стоимость распознавания и, если надо, спускается по лестнице.
    fn observe_budget(&mut self, speech_ms: u32, elapsed_ms: u32) {
        match self.ladder.observe(speech_ms, elapsed_ms) {
            LadderAction::Keep => {}
            LadderAction::Downgrade(rung) => {
                if self.engine.switch_rung(rung) {
                    self.notices.push(EngineNotice::RungChanged(rung));
                } else {
                    // Нужной ступени в поставке нет: притворяться, что модель
                    // сменилась, нельзя — это сразу деградация.
                    self.degrade();
                }
            }
            LadderAction::Degrade => self.degrade(),
        }
    }

    fn degrade(&mut self) {
        self.notices.push(EngineNotice::Degraded);
        let _ = self.set_state(ListeningState::PausedByPolicy);
    }
}

/// Длительность моно-буфера 16 кГц в миллисекундах.
fn samples_to_ms(samples: usize) -> u32 {
    u32::try_from(samples * 1000 / 16_000).unwrap_or(u32::MAX)
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("EvoHime")))
        .unwrap_or_else(|| PathBuf::from(".evohime"))
}

/// Suppresses the legacy Windows error dialog and automatic WER UI for this
/// capture process. This does not claim to control pagefile contents.
#[cfg(windows)]
pub fn harden_process() {
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode(
            windows_sys::Win32::System::Diagnostics::Debug::SEM_NOGPFAULTERRORBOX
                | windows_sys::Win32::System::Diagnostics::Debug::SEM_NOOPENFILEERRORBOX,
        );
    }
}

#[cfg(not(windows))]
pub fn harden_process() {}

pub fn backoff(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_millis(250u64.saturating_mul(2u64.saturating_pow(attempt.min(6))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_listener_contract::QuietHours;

    fn listening(engine: Box<dyn SpeechEngine>) -> ListenerRuntime {
        let (tx, _rx) = watch::channel(ListeningState::Stopped);
        let mut runtime = ListenerRuntime::new(AmbientPolicy::default(), engine, tx);
        runtime.set_state(ListeningState::Starting).unwrap();
        runtime.set_state(ListeningState::Listening).unwrap();
        runtime
    }

    /// Три voiced-кадра открывают сегмент, дальше тишина его закрывает.
    fn speak(runtime: &mut ListenerRuntime, now_ms: u64) -> Vec<Utterance> {
        let voiced = vec![0.5; 480];
        let silent = vec![0.0; 480];
        let mut produced = Vec::new();
        for _ in 0..40 {
            produced.extend(runtime.process_frame(&voiced, now_ms));
        }
        for _ in 0..40 {
            produced.extend(runtime.process_frame(&silent, now_ms));
        }
        produced
    }

    #[test]
    fn glob_and_blocklist_are_bounded() {
        let p = AmbientPolicy {
            process_blocklist: vec!["zoom*.exe".into()],
            ..Default::default()
        };
        assert!(blocked(
            &p,
            &ForegroundWindow {
                process_name: "zoom.exe".into(),
                title: String::new()
            }
        ));
    }

    #[test]
    fn stop_word_is_exact_token_and_not_substring() {
        assert!(contains_stop_word("стоп, пожалуйста"));
        assert!(!contains_stop_word("стопка"));
    }

    #[test]
    fn recognized_text_reaches_the_caller() {
        let mut runtime = listening(Box::new(FixtureEngine::new([
            "первая мысль вслух".to_owned()
        ])));
        let produced = speak(&mut runtime, 1_000);
        assert_eq!(produced.len(), 1);
        assert_eq!(produced[0].text, "первая мысль вслух");
        assert_eq!(produced[0].sequence, 0);
        assert!(produced[0].duration_ms > 0);
    }

    /// Повтор не уходит в Core вовсе: подавление считается счётчиком.
    #[test]
    fn a_repeat_is_suppressed_and_counted() {
        let mut runtime = listening(Box::new(FixtureEngine::new([
            "позвони маме".to_owned(),
            "позвони маме".to_owned(),
        ])));
        assert_eq!(speak(&mut runtime, 1_000).len(), 1);
        assert!(speak(&mut runtime, 2_000).is_empty());
        assert_eq!(runtime.suppressed(), 1);
    }

    /// Отсутствие движка — честное состояние с кодом, а не тишина.
    #[test]
    fn a_missing_engine_reports_its_reason() {
        let mut runtime = listening(Box::new(NullEngine::new(EngineUnavailable::FileMissing)));
        assert!(speak(&mut runtime, 1_000).is_empty());
        assert_eq!(
            runtime.take_notices(),
            vec![EngineNotice::Unavailable(EngineUnavailable::FileMissing)]
        );
    }

    /// Стоп-слово не только останавливает слушание, но и не сохраняется.
    #[test]
    fn a_stop_word_pauses_without_writing_anything() {
        let mut runtime = listening(Box::new(FixtureEngine::new(["стоп".to_owned()])));
        assert!(speak(&mut runtime, 1_000).is_empty());
        assert_eq!(runtime.state, ListeningState::PausedByUser);
    }

    /// Лестница вниз без нужной модели — это деградация, а не тихое «оставим
    /// как было».
    #[test]
    fn a_downgrade_without_a_model_degrades() {
        let (tx, _rx) = watch::channel(ListeningState::Stopped);
        let mut runtime = ListenerRuntime::new(
            AmbientPolicy::default(),
            Box::new(FixtureEngine::new(Vec::new())),
            tx,
        );
        runtime.set_state(ListeningState::Starting).unwrap();
        runtime.set_state(ListeningState::Listening).unwrap();
        for _ in 0..engine::RTF_BREACH_STREAK {
            runtime.observe_budget(1_000, 900);
        }
        assert_eq!(runtime.take_notices(), vec![EngineNotice::Degraded]);
        assert_eq!(runtime.state, ListeningState::PausedByPolicy);
    }

    /// Причина остановки по бюджету существует в закрытом наборе контракта.
    #[test]
    fn engine_degraded_is_a_contract_reason() {
        assert_ne!(ListeningReason::EngineDegraded, ListeningReason::Unknown);
    }

    #[test]
    fn a_paused_policy_never_reaches_the_engine() {
        let mut runtime = listening(Box::new(FixtureEngine::new([
            "не должно прозвучать".to_owned()
        ])));
        runtime.policy.paused = true;
        assert!(speak(&mut runtime, 1_000).is_empty());
    }

    /// Выключение и пауза — разные состояния: пользователь видит «выключено»
    /// и «на паузе» разными строками, и слить их значило бы соврать про то,
    /// что именно он нажал.
    #[test]
    fn disabled_and_paused_are_different_states() {
        let (tx, _rx) = watch::channel(ListeningState::Stopped);
        let mut runtime = ListenerRuntime::new(
            AmbientPolicy::default(),
            Box::new(FixtureEngine::new(Vec::new())),
            tx,
        );
        assert_eq!(
            runtime.desired_state(12 * 60, true),
            (ListeningState::Stopped, ListeningReason::UserRequest)
        );
        runtime.enabled = true;
        runtime.policy.paused = true;
        assert_eq!(
            runtime.desired_state(12 * 60, true),
            (ListeningState::PausedByUser, ListeningReason::UserRequest)
        );
        runtime.policy.paused = false;
        assert_eq!(
            runtime.desired_state(12 * 60, true),
            (ListeningState::Listening, ListeningReason::UserRequest)
        );
    }

    /// Тихие часы закрывают поток сами, без команды снаружи.
    #[test]
    fn quiet_hours_close_the_stream_without_a_command() {
        let (tx, _rx) = watch::channel(ListeningState::Stopped);
        let mut runtime = ListenerRuntime::new(
            AmbientPolicy {
                quiet_hours: vec![QuietHours::new(23 * 60, 7 * 60).unwrap()],
                ..AmbientPolicy::default()
            },
            Box::new(FixtureEngine::new(Vec::new())),
            tx,
        );
        runtime.enabled = true;
        assert_eq!(
            runtime.desired_state(2 * 60, true),
            (ListeningState::PausedByPolicy, ListeningReason::QuietHours)
        );
        assert_eq!(
            runtime.desired_state(12 * 60, true),
            (ListeningState::Listening, ListeningReason::UserRequest)
        );
    }

    /// Без движка микрофон не открывается ни при каком намерении: это первая
    /// проверка, а не последняя.
    #[test]
    fn a_missing_engine_outranks_every_intent() {
        let (tx, _rx) = watch::channel(ListeningState::Stopped);
        let mut runtime = ListenerRuntime::new(
            AmbientPolicy::default(),
            Box::new(NullEngine::new(EngineUnavailable::FileMissing)),
            tx,
        );
        runtime.enabled = true;
        assert_eq!(
            runtime.desired_state(12 * 60, false),
            (
                ListeningState::EngineUnavailable,
                ListeningReason::EngineUnavailable
            )
        );
    }

    /// Пауза достижима и из выключенного состояния: иначе включение «сразу на
    /// паузе» показало бы «выключено».
    #[test]
    fn pause_is_reachable_from_a_stopped_listener() {
        assert!(ListeningState::Stopped.can_transition(ListeningState::PausedByUser));
        assert!(ListeningState::Starting.can_transition(ListeningState::PausedByUser));
        assert!(!ListeningState::PausedByUser.is_capturing());
    }

    #[test]
    fn samples_convert_to_milliseconds() {
        assert_eq!(samples_to_ms(16_000), 1_000);
        assert_eq!(samples_to_ms(0), 0);
    }
}
