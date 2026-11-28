//! Типы управления постоянным слушанием: намерение пользователя, команды
//! процессу листенера и снимок состояния для UI.
//!
//! Здесь нет ни SQL, ни файлов, ни каналов — только сериализуемые контракты.
//! Файлы политики и намерения пишет Core, каналы к листенеру держит Core.

use crate::{
    policy::AmbientPolicy,
    state::{ListeningReason, ListeningState},
};
use serde::{Deserialize, Serialize};

/// Что пользователь выбрал в панели «Слух»: включено ли слушание вообще и
/// каким устройством.
///
/// Живёт отдельно от [`AmbientPolicy`]: политика — это правила (тишина,
/// чёрные списки, срок хранения), а это — переключатель и выбор микрофона.
/// Смешать их значило бы завести в side-effect-free контракте политики поле,
/// которого там быть не должно.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmbientControl {
    /// Слушание выключено по умолчанию. Значение по умолчанию здесь — это не
    /// стиль, а требование: неизвестное намерение не должно открывать
    /// микрофон.
    #[serde(default)]
    pub enabled: bool,
    /// Выбранное устройство; пустая строка — «устройство системы».
    #[serde(default)]
    pub device_id: String,
}

/// Одно устройство захвата в снимке состояния.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AmbientDeviceInfo {
    /// Stable identifier reported by the audio backend.
    pub device_id: String,
    /// User-facing device name.
    pub display_name: String,
    /// Whether the backend identifies this as its default device.
    pub is_default: bool,
    /// Whether this device is currently selected for capture.
    pub is_active: bool,
}

/// Снимок того, что панель «Слух», трей и хоткей показывают пользователю.
///
/// Единственный источник истины живёт в Core: три точки входа отправляют одну
/// и ту же команду и ждут события, а не рисуют себе состояние сами.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AmbientStatusSnapshot {
    /// Current listening state reported by Core and the listener.
    pub state: ListeningState,
    /// Reason associated with the current listening state.
    pub reason: ListeningReason,
    /// Selected device identifier, or an empty string for the system default.
    pub active_device_id: String,
    /// Version of the active recognition engine, if known.
    pub engine_version: String,
    /// Whether the listener confirms that the engine is ready.
    pub engine_ready: bool,
    /// Devices known to the listener at snapshot time.
    pub devices: Vec<AmbientDeviceInfo>,
    /// Живёт ли подписка на смену устройств. `false` означает, что список —
    /// снимок, который сам не обновится, и панель обязана это сказать.
    pub watching_devices: bool,
}

impl Default for AmbientStatusSnapshot {
    /// Стартовое состояние — «неизвестно», а не «выключено».
    ///
    /// Пока процесс листенера не подключился, Core не знает, читается ли
    /// микрофон, и обязан сказать именно это: `EngineUnavailable` попадает в
    /// `ListeningState::is_degraded`, и индикатор показывает «проверка
    /// состояния» с предупреждением, а не спокойное «выключено».
    fn default() -> Self {
        Self {
            state: ListeningState::EngineUnavailable,
            reason: ListeningReason::EngineUnavailable,
            active_device_id: String::new(),
            engine_version: String::new(),
            engine_ready: false,
            devices: Vec::new(),
            watching_devices: false,
        }
    }
}

/// Команда процессу листенера. Отправляется только из Core: у трея, хоткея и
/// панели своего канала к листенеру нет.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerControl {
    /// Enable or disable continuous listening.
    Enabled(bool),
    /// Pause or resume capture temporarily.
    Paused(bool),
    /// Select a capture device by backend identifier.
    SelectDevice(String),
    /// Clear buffered audio without changing listening policy.
    ResetBuffers,
    /// Полная политика целиком: тихие часы и чёрные списки меняются вместе, а
    /// не по одному полю, поэтому listener получает новый снимок, а не патч.
    Policy(Box<(AmbientPolicy, AmbientControl)>),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Стартовое состояние — «неизвестно»: пока листенер не подключился,
    /// панель не имеет права показывать спокойное «выключено».
    #[test]
    fn the_default_snapshot_is_unknown_not_disabled() {
        let snapshot = AmbientStatusSnapshot::default();
        assert_eq!(snapshot.state, ListeningState::EngineUnavailable);
        assert_eq!(snapshot.reason, ListeningReason::EngineUnavailable);
        assert!(!snapshot.engine_ready);
        assert!(snapshot.devices.is_empty());
    }

    /// Слушание выключено по умолчанию: неизвестное намерение не открывает
    /// микрофон.
    #[test]
    fn the_default_control_keeps_the_microphone_closed() {
        let control = AmbientControl::default();
        assert!(!control.enabled);
        assert!(control.device_id.is_empty());
    }
}
