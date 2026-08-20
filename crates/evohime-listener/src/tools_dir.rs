//! Резолвер каталога инструментов и проверка поставленного рантайма.
//!
//! Корень доверия здесь ровно один: SHA-256 каждого файла из
//! `listener-runtime.json`, полученного тем же релизным каналом GitHub, что и
//! установщик продукта. Подпись Authenticode — дополнительная проверка и
//! только для файлов, у которых она вообще бывает (см. [`crate::authenticode`]).
//!
//! Модуль намеренно не скачивает ничего: загрузка — работа Electron main.
//! Листенер только выбирает каталог, читает манифест и сверяет содержимое.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::engine::{EngineUnavailable, ModelRung};

/// Каталог рантайма листенера, заданный явно.
pub const TOOLS_DIR_ENV: &str = "EVOHIME_LISTENER_TOOLS_DIR";
/// Общий каталог инструментов; листенер берёт в нём подкаталог `listener`.
pub const TOOLS_ROOT_ENV: &str = "EVOHIME_TOOLS_DIR";
/// Имя манифеста внутри каталога рантайма.
pub const MANIFEST_FILE: &str = "listener-runtime.json";
/// Единственная версия схемы манифеста, которую понимает этот код.
pub const MANIFEST_SCHEMA: u32 = 1;

/// Потолок манифеста. Он описывает несколько файлов, а не содержит их.
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
/// Потолок любого файла рантайма: самая большая модель лестницы — около 500 МБ.
pub const MAX_RUNTIME_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Роль файла в поставке. Роль, а не имя, решает, обязателен ли файл и нужна
/// ли ему подпись, — иначе переименование файла меняло бы политику проверки.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    /// Собственная сборка whisper.cpp. Подписи у неё нет и не будет, пока в
    /// проекте не появится signing pipeline.
    WhisperDll,
    /// Runtime ONNX от Microsoft. Опционален: без него остаётся
    /// энергетический VAD.
    OnnxRuntimeDll,
    /// Модель Silero VAD. Опциональна ровно по той же причине.
    SileroVad,
    /// Зависимость whisper.dll (`ggml*.dll` и подобные). Обязательна, если
    /// объявлена: загрузчик Windows подтянет её сам, поэтому хеш должен быть
    /// сверен до загрузки, а не после.
    SupportDll,
}

impl FileRole {
    /// Обязательные файлы: без них движка нет вовсе.
    pub const fn required(self) -> bool {
        matches!(self, FileRole::WhisperDll | FileRole::SupportDll)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            FileRole::WhisperDll => "whisper_dll",
            FileRole::OnnxRuntimeDll => "onnxruntime_dll",
            FileRole::SileroVad => "silero_vad",
            FileRole::SupportDll => "support_dll",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RuntimeAbi {
    /// Токен ABI, который знает этот код. Чужой токен — отказ, а не попытка.
    pub name: String,
    /// Размер `whisper_context_params` в поставленной DLL.
    pub context_params_size: u32,
    /// Размер `whisper_full_params` в поставленной DLL.
    pub full_params_size: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RuntimeFile {
    pub role: FileRole,
    /// Относительный путь внутри каталога рантайма.
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RuntimeModel {
    pub rung: ModelRung,
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

/// Разобранный `listener-runtime.json`.
#[derive(Clone, Debug, Deserialize)]
pub struct RuntimeManifest {
    pub schema: u32,
    /// Версия движка целиком, например `whisper-small-q5_1`. Уезжает в
    /// `ambient.engine` как opaque-токен.
    pub version: String,
    pub abi: RuntimeAbi,
    pub files: Vec<RuntimeFile>,
    pub models: Vec<RuntimeModel>,
}

/// Проверенный набор рантайма: пути уже сверены с хешами манифеста.
#[derive(Clone, Debug)]
pub struct ResolvedRuntime {
    pub root: PathBuf,
    pub manifest: RuntimeManifest,
    /// Абсолютные пути обязательных и найденных опциональных файлов. Роль
    /// `support_dll` может встречаться несколько раз, поэтому карта хранит
    /// список.
    pub files: BTreeMap<FileRole, Vec<PathBuf>>,
    /// Модели лестницы, у которых файл на месте и хеш сошёлся.
    pub models: BTreeMap<ModelRung, PathBuf>,
    /// Роли, объявленные манифестом опционально и отсутствующие на диске.
    /// Пользователю про них говорят прямо, а не прячут за «всё готово».
    pub missing_optional: Vec<FileRole>,
}

impl ResolvedRuntime {
    pub fn whisper_dll(&self) -> Option<&Path> {
        self.files
            .get(&FileRole::WhisperDll)
            .and_then(|paths| paths.first())
            .map(PathBuf::as_path)
    }

    /// Самая тяжёлая доступная ступень лестницы: с неё начинается сессия.
    pub fn best_rung(&self) -> Option<ModelRung> {
        ModelRung::LADDER
            .iter()
            .copied()
            .find(|rung| self.models.contains_key(rung))
    }

    pub fn model_path(&self, rung: ModelRung) -> Option<&Path> {
        self.models.get(&rung).map(PathBuf::as_path)
    }
}

/// Источник переменных окружения. Отдельный трейт — чтобы тесты не мутировали
/// процесс: `std::env::set_var` глобален и делает тесты зависимыми от порядка.
pub trait EnvSource {
    fn var_os(&self, key: &str) -> Option<OsString>;
}

pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn var_os(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }
}

impl<F> EnvSource for F
where
    F: Fn(&str) -> Option<OsString>,
{
    fn var_os(&self, key: &str) -> Option<OsString> {
        self(key)
    }
}

/// Кандидаты каталога рантайма в порядке приоритета.
///
/// Хардкод путей рабочих checkout запрещён: список полностью выводится из
/// окружения и профиля Windows.
pub fn candidate_dirs(env: &dyn EnvSource) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(explicit) = env.var_os(TOOLS_DIR_ENV) {
        if !explicit.is_empty() {
            candidates.push(PathBuf::from(explicit));
        }
    }
    if let Some(root) = env.var_os(TOOLS_ROOT_ENV) {
        if !root.is_empty() {
            candidates.push(PathBuf::from(root).join("listener"));
        }
    }
    if let Some(local) = env.var_os("LOCALAPPDATA") {
        if !local.is_empty() {
            candidates.push(
                PathBuf::from(local)
                    .join("EvoHime")
                    .join("tools")
                    .join("listener"),
            );
        }
    }
    candidates
}

/// Выбирает первый кандидат с валидным манифестом и целыми файлами.
///
/// Недоступный каталог — переход к следующему кандидату, а не ошибка: иначе
/// пустая переменная окружения ломала бы штатную установку. Возвращается
/// ошибка последнего кандидата, потому что «ничего не нашли» без причины
/// нельзя показать пользователю.
pub fn resolve(env: &dyn EnvSource) -> Result<ResolvedRuntime, EngineUnavailable> {
    resolve_in(&candidate_dirs(env))
}

pub fn resolve_in(candidates: &[PathBuf]) -> Result<ResolvedRuntime, EngineUnavailable> {
    let mut last = EngineUnavailable::ToolsDirMissing;
    for candidate in candidates {
        match load(candidate) {
            Ok(runtime) => return Ok(runtime),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// Читает и проверяет один каталог целиком.
pub fn load(root: &Path) -> Result<ResolvedRuntime, EngineUnavailable> {
    if !root.is_dir() {
        return Err(EngineUnavailable::ToolsDirMissing);
    }
    let manifest_path = root.join(MANIFEST_FILE);
    let metadata =
        std::fs::metadata(&manifest_path).map_err(|_| EngineUnavailable::ManifestMissing)?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(EngineUnavailable::ManifestInvalid);
    }
    let bytes = std::fs::read(&manifest_path).map_err(|_| EngineUnavailable::ManifestMissing)?;
    let manifest: RuntimeManifest =
        serde_json::from_slice(&bytes).map_err(|_| EngineUnavailable::ManifestInvalid)?;
    verify(root, manifest)
}

/// Сверяет уже разобранный манифест с содержимым каталога.
pub fn verify(
    root: &Path,
    manifest: RuntimeManifest,
) -> Result<ResolvedRuntime, EngineUnavailable> {
    if manifest.schema != MANIFEST_SCHEMA {
        return Err(EngineUnavailable::ManifestInvalid);
    }
    if manifest.version.is_empty() || !is_opaque_token(&manifest.version) {
        return Err(EngineUnavailable::ManifestInvalid);
    }
    if manifest.models.is_empty() {
        return Err(EngineUnavailable::ManifestInvalid);
    }

    let mut files: BTreeMap<FileRole, Vec<PathBuf>> = BTreeMap::new();
    let mut missing_optional = Vec::new();
    let mut declared: Vec<PathBuf> = Vec::new();
    for entry in &manifest.files {
        let path = contained_path(root, &entry.name)?;
        match check_file(&path, &entry.sha256, entry.size) {
            Ok(()) => {
                crate::authenticode::require(entry.role, &path)?;
                declared.push(path.clone());
                files.entry(entry.role).or_default().push(path);
            }
            Err(EngineUnavailable::FileMissing) if !entry.role.required() => {
                missing_optional.push(entry.role);
            }
            Err(error) => return Err(error),
        }
    }
    if !files.contains_key(&FileRole::WhisperDll) {
        return Err(EngineUnavailable::FileMissing);
    }
    reject_undeclared_libraries(root, &declared)?;

    let mut models = BTreeMap::new();
    for entry in &manifest.models {
        let path = contained_path(root, &entry.name)?;
        match check_file(&path, &entry.sha256, entry.size) {
            Ok(()) => {
                models.insert(entry.rung, path);
            }
            // Отсутствие одной ступени лестницы не отменяет остальные: без
            // `small` движок работает с `base`, и это лучше, чем молчание.
            Err(EngineUnavailable::FileMissing) => {}
            Err(error) => return Err(error),
        }
    }
    if models.is_empty() {
        return Err(EngineUnavailable::FileMissing);
    }

    Ok(ResolvedRuntime {
        root: root.to_path_buf(),
        manifest,
        files,
        models,
        missing_optional,
    })
}

/// Путь строго внутри каталога рантайма.
///
/// Манифест подписан только хешами, а не подписью, поэтому `..` в имени файла
/// проверяется здесь, а не «доверием к каналу»: иначе манифест мог бы
/// заставить листенер загрузить DLL откуда угодно с диска.
fn contained_path(root: &Path, name: &str) -> Result<PathBuf, EngineUnavailable> {
    if name.is_empty() || name.len() > 256 {
        return Err(EngineUnavailable::ManifestPathEscapes);
    }
    let relative = Path::new(name);
    let normal = relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    if !normal {
        return Err(EngineUnavailable::ManifestPathEscapes);
    }
    Ok(root.join(relative))
}

/// Ни одной необъявленной библиотеки рядом с whisper.dll.
///
/// Загрузчик Windows подтягивает зависимости из каталога самой DLL, поэтому
/// лишний `ggml-cpu.dll`, которого нет в манифесте, попал бы в процесс мимо
/// проверки хеша. Каталог рантайма принадлежит установке целиком, так что
/// посторонняя библиотека в нём — повод отказаться, а не «наверное, ничего».
fn reject_undeclared_libraries(root: &Path, declared: &[PathBuf]) -> Result<(), EngineUnavailable> {
    let entries = std::fs::read_dir(root).map_err(|_| EngineUnavailable::ToolsDirMissing)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_library = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"));
        if is_library && !declared.iter().any(|known| known == &path) {
            return Err(EngineUnavailable::UnexpectedFile);
        }
    }
    Ok(())
}

fn check_file(
    path: &Path,
    expected_hash: &str,
    expected_size: u64,
) -> Result<(), EngineUnavailable> {
    if expected_size == 0 || expected_size > MAX_RUNTIME_FILE_BYTES {
        return Err(EngineUnavailable::ManifestInvalid);
    }
    if !is_sha256(expected_hash) {
        return Err(EngineUnavailable::ManifestInvalid);
    }
    let metadata = std::fs::metadata(path).map_err(|_| EngineUnavailable::FileMissing)?;
    if metadata.len() != expected_size {
        return Err(EngineUnavailable::SizeMismatch);
    }
    let actual = sha256_file(path).map_err(|_| EngineUnavailable::FileMissing)?;
    if actual != expected_hash {
        return Err(EngineUnavailable::HashMismatch);
    }
    Ok(())
}

/// Потоковый SHA-256: модель весит сотни мегабайт, читать её целиком в память
/// ради хеша незачем.
fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn is_opaque_token(value: &str) -> bool {
    value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MapEnv(HashMap<String, OsString>);

    impl EnvSource for MapEnv {
        fn var_os(&self, key: &str) -> Option<OsString> {
            self.0.get(key).cloned()
        }
    }

    fn env(pairs: &[(&str, &str)]) -> MapEnv {
        MapEnv(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), OsString::from(*value)))
                .collect(),
        )
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("evohime-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn digest(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn write(root: &Path, name: &str, bytes: &[u8]) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn manifest_json(root: &Path, dll: &[u8], model: &[u8]) -> String {
        let _ = root;
        format!(
            r#"{{
              "schema": 1,
              "version": "whisper-small-q5_1",
              "abi": {{ "name": "whisper-cpp-full-params-v1", "context_params_size": 48, "full_params_size": 400 }},
              "files": [
                {{ "role": "whisper_dll", "name": "whisper.dll", "sha256": "{dll_hash}", "size": {dll_size} }}
              ],
              "models": [
                {{ "rung": "small", "name": "models/ggml-small-q5_1.bin", "sha256": "{model_hash}", "size": {model_size} }}
              ]
            }}"#,
            dll_hash = digest(dll),
            dll_size = dll.len(),
            model_hash = digest(model),
            model_size = model.len(),
        )
    }

    fn valid_runtime(name: &str) -> PathBuf {
        let root = temp_dir(name);
        let dll = b"not-a-real-dll".as_slice();
        let model = b"not-a-real-model".as_slice();
        write(&root, "whisper.dll", dll);
        write(&root, "models/ggml-small-q5_1.bin", model);
        write(
            &root,
            MANIFEST_FILE,
            manifest_json(&root, dll, model).as_bytes(),
        );
        root
    }

    #[test]
    fn candidates_follow_the_documented_order() {
        let candidates = candidate_dirs(&env(&[
            (TOOLS_DIR_ENV, "C:\\explicit"),
            (TOOLS_ROOT_ENV, "C:\\tools"),
            ("LOCALAPPDATA", "C:\\Users\\u\\AppData\\Local"),
        ]));
        assert_eq!(
            candidates,
            vec![
                PathBuf::from("C:\\explicit"),
                PathBuf::from("C:\\tools").join("listener"),
                PathBuf::from("C:\\Users\\u\\AppData\\Local")
                    .join("EvoHime")
                    .join("tools")
                    .join("listener"),
            ]
        );
    }

    #[test]
    fn empty_variables_do_not_become_candidates() {
        let candidates = candidate_dirs(&env(&[(TOOLS_DIR_ENV, ""), (TOOLS_ROOT_ENV, "")]));
        assert!(candidates.is_empty());
    }

    #[test]
    fn missing_candidate_falls_through_to_the_next_one() {
        let good = valid_runtime("fallthrough");
        let runtime = resolve_in(&[PathBuf::from("Z:\\does\\not\\exist"), good.clone()]).unwrap();
        assert_eq!(runtime.root, good);
        assert_eq!(runtime.manifest.version, "whisper-small-q5_1");
        assert_eq!(runtime.best_rung(), Some(ModelRung::Small));
    }

    #[test]
    fn tampered_file_is_not_loaded() {
        let root = valid_runtime("tampered");
        std::fs::write(root.join("whisper.dll"), b"tampered-dll!!").unwrap();
        assert_eq!(load(&root).err(), Some(EngineUnavailable::HashMismatch));
    }

    #[test]
    fn truncated_file_is_reported_as_a_size_mismatch() {
        let root = valid_runtime("truncated");
        std::fs::write(root.join("whisper.dll"), b"short").unwrap();
        assert_eq!(load(&root).err(), Some(EngineUnavailable::SizeMismatch));
    }

    #[test]
    fn missing_required_file_is_engine_unavailable() {
        let root = valid_runtime("no-dll");
        std::fs::remove_file(root.join("whisper.dll")).unwrap();
        assert_eq!(load(&root).err(), Some(EngineUnavailable::FileMissing));
        let root = valid_runtime("no-model");
        std::fs::remove_file(root.join("models").join("ggml-small-q5_1.bin")).unwrap();
        assert_eq!(load(&root).err(), Some(EngineUnavailable::FileMissing));
    }

    #[test]
    fn missing_optional_file_is_reported_but_not_fatal() {
        let root = temp_dir("optional");
        let dll = b"not-a-real-dll".as_slice();
        let model = b"not-a-real-model".as_slice();
        write(&root, "whisper.dll", dll);
        write(&root, "models/ggml-small-q5_1.bin", model);
        let manifest = format!(
            r#"{{
              "schema": 1,
              "version": "whisper-small-q5_1",
              "abi": {{ "name": "whisper-cpp-full-params-v1", "context_params_size": 48, "full_params_size": 400 }},
              "files": [
                {{ "role": "whisper_dll", "name": "whisper.dll", "sha256": "{dll_hash}", "size": {dll_size} }},
                {{ "role": "silero_vad", "name": "silero_vad.onnx", "sha256": "{model_hash}", "size": 999999 }}
              ],
              "models": [
                {{ "rung": "small", "name": "models/ggml-small-q5_1.bin", "sha256": "{model_hash}", "size": {model_size} }}
              ]
            }}"#,
            dll_hash = digest(dll),
            dll_size = dll.len(),
            model_hash = digest(model),
            model_size = model.len(),
        );
        write(&root, MANIFEST_FILE, manifest.as_bytes());
        let runtime = load(&root).unwrap();
        assert_eq!(runtime.missing_optional, vec![FileRole::SileroVad]);
        assert!(runtime.whisper_dll().is_some());
    }

    #[test]
    fn manifest_cannot_point_outside_the_tools_directory() {
        let root = temp_dir("escape");
        for name in [
            "..\\evil.dll",
            "../evil.dll",
            "C:\\Windows\\System32\\evil.dll",
        ] {
            assert_eq!(
                contained_path(&root, name),
                Err(EngineUnavailable::ManifestPathEscapes),
                "{name} escaped containment"
            );
        }
        assert_eq!(
            contained_path(&root, "models/ok.bin").unwrap(),
            root.join("models").join("ok.bin")
        );
    }

    /// Загрузчик Windows берёт зависимости из каталога DLL, поэтому лишняя
    /// библиотека рядом — это обход проверки хеша, а не безобидный мусор.
    #[test]
    fn an_undeclared_library_next_to_the_dll_blocks_loading() {
        let root = valid_runtime("stray");
        std::fs::write(root.join("ggml-cpu.dll"), b"stray").unwrap();
        assert_eq!(load(&root).err(), Some(EngineUnavailable::UnexpectedFile));
    }

    #[test]
    fn broken_manifest_is_invalid_not_missing() {
        let root = temp_dir("broken");
        write(&root, "whisper.dll", b"x");
        write(&root, MANIFEST_FILE, b"{ not json");
        assert_eq!(load(&root).err(), Some(EngineUnavailable::ManifestInvalid));
        write(&root, MANIFEST_FILE, br#"{"schema":99,"version":"v","abi":{"name":"x","context_params_size":1,"full_params_size":1},"files":[],"models":[]}"#);
        assert_eq!(load(&root).err(), Some(EngineUnavailable::ManifestInvalid));
    }

    #[test]
    fn hash_field_must_be_lowercase_hex_of_the_right_length() {
        assert!(is_sha256(&"a".repeat(64)));
        assert!(!is_sha256(&"A".repeat(64)));
        assert!(!is_sha256(&"a".repeat(63)));
        assert!(!is_sha256("zz"));
    }
}
