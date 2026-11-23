use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

use crate::{observability, sensitive_data_guardrails};

/// Максимальный размер одного сегмента JSONL-лога: при превышении запись
/// продолжается в следующий `<имя>.<n>.jsonl`, чтобы суточный файл оставался
/// читаемым для анализа. Явка ровно один сегмент сверх лимита допустима: это
/// страховка, а не форма контроля размера.
pub(crate) const LOG_SEGMENT_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Сколько ротированных сегментов `<имя>.<n>.jsonl` может существовать
/// в одном суточном каталоге.
pub(crate) const LOG_SEGMENT_MAX_FILES: u32 = 8;

/// Сколько суточных каталогов логов хранится; более старые удаляются при
/// старте Core и supervisor.
pub const LOG_RETENTION_DAYS: i64 = 14;

/// Количество дней с Unix-epoch, преобразованное в гражданскую дату UTC
/// (алгоритм Говарда Хиннанта, `civil_from_days`).
pub(crate) fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Имя суточного каталога логов (`YYYY-MM-DD`, UTC) для указанного
/// миллисекундного момента Unix-времени.
pub(crate) fn utc_day_string(now_ms: u64) -> String {
    let days = (now_ms / 86_400_000) as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Каталог суточных логов: `logs/<YYYY-MM-DD>`.
pub(crate) fn log_day_dir(logs_dir: &Path, now_ms: u64) -> PathBuf {
    logs_dir.join(utc_day_string(now_ms))
}

/// Текущий момент Unix-времени в миллисекундах.
pub fn now_unix_millis() -> u64 {
    now_millis_u64()
}

/// Возвращает текущий момент Unix-времени в миллисекундах.
pub(crate) fn now_millis_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

/// Открывает очередной сегмент журнала `stem.jsonl` в суточном каталоге.
///
/// Первый сегмент не заполнен до [`LOG_SEGMENT_MAX_BYTES`] — пишет в
/// `stem.jsonl`; иначе в первый свободный `stem.<n>.jsonl`. Если все сегменты
/// заполнены, пишет в последний из них (размер может немного переехать).
pub(crate) fn open_rotated_append(day_dir: &Path, stem: &str) -> io::Result<(PathBuf, File)> {
    open_rotated_append_limited(day_dir, stem, LOG_SEGMENT_MAX_BYTES, LOG_SEGMENT_MAX_FILES)
}

fn open_rotated_append_limited(
    day_dir: &Path,
    stem: &str,
    segment_max_bytes: u64,
    segment_max_files: u32,
) -> io::Result<(PathBuf, File)> {
    fs::create_dir_all(day_dir)?;
    let segment = |suffix: Option<u32>| -> PathBuf {
        match suffix {
            None => day_dir.join(format!("{stem}.jsonl")),
            Some(index) => day_dir.join(format!("{stem}.{index}.jsonl")),
        }
    };
    let mut target = segment(None);
    if existing_size(&target).is_some_and(|size| size >= segment_max_bytes) {
        let mut fallback = segment(None);
        for index in 1..=segment_max_files {
            let candidate = segment(Some(index));
            match existing_size(&candidate) {
                None => {
                    target = candidate;
                    break;
                }
                Some(size) if size < segment_max_bytes => {
                    target = candidate;
                    break;
                }
                Some(_) => fallback = candidate,
            }
        }
        if target == segment(None) {
            target = fallback;
        }
    }
    let file = OpenOptions::new().create(true).append(true).open(&target)?;
    Ok((target, file))
}

fn existing_size(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().map(|meta| meta.len())
}

/// Удаляет устаревшие логи: суточные каталоги старше [`LOG_RETENTION_DAYS`]
/// и корневые файлы старой плоской раскладки, к которым писали прежние
/// версии продукта.
///
/// Вызов даёт лучший результат без гарантий: неудача удаления отдельного
/// элемента не прерывает обработку остальных и не возвращается наружу.
/// Удаляет устаревшие логи: суточные каталоги старше [`LOG_RETENTION_DAYS`]
/// и корневые файлы старой плоской раскладки, к которым писали прежние
/// версии продукта. Каталоги, не разбирающиеся в валидную дату, не
/// трогаются — под очистку не должен попасть лишний каталог пользователя.
///
/// Вызов даёт лучший результат без гарантий: неудача удаления отдельного
/// элемента не прерывает обработку остальных и не возвращается наружу.
pub fn prune_old_logs(logs_dir: &Path, now_ms: u64) {
    let cutoff_days = (now_ms / 86_400_000) as i64 - (LOG_RETENTION_DAYS - 1);
    let legacy_cutoff_ms = now_ms.saturating_sub(25 * 3_600_000);
    let Ok(entries) = fs::read_dir(logs_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let is_dir = entry.file_type().is_ok_and(|file_type| file_type.is_dir());
        if is_dir && is_day_catalog_name(name) {
            if let Some(days) = day_catalog_epoch(name) {
                if days < cutoff_days {
                    let _ = fs::remove_dir_all(&path);
                }
            }
        } else if !is_dir && is_legacy_log_name(name) {
            let modified_ms = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64);
            if modified_ms.is_none_or(|modified| modified < legacy_cutoff_ms) {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// Имена корневых JSONL-файлов старой раскладки, обслуживаемые этим же
/// модулем. Файлы других компонентов (например, `listener.jsonl`) не
/// затрагиваются, чтобы не удалять чужие активные журналы.
fn is_legacy_log_name(name: &str) -> bool {
    matches!(
        name,
        "core.jsonl" | "supervisor.jsonl" | "model-trace.jsonl" | "audit.jsonl"
    )
}

pub(crate) fn is_day_catalog_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    name.len() == 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

/// Восстанавливает количество дней с Unix-epoch из имени суточного каталога.
pub(crate) fn day_catalog_epoch(name: &str) -> Option<i64> {
    if !is_day_catalog_name(name) {
        return None;
    }
    let year: i64 = name[0..4].parse().ok()?;
    let month: u32 = name[5..7].parse().ok()?;
    let day: u32 = name[8..10].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = shifted_year - era * 400;
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) as i64 + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(day_of_era + era * 146_097 - 719_468)
}

/// Appends structured JSON records to a newline-delimited log file.
///
/// Writes are serialized through an internal mutex and flushed before
/// [`write`](Self::write) returns. The logger creates the parent directory when
/// opened; callers remain responsible for choosing an appropriate log path.
///
/// A dated logger (see [`open_dated`](Self::open_dated)) writes into the log
/// directory's current-day catalog, switches to the next catalog when the UTC
/// day changes and rotates to `stem.<n>.jsonl` when a segment reaches
/// [`LOG_SEGMENT_MAX_BYTES`].
pub struct StructuredLogger {
    state: Mutex<LoggerInner>,
}

enum LoggerInner {
    /// Single fixed file, for tests and one-shot diagnostics.
    Fixed { writer: BufWriter<File> },
    /// Current-day catalog with day rollover and segment rotation.
    Dated {
        logs_dir: PathBuf,
        stem: String,
        day: String,
        segment_bytes: u64,
        writer: Option<BufWriter<File>>,
    },
}

impl StructuredLogger {
    /// Opens or creates an append-only structured log at `path`.
    ///
    /// Missing parent directories are created. Existing files are preserved
    /// and new records are appended.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the directory or file cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            state: Mutex::new(LoggerInner::Fixed {
                writer: BufWriter::new(file),
            }),
        })
    }

    /// Opens a dated structured log named `<stem>.jsonl` inside the current
    /// UTC day catalog of `logs_dir`.
    ///
    /// The file is not opened until the first write, so a call before the log
    /// directory exists still succeeds.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the first write cannot create its target.
    pub fn open_dated(logs_dir: impl AsRef<Path>, stem: impl Into<String>) -> Self {
        Self {
            state: Mutex::new(LoggerInner::Dated {
                logs_dir: logs_dir.as_ref().to_path_buf(),
                stem: stem.into(),
                day: String::new(),
                segment_bytes: 0,
                writer: None,
            }),
        }
    }

    /// Appends one JSON object containing timestamp, level, event, and fields.
    ///
    /// The record is serialized as a single JSONL line and flushed before this
    /// method returns.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if serialization, locking, writing, or flushing
    /// fails.
    pub fn write(&self, level: &str, event: &str, fields: Value) -> io::Result<()> {
        let timestamp_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_millis(),
            Err(error) => {
                tracing::warn!(%error, "system clock is before Unix epoch");
                0
            }
        };
        let record = json!({
            "timestamp_ms": timestamp_ms,
            "level": level,
            "event": event,
            "fields": fields,
        });
        let mut buffer = Vec::new();
        serde_json::to_writer(&mut buffer, &record)?;
        buffer.push(b'\n');
        let mut inner = self
            .state
            .lock()
            .map_err(|_| io::Error::other("structured logger lock poisoned"))?;
        let is_dated = matches!(&*inner, LoggerInner::Dated { .. });
        if is_dated {
            ensure_dated_writer(&mut inner, timestamp_ms as u64)?;
            let LoggerInner::Dated {
                segment_bytes,
                writer,
                ..
            } = &mut *inner
            else {
                return Err(io::Error::other("dated logger writer unavailable"));
            };
            if let Some(current) = writer {
                current.write_all(&buffer)?;
                current.flush()?;
                *segment_bytes += buffer.len() as u64;
                if *segment_bytes >= LOG_SEGMENT_MAX_BYTES {
                    // Закрыть писателя: следующая запись откроет следующий
                    // сегмент через open_rotated_append.
                    *writer = None;
                    *segment_bytes = 0;
                }
            }
        } else if let LoggerInner::Fixed { writer } = &mut *inner {
            writer.write_all(&buffer)?;
            writer.flush()?;
        }
        Ok(())
    }
}

/// Готовит dated-писателя: открывает сегмент при первой записи, при смене
/// UTC-дня или после ротации сегмента.
fn ensure_dated_writer(inner: &mut LoggerInner, now_ms: u64) -> io::Result<()> {
    let LoggerInner::Dated {
        logs_dir,
        stem,
        day,
        segment_bytes,
        writer,
    } = inner
    else {
        return Ok(());
    };
    let today = utc_day_string(now_ms);
    if *day == today && writer.is_some() {
        return Ok(());
    }
    writer.take();
    let (_, file) = open_rotated_append(&log_day_dir(logs_dir, now_ms), stem)?;
    *day = today;
    *segment_bytes = 0;
    *writer = Some(BufWriter::new(file));
    Ok(())
}

/// Дописывает готовую JSONL-строку (`line` должен заканчиваться `\n`) в
/// очередной сегмент `stem.jsonl` суточного каталога `logs_dir`.
fn append_line_dated(logs_dir: &Path, stem: &str, line: &[u8]) -> io::Result<()> {
    let (_, mut file) = open_rotated_append(&log_day_dir(logs_dir, now_millis_u64()), stem)?;
    file.write_all(line)?;
    file.flush()
}

/// Каталог суточных логов активной раскладки.
fn dated_logs_dir() -> PathBuf {
    crate::get_data_directory().join("logs")
}

pub(crate) fn append_audit_line(line: &str) {
    let logs_dir = dated_logs_dir();
    let mut bytes = line.as_bytes().to_vec();
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    if let Err(error) = append_line_dated(&logs_dir, "audit", &bytes) {
        tracing::warn!(error = %error, "audit log write failed");
    }
}

pub(crate) fn write_model_trace(event: &str, fields: serde_json::Value) {
    let policy = sensitive_data_guardrails::default_policy("local-trace");
    let fields = match sensitive_data_guardrails::redact_json(&policy, &fields) {
        Ok((value, _)) => value,
        Err(error) => {
            tracing::warn!(error = %error, event, "model trace redaction failed");
            serde_json::json!({"redaction_status":"blocked"})
        }
    };
    let logs_dir = dated_logs_dir();
    let timestamp_ms = now_millis_u64();
    let record = serde_json::json!({
        "timestamp_ms": timestamp_ms,
        "event": event,
        "fields": fields,
    });
    let mut line = Vec::new();
    if serde_json::to_writer(&mut line, &record).is_err() {
        return;
    }
    line.push(b'\n');
    if let Err(error) = append_line_dated(&logs_dir, "model-trace", &line) {
        tracing::warn!(error = %error, event, "model trace write failed");
    }
}

pub(crate) fn redact_boundary_text(
    destination: &str,
    value: &str,
) -> Result<String, sensitive_data_guardrails::GuardrailError> {
    // Чанкованная редакция: один tool output или сообщение может быть больше
    // MAX_INPUT_BYTES, и отказ по размеру не должен превращаться в отказ
    // отправки модели с подменой причины на sensitive_data_blocked.
    sensitive_data_guardrails::redact_text_chunked(
        &sensitive_data_guardrails::default_policy(destination),
        value,
    )
    .map(|result| result.value)
}

pub(crate) fn write_observability_hook(
    task_id: &str,
    sequence: u64,
    hook: observability::HookName,
    fields: impl IntoIterator<Item = (String, String)>,
) {
    let Ok(payload) = observability::HookPayload::new(fields) else {
        return;
    };
    let Ok(context_order) = observability::ContextOrder::capture(
        ["system", "user", "assistant", "tool"]
            .into_iter()
            .map(String::from),
    ) else {
        return;
    };
    let decision = observability::HookPolicy::default().decide(hook);
    let event_id = format!("{task_id}:{sequence}");
    let Ok(event) = observability::HookEvent::new(
        hook,
        event_id,
        task_id,
        sequence,
        decision,
        context_order,
        payload,
    ) else {
        return;
    };
    let fields = event
        .to_deterministic_json()
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or(serde_json::Value::Null);
    write_model_trace("observability.hook", fields);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writes_one_valid_json_record_per_line() {
        let path = std::env::temp_dir().join(format!("evohime-log-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let logger = StructuredLogger::open(&path).expect("logger opens");
        logger
            .write("info", "core.started", json!({"protocol_major": 1}))
            .expect("record writes");
        drop(logger);
        let content = std::fs::read_to_string(&path).expect("log reads");
        let record: serde_json::Value = serde_json::from_str(content.trim()).expect("valid JSON");
        assert_eq!(record["event"], "core.started");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-11-22 = 20779 дней с epoch.
        assert_eq!(utc_day_string(20_779 * 86_400_000), "2026-11-22");
        assert_eq!(utc_day_string(0), "1970-01-01");
        // Граница месяца: 1970-03-01 после високосного февраля.
        assert_eq!(utc_day_string(59 * 86_400_000), "1970-03-01");
        // Високосный год: 2024-02-29.
        assert_eq!(utc_day_string(19_782 * 86_400_000), "2024-02-29");
    }

    #[test]
    fn day_catalog_epoch_round_trips_utc_day_string() {
        for days in [0i64, 59, 365, 19_782, 20_679, 25_000] {
            let day = utc_day_string(days as u64 * 86_400_000);
            assert_eq!(day_catalog_epoch(&day), Some(days));
        }
        assert_eq!(day_catalog_epoch("not-a-day"), None);
        assert_eq!(day_catalog_epoch("2026-13-01"), None);
    }

    #[test]
    fn dated_logger_writes_into_day_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let logs_dir = dir.path().join("logs");
        let now_ms = now_millis_u64();
        let logger = StructuredLogger::open_dated(&logs_dir, "core");
        logger
            .write("info", "core.test", json!({"ok": true}))
            .expect("dated write");
        drop(logger);
        let day_dir = log_day_dir(&logs_dir, now_ms);
        let content = std::fs::read_to_string(day_dir.join("core.jsonl")).expect("dated log reads");
        assert!(content.contains("core.test"));
    }

    #[test]
    fn rotation_fills_segments_in_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let day_dir = dir.path().join("2026-01-01");
        let stem = "core";
        // Первый сегмент пуст — открывается базовый файл.
        let (target, file) =
            open_rotated_append_limited(&day_dir, stem, 16, 3).expect("first segment");
        assert_eq!(target.file_name().expect("filename"), "core.jsonl");
        drop(file);
        std::fs::write(&target, "x".repeat(16)).expect("fill first segment");
        // Текущий сегмент полон — переходит к core.1.jsonl.
        let (target, file) =
            open_rotated_append_limited(&day_dir, stem, 16, 3).expect("second segment");
        assert_eq!(target.file_name().expect("filename"), "core.1.jsonl");
        drop(file);
        std::fs::write(&target, "y".repeat(16)).expect("fill second segment");
        let (target, file) =
            open_rotated_append_limited(&day_dir, stem, 16, 3).expect("third segment");
        assert_eq!(target.file_name().expect("filename"), "core.2.jsonl");
        drop(file);
        std::fs::write(&target, "z".repeat(16)).expect("fill third segment");
        // Все сегменты полны — запись продолжается в последний из них.
        let (target, _) =
            open_rotated_append_limited(&day_dir, stem, 16, 3).expect("fallback segment");
        assert_eq!(target.file_name().expect("filename"), "core.3.jsonl");
    }

    #[test]
    fn prune_keeps_recent_day_catalogs_and_removes_old_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        let logs_dir = dir.path().join("logs");
        let now_days = (now_millis_u64() / 86_400_000) as i64;
        let recent = logs_dir.join(utc_day_string(now_days as u64 * 86_400_000));
        let old = logs_dir.join(utc_day_string(
            (now_days - LOG_RETENTION_DAYS) as u64 * 86_400_000,
        ));
        fs::create_dir_all(&recent).expect("recent dir");
        fs::create_dir_all(&old).expect("old dir");
        fs::write(recent.join("core.jsonl"), "{}\n").expect("recent log");
        fs::write(old.join("core.jsonl"), "{}\n").expect("old log");
        // Чужой корневой файл и чужой суточный каталог не трогаем.
        let foreign = logs_dir.join("listener.jsonl");
        fs::write(&foreign, "{}\n").expect("legacy ancestor file");
        let not_a_day = logs_dir.join("2026-13-99");
        fs::create_dir_all(&not_a_day).expect("bogus dir");

        prune_old_logs(&logs_dir, now_millis_u64());

        assert!(recent.join("core.jsonl").exists(), "recent day survives");
        assert!(!old.exists(), "old day catalog removed");
        assert!(foreign.exists(), "foreign legacy log survives");
        assert!(not_a_day.exists(), "non-date directory survives");
    }

    #[test]
    fn prune_removes_stale_legacy_jsonl_but_keeps_fresh_ones() {
        use std::fs::FileTimes;
        let dir = tempfile::tempdir().expect("tempdir");
        let logs_dir = dir.path().join("logs");
        fs::create_dir_all(&logs_dir).expect("logs dir");
        let fresh = logs_dir.join("core.jsonl");
        let stale = logs_dir.join("supervisor.jsonl");
        fs::write(&fresh, "{}\n").expect("fresh legacy log");
        fs::write(&stale, "{}\n").expect("stale legacy log");
        let old_time = std::time::SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(now_millis_u64() / 1000 - 100 * 86_400);
        let file = OpenOptions::new()
            .append(true)
            .open(&stale)
            .expect("stale open");
        file.set_times(FileTimes::new().set_modified(old_time))
            .expect("set mtime");
        drop(file);

        prune_old_logs(&logs_dir, now_millis_u64());

        assert!(fresh.exists(), "fresh legacy file survives");
        assert!(!stale.exists(), "stale legacy file removed");
    }
}
