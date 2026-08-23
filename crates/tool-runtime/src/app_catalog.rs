//! Каталог приложений, которые Ева умеет открывать, и их запуск.
//!
//! Каталог — это allow-list, а не «запусти что скажут». Открыть можно только
//! запись каталога, поэтому ни услышанная фраза, ни ответ модели не доходят до
//! `CreateProcess` в виде пути: они доходят в виде ключа, который каталог либо
//! знает, либо нет.
//!
//! Источников три, и порядок между ними — это порядок доверия:
//!
//! 1. встроенные системные приложения Windows: путь собирается из `%SystemRoot%`
//!    и проверяется на существование, поэтому «Блокнот» не появится в каталоге
//!    на машине, где его нет;
//! 2. `App Paths` реестра — канонический список установленного софта. Русские
//!    синонимы к нему добавляются по имени исполняемого файла: реестр знает
//!    `chrome.exe`, но не знает слова «хром»;
//! 3. пользовательский `app-catalog.json` в data dir — он последний и
//!    перекрывает найденное по `id`, потому что человек про свои приложения
//!    знает больше, чем реестр.
//!
//! Запуск идёт мимо оболочки: `CreateProcess` без `cmd`/`ShellExecute`, аргументы
//! отдельным списком. Строка команды не собирается конкатенацией, поэтому в ней
//! нечего кавычить и нечего экранировать.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Имя пользовательского файла каталога в data dir.
pub const CATALOG_FILE_NAME: &str = "app-catalog.json";

/// Потолок размера пользовательского файла: каталог — это список приложений,
/// а не хранилище.
const MAX_CATALOG_BYTES: u64 = 256 * 1024;

/// Потолок числа записей каталога целиком.
pub const MAX_ENTRIES: usize = 512;

/// Каталог по умолчанию: тот же data dir, что у остального локального
/// состояния. Отдельной настройки пути нет намеренно — иначе allow-list
/// приложений можно было бы подменить переменной окружения, не относящейся к
/// EvoHime.
pub fn default_catalog() -> AppCatalog {
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA").map(|value| PathBuf::from(value).join("EvoHime"))
        });
    AppCatalog::discover(data_dir.as_deref())
}

/// Одна запись каталога.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEntry {
    /// Ключ записи. Тот же charset, что у идентификаторов ambient-контракта:
    /// ASCII без пробелов, потому что этот ключ уходит в события.
    pub id: String,
    /// Человекочитаемое название для карточки.
    pub title: String,
    /// Как это приложение могут назвать вслух.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Полный путь к исполняемому файлу.
    pub exec: PathBuf,
    /// Постоянные аргументы запуска.
    #[serde(default)]
    pub args: Vec<String>,
}

impl AppEntry {
    /// Валиден ли `id` как ключ события ambient-контракта.
    pub fn id_is_wire_safe(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 128
            && id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '+'))
    }
}

/// Чем кончился поиск по каталогу.
///
/// `Ambiguous` существует отдельно от `NotFound` потому, что «я не знаю такого
/// приложения» и «я знаю сразу три» требуют разного ответа человеку: во втором
/// случае выбор есть, и подставлять за пользователя первый попавшийся вариант
/// нельзя.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Found(AppEntry),
    Ambiguous(Vec<String>),
    NotFound,
}

/// Каталог приложений.
#[derive(Clone, Debug, Default)]
pub struct AppCatalog {
    entries: Vec<AppEntry>,
}

impl AppCatalog {
    /// Каталог из готового списка. Используется тестами и пользовательским
    /// файлом; порядок сохраняется, дубликаты по `id` схлопываются в пользу
    /// последней записи.
    pub fn from_entries(entries: Vec<AppEntry>) -> Self {
        let mut catalog = Self {
            entries: Vec::new(),
        };
        for entry in entries {
            catalog.upsert(entry);
        }
        catalog
    }

    /// Полный каталог: системные приложения, реестр и пользовательский файл.
    pub fn discover(data_dir: Option<&Path>) -> Self {
        let mut catalog = Self::from_entries(builtin_entries());
        for entry in registry_entries() {
            catalog.add_discovered(entry);
        }
        if let Some(dir) = data_dir {
            for entry in user_entries(dir) {
                catalog.upsert(entry);
            }
        }
        catalog
            .entries
            .sort_by(|left, right| left.id.cmp(&right.id));
        catalog.entries.truncate(MAX_ENTRIES);
        catalog
    }

    /// Найденное автоматически дополняет каталог, но не переписывает
    /// выверенную запись: у реестра нет ни русского названия, ни знания о том,
    /// что `Notepad` по-русски «Блокнот». Совпадение по `id` поэтому отдаёт
    /// только синонимы, а путь и заголовок остаются прежними.
    fn add_discovered(&mut self, entry: AppEntry) {
        if !AppEntry::id_is_wire_safe(&entry.id) {
            return;
        }
        if let Some(existing) = self.entries.iter_mut().find(|item| item.id == entry.id) {
            for alias in entry.aliases {
                if !existing.aliases.contains(&alias) {
                    existing.aliases.push(alias);
                }
            }
            return;
        }
        self.upsert(entry);
    }

    fn upsert(&mut self, entry: AppEntry) {
        if !AppEntry::id_is_wire_safe(&entry.id) || entry.title.trim().is_empty() {
            return;
        }
        match self.entries.iter_mut().find(|item| item.id == entry.id) {
            // Совпадение по `id` — это не второе приложение, а более
            // достоверный источник о том же: у него побеждает путь, а синонимы
            // объединяются, иначе пользовательская запись молча стирала бы
            // русские названия встроенной таблицы.
            Some(existing) => {
                let mut aliases = entry.aliases.clone();
                for alias in &existing.aliases {
                    if !aliases.iter().any(|value| value == alias) {
                        aliases.push(alias.clone());
                    }
                }
                existing.title = entry.title;
                existing.exec = entry.exec;
                existing.args = entry.args;
                existing.aliases = aliases;
            }
            None => self.entries.push(entry),
        }
    }

    pub fn entries(&self) -> &[AppEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&AppEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Находит приложение по тому, как его назвали.
    pub fn resolve(&self, query: &str) -> Resolution {
        let query = strip_filler_words(&normalize(query));
        if query.is_empty() {
            return Resolution::NotFound;
        }
        let exact = self.matching(|entry| {
            normalize(&entry.id) == query
                || normalize(&entry.title) == query
                || entry.aliases.iter().any(|alias| normalize(alias) == query)
        });
        if let Some(resolution) = single(exact) {
            return resolution;
        }
        // Нестрогое совпадение: сказанное — начало названия или наоборот.
        // «вижуал студио» находит «Visual Studio Code», «телеграм дэск» —
        // «Telegram Desktop».
        let partial = self.matching(|entry| {
            let names = std::iter::once(entry.title.clone())
                .chain(entry.aliases.iter().cloned())
                .chain(std::iter::once(entry.id.clone()));
            names.map(|name| normalize(&name)).any(|name| {
                !name.is_empty() && (prefix_words(&name, &query) || prefix_words(&query, &name))
            })
        });
        if let Some(resolution) = single(partial) {
            return resolution;
        }
        // Категория: «браузер» — это не название приложения, а класс. Берётся
        // первый доступный по фиксированному приоритету, а не случайный.
        for (category, order) in CATEGORIES {
            if normalize(category) != query {
                continue;
            }
            for id in *order {
                if let Some(entry) = self.get(id) {
                    return Resolution::Found(entry.clone());
                }
            }
        }
        Resolution::NotFound
    }

    fn matching(&self, predicate: impl Fn(&AppEntry) -> bool) -> Vec<AppEntry> {
        self.entries
            .iter()
            .filter(|entry| predicate(entry))
            .cloned()
            .collect()
    }
}

fn single(mut matches: Vec<AppEntry>) -> Option<Resolution> {
    match matches.len() {
        0 => None,
        1 => Some(Resolution::Found(matches.remove(0))),
        _ => Some(Resolution::Ambiguous(
            matches.into_iter().map(|entry| entry.title).collect(),
        )),
    }
}

/// Начинается ли `value` со слов `prefix` — именно со слов, а не с байтов:
/// «фото» не должно находить «Фотошоп», а «гугл хром» — находить.
fn prefix_words(value: &str, prefix: &str) -> bool {
    value == prefix
        || (value.starts_with(prefix) && value.as_bytes().get(prefix.len()) == Some(&b' '))
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

/// Убирает служебные слова из уже нормализованного запроса.
pub fn strip_filler_words(normalized: &str) -> String {
    normalized
        .split(' ')
        .filter(|word| !word.is_empty() && !FILLER_WORDS.contains(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Категории: слово класса и приоритет приложений внутри него.
const CATEGORIES: &[(&str, &[&str])] = &[
    (
        "браузер",
        &["chrome", "msedge", "firefox", "yandex", "opera", "brave"],
    ),
    ("почта", &["outlook", "thunderbird"]),
    ("музыка", &["spotify", "yandexmusic"]),
    ("терминал", &["wt", "powershell", "cmd"]),
    ("редактор", &["code", "notepad"]),
    ("мессенджер", &["telegram", "discord", "slack"]),
];

/// Русские синонимы к известным исполняемым файлам.
///
/// Реестр знает `chrome.exe`, но не знает слова «хром», а произносят вслух
/// именно его. Таблица привязана к имени файла, а не к записи каталога,
/// поэтому синонимы достаются любому найденному источнику.
const KNOWN_ALIASES: &[(&str, &str, &[&str])] = &[
    (
        "chrome",
        "Google Chrome",
        &["хром", "гугл хром", "chrome", "гугл"],
    ),
    (
        "msedge",
        "Microsoft Edge",
        &["эдж", "едж", "edge", "майкрософт эдж"],
    ),
    (
        "firefox",
        "Mozilla Firefox",
        &["фаерфокс", "файрфокс", "firefox", "мозилла", "мозила"],
    ),
    (
        "browser",
        "Яндекс Браузер",
        &["яндекс браузер", "яндекс", "yandex"],
    ),
    ("opera", "Opera", &["опера", "opera"]),
    ("brave", "Brave", &["брейв", "brave"]),
    (
        "code",
        "Visual Studio Code",
        &[
            "вс код",
            "вскод",
            "вижуал студио код",
            "код",
            "code",
            "vs code",
        ],
    ),
    (
        "telegram",
        "Telegram",
        &["телеграм", "телега", "telegram", "тг"],
    ),
    ("discord", "Discord", &["дискорд", "discord"]),
    ("steam", "Steam", &["стим", "steam"]),
    ("spotify", "Spotify", &["спотифай", "spotify"]),
    ("obsidian", "Obsidian", &["обсидиан", "obsidian"]),
    ("slack", "Slack", &["слак", "слэк", "slack"]),
    ("winword", "Microsoft Word", &["ворд", "word"]),
    ("excel", "Microsoft Excel", &["эксель", "ексель", "excel"]),
    (
        "powerpnt",
        "Microsoft PowerPoint",
        &["повер поинт", "поверпоинт", "powerpoint"],
    ),
    ("outlook", "Microsoft Outlook", &["аутлук", "outlook"]),
    ("thunderbird", "Thunderbird", &["тандерберд", "thunderbird"]),
    ("vlc", "VLC", &["влс", "vlc", "плеер"]),
    (
        "photoshop",
        "Adobe Photoshop",
        &["фотошоп", "photoshop", "пс"],
    ),
    ("figma", "Figma", &["фигма", "figma"]),
    ("notepad", "Блокнот", &["блокнот", "notepad", "ноутпад"]),
    (
        "mspaint",
        "Paint",
        &["пейнт", "паинт", "paint", "рисовалка"],
    ),
    (
        "snippingtool",
        "Ножницы",
        &["ножницы", "скриншот", "snipping tool"],
    ),
    (
        "wt",
        "Windows Terminal",
        &["терминал", "виндовс терминал", "terminal"],
    ),
    ("calc", "Калькулятор", &["калькулятор", "calc"]),
    (
        "explorer",
        "Проводник",
        &["проводник", "файлы", "explorer", "эксплорер"],
    ),
    ("zoom", "Zoom", &["зум", "zoom"]),
    ("blender", "Blender", &["блендер", "blender"]),
];

/// Встроенные приложения Windows.
///
/// Путь проверяется на существование: запись каталога обязана быть чем-то,
/// что действительно можно запустить.
fn builtin_entries() -> Vec<AppEntry> {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_owned());
    let system_root = PathBuf::from(system_root);
    let system32 = system_root.join("System32");
    let mut entries = Vec::new();
    let table: &[(&str, &str, &[&str], PathBuf)] = &[
        (
            "notepad",
            "Блокнот",
            &["блокнот", "notepad", "ноутпад"],
            system32.join("notepad.exe"),
        ),
        (
            "calc",
            "Калькулятор",
            &["калькулятор", "calc", "калькулятор виндовс"],
            system32.join("calc.exe"),
        ),
        (
            "explorer",
            "Проводник",
            &["проводник", "explorer", "файлы", "эксплорер"],
            system_root.join("explorer.exe"),
        ),
        (
            "mspaint",
            "Paint",
            &["пейнт", "paint", "паинт"],
            system32.join("mspaint.exe"),
        ),
        (
            "snippingtool",
            "Ножницы",
            &["ножницы", "снипинг тул", "snipping tool"],
            system32.join("SnippingTool.exe"),
        ),
        (
            "taskmgr",
            "Диспетчер задач",
            &["диспетчер задач", "таск менеджер", "task manager"],
            system32.join("Taskmgr.exe"),
        ),
        (
            "control",
            "Панель управления",
            &["панель управления", "control panel"],
            system32.join("control.exe"),
        ),
        (
            "powershell",
            "PowerShell",
            &["повершелл", "пауэршелл", "powershell"],
            system32.join("WindowsPowerShell\\v1.0\\powershell.exe"),
        ),
        (
            "cmd",
            "Командная строка",
            &["командная строка", "консоль", "cmd"],
            system32.join("cmd.exe"),
        ),
    ];
    for (id, title, aliases, exec) in table {
        if exec.is_file() {
            entries.push(AppEntry {
                id: (*id).to_owned(),
                title: (*title).to_owned(),
                aliases: aliases.iter().map(|value| (*value).to_owned()).collect(),
                exec: exec.clone(),
                args: Vec::new(),
            });
        }
    }
    // Windows Terminal живёт в WindowsApps и появился позже остальных.
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let terminal = PathBuf::from(local).join("Microsoft\\WindowsApps\\wt.exe");
        if terminal.is_file() {
            entries.push(AppEntry {
                id: "wt".to_owned(),
                title: "Windows Terminal".to_owned(),
                aliases: vec![
                    "терминал".to_owned(),
                    "виндовс терминал".to_owned(),
                    "terminal".to_owned(),
                ],
                exec: terminal,
                args: Vec::new(),
            });
        }
    }
    entries
}

/// Записи из `App Paths`.
fn registry_entries() -> Vec<AppEntry> {
    let mut entries = Vec::new();
    for (executable, path) in read_app_paths() {
        let stem = Path::new(&executable)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if stem.is_empty() || !path.is_file() {
            continue;
        }
        let id = slug(&stem);
        if !AppEntry::id_is_wire_safe(&id) {
            continue;
        }
        let path = launchable_path(path);
        let known = KNOWN_ALIASES.iter().find(|(name, _, _)| *name == stem);
        let title = known.map_or_else(|| pretty_title(&stem), |(_, title, _)| (*title).to_owned());
        let mut aliases = known.map_or_else(Vec::new, |(_, _, aliases)| {
            aliases.iter().map(|value| (*value).to_owned()).collect()
        });
        if !aliases.iter().any(|alias| alias == &stem) {
            aliases.push(stem.clone());
        }
        entries.push(AppEntry {
            id,
            title,
            aliases,
            exec: path,
            args: Vec::new(),
        });
    }
    entries
}

/// Путь, который действительно можно передать в `CreateProcess`.
///
/// Исполняемый файл MSIX-пакета лежит в `Program Files\WindowsApps`, откуда
/// прямой запуск запрещён ACL. Для таких пакетов Windows кладёт alias в
/// `%LOCALAPPDATA%\Microsoft\WindowsApps` — он и есть рабочая точка входа.
fn launchable_path(path: PathBuf) -> PathBuf {
    let lowered = path.to_string_lossy().to_ascii_lowercase();
    if !(lowered.contains("\\windowsapps\\") && lowered.contains("program files")) {
        return path;
    }
    let Some(file_name) = path.file_name().map(std::ffi::OsString::from) else {
        return path;
    };
    let Ok(local) = std::env::var("LOCALAPPDATA") else {
        return path;
    };
    let alias = PathBuf::from(local)
        .join("Microsoft\\WindowsApps")
        .join(file_name);
    if alias.is_file() {
        alias
    } else {
        path
    }
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn pretty_title(stem: &str) -> String {
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Пользовательский файл каталога. Битый файл — это пустое дополнение, а не
/// падение: каталог обязан пережить опечатку в JSON.
fn user_entries(data_dir: &Path) -> Vec<AppEntry> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        apps: Vec<AppEntry>,
    }

    let path = data_dir.join(CATALOG_FILE_NAME);
    let Ok(metadata) = std::fs::metadata(&path) else {
        return Vec::new();
    };
    if metadata.len() > MAX_CATALOG_BYTES {
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_str::<File>(&text) else {
        return Vec::new();
    };
    file.apps
        .into_iter()
        .filter(|entry| entry.exec.is_file())
        .collect()
}

/// Запуск приложения. Возвращает pid.
///
/// Процесс отвязывается от job object супервизора: открытое приложение
/// принадлежит пользователю, а не Core, и переживает его перезапуск. Если job
/// отвязку запрещает, запуск повторяется без флага — приложение всё равно
/// откроется, просто разделит судьбу Core.
pub fn launch(entry: &AppEntry) -> std::io::Result<u32> {
    if !entry.exec.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("исполняемый файл не найден: {}", entry.exec.display()),
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

        let spawn = |flags: u32| {
            let mut command = std::process::Command::new(&entry.exec);
            command
                .args(&entry.args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(flags);
            if let Some(parent) = entry.exec.parent() {
                command.current_dir(parent);
            }
            crate::shell_env::apply_scrubbed_env_std(&mut command);
            command.spawn()
        };
        let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB;
        match spawn(flags) {
            Ok(child) => Ok(child.id()),
            Err(error) if error.raw_os_error() == Some(5) => {
                let child = spawn(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)?;
                Ok(child.id())
            }
            Err(error) => Err(error),
        }
    }
    #[cfg(not(windows))]
    {
        let mut command = std::process::Command::new(&entry.exec);
        command
            .args(&entry.args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        Ok(command.spawn()?.id())
    }
}

#[cfg(windows)]
fn read_app_paths() -> Vec<(String, PathBuf)> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, RRF_RT_REG_EXPAND_SZ,
        RRF_RT_REG_SZ,
    };

    const ROOT: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths";
    /// Потолок имени подключа реестра: имя длиннее — это не имя приложения.
    const MAX_NAME: usize = 256;
    /// Потолок значения: путь длиннее `MAX_PATH * 2` не бывает.
    const MAX_VALUE_BYTES: u32 = 2048;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let mut found: Vec<(String, PathBuf)> = Vec::new();
    let hives: [(HKEY, u32); 3] = [
        (HKEY_CURRENT_USER, KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
    ];
    for (hive, view) in hives {
        let mut root: HKEY = std::ptr::null_mut();
        // SAFETY: путь — валидная нуль-терминированная строка, `root`
        // записывается только при `ERROR_SUCCESS` и закрывается ниже.
        let opened = unsafe {
            RegOpenKeyExW(
                hive,
                wide(ROOT).as_ptr(),
                0,
                KEY_READ | view,
                &mut root as *mut HKEY,
            )
        };
        if opened != ERROR_SUCCESS {
            continue;
        }
        let mut index = 0u32;
        loop {
            let mut name = [0u16; MAX_NAME];
            let mut name_len = MAX_NAME as u32;
            // SAFETY: буфер и его длина принадлежат этому кадру стека,
            // остальные аргументы необязательны и передаются как null.
            let status = unsafe {
                RegEnumKeyExW(
                    root,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if status != ERROR_SUCCESS {
                break;
            }
            index += 1;
            let executable = std::ffi::OsString::from_wide(&name[..name_len as usize])
                .to_string_lossy()
                .into_owned();
            let mut buffer = [0u8; MAX_VALUE_BYTES as usize];
            let mut size = MAX_VALUE_BYTES;
            // SAFETY: читается значение по умолчанию подключа в буфер
            // фиксированного размера; `size` ограничивает запись.
            let status = unsafe {
                RegGetValueW(
                    root,
                    wide(&executable).as_ptr(),
                    std::ptr::null(),
                    RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            if status != ERROR_SUCCESS || size < 2 {
                continue;
            }
            let units = (size as usize / 2).min(buffer.len() / 2);
            let mut wide_value = Vec::with_capacity(units);
            for chunk in buffer[..units * 2].as_chunks::<2>().0 {
                wide_value.push(u16::from_le_bytes(*chunk));
            }
            while matches!(wide_value.last(), Some(0)) {
                wide_value.pop();
            }
            let value = std::ffi::OsString::from_wide(&wide_value)
                .to_string_lossy()
                .trim()
                .trim_matches('"')
                .to_owned();
            if value.is_empty() {
                continue;
            }
            if found
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(&executable))
            {
                continue;
            }
            found.push((executable, PathBuf::from(value)));
        }
        // SAFETY: `root` открыт выше и больше не используется.
        unsafe { RegCloseKey(root) };
    }
    found
}

#[cfg(not(windows))]
fn read_app_paths() -> Vec<(String, PathBuf)> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, title: &str, aliases: &[&str]) -> AppEntry {
        AppEntry {
            id: id.to_owned(),
            title: title.to_owned(),
            aliases: aliases.iter().map(|value| (*value).to_owned()).collect(),
            exec: PathBuf::from(format!("C:\\apps\\{id}.exe")),
            args: Vec::new(),
        }
    }

    fn catalog() -> AppCatalog {
        AppCatalog::from_entries(vec![
            entry("chrome", "Google Chrome", &["хром", "гугл хром"]),
            entry("firefox", "Mozilla Firefox", &["фаерфокс", "мозилла"]),
            entry(
                "code",
                "Visual Studio Code",
                &["вс код", "вижуал студио код"],
            ),
            entry("notepad", "Блокнот", &["блокнот", "notepad"]),
        ])
    }

    #[test]
    fn exact_alias_wins_over_everything() {
        assert_eq!(
            catalog().resolve("хром"),
            Resolution::Found(entry("chrome", "Google Chrome", &["хром", "гугл хром"]))
        );
        assert_eq!(
            catalog().resolve("Блокнот!"),
            Resolution::Found(entry("notepad", "Блокнот", &["блокнот", "notepad"]))
        );
    }

    #[test]
    fn normalization_ignores_case_punctuation_and_yo() {
        assert_eq!(normalize("  Гугл,  ХРОМ!  "), "гугл хром");
        assert_eq!(normalize("Ёжик"), "ежик");
        assert_eq!(normalize("—"), "");
    }

    #[test]
    fn partial_match_needs_a_word_boundary() {
        assert_eq!(
            catalog().resolve("вижуал студио"),
            Resolution::Found(entry(
                "code",
                "Visual Studio Code",
                &["вс код", "вижуал студио код"]
            ))
        );
        // «хро» — обрывок слова, а не название: подставлять за пользователя
        // Chrome по нему нельзя.
        assert_eq!(catalog().resolve("хро"), Resolution::NotFound);
    }

    #[test]
    fn several_candidates_are_reported_instead_of_a_guess() {
        let ambiguous = AppCatalog::from_entries(vec![
            entry("code", "Visual Studio Code", &["код"]),
            entry("codium", "VSCodium", &["код"]),
        ]);
        assert_eq!(
            ambiguous.resolve("код"),
            Resolution::Ambiguous(vec!["Visual Studio Code".to_owned(), "VSCodium".to_owned()])
        );
    }

    #[test]
    fn category_falls_back_to_the_first_available_by_priority() {
        assert_eq!(
            catalog().resolve("браузер"),
            Resolution::Found(entry("chrome", "Google Chrome", &["хром", "гугл хром"]))
        );
        let without_chrome =
            AppCatalog::from_entries(vec![entry("firefox", "Mozilla Firefox", &["фаерфокс"])]);
        assert_eq!(
            without_chrome.resolve("браузер"),
            Resolution::Found(entry("firefox", "Mozilla Firefox", &["фаерфокс"]))
        );
    }

    #[test]
    fn unknown_and_empty_queries_resolve_to_nothing() {
        assert_eq!(catalog().resolve("холодильник"), Resolution::NotFound);
        assert_eq!(catalog().resolve("   "), Resolution::NotFound);
    }

    #[test]
    fn a_more_trusted_source_replaces_the_path_and_keeps_the_aliases() {
        let mut merged = catalog();
        merged.upsert(AppEntry {
            id: "chrome".to_owned(),
            title: "Google Chrome".to_owned(),
            aliases: vec!["chrome".to_owned()],
            exec: PathBuf::from("D:\\portable\\chrome.exe"),
            args: vec!["--profile-directory=Work".to_owned()],
        });
        let entry = merged.get("chrome").unwrap();
        assert_eq!(entry.exec, PathBuf::from("D:\\portable\\chrome.exe"));
        assert_eq!(entry.args, vec!["--profile-directory=Work".to_owned()]);
        assert!(entry.aliases.iter().any(|alias| alias == "хром"));
        assert_eq!(merged.entries().len(), 4);
    }

    #[test]
    fn ids_that_cannot_travel_in_an_event_are_dropped() {
        let catalog = AppCatalog::from_entries(vec![
            entry("ok-1", "Годится", &[]),
            entry("не ascii", "Не годится", &[]),
        ]);
        assert_eq!(catalog.entries().len(), 1);
        assert!(catalog.get("ok-1").is_some());
    }

    #[test]
    fn filler_words_do_not_reach_the_catalog() {
        assert_eq!(strip_filler_words("мне пожалуйста хром"), "хром");
        assert_eq!(strip_filler_words("приложение телеграм"), "телеграм");
        assert_eq!(strip_filler_words("хром"), "хром");
    }

    #[test]
    fn launch_refuses_a_path_that_is_not_a_file() {
        let missing = entry("ghost", "Призрак", &[]);
        let error = launch(&missing).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn discovery_only_adds_aliases_to_a_curated_entry() {
        let mut catalog = AppCatalog::from_entries(vec![entry("notepad", "Блокнот", &["блокнот"])]);
        catalog.add_discovered(AppEntry {
            id: "notepad".to_owned(),
            title: "Notepad".to_owned(),
            aliases: vec!["notepad".to_owned()],
            exec: PathBuf::from("C:\\Program Files\\WindowsApps\\Notepad.exe"),
            args: Vec::new(),
        });
        let stored = catalog.get("notepad").unwrap();
        assert_eq!(stored.title, "Блокнот");
        assert_eq!(stored.exec, PathBuf::from("C:\\apps\\notepad.exe"));
        assert_eq!(
            stored.aliases,
            vec!["блокнот".to_owned(), "notepad".to_owned()]
        );
    }

    #[cfg(windows)]
    #[test]
    fn packaged_executables_are_launched_through_their_windowsapps_alias() {
        let packaged = PathBuf::from(
            "C:\\Program Files\\WindowsApps\\Microsoft.WindowsNotepad_1_x64__8we\\Notepad.exe",
        );
        let resolved = launchable_path(packaged.clone());
        let expected = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap())
            .join("Microsoft\\WindowsApps")
            .join("Notepad.exe");
        if expected.is_file() {
            assert_eq!(resolved, expected);
        } else {
            assert_eq!(resolved, packaged);
        }
        assert_eq!(
            launchable_path(PathBuf::from("C:\\apps\\thing.exe")),
            PathBuf::from("C:\\apps\\thing.exe")
        );
    }

    #[cfg(windows)]
    #[test]
    fn discovery_finds_system_applications_and_keeps_them_launchable() {
        let catalog = AppCatalog::discover(None);
        let notepad = catalog.get("notepad").expect("Блокнот есть в Windows");
        assert!(notepad.exec.is_file());
        assert!(matches!(catalog.resolve("блокнот"), Resolution::Found(_)));
    }
}
