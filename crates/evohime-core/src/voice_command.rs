//! Разбор голосовой команды, адресованной Еве, и очередь услышанного.
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
//!
//! Запуск при этом не следует из разбора: услышанная команда становится
//! карточкой, и открывает приложение клик, если пользователь явно не разрешил
//! автозапуск в ambient-политике.

use evohime_listener_contract::VoiceCommandKind;
use evohime_tool_runtime::app_catalog;

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

/// Сколько слов допускается между обращением и глаголом.
const MAX_BRIDGE_WORDS: usize = 2;

/// Потолок длины цели: название приложения — это одно-три слова, а не остаток
/// разговора.
const MAX_TARGET_WORDS: usize = 4;
const MAX_TARGET_CHARS: usize = 64;

/// Что услышано.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceIntent {
    pub kind: VoiceCommandKind,
    /// Название приложения так, как его произнесли. Каталог разбирается с ним
    /// сам — здесь оно остаётся текстом.
    pub target: String,
}

/// Разбирает высказывание. `None` — обычная фраза, а не команда.
pub fn parse(text: &str) -> Option<VoiceIntent> {
    let normalized = app_catalog::normalize(text);
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
    let target = app_catalog::strip_filler_words(&words[index + 1..].join(" "));
    if target.is_empty()
        || target.split(' ').count() > MAX_TARGET_WORDS
        || target.chars().count() > MAX_TARGET_CHARS
    {
        return None;
    }
    Some(VoiceIntent {
        kind: VoiceCommandKind::OpenApp,
        target,
    })
}

/// Сколько живёт непринятая карточка команды.
///
/// Пять минут, а не сутки как у предложения: «открой хром» устаревает вместе с
/// намерением. Карточка, на которую не ответили за это время, — уже не то, что
/// человек имел в виду.
pub const COMMAND_TTL_MS: u64 = 5 * 60 * 1000;

/// Сколько карточек висит одновременно. Больше — это не очередь, а лавина:
/// самая старая уходит.
pub const MAX_PENDING: usize = 8;

/// Как часто перечитывается каталог приложений. Реестр читается не на каждое
/// слово: между установкой приложения и его появлением в каталоге проходит до
/// этого времени, зато распознавание не платит за реестр на каждой фразе.
pub const CATALOG_TTL_MS: u64 = 5 * 60 * 1000;

/// Услышанная команда, ждущая решения.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCommand {
    pub command_id: String,
    pub kind: VoiceCommandKind,
    pub app_id: String,
    pub title: String,
    pub created_at_ms: u64,
}

impl PendingCommand {
    pub const fn expires_at_ms(&self) -> u64 {
        self.created_at_ms.saturating_add(COMMAND_TTL_MS)
    }

    fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms()
    }
}

/// Что делать с услышанным.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Это не команда, либо каталог не знает такого приложения, либо название
    /// подходит нескольким сразу. Во всех трёх случаях не происходит ничего:
    /// догадка вслух хуже молчания.
    Ignore,
    /// Показать карточку и ждать клика.
    Confirm(PendingCommand),
    /// Пользователь заранее разрешил автозапуск — открыть сразу.
    Autorun(PendingCommand),
}

/// Очередь услышанных команд и кэш каталога.
///
/// Очередь живёт в памяти и намеренно не переживает перезапуск: карточка
/// «открыть хром» после падения Core — это ответ на разговор, которого больше
/// нет.
#[derive(Debug)]
pub struct VoiceCommandRegistry {
    pending: std::sync::Mutex<std::collections::VecDeque<PendingCommand>>,
    catalog: std::sync::Mutex<Option<(u64, std::sync::Arc<app_catalog::AppCatalog>)>>,
    /// Каталог, заданный вместо обнаружения. Существует ради тестов: они не
    /// имеют права зависеть от того, что установлено на машине.
    fixed_catalog: Option<std::sync::Arc<app_catalog::AppCatalog>>,
}

impl Default for VoiceCommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceCommandRegistry {
    pub fn new() -> Self {
        Self {
            pending: std::sync::Mutex::new(std::collections::VecDeque::new()),
            catalog: std::sync::Mutex::new(None),
            fixed_catalog: None,
        }
    }

    pub fn with_catalog(catalog: app_catalog::AppCatalog) -> Self {
        Self {
            fixed_catalog: Some(std::sync::Arc::new(catalog)),
            ..Self::new()
        }
    }

    /// Каталог приложений, не старше [`CATALOG_TTL_MS`].
    pub fn catalog(&self, now_ms: u64) -> std::sync::Arc<app_catalog::AppCatalog> {
        if let Some(catalog) = &self.fixed_catalog {
            return catalog.clone();
        }
        let mut slot = lock(&self.catalog);
        if let Some((built_at, catalog)) = slot.as_ref() {
            if now_ms.saturating_sub(*built_at) < CATALOG_TTL_MS && now_ms >= *built_at {
                return catalog.clone();
            }
        }
        let catalog = std::sync::Arc::new(app_catalog::default_catalog());
        *slot = Some((now_ms, catalog.clone()));
        catalog
    }

    /// Карточки, ждущие решения. Истёкшие не возвращаются и снимаются с
    /// очереди тем же вызовом.
    pub fn list(&self, now_ms: u64) -> Vec<PendingCommand> {
        let mut pending = lock(&self.pending);
        pending.retain(|command| !command.is_expired(now_ms));
        pending.iter().cloned().collect()
    }

    /// Снимает карточку с очереди. `None` — истекла, уже решена или её не было.
    pub fn take(&self, command_id: &str, now_ms: u64) -> Option<PendingCommand> {
        let mut pending = lock(&self.pending);
        pending.retain(|command| !command.is_expired(now_ms));
        let index = pending
            .iter()
            .position(|command| command.command_id == command_id)?;
        pending.remove(index)
    }

    /// Убирает истёкшие карточки и возвращает их: событие «истекла» обязано
    /// дойти до панели, иначе карточка останется висеть в интерфейсе.
    pub fn expire(&self, now_ms: u64) -> Vec<PendingCommand> {
        let mut pending = lock(&self.pending);
        let expired: Vec<PendingCommand> = pending
            .iter()
            .filter(|command| command.is_expired(now_ms))
            .cloned()
            .collect();
        pending.retain(|command| !command.is_expired(now_ms));
        expired
    }

    fn record(&self, command: PendingCommand) {
        let mut pending = lock(&self.pending);
        pending.retain(|item| !item.is_expired(command.created_at_ms));
        // Повтор той же команды не плодит вторую карточку: человек повторил
        // фразу, потому что не увидел первую, а не потому что хочет два окна.
        pending.retain(|item| item.app_id != command.app_id);
        pending.push_back(command);
        while pending.len() > MAX_PENDING {
            pending.pop_front();
        }
    }
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Единственный вход услышанного в открытие приложений.
///
/// Политика проверяется здесь, а не у вызывающего: «слушание включено» и
/// «команды разрешены» — разные разрешения, и запуск обязан требовать оба.
pub fn decide(
    registry: &VoiceCommandRegistry,
    policy: &evohime_listener_contract::AmbientPolicy,
    text: &str,
    now_ms: u64,
    command_id: String,
) -> Decision {
    if !policy.voice_commands || policy.paused {
        return Decision::Ignore;
    }
    let Some(intent) = parse(text) else {
        return Decision::Ignore;
    };
    let catalog = registry.catalog(now_ms);
    let app_catalog::Resolution::Found(entry) = catalog.resolve(&intent.target) else {
        // Ни «не нашла», ни «нашла три» не превращаются в запуск: выбор за
        // пользователя здесь сделать не из чего.
        return Decision::Ignore;
    };
    let command = PendingCommand {
        command_id,
        kind: intent.kind,
        app_id: entry.id.clone(),
        title: entry.title.clone(),
        created_at_ms: now_ms,
    };
    if policy.voice_commands_autorun {
        Decision::Autorun(command)
    } else {
        registry.record(command.clone());
        Decision::Confirm(command)
    }
}

/// Открывает приложение по решённой карточке.
pub fn launch(
    registry: &VoiceCommandRegistry,
    command: &PendingCommand,
    now_ms: u64,
) -> Result<u32, String> {
    let catalog = registry.catalog(now_ms);
    let Some(entry) = catalog.get(&command.app_id) else {
        return Err("каталог больше не знает это приложение".to_owned());
    };
    app_catalog::launch(entry).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(target: &str) -> Option<VoiceIntent> {
        Some(VoiceIntent {
            kind: VoiceCommandKind::OpenApp,
            target: target.to_owned(),
        })
    }

    fn catalog() -> app_catalog::AppCatalog {
        app_catalog::AppCatalog::from_entries(vec![app_catalog::AppEntry {
            id: "chrome".to_owned(),
            title: "Google Chrome".to_owned(),
            aliases: vec!["хром".to_owned()],
            exec: std::path::PathBuf::from("C:\\apps\\chrome.exe"),
            args: Vec::new(),
        }])
    }

    fn policy() -> evohime_listener_contract::AmbientPolicy {
        evohime_listener_contract::AmbientPolicy::default()
    }

    #[test]
    fn a_heard_command_waits_for_a_click_by_default() {
        let registry = VoiceCommandRegistry::with_catalog(catalog());
        let decision = decide(&registry, &policy(), "Ева, открой хром", 1_000, "c1".into());
        let Decision::Confirm(command) = decision else {
            panic!("по умолчанию услышанное обязано спрашивать, а не запускать");
        };
        assert_eq!(command.app_id, "chrome");
        assert_eq!(command.title, "Google Chrome");
        assert_eq!(registry.list(1_000).len(), 1);
    }

    #[test]
    fn autorun_is_only_reachable_through_the_policy() {
        let registry = VoiceCommandRegistry::with_catalog(catalog());
        let policy = evohime_listener_contract::AmbientPolicy {
            voice_commands_autorun: true,
            ..policy()
        };
        let decision = decide(&registry, &policy, "Ева, открой хром", 1_000, "c1".into());
        assert!(matches!(decision, Decision::Autorun(_)));
        // Автозапуск не оставляет карточки: решать уже нечего.
        assert!(registry.list(1_000).is_empty());
    }

    #[test]
    fn disabled_voice_commands_and_a_pause_stop_everything() {
        let registry = VoiceCommandRegistry::with_catalog(catalog());
        for policy in [
            evohime_listener_contract::AmbientPolicy {
                voice_commands: false,
                ..policy()
            },
            evohime_listener_contract::AmbientPolicy {
                paused: true,
                ..policy()
            },
        ] {
            assert_eq!(
                decide(&registry, &policy, "Ева, открой хром", 1_000, "c1".into()),
                Decision::Ignore
            );
        }
        assert!(registry.list(1_000).is_empty());
    }

    #[test]
    fn an_unknown_application_produces_nothing_at_all() {
        let registry = VoiceCommandRegistry::with_catalog(catalog());
        assert_eq!(
            decide(
                &registry,
                &policy(),
                "Ева, открой фотошоп",
                1_000,
                "c1".into()
            ),
            Decision::Ignore
        );
        assert!(registry.list(1_000).is_empty());
    }

    #[test]
    fn an_ambiguous_name_is_never_guessed() {
        let ambiguous = app_catalog::AppCatalog::from_entries(vec![
            app_catalog::AppEntry {
                id: "code".to_owned(),
                title: "Visual Studio Code".to_owned(),
                aliases: vec!["код".to_owned()],
                exec: std::path::PathBuf::from("C:\\apps\\code.exe"),
                args: Vec::new(),
            },
            app_catalog::AppEntry {
                id: "codium".to_owned(),
                title: "VSCodium".to_owned(),
                aliases: vec!["код".to_owned()],
                exec: std::path::PathBuf::from("C:\\apps\\codium.exe"),
                args: Vec::new(),
            },
        ]);
        let registry = VoiceCommandRegistry::with_catalog(ambiguous);
        assert_eq!(
            decide(&registry, &policy(), "Ева, открой код", 1_000, "c1".into()),
            Decision::Ignore
        );
    }

    #[test]
    fn a_repeated_command_replaces_the_card_instead_of_stacking() {
        let registry = VoiceCommandRegistry::with_catalog(catalog());
        decide(&registry, &policy(), "Ева, открой хром", 1_000, "c1".into());
        decide(&registry, &policy(), "Ева, открой хром", 2_000, "c2".into());
        let pending = registry.list(2_000);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].command_id, "c2");
    }

    #[test]
    fn a_card_expires_and_says_so_once() {
        let registry = VoiceCommandRegistry::with_catalog(catalog());
        decide(&registry, &policy(), "Ева, открой хром", 1_000, "c1".into());
        let expired = registry.expire(1_000 + COMMAND_TTL_MS);
        assert_eq!(expired.len(), 1);
        assert!(registry.expire(1_000 + COMMAND_TTL_MS).is_empty());
        assert!(registry.list(1_000 + COMMAND_TTL_MS).is_empty());
    }

    #[test]
    fn a_decision_removes_the_card_and_a_second_click_finds_nothing() {
        let registry = VoiceCommandRegistry::with_catalog(catalog());
        decide(&registry, &policy(), "Ева, открой хром", 1_000, "c1".into());
        assert!(registry.take("c1", 1_500).is_some());
        assert!(registry.take("c1", 1_500).is_none());
    }

    #[test]
    fn an_expired_card_can_no_longer_be_accepted() {
        let registry = VoiceCommandRegistry::with_catalog(catalog());
        decide(&registry, &policy(), "Ева, открой хром", 1_000, "c1".into());
        assert!(registry.take("c1", 1_000 + COMMAND_TTL_MS).is_none());
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
        // Рядом с микрофоном разговаривают люди: «открой окно» в их разговоре
        // не должно ничего запускать.
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
