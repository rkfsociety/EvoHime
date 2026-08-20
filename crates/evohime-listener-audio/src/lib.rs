//! Детерминированный аудио-контур листенера.
//!
//! Этот крейт намеренно не содержит файлового API. PCM живёт только в памяти;
//! Windows VirtualLock используется как best-effort защита страниц от pagefile.

use evohime_listener_contract::AmbientLimits;
use std::collections::VecDeque;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AudioError {
    #[error("unsupported sample rate {0}")]
    UnsupportedRate(u32),
    #[error("capture device is unavailable: {0}")]
    DeviceUnavailable(String),
}

/// Кольцевой буфер с bounded memory и явным занулением при сбросе.
#[derive(Debug)]
pub struct RingBuffer {
    samples: VecDeque<f32>,
    capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, input: &[f32]) {
        for &sample in input {
            if self.samples.len() == self.capacity {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }

    pub fn snapshot(&self) -> Vec<f32> {
        self.samples.iter().copied().collect()
    }

    pub fn clear(&mut self) {
        for sample in &mut self.samples {
            *sample = 0.0;
        }
        self.samples.clear();
    }
}

#[cfg(windows)]
pub fn lock_memory(samples: &mut [f32]) -> bool {
    unsafe {
        windows_sys::Win32::System::Memory::VirtualLock(
            samples.as_mut_ptr().cast(),
            std::mem::size_of_val(samples),
        ) != 0
    }
}

#[cfg(not(windows))]
pub fn lock_memory(_samples: &mut [f32]) -> bool {
    false
}

/// Открывает shared-mode вход cpal. Callback получает PCM в памяти и не имеет
/// доступа к файловой системе; конкретное устройство можно сменить повторным
/// вызовом после `DeviceConflict`.
#[cfg(windows)]
pub fn open_default_capture<F>(mut callback: F) -> Result<cpal::Stream, AudioError>
where
    F: FnMut(&[f32]) + Send + 'static,
{
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| AudioError::DeviceUnavailable("default input device is missing".into()))?;
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::DeviceUnavailable(e.to_string()))?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err(AudioError::DeviceUnavailable(
            "capture format is not f32".into(),
        ));
    }
    let config: cpal::StreamConfig = supported.into();
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _| callback(data),
            move |error| {
                let _ = error;
            },
            None,
        )
        .map_err(|e| AudioError::DeviceUnavailable(e.to_string()))?;
    stream
        .play()
        .map_err(|e| AudioError::DeviceUnavailable(e.to_string()))?;
    Ok(stream)
}

/// Фиксированный дециматор для распространённых голосовых частот. Для 48 и
/// 32 кГц отношение целое, поэтому результат не зависит от платформы.
pub fn resample_to_16khz(input: &[f32], sample_rate: u32) -> Result<Vec<f32>, AudioError> {
    let factor = match sample_rate {
        48_000 => 3,
        32_000 => 2,
        _ => return Err(AudioError::UnsupportedRate(sample_rate)),
    };
    Ok(input.iter().step_by(factor).copied().collect())
}

#[derive(Debug, Clone, Copy)]
pub struct VadDecision {
    pub voiced: bool,
    pub rms: f32,
    pub zero_crossings: u32,
}

#[derive(Debug, Clone)]
pub struct EnergyVad {
    noise_floor: f32,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self {
            noise_floor: 0.0001,
        }
    }
}

impl EnergyVad {
    pub fn decide(&mut self, frame: &[f32]) -> VadDecision {
        if frame.is_empty() {
            return VadDecision {
                voiced: false,
                rms: 0.0,
                zero_crossings: 0,
            };
        }
        let energy = frame.iter().map(|v| v * v).sum::<f32>() / frame.len() as f32;
        let rms = energy.sqrt();
        let crossings = frame
            .windows(2)
            .filter(|pair| (pair[0] >= 0.0) != (pair[1] >= 0.0))
            .count() as u32;
        let voiced = rms > self.noise_floor * 3.0 && crossings < frame.len() as u32 / 2;
        if !voiced {
            self.noise_floor = (self.noise_floor * 0.95 + energy * 0.05).max(1e-8);
        }
        VadDecision {
            voiced,
            rms,
            zero_crossings: crossings,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub episode_id: String,
    pub samples: Vec<f32>,
    pub continued: bool,
    pub start_frame: u64,
}

/// Автомат сегментации: три voiced-кадра на входе, pre-roll, hangover и
/// разбиение длинной речи на последовательные сегменты.
#[derive(Debug)]
pub struct Segmenter {
    limits: AmbientLimits,
    ring: RingBuffer,
    episode: u64,
    active: Option<ActiveSegment>,
    voiced_streak: u8,
    next_frame: u64,
    episode_elapsed_ms: u32,
}

#[derive(Debug)]
struct ActiveSegment {
    id: String,
    samples: Vec<f32>,
    silent_frames: u32,
    start_frame: u64,
    continued: bool,
}

impl Segmenter {
    pub fn new(limits: AmbientLimits, sample_rate: usize) -> Self {
        let preroll = sample_rate * limits.pre_roll_ms as usize / 1000;
        Self {
            limits,
            ring: RingBuffer::new(preroll.max(1)),
            episode: 0,
            active: None,
            voiced_streak: 0,
            next_frame: 0,
            episode_elapsed_ms: 0,
        }
    }

    pub fn reset(&mut self) {
        self.ring.clear();
        self.active = None;
        self.voiced_streak = 0;
        self.episode_elapsed_ms = 0;
    }

    pub fn push_frame(&mut self, frame: &[f32], decision: VadDecision) -> Vec<Segment> {
        let mut completed = Vec::new();
        self.ring.push(frame);
        let frame_ms = self.limits.frame_ms;
        if self.active.is_none() {
            self.voiced_streak = if decision.voiced {
                self.voiced_streak.saturating_add(1)
            } else {
                0
            };
            if self.voiced_streak >= 3 {
                self.episode = self.episode.saturating_add(1);
                let id = format!("episode-{}", self.episode);
                let mut samples = self.ring.snapshot();
                samples.extend_from_slice(frame);
                self.active = Some(ActiveSegment {
                    id,
                    samples,
                    silent_frames: 0,
                    start_frame: self.next_frame.saturating_sub(2),
                    continued: false,
                });
                self.ring.clear();
                self.voiced_streak = 0;
            }
        } else if let Some(active) = &mut self.active {
            active.samples.extend_from_slice(frame);
            active.silent_frames = if decision.voiced {
                0
            } else {
                active.silent_frames + 1
            };
            let duration = active.samples.len() as u32 * frame_ms / frame.len().max(1) as u32;
            let end = active.silent_frames * frame_ms >= self.limits.hangover_ms
                || duration >= self.limits.max_utterance_ms;
            if end {
                let finished = self.active.take().unwrap();
                let episode_id = finished.id.clone();
                if self
                    .limits
                    .accepts_utterance(duration.min(self.limits.max_utterance_ms))
                {
                    completed.push(Segment {
                        episode_id: episode_id.clone(),
                        samples: finished.samples,
                        continued: finished.continued,
                        start_frame: finished.start_frame,
                    });
                }
                if duration >= self.limits.max_utterance_ms && decision.voiced {
                    self.episode_elapsed_ms = self.episode_elapsed_ms.saturating_add(duration);
                    let continued = self.episode_elapsed_ms < self.limits.max_episode_ms;
                    if !continued {
                        self.episode = self.episode.saturating_add(1);
                        self.episode_elapsed_ms = 0;
                    }
                    self.active = Some(ActiveSegment {
                        id: if continued {
                            episode_id
                        } else {
                            format!("episode-{}", self.episode)
                        },
                        samples: frame.to_vec(),
                        silent_frames: 0,
                        start_frame: self.next_frame,
                        continued,
                    });
                } else {
                    self.episode_elapsed_ms = 0;
                }
            }
        }
        self.next_frame = self.next_frame.saturating_add(1);
        completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_decimators_are_stable() {
        assert_eq!(
            resample_to_16khz(&[0., 1., 2., 3., 4., 5.], 48_000).unwrap(),
            vec![0., 3.]
        );
        assert_eq!(
            resample_to_16khz(&[0., 1., 2., 3.], 32_000).unwrap(),
            vec![0., 2.]
        );
    }
    #[test]
    fn ring_is_bounded_and_zeroed() {
        let mut ring = RingBuffer::new(2);
        ring.push(&[1., 2., 3.]);
        assert_eq!(ring.snapshot(), vec![2., 3.]);
        ring.clear();
        assert!(ring.snapshot().is_empty());
    }
    #[test]
    fn three_voiced_frames_start_segment() {
        let mut s = Segmenter::new(AmbientLimits::DEFAULT, 16_000);
        let mut v = EnergyVad::default();
        let frame = vec![0.5; 480];
        for _ in 0..3 {
            assert!(s.push_frame(&frame, v.decide(&frame)).is_empty());
        }
        assert!(s.active.is_some());
    }
}
