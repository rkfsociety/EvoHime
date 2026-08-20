//! Процесс листенера. Состояние и сырые PCM остаются в этом процессе; Core
//! получает только bounded распознанные высказывания и typed state events.

use evohime_listener_audio::{EnergyVad, Segmenter};
use evohime_listener_contract::{AmbientLimits, AmbientPolicy, ListeningState};
use std::path::PathBuf;
use tokio::sync::watch;

pub trait SpeechEngine: Send {
    fn recognize(&mut self, samples: &[f32]) -> Result<String, String>;
    fn version(&self) -> &'static str;
}

#[derive(Default)]
pub struct NullEngine;
impl SpeechEngine for NullEngine {
    fn recognize(&mut self, _samples: &[f32]) -> Result<String, String> {
        Err("engine unavailable".into())
    }
    fn version(&self) -> &'static str {
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
    fn recognize(&mut self, _samples: &[f32]) -> Result<String, String> {
        self.outputs
            .pop_front()
            .ok_or_else(|| "fixture exhausted".into())
    }
    fn version(&self) -> &'static str {
        "fixture-v1"
    }
}

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

pub struct ListenerRuntime<E: SpeechEngine> {
    pub policy: AmbientPolicy,
    pub state: ListeningState,
    engine: E,
    segmenter: Segmenter,
    vad: EnergyVad,
    state_tx: watch::Sender<ListeningState>,
}
impl<E: SpeechEngine> ListenerRuntime<E> {
    pub fn new(policy: AmbientPolicy, engine: E, state_tx: watch::Sender<ListeningState>) -> Self {
        Self {
            policy,
            state: ListeningState::Stopped,
            engine,
            segmenter: Segmenter::new(AmbientLimits::DEFAULT, 16_000),
            vad: EnergyVad::default(),
            state_tx,
        }
    }
    pub fn set_state(&mut self, next: ListeningState) -> Result<(), String> {
        self.state = self.state.transition(next).map_err(|e| e.to_string())?;
        let _ = self.state_tx.send(self.state);
        Ok(())
    }
    pub fn reset_buffers(&mut self) {
        self.segmenter.reset();
    }
    pub fn process_frame(&mut self, frame: &[f32]) -> Vec<String> {
        if !self.state.is_capturing()
            || self.policy.paused
            || blocked(&self.policy, &foreground_window())
        {
            return Vec::new();
        }
        let decision = self.vad.decide(frame);
        let texts: Vec<String> = self
            .segmenter
            .push_frame(frame, decision)
            .into_iter()
            .filter_map(|segment| self.engine.recognize(&segment.samples).ok())
            .filter(|text| !text.trim().is_empty())
            .collect();
        if texts.iter().any(|text| contains_stop_word(text)) {
            self.segmenter.reset();
            let _ = self.set_state(ListeningState::PausedByUser);
            Vec::new()
        } else {
            texts
        }
    }
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
    fn fixture_is_deterministic() {
        let (tx, _) = watch::channel(ListeningState::Stopped);
        let mut r = ListenerRuntime::new(
            AmbientPolicy::default(),
            FixtureEngine::new(["hello".into()]),
            tx,
        );
        r.set_state(ListeningState::Starting).unwrap();
        r.set_state(ListeningState::Listening).unwrap();
        let frame = vec![0.5; 480];
        for _ in 0..3 {
            r.process_frame(&frame);
        }
    }
}
