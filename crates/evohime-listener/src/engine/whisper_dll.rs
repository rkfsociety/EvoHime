//! Движок whisper.cpp, загружаемый из проверенной DLL.
//!
//! Биндинги сделаны руками поверх `libloading`, а не через `whisper-rs`,
//! потому что сборка проекта не должна требовать CMake: self-update ставит
//! только Git, Node, Rustup и MSVC Build Tools, и добавление CMake сломало бы
//! обновление из исходников.
//!
//! ## Почему структуры можно передавать по указателю
//!
//! `whisper_init_from_file_with_params` и `whisper_full` принимают структуры
//! **по значению**. И в x64-ABI Windows, и в AArch64 композит крупнее
//! регистра передаётся косвенно: вызывающий кладёт копию в свою память и
//! передаёт указатель. Поэтому здесь они объявлены как функции с
//! `*const`-параметром, а копия создаётся на каждый вызов — callee вправе
//! писать в свою «копию по значению».
//!
//! ## Почему совпадение раскладки вообще проверяется
//!
//! Зеркало `whisper_full_params` верно ровно для одной раскладки. Манифест
//! объявляет её размеры, и они сверяются с `size_of` до первого вызова:
//! несовпадение — `AbiUnsupported`, а не попытка «как-нибудь вызвать».
//! Подписи у собственной DLL нет, поэтому лишняя проверка перед прыжком в
//! чужой код здесь не роскошь.

use std::collections::BTreeMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use libloading::os::windows::{Library, Symbol};

use super::{EngineError, EngineUnavailable, ModelRung, Recognition, SpeechEngine};
use crate::tools_dir::{ResolvedRuntime, RuntimeManifest};

/// Токен раскладки, которую зеркалит этот модуль. Принадлежит проекту: DLL и
/// манифест приезжают одним ассетом, поэтому пару «код ↔ поставка» задаём мы,
/// а не апстрим-версия whisper.cpp.
pub const ABI_TOKEN: &str = "whisper-cpp-full-params-v1";

/// Частота, на которой работает whisper.cpp. Сегментатор отдаёт ровно её.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

const WHISPER_SAMPLING_GREEDY: c_int = 0;
/// Флаг `LOAD_WITH_ALTERED_SEARCH_PATH`: зависимые библиотеки ищутся рядом с
/// самой DLL, а не в текущем каталоге процесса.
const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x0000_0008;

#[repr(C)]
#[derive(Clone, Copy)]
struct WhisperAhead {
    n_text_layer: c_int,
    n_head: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WhisperAheads {
    n_heads: usize,
    heads: *const WhisperAhead,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WhisperContextParams {
    use_gpu: bool,
    flash_attn: bool,
    gpu_device: c_int,
    dtw_token_timestamps: bool,
    dtw_aheads_preset: c_int,
    dtw_n_top: c_int,
    dtw_aheads: WhisperAheads,
    dtw_mem_size: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WhisperVadParams {
    threshold: f32,
    min_speech_duration_ms: c_int,
    min_silence_duration_ms: c_int,
    max_speech_duration_s: f32,
    speech_pad_ms: c_int,
    samples_overlap: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WhisperGreedy {
    best_of: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WhisperBeamSearch {
    beam_size: c_int,
    patience: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WhisperFullParams {
    strategy: c_int,
    n_threads: c_int,
    n_max_text_ctx: c_int,
    offset_ms: c_int,
    duration_ms: c_int,
    translate: bool,
    no_context: bool,
    no_timestamps: bool,
    single_segment: bool,
    print_special: bool,
    print_progress: bool,
    print_realtime: bool,
    print_timestamps: bool,
    token_timestamps: bool,
    thold_pt: f32,
    thold_ptsum: f32,
    max_len: c_int,
    split_on_word: bool,
    max_tokens: c_int,
    debug_mode: bool,
    audio_ctx: c_int,
    tdrz_enable: bool,
    suppress_regex: *const c_char,
    initial_prompt: *const c_char,
    carry_initial_prompt: bool,
    prompt_tokens: *const i32,
    prompt_n_tokens: c_int,
    language: *const c_char,
    detect_language: bool,
    suppress_blank: bool,
    suppress_nst: bool,
    temperature: f32,
    max_initial_ts: f32,
    length_penalty: f32,
    temperature_inc: f32,
    entropy_thold: f32,
    logprob_thold: f32,
    no_speech_thold: f32,
    greedy: WhisperGreedy,
    beam_search: WhisperBeamSearch,
    new_segment_callback: *mut c_void,
    new_segment_callback_user_data: *mut c_void,
    progress_callback: *mut c_void,
    progress_callback_user_data: *mut c_void,
    encoder_begin_callback: *mut c_void,
    encoder_begin_callback_user_data: *mut c_void,
    abort_callback: *mut c_void,
    abort_callback_user_data: *mut c_void,
    logits_filter_callback: *mut c_void,
    logits_filter_callback_user_data: *mut c_void,
    grammar_rules: *const *const c_void,
    n_grammar_rules: usize,
    i_start_rule: usize,
    grammar_penalty: f32,
    vad: bool,
    vad_model_path: *const c_char,
    vad_params: WhisperVadParams,
}

/// Сверяет объявленную манифестом раскладку с зеркалом в этом модуле.
pub fn verify_abi(manifest: &RuntimeManifest) -> Result<(), EngineUnavailable> {
    if manifest.abi.name != ABI_TOKEN {
        return Err(EngineUnavailable::AbiUnsupported);
    }
    if manifest.abi.context_params_size as usize != std::mem::size_of::<WhisperContextParams>() {
        return Err(EngineUnavailable::AbiUnsupported);
    }
    if manifest.abi.full_params_size as usize != std::mem::size_of::<WhisperFullParams>() {
        return Err(EngineUnavailable::AbiUnsupported);
    }
    Ok(())
}

/// Размеры зеркала — их и надо положить в манифест поставки.
pub const fn mirrored_sizes() -> (usize, usize) {
    (
        std::mem::size_of::<WhisperContextParams>(),
        std::mem::size_of::<WhisperFullParams>(),
    )
}

type FnContextDefaultsByRef = unsafe extern "C" fn() -> *mut WhisperContextParams;
type FnFullDefaultsByRef = unsafe extern "C" fn(c_int) -> *mut WhisperFullParams;
type FnInitFromFile =
    unsafe extern "C" fn(*const c_char, *const WhisperContextParams) -> *mut c_void;
type FnFull =
    unsafe extern "C" fn(*mut c_void, *const WhisperFullParams, *const f32, c_int) -> c_int;
type FnSegments = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnSegmentText = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;
type FnSegmentTime = unsafe extern "C" fn(*mut c_void, c_int) -> i64;
type FnLangId = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnLangStr = unsafe extern "C" fn(c_int) -> *const c_char;
type FnFree = unsafe extern "C" fn(*mut c_void);
type FnFreeFullParams = unsafe extern "C" fn(*mut WhisperFullParams);
type FnFreeContextParams = unsafe extern "C" fn(*mut WhisperContextParams);
type FnLogSet = unsafe extern "C" fn(*mut c_void, *mut c_void);

struct WhisperApi {
    context_defaults: Symbol<FnContextDefaultsByRef>,
    full_defaults: Symbol<FnFullDefaultsByRef>,
    init_from_file: Symbol<FnInitFromFile>,
    full: Symbol<FnFull>,
    n_segments: Symbol<FnSegments>,
    segment_text: Symbol<FnSegmentText>,
    segment_t0: Symbol<FnSegmentTime>,
    segment_t1: Symbol<FnSegmentTime>,
    lang_id: Option<Symbol<FnLangId>>,
    lang_str: Option<Symbol<FnLangStr>>,
    free: Symbol<FnFree>,
    free_full_params: Option<Symbol<FnFreeFullParams>>,
    free_context_params: Option<Symbol<FnFreeContextParams>>,
}

/// Пустой лог-приёмник whisper.cpp.
///
/// Библиотека по умолчанию печатает прогресс и отладку в stderr. Для процесса,
/// который слушает микрофон, это лишний канал наружу, поэтому он глушится
/// сразу после загрузки.
unsafe extern "C" fn silent_log(_level: c_int, _text: *const c_char, _user_data: *mut c_void) {}

/// Загруженная DLL whisper.cpp с открытой моделью.
pub struct WhisperDllEngine {
    api: WhisperApi,
    context: *mut c_void,
    version: String,
    rung: ModelRung,
    models: BTreeMap<ModelRung, PathBuf>,
    /// «auto» для авто-определения языка; строка обязана пережить вызов.
    language: CString,
    /// Библиотека объявлена последней: поля дропаются в порядке объявления, а
    /// выгружать DLL можно только после освобождения контекста и символов.
    _library: Library,
}

// SAFETY: контекст whisper не разделяется между потоками — движок живёт в
// одном владельце и вызывается по одному вызову за раз. `Send` нужен, потому
// что владелец переезжает в поток обработки аудио.
unsafe impl Send for WhisperDllEngine {}

impl WhisperDllEngine {
    /// Открывает движок по проверенному набору рантайма.
    pub fn open(runtime: &ResolvedRuntime) -> Result<Self, EngineUnavailable> {
        verify_abi(&runtime.manifest)?;
        if std::mem::size_of::<usize>() != 8 {
            // 32-битная сборка передаёт крупные структуры иначе; зеркало для
            // неё не проверено, поэтому загрузки не будет.
            return Err(EngineUnavailable::AbiUnsupported);
        }
        let dll = runtime
            .whisper_dll()
            .ok_or(EngineUnavailable::FileMissing)?
            .to_path_buf();
        let rung = runtime.best_rung().ok_or(EngineUnavailable::FileMissing)?;
        let model = runtime
            .model_path(rung)
            .ok_or(EngineUnavailable::FileMissing)?
            .to_path_buf();

        // SAFETY: путь ведёт к файлу, чей SHA-256 уже сверен с манифестом;
        // флаг заставляет загрузчик искать зависимости рядом с DLL, а не в
        // рабочем каталоге процесса.
        let library = unsafe { Library::load_with_flags(&dll, LOAD_WITH_ALTERED_SEARCH_PATH) }
            .map_err(|_| EngineUnavailable::LoadFailed)?;
        let api = unsafe { WhisperApi::bind(&library)? };
        if let Ok(log_set) = unsafe { library.get::<FnLogSet>(b"whisper_log_set\0") } {
            // SAFETY: колбэк — обычная extern "C" функция без состояния.
            unsafe { log_set(silent_log as *mut c_void, std::ptr::null_mut()) };
        }

        let mut engine = Self {
            api,
            context: std::ptr::null_mut(),
            version: version_token(&runtime.manifest.version, rung),
            rung,
            models: runtime.models.clone(),
            language: CString::new("auto").expect("literal has no interior nul"),
            _library: library,
        };
        engine.context = engine.open_context(&model)?;
        Ok(engine)
    }

    fn open_context(&self, model: &Path) -> Result<*mut c_void, EngineUnavailable> {
        let path = path_to_cstring(model)?;
        // SAFETY: обе функции взяты из загруженной DLL; параметры контекста
        // копируются из значений самой библиотеки, а не собираются вручную.
        unsafe {
            let defaults = (self.api.context_defaults)();
            if defaults.is_null() {
                return Err(EngineUnavailable::LoadFailed);
            }
            let mut params = *defaults;
            if let Some(free) = &self.api.free_context_params {
                free(defaults);
            }
            // GPU не используется: поставка ограничена CPU-сборкой, а тихое
            // включение GPU меняло бы и потребление, и результат.
            params.use_gpu = false;
            params.flash_attn = false;
            let context = (self.api.init_from_file)(path.as_ptr(), &params);
            if context.is_null() {
                Err(EngineUnavailable::ModelLoadFailed)
            } else {
                Ok(context)
            }
        }
    }

    fn full_params(&self) -> Result<WhisperFullParams, EngineError> {
        // SAFETY: значения по умолчанию берутся из самой библиотеки, поэтому
        // поля, которых зеркало не трогает, остаются такими, какими их задал
        // whisper.cpp.
        unsafe {
            let defaults = (self.api.full_defaults)(WHISPER_SAMPLING_GREEDY);
            if defaults.is_null() {
                return Err(EngineError::RecognizeFailed);
            }
            let mut params = *defaults;
            if let Some(free) = &self.api.free_full_params {
                free(defaults);
            }
            params.n_threads = worker_threads();
            params.translate = false;
            params.no_context = true;
            params.single_segment = false;
            params.print_special = false;
            params.print_progress = false;
            params.print_realtime = false;
            params.print_timestamps = false;
            params.no_timestamps = false;
            // «auto» с выключенным `detect_language`: whisper определяет язык
            // и продолжает распознавание. С включённым флагом он определил бы
            // язык и вернулся, не отдав текста.
            params.language = self.language.as_ptr();
            params.detect_language = false;
            Ok(params)
        }
    }
}

impl Drop for WhisperDllEngine {
    fn drop(&mut self) {
        if !self.context.is_null() {
            // SAFETY: контекст создан этой же библиотекой и освобождается один
            // раз — после этого поле обнуляется.
            unsafe { (self.api.free)(self.context) };
            self.context = std::ptr::null_mut();
        }
    }
}

impl SpeechEngine for WhisperDllEngine {
    fn recognize(&mut self, samples: &[f32]) -> Result<Recognition, EngineError> {
        if samples.is_empty() || self.context.is_null() {
            return Err(EngineError::RecognizeFailed);
        }
        let count = c_int::try_from(samples.len()).map_err(|_| EngineError::RecognizeFailed)?;
        let params = self.full_params()?;
        // SAFETY: `params` — свежая копия на этот вызов: callee получает её как
        // собственную копию по значению и вправе её менять.
        let status = unsafe { (self.api.full)(self.context, &params, samples.as_ptr(), count) };
        if status != 0 {
            return Err(EngineError::RecognizeFailed);
        }
        // SAFETY: дальше читается только результат последнего вызова.
        unsafe {
            let segments = (self.api.n_segments)(self.context);
            let mut text = String::new();
            let mut first_t0 = i64::MAX;
            let mut last_t1 = 0i64;
            for index in 0..segments {
                let raw = (self.api.segment_text)(self.context, index);
                if raw.is_null() {
                    continue;
                }
                let piece = CStr::from_ptr(raw).to_string_lossy();
                text.push_str(piece.trim());
                text.push(' ');
                let t0 = (self.api.segment_t0)(self.context, index);
                let t1 = (self.api.segment_t1)(self.context, index);
                first_t0 = first_t0.min(t0);
                last_t1 = last_t1.max(t1);
            }
            let language = match (&self.api.lang_id, &self.api.lang_str) {
                (Some(lang_id), Some(lang_str)) => {
                    let id = lang_id(self.context);
                    let raw = lang_str(id);
                    if raw.is_null() {
                        "und".to_owned()
                    } else {
                        CStr::from_ptr(raw).to_string_lossy().into_owned()
                    }
                }
                _ => "und".to_owned(),
            };
            // Метки whisper идут в сотых долях секунды.
            let duration_ms = if first_t0 == i64::MAX {
                0
            } else {
                u32::try_from((last_t1 - first_t0).max(0) * 10).unwrap_or(u32::MAX)
            };
            Ok(Recognition {
                text: text.trim().to_owned(),
                language,
                duration_ms,
            })
        }
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn rung(&self) -> Option<ModelRung> {
        Some(self.rung)
    }

    fn switch_rung(&mut self, rung: ModelRung) -> bool {
        let Some(model) = self.models.get(&rung).cloned() else {
            return false;
        };
        let Ok(context) = self.open_context(&model) else {
            return false;
        };
        if !self.context.is_null() {
            // SAFETY: старый контекст больше не используется — новый уже открыт.
            unsafe { (self.api.free)(self.context) };
        }
        self.context = context;
        self.rung = rung;
        self.version = swap_rung(&self.version, rung);
        true
    }
}

impl WhisperApi {
    /// # Safety
    /// `library` — загруженная whisper.cpp; символы живут не дольше её.
    unsafe fn bind(library: &Library) -> Result<Self, EngineUnavailable> {
        macro_rules! required {
            ($name:literal, $ty:ty) => {
                library
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .map_err(|_| EngineUnavailable::LoadFailed)?
            };
        }
        macro_rules! optional {
            ($name:literal, $ty:ty) => {
                library.get::<$ty>(concat!($name, "\0").as_bytes()).ok()
            };
        }
        Ok(Self {
            context_defaults: required!(
                "whisper_context_default_params_by_ref",
                FnContextDefaultsByRef
            ),
            full_defaults: required!("whisper_full_default_params_by_ref", FnFullDefaultsByRef),
            init_from_file: required!("whisper_init_from_file_with_params", FnInitFromFile),
            full: required!("whisper_full", FnFull),
            n_segments: required!("whisper_full_n_segments", FnSegments),
            segment_text: required!("whisper_full_get_segment_text", FnSegmentText),
            segment_t0: required!("whisper_full_get_segment_t0", FnSegmentTime),
            segment_t1: required!("whisper_full_get_segment_t1", FnSegmentTime),
            lang_id: optional!("whisper_full_lang_id", FnLangId),
            lang_str: optional!("whisper_lang_str", FnLangStr),
            free: required!("whisper_free", FnFree),
            free_full_params: optional!("whisper_free_params", FnFreeFullParams),
            free_context_params: optional!("whisper_free_context_params", FnFreeContextParams),
        })
    }
}

/// Версия движка как opaque-токен: `<версия поставки>+<ступень>`.
fn version_token(manifest_version: &str, rung: ModelRung) -> String {
    format!("{manifest_version}+{}", rung.as_str())
}

fn swap_rung(version: &str, rung: ModelRung) -> String {
    let base = version.split('+').next().unwrap_or(version);
    version_token(base, rung)
}

/// Потоки распознавания: не больше четырёх, чтобы фоновое слушание не
/// вытесняло работу пользователя.
fn worker_threads() -> c_int {
    std::thread::available_parallelism()
        .map(|value| value.get().min(4))
        .unwrap_or(2) as c_int
}

/// Путь модели для C API. Не-UTF-8 путь отвергается, а не подменяется.
fn path_to_cstring(path: &Path) -> Result<CString, EngineUnavailable> {
    let text = path.to_str().ok_or(EngineUnavailable::ModelLoadFailed)?;
    CString::new(text).map_err(|_| EngineUnavailable::ModelLoadFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools_dir::RuntimeAbi;

    fn manifest(name: &str, context: usize, full: usize) -> RuntimeManifest {
        RuntimeManifest {
            schema: 1,
            version: "whisper-small-q5_1".into(),
            abi: RuntimeAbi {
                name: name.into(),
                context_params_size: context as u32,
                full_params_size: full as u32,
            },
            files: Vec::new(),
            models: Vec::new(),
        }
    }

    #[test]
    fn abi_must_match_the_mirrored_layout() {
        let (context, full) = mirrored_sizes();
        assert_eq!(verify_abi(&manifest(ABI_TOKEN, context, full)), Ok(()));
        assert_eq!(
            verify_abi(&manifest(ABI_TOKEN, context + 8, full)),
            Err(EngineUnavailable::AbiUnsupported)
        );
        assert_eq!(
            verify_abi(&manifest(ABI_TOKEN, context, full + 8)),
            Err(EngineUnavailable::AbiUnsupported)
        );
        assert_eq!(
            verify_abi(&manifest("whisper-cpp-full-params-v2", context, full)),
            Err(EngineUnavailable::AbiUnsupported)
        );
    }

    /// Числа зафиксированы намеренно: именно их кладёт в манифест сборка
    /// поставки, и именно по ним отличается чужая раскладка от своей. Правка
    /// зеркала без правки манифеста должна ломать этот тест, а не рантайм.
    #[test]
    fn mirrored_layout_has_the_documented_sizes() {
        assert_eq!(mirrored_sizes(), (48, 304));
        let (context, full) = mirrored_sizes();
        assert_eq!(context % std::mem::align_of::<usize>(), 0);
        assert_eq!(full % std::mem::align_of::<usize>(), 0);
    }

    #[test]
    fn version_token_carries_the_rung_and_stays_opaque() {
        let version = version_token("whisper-small-q5_1", ModelRung::Small);
        assert_eq!(version, "whisper-small-q5_1+small");
        assert_eq!(
            swap_rung(&version, ModelRung::Tiny),
            "whisper-small-q5_1+tiny"
        );
        assert!(version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '+')));
    }

    #[test]
    fn worker_threads_stay_bounded() {
        let threads = worker_threads();
        assert!((1..=4).contains(&threads));
    }
}
