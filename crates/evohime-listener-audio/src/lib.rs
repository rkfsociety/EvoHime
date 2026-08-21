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

/// Формат открытого входа. Листенеру он нужен целиком: частоту надо знать,
/// чтобы децимировать, а число каналов — чтобы свести в моно до VAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Открывает shared-mode вход cpal. Callback получает PCM в памяти и не имеет
/// доступа к файловой системе; конкретное устройство можно сменить повторным
/// вызовом после `DeviceConflict`.
///
/// Возвращается ещё и формат: без него вызывающий не может ни свести каналы,
/// ни децимировать, а угадывание частоты — это тихий брак распознавания.
///
/// Callback не передаётся готовым, а собирается из формата: иначе он замыкал
/// бы копию предполагаемого формата, и настоящая частота устройства до него
/// уже не дошла бы. Такая ошибка не падает, а тихо портит звук.
#[cfg(windows)]
pub fn open_default_capture<F, B>(build: B) -> Result<(cpal::Stream, CaptureFormat), AudioError>
where
    B: FnOnce(CaptureFormat) -> F,
    F: FnMut(&[f32]) + Send + 'static,
{
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| AudioError::DeviceUnavailable("default input device is missing".into()))?;
    open_device_capture(device, build)
}

/// Общая часть открытия потока: формат берётся у самого устройства, а не
/// угадывается вызывающим.
#[cfg(windows)]
fn open_device_capture<F, B>(
    device: cpal::Device,
    build: B,
) -> Result<(cpal::Stream, CaptureFormat), AudioError>
where
    B: FnOnce(CaptureFormat) -> F,
    F: FnMut(&[f32]) + Send + 'static,
{
    use cpal::traits::{DeviceTrait, StreamTrait};
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::DeviceUnavailable(e.to_string()))?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err(AudioError::DeviceUnavailable(
            "capture format is not f32".into(),
        ));
    }
    let config: cpal::StreamConfig = supported.into();
    let format = CaptureFormat {
        sample_rate: config.sample_rate.0,
        channels: config.channels,
    };
    let mut callback = build(format);
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
    Ok((stream, format))
}

/// Одно устройство захвата в том виде, в каком его показывает панель «Слух».
///
/// `id` — это не отображаемое имя: cpal не выдаёт стабильных системных
/// идентификаторов, поэтому им служит нормализованное имя устройства. Оно
/// проходит через bounded-контракт идентификаторов 04.1, поэтому фраза
/// в поле `id` невозможна по построению.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDevice {
    pub id: String,
    pub display_name: String,
    pub is_default: bool,
}

/// Максимальная длина отображаемого имени устройства.
///
/// Имя приходит из системы и не обязано быть коротким; в UI оно занимает
/// одну строку списка.
pub const MAX_DEVICE_NAME_CHARS: usize = 96;

/// Идентификатор устройства из его имени: charset совпадает с bounded-id
/// контракта 04.1 (`[A-Za-z0-9-_.:+]`), всё остальное схлопывается в `-`.
///
/// Пустой результат означает, что имя целиком состояло из запрещённых
/// символов; такое устройство получает синтетический идентификатор по
/// индексу, а не пустую строку, которую нельзя выбрать.
pub fn device_id_from_name(name: &str, index: usize) -> String {
    let mut id = String::new();
    let mut previous_separator = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '+') {
            id.push(ch);
            previous_separator = false;
        } else if !previous_separator && !id.is_empty() {
            id.push('-');
            previous_separator = true;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    id.truncate(64);
    while id.ends_with('-') {
        id.pop();
    }
    if id.is_empty() {
        format!("input-{index}")
    } else {
        id
    }
}

/// Обрезает отображаемое имя устройства по границе символа.
pub fn bound_device_name(name: &str) -> String {
    if name.chars().count() <= MAX_DEVICE_NAME_CHARS {
        return name.to_owned();
    }
    name.chars().take(MAX_DEVICE_NAME_CHARS).collect()
}

/// Перечисляет входы, доступные системе.
///
/// Список нужен ровно затем, чтобы пользователь мог выбрать микрофон: у
/// renderer доступа к `navigator.mediaDevices` нет по построению
/// (`security.ts` отказывает во всех разрешениях), поэтому единственный
/// источник списка — этот процесс.
#[cfg(windows)]
pub fn list_capture_devices() -> Result<Vec<CaptureDevice>, AudioError> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let devices = host
        .input_devices()
        .map_err(|error| AudioError::DeviceUnavailable(error.to_string()))?;
    let mut listed = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, device) in devices.enumerate() {
        let Ok(name) = device.name() else { continue };
        let id = device_id_from_name(&name, index);
        if !seen.insert(id.clone()) {
            continue;
        }
        listed.push(CaptureDevice {
            is_default: default_name.as_deref() == Some(name.as_str()),
            display_name: bound_device_name(&name),
            id,
        });
    }
    Ok(listed)
}

#[cfg(not(windows))]
pub fn list_capture_devices() -> Result<Vec<CaptureDevice>, AudioError> {
    Ok(Vec::new())
}

/// Открывает вход выбранного устройства; пустой `device_id` означает
/// «устройство по умолчанию».
///
/// Смена устройства не требует перезапуска процесса: вызывающий закрывает
/// прежний поток и открывает новый этой же функцией.
#[cfg(windows)]
pub fn open_capture<F, B>(
    device_id: &str,
    build: B,
) -> Result<(cpal::Stream, CaptureFormat), AudioError>
where
    B: FnOnce(CaptureFormat) -> F,
    F: FnMut(&[f32]) + Send + 'static,
{
    use cpal::traits::{DeviceTrait, HostTrait};
    if device_id.is_empty() {
        return open_default_capture(build);
    }
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|error| AudioError::DeviceUnavailable(error.to_string()))?;
    let device = devices
        .enumerate()
        .find(|(index, device)| {
            device
                .name()
                .map(|name| device_id_from_name(&name, *index) == device_id)
                .unwrap_or(false)
        })
        .map(|(_, device)| device)
        .ok_or_else(|| {
            // Выбранного устройства больше нет. Открывать вместо него
            // умолчание нельзя: пользователь выбирал конкретный микрофон.
            AudioError::DeviceUnavailable("selected input device is missing".into())
        })?;
    open_device_capture(device, build)
}

#[cfg(not(windows))]
pub fn open_capture<F, B>(_device_id: &str, _build: B) -> Result<(), AudioError>
where
    B: FnOnce(CaptureFormat) -> F,
    F: FnMut(&[f32]) + Send + 'static,
{
    Err(AudioError::DeviceUnavailable(
        "capture is a Windows-only path".into(),
    ))
}

/// Сводит чередующиеся каналы в моно усреднением.
///
/// Усреднение, а не «взять первый канал»: на стереогарнитуре речь часто
/// заметно тише в одном из каналов, и выбор канала наугад срезал бы половину
/// громкости ещё до VAD.
pub fn downmix_to_mono(input: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return input.to_vec();
    }
    input
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Фиксированный дециматор для распространённых голосовых частот. Для 48 и
/// 32 кГц отношение целое, поэтому результат не зависит от платформы; 16 кГц
/// проходит без изменений. Частота вроде 44,1 кГц осознанно остаётся
/// ошибкой: дробное отношение потребовало бы интерполяции, а тихо испорченный
/// звук хуже честного отказа.
pub fn resample_to_16khz(input: &[f32], sample_rate: u32) -> Result<Vec<f32>, AudioError> {
    let factor = match sample_rate {
        48_000 => 3,
        32_000 => 2,
        16_000 => 1,
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

/// Наблюдатель за появлением и исчезновением устройств захвата.
///
/// Список микрофонов нельзя обновлять опросом: пользователь выдёргивает
/// гарнитуру и ждёт, что панель это заметит, а не через несколько секунд.
/// Windows сообщает об этом сообщением `WM_DEVICECHANGE`, поэтому процесс
/// держит message-only окно и подписку `RegisterDeviceNotificationW` на класс
/// интерфейсов захвата.
///
/// Окно создаётся с `HWND_MESSAGE`: оно не видимо, не попадает в Alt+Tab и
/// существует только ради этой подписки.
#[cfg(windows)]
pub struct DeviceWatcher {
    hwnd: isize,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
const DEVICE_WATCHER_CLASS: &[u16] = &[
    69, 118, 111, 72, 105, 109, 101, 68, 101, 118, 105, 99, 101, 87, 97, 116, 99, 104, 0,
];

#[cfg(windows)]
unsafe extern "system" fn device_watcher_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DestroyWindow, GetWindowLongPtrW, PostQuitMessage, GWLP_USERDATA, WM_CLOSE,
        WM_DESTROY, WM_DEVICECHANGE,
    };
    unsafe {
        match message {
            WM_DEVICECHANGE => {
                let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if pointer != 0 {
                    // Вызывается только уведомление: перечисление устройств
                    // из оконной процедуры заблокировало бы очередь сообщений.
                    let callback = &*(pointer as *const Box<dyn Fn() + Send>);
                    callback();
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

#[cfg(windows)]
impl DeviceWatcher {
    /// Поднимает подписку. `on_change` вызывается из потока окна и обязан
    /// быть коротким: тяжёлую работу вызывающий переносит в свой поток.
    ///
    /// Отказ подписки — это не «устройства не менялись», а «мы этого не
    /// узнаем»: вызывающий получает `Err` и обязан сказать об этом
    /// пользователю, а не молча считать список вечным.
    pub fn start<F>(on_change: F) -> Result<Self, AudioError>
    where
        F: Fn() + Send + 'static,
    {
        use windows_sys::Win32::Media::KernelStreaming::KSCATEGORY_CAPTURE;
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, RegisterClassW,
            RegisterDeviceNotificationW, SetWindowLongPtrW, TranslateMessage,
            UnregisterDeviceNotification, DBT_DEVTYP_DEVICEINTERFACE, DEVICE_NOTIFY_WINDOW_HANDLE,
            DEV_BROADCAST_DEVICEINTERFACE_W, GWLP_USERDATA, HWND_MESSAGE, MSG, WNDCLASSW,
        };

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<isize, String>>();
        let thread = std::thread::spawn(move || {
            let boxed: Box<Box<dyn Fn() + Send>> = Box::new(Box::new(on_change));
            let callback = Box::into_raw(boxed);
            let hwnd = unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let mut class: WNDCLASSW = std::mem::zeroed();
                class.lpfnWndProc = Some(device_watcher_proc);
                class.hInstance = instance;
                class.lpszClassName = DEVICE_WATCHER_CLASS.as_ptr();
                // Повторная регистрация того же класса возвращает 0 и
                // ERROR_CLASS_ALREADY_EXISTS: это не отказ, окно всё равно
                // создастся.
                RegisterClassW(&class);
                CreateWindowExW(
                    0,
                    DEVICE_WATCHER_CLASS.as_ptr(),
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                )
            };
            if hwnd.is_null() {
                unsafe { drop(Box::from_raw(callback)) };
                let _ = ready_tx.send(Err("message-only window was not created".into()));
                return;
            }
            // Указатель ставится до подписки: раньше неё сообщение прийти не
            // может, поэтому гонки между записью и чтением нет.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, callback as isize) };

            let notification = unsafe {
                let mut filter: DEV_BROADCAST_DEVICEINTERFACE_W = std::mem::zeroed();
                filter.dbcc_size = std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32;
                filter.dbcc_devicetype = DBT_DEVTYP_DEVICEINTERFACE;
                filter.dbcc_classguid = KSCATEGORY_CAPTURE;
                RegisterDeviceNotificationW(
                    hwnd.cast(),
                    std::ptr::addr_of!(filter).cast(),
                    DEVICE_NOTIFY_WINDOW_HANDLE,
                )
            };
            if notification.is_null() {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    DestroyWindow(hwnd);
                    drop(Box::from_raw(callback));
                }
                let _ = ready_tx.send(Err("device notification was refused".into()));
                return;
            }
            let _ = ready_tx.send(Ok(hwnd as isize));

            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            unsafe {
                UnregisterDeviceNotification(notification);
                drop(Box::from_raw(callback));
            }
        });

        match ready_rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self {
                hwnd,
                thread: Some(thread),
            }),
            Ok(Err(error)) => Err(AudioError::DeviceUnavailable(error)),
            Err(_) => Err(AudioError::DeviceUnavailable(
                "device watcher thread stopped before it subscribed".into(),
            )),
        }
    }
}

#[cfg(windows)]
impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        unsafe {
            PostMessageW(self.hwnd as _, WM_CLOSE, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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
    fn native_16khz_passes_through_untouched() {
        assert_eq!(
            resample_to_16khz(&[0., 1., 2.], 16_000).unwrap(),
            vec![0., 1., 2.]
        );
        assert_eq!(
            resample_to_16khz(&[0., 1.], 44_100),
            Err(AudioError::UnsupportedRate(44_100))
        );
    }
    /// Идентификатор устройства выводится из имени, но остаётся bounded-токеном
    /// контракта 04.1: через него нельзя протащить фразу.
    #[test]
    fn a_device_id_is_an_opaque_token_not_a_phrase() {
        assert_eq!(
            device_id_from_name("Микрофон (Realtek High Definition Audio)", 0),
            "Realtek-High-Definition-Audio"
        );
        assert_eq!(device_id_from_name("Headset Mic 2", 1), "Headset-Mic-2");
        // Имя целиком из запрещённых символов не превращается в пустую строку:
        // такое устройство всё равно должно быть выбираемым.
        assert_eq!(device_id_from_name("«»", 3), "input-3");
        for id in [
            device_id_from_name("Микрофон (Realtek)", 0),
            device_id_from_name("Headset Mic 2", 1),
            device_id_from_name("«»", 3),
        ] {
            assert!(!id.is_empty());
            assert!(id.len() <= 64);
            assert!(id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '+')));
        }
    }

    /// Отображаемое имя ограничено по длине и режется по границе символа.
    #[test]
    fn a_device_name_is_bounded_at_a_character_boundary() {
        let long = "микрофон ".repeat(40);
        let bounded = bound_device_name(&long);
        assert_eq!(bounded.chars().count(), MAX_DEVICE_NAME_CHARS);
        assert!(long.starts_with(&bounded));
        assert_eq!(bound_device_name("короткое"), "короткое");
    }

    #[test]
    fn downmix_averages_channels() {
        assert_eq!(downmix_to_mono(&[1., 3., 5., 7.], 2), vec![2., 6.]);
        assert_eq!(downmix_to_mono(&[1., 2., 3.], 1), vec![1., 2., 3.]);
        // Хвост неполного кадра отбрасывается, а не смешивается с тишиной.
        assert_eq!(downmix_to_mono(&[1., 3., 9.], 2), vec![2.]);
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
