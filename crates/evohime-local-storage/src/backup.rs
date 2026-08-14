//! Core-first SQLite backup/restore contract.
//!
//! The container deliberately keeps the manifest separate from the database
//! bytes.  Preview can therefore inspect format/schema/checksum metadata
//! without opening or exposing table contents.  Database bytes are produced
//! with SQLite's Online Backup API, never by copying an open `.db` file.

use crate::{LocalDatabase, StorageError};
use rusqlite::{backup::Backup, backup::StepResult, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    ptr, thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const BACKUP_FORMAT_VERSION: u32 = 1;
pub const MAX_BACKUP_BYTES: u64 = 512 * 1024 * 1024;
const MAGIC: &[u8] = b"EVOHIME_SQLITE_BACKUP_V1\n";
const MAX_MANIFEST_BYTES: u32 = 64 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupProgressPhase {
    Prepare,
    Backup,
    Validate,
    Restore,
    Reopen,
    Cleanup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupProgress {
    pub phase: BackupProgressPhase,
    pub completed: u64,
    pub total: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupObjectSummary {
    pub object_type: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupPreview {
    pub format_version: u32,
    pub app_version: String,
    pub schema_version: u32,
    pub created_at_unix_ms: u64,
    pub container_size_bytes: u64,
    pub database_size_bytes: u64,
    pub checksum_sha256: String,
    pub objects: Vec<BackupObjectSummary>,
    pub source_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupResult {
    pub preview: BackupPreview,
    pub destination_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreResult {
    pub preview: BackupPreview,
    pub safety_backup_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupManifest {
    format_version: u32,
    app_version: String,
    schema_version: u32,
    created_at_unix_ms: u64,
    database_size_bytes: u64,
    checksum_sha256: String,
    objects: Vec<BackupObjectSummary>,
    source_name: String,
}

impl BackupManifest {
    fn preview(&self, container_size_bytes: u64) -> BackupPreview {
        BackupPreview {
            format_version: self.format_version,
            app_version: self.app_version.clone(),
            schema_version: self.schema_version,
            created_at_unix_ms: self.created_at_unix_ms,
            container_size_bytes,
            database_size_bytes: self.database_size_bytes,
            checksum_sha256: self.checksum_sha256.clone(),
            objects: self.objects.clone(),
            source_name: self.source_name.clone(),
        }
    }
}

struct ExtractedDatabase {
    path: PathBuf,
    preview: BackupPreview,
}

impl Drop for ExtractedDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        remove_sqlite_sidecars(&self.path);
    }
}

impl LocalDatabase {
    /// Reads only the bounded JSON manifest. Database contents are not opened.
    pub fn preview_backup(path: impl AsRef<Path>) -> Result<BackupPreview, StorageError> {
        let path = path.as_ref();
        let (manifest, container_size_bytes) = read_manifest(path)?;
        validate_manifest(&manifest)?;
        Ok(manifest.preview(container_size_bytes))
    }

    /// Creates an atomic, checksum-protected backup container.
    pub fn create_backup(
        &self,
        destination: impl AsRef<Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress),
    ) -> Result<BackupResult, StorageError> {
        self.create_backup_with_cancel(destination, app_version, progress, || false)
    }

    pub fn create_backup_with_cancel(
        &self,
        destination: impl AsRef<Path>,
        app_version: &str,
        mut progress: impl FnMut(BackupProgress),
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<BackupResult, StorageError> {
        let destination = destination.as_ref();
        reject_existing_destination(destination)?;
        ensure_parent(destination)?;
        report_progress(
            &mut progress,
            &mut cancelled,
            progress_event(
                BackupProgressPhase::Prepare,
                0,
                Some(5),
                "preparing SQLite backup",
            ),
        )?;

        checkpoint(&self.connection)?;
        let sqlite_path = temporary_sibling(destination, "sqlite");
        let protected_path = temporary_sibling(destination, "protected");
        let container_path = temporary_sibling(destination, "container");
        let result = (|| {
            let objects = object_summary(&self.connection)?;
            let _sqlite_size = online_backup(
                &self.connection,
                &sqlite_path,
                &mut progress,
                &mut cancelled,
            )?;
            report_progress(
                &mut progress,
                &mut cancelled,
                progress_event(
                    BackupProgressPhase::Validate,
                    2,
                    Some(5),
                    "validating SQLite backup",
                ),
            )?;
            validate_sqlite(&sqlite_path, self.schema_version()?)?;
            if cancelled() {
                return Err(StorageError::BackupCancelled);
            }
            let protected_size = protect_file(&sqlite_path, &protected_path)?;
            let checksum = sha256_file(&protected_path)?;
            let manifest = BackupManifest {
                format_version: BACKUP_FORMAT_VERSION,
                app_version: bounded_text(app_version),
                schema_version: self.schema_version()?,
                created_at_unix_ms: now_unix_ms(),
                database_size_bytes: protected_size,
                checksum_sha256: checksum,
                objects,
                source_name: file_name(self.path()),
            };
            report_progress(
                &mut progress,
                &mut cancelled,
                progress_event(
                    BackupProgressPhase::Backup,
                    3,
                    Some(5),
                    "writing backup container",
                ),
            )?;
            write_container(&container_path, &protected_path, &manifest)?;
            report_progress(
                &mut progress,
                &mut cancelled,
                progress_event(
                    BackupProgressPhase::Cleanup,
                    4,
                    Some(5),
                    "committing backup container",
                ),
            )?;
            fs::rename(&container_path, destination)?;
            let container_size = fs::metadata(destination)?.len();
            report_progress(
                &mut progress,
                &mut cancelled,
                progress_event(BackupProgressPhase::Cleanup, 5, Some(5), "backup completed"),
            )?;
            Ok(BackupResult {
                preview: manifest.preview(container_size),
                destination_name: file_name(destination),
            })
        })();
        let _ = fs::remove_file(&sqlite_path);
        let _ = fs::remove_file(&protected_path);
        let _ = fs::remove_file(&container_path);
        result
    }

    /// Validates a backup and restores it through SQLite's Online Backup API.
    /// Deletes EvoHime backup containers in `directory` older than
    /// `retention_ms`, returning the names removed.
    ///
    /// This is the rotation a memory `forget` relies on: a logically deleted
    /// statement still lives inside every backup taken before it was erased,
    /// so those containers must age out on a bounded schedule.
    ///
    /// Only files carrying the EvoHime backup magic and a readable manifest
    /// are considered, and the age comes from that manifest rather than from
    /// filesystem timestamps, which a copy or a restore would reset. Anything
    /// else in the directory is left untouched.
    pub fn purge_expired_backups(
        directory: impl AsRef<Path>,
        retention_ms: u64,
        now_unix_ms: u64,
    ) -> Result<Vec<String>, StorageError> {
        let directory = directory.as_ref();
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let cutoff = now_unix_ms.saturating_sub(retention_ms);
        let mut removed = Vec::new();
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if !path.is_file() {
                continue;
            }
            // An unreadable or foreign file is not ours to delete.
            let Ok((manifest, _)) = read_manifest(&path) else {
                continue;
            };
            if validate_manifest(&manifest).is_err() || manifest.created_at_unix_ms > cutoff {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                removed.push(file_name(&path));
            }
        }
        removed.sort();
        Ok(removed)
    }

    /// The caller holds the EventJournal lock, which is the Core connection
    /// drain boundary. A safety backup is created before touching the target.
    pub fn restore_backup(
        &mut self,
        backup_path: impl AsRef<Path>,
        safety_backup_path: impl AsRef<Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress),
    ) -> Result<RestoreResult, StorageError> {
        self.restore_backup_with_cancel(
            backup_path,
            safety_backup_path,
            app_version,
            progress,
            || false,
        )
    }

    pub fn restore_backup_with_cancel(
        &mut self,
        backup_path: impl AsRef<Path>,
        safety_backup_path: impl AsRef<Path>,
        app_version: &str,
        mut progress: impl FnMut(BackupProgress),
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<RestoreResult, StorageError> {
        let backup_path = backup_path.as_ref();
        let safety_backup_path = safety_backup_path.as_ref();
        reject_existing_destination(safety_backup_path)?;
        ensure_parent(safety_backup_path)?;
        report_progress(
            &mut progress,
            &mut cancelled,
            progress_event(
                BackupProgressPhase::Prepare,
                0,
                Some(6),
                "preparing restore",
            ),
        )?;
        let expected_schema = self.schema_version()?;
        let extracted =
            extract_database(backup_path, expected_schema, &mut progress, &mut cancelled)?;
        if extracted.preview.app_version.is_empty() || app_version.trim().is_empty() {
            return Err(StorageError::BackupFormat(
                "backup and application versions must be present".into(),
            ));
        }
        report_progress(
            &mut progress,
            &mut cancelled,
            progress_event(
                BackupProgressPhase::Backup,
                2,
                Some(6),
                "creating pre-restore safety backup",
            ),
        )?;
        self.create_backup_with_cancel(safety_backup_path, app_version, |_| {}, &mut cancelled)?;

        let restore_result = (|| {
            report_progress(
                &mut progress,
                &mut cancelled,
                progress_event(
                    BackupProgressPhase::Restore,
                    3,
                    Some(6),
                    "restoring validated SQLite image",
                ),
            )?;
            if cancelled() {
                return Err(StorageError::BackupCancelled);
            }
            atomic_swap_database(self, &extracted.path, expected_schema)?;
            report_progress(
                &mut progress,
                &mut cancelled,
                progress_event(
                    BackupProgressPhase::Reopen,
                    4,
                    Some(6),
                    "reopening restored SQLite state",
                ),
            )?;
            validate_sqlite_connection(&self.connection, expected_schema)?;
            report_progress(
                &mut progress,
                &mut cancelled,
                progress_event(
                    BackupProgressPhase::Cleanup,
                    5,
                    Some(6),
                    "restore completed",
                ),
            )?;
            Ok(RestoreResult {
                preview: extracted.preview.clone(),
                safety_backup_name: file_name(safety_backup_path),
            })
        })();

        if restore_result.is_err() {
            let rollback = rollback_from_safety(self, safety_backup_path);
            if let Err(error) = rollback {
                return Err(StorageError::Backup(format!(
                    "restore failed and rollback failed: {error}"
                )));
            }
        }
        restore_result
    }
}

/// Replaces the live database only after the extracted image has passed all
/// integrity/schema checks. The old file remains available until the new
/// connection has reopened and validated successfully.
fn atomic_swap_database(
    database: &mut LocalDatabase,
    replacement: &Path,
    expected_schema: u32,
) -> Result<(), StorageError> {
    let database_path = database.path().to_path_buf();
    let old_path = database_path.with_file_name(format!(
        ".{}.pre-swap-{}.db",
        file_name(&database_path),
        std::process::id()
    ));
    reject_existing_destination(&old_path)?;

    let placeholder = Connection::open_in_memory()?;
    let current = std::mem::replace(&mut database.connection, placeholder);
    drop(current);
    remove_sqlite_sidecars(&database_path);

    if let Err(error) = fs::rename(&database_path, &old_path) {
        database.connection = reopen_connection(&database_path)?;
        return Err(error.into());
    }

    let swap_result = (|| {
        fs::rename(replacement, &database_path)?;
        let reopened = reopen_connection(&database_path)?;
        validate_sqlite_connection(&reopened, expected_schema)?;
        database.connection = reopened;
        // Cleanup is deliberately best-effort after the new connection has
        // been validated. A cleanup failure must not roll back a valid swap
        // and must never turn into data loss.
        let _ = fs::remove_file(&old_path);
        Ok::<(), StorageError>(())
    })();

    if let Err(error) = swap_result {
        let _ = fs::remove_file(&database_path);
        remove_sqlite_sidecars(&database_path);
        let _ = fs::rename(&old_path, &database_path);
        database.connection = reopen_connection(&database_path)?;
        return Err(error);
    }
    Ok(())
}

fn reopen_connection(path: &Path) -> Result<Connection, StorageError> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    Ok(connection)
}

fn online_backup(
    source: &Connection,
    destination: &Path,
    progress: &mut impl FnMut(BackupProgress),
    cancelled: &mut impl FnMut() -> bool,
) -> Result<u64, StorageError> {
    let mut destination_connection = Connection::open(destination)?;
    let backup = Backup::new(source, &mut destination_connection)?;
    loop {
        let step = backup.step(16)?;
        let current = backup.progress();
        let completed = current.pagecount.saturating_sub(current.remaining).max(0) as u64;
        let total = (current.pagecount > 0).then_some(current.pagecount as u64);
        report_progress(
            progress,
            cancelled,
            progress_event(
                BackupProgressPhase::Backup,
                completed,
                total,
                "copying SQLite pages",
            ),
        )?;
        match step {
            StepResult::Done => break,
            StepResult::More | StepResult::Busy | StepResult::Locked => {
                thread::sleep(Duration::from_millis(2));
            }
            _ => thread::sleep(Duration::from_millis(2)),
        }
    }
    drop(backup);
    destination_connection.execute_batch("PRAGMA journal_mode = DELETE;")?;
    drop(destination_connection);
    Ok(fs::metadata(destination)?.len())
}

fn extract_database(
    backup_path: &Path,
    expected_schema: u32,
    progress: &mut impl FnMut(BackupProgress),
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ExtractedDatabase, StorageError> {
    let (manifest, container_size) = read_manifest(backup_path)?;
    validate_manifest(&manifest)?;
    if manifest.schema_version != expected_schema {
        return Err(StorageError::BackupSchemaMismatch {
            expected: expected_schema,
            actual: manifest.schema_version,
        });
    }
    report_progress(
        progress,
        cancelled,
        progress_event(
            BackupProgressPhase::Validate,
            1,
            Some(6),
            "validating backup checksum",
        ),
    )?;
    let extracted_path = temporary_sibling(backup_path, "restore");
    let encrypted_path = temporary_sibling(backup_path, "encrypted");
    let result = (|| {
        let mut file = BufReader::new(File::open(backup_path)?);
        skip_container_header(&mut file)?;
        let mut output = BufWriter::new(File::create(&encrypted_path)?);
        let mut remaining = manifest.database_size_bytes;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
        while remaining > 0 {
            if cancelled() {
                return Err(StorageError::BackupCancelled);
            }
            let wanted = remaining.min(buffer.len() as u64) as usize;
            let read = file.read(&mut buffer[..wanted])?;
            if read == 0 {
                return Err(StorageError::BackupFormat(
                    "backup ended before database payload".into(),
                ));
            }
            output.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
            remaining -= read as u64;
        }
        if file.read(&mut [0_u8; 1])? != 0 {
            return Err(StorageError::BackupFormat(
                "backup contains trailing bytes".into(),
            ));
        }
        output.flush()?;
        let observed = hex_digest(hasher.finalize());
        if observed != manifest.checksum_sha256 {
            return Err(StorageError::BackupChecksumMismatch {
                expected: manifest.checksum_sha256.clone(),
                actual: observed,
            });
        }
        unprotect_file(&encrypted_path, &extracted_path)?;
        let _ = fs::remove_file(&encrypted_path);
        validate_sqlite(&extracted_path, expected_schema)?;
        Ok(ExtractedDatabase {
            path: extracted_path.clone(),
            preview: manifest.preview(container_size),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&extracted_path);
        let _ = fs::remove_file(&encrypted_path);
    }
    result
}

fn protect_file(source: &Path, destination: &Path) -> Result<u64, StorageError> {
    let input = fs::read(source)?;
    let protected = protect_bytes(&input)?;
    fs::write(destination, &protected)?;
    Ok(protected.len() as u64)
}

fn unprotect_file(source: &Path, destination: &Path) -> Result<(), StorageError> {
    let input = fs::read(source)?;
    let plaintext = unprotect_bytes(&input)?;
    fs::write(destination, plaintext)?;
    Ok(())
}

#[cfg(not(windows))]
fn protect_bytes(input: &[u8]) -> Result<Vec<u8>, StorageError> {
    // Native product support is Windows-only; retain portable Rust test
    // coverage on Unix where DPAPI does not exist.
    Ok(input.to_vec())
}

#[cfg(not(windows))]
fn unprotect_bytes(input: &[u8]) -> Result<Vec<u8>, StorageError> {
    Ok(input.to_vec())
}

#[cfg(windows)]
#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

#[cfg(windows)]
#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(
        data_in: *const DataBlob,
        description: *const u16,
        entropy: *const DataBlob,
        reserved: *mut core::ffi::c_void,
        prompt: *mut core::ffi::c_void,
        flags: u32,
        data_out: *mut DataBlob,
    ) -> i32;
    fn CryptUnprotectData(
        data_in: *const DataBlob,
        description: *mut *mut u16,
        entropy: *const DataBlob,
        reserved: *mut core::ffi::c_void,
        prompt: *mut core::ffi::c_void,
        flags: u32,
        data_out: *mut DataBlob,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
}

#[cfg(windows)]
fn protect_bytes(input: &[u8]) -> Result<Vec<u8>, StorageError> {
    crypt_bytes(input, true)
}

#[cfg(windows)]
fn unprotect_bytes(input: &[u8]) -> Result<Vec<u8>, StorageError> {
    crypt_bytes(input, false)
}

#[cfg(windows)]
fn crypt_bytes(input: &[u8], protect: bool) -> Result<Vec<u8>, StorageError> {
    let input_blob = DataBlob {
        cb_data: input.len() as u32,
        pb_data: input.as_ptr() as *mut u8,
    };
    let mut output_blob = DataBlob {
        cb_data: 0,
        pb_data: ptr::null_mut(),
    };
    let success = unsafe {
        if protect {
            CryptProtectData(
                &input_blob,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                &mut output_blob,
            )
        } else {
            CryptUnprotectData(
                &input_blob,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                &mut output_blob,
            )
        }
    };
    if success == 0 || output_blob.pb_data.is_null() {
        return Err(StorageError::Backup(
            "Windows DPAPI rejected the backup payload".into(),
        ));
    }
    let result = unsafe {
        std::slice::from_raw_parts(output_blob.pb_data, output_blob.cb_data as usize).to_vec()
    };
    unsafe { LocalFree(output_blob.pb_data.cast()) };
    Ok(result)
}

fn rollback_from_safety(
    database: &mut LocalDatabase,
    safety_path: &Path,
) -> Result<(), StorageError> {
    let schema = database.schema_version()?;
    let extracted = extract_database(safety_path, schema, &mut |_| {}, &mut || false)?;
    atomic_swap_database(database, &extracted.path, schema)
}

fn report_progress(
    progress: &mut impl FnMut(BackupProgress),
    cancelled: &mut impl FnMut() -> bool,
    item: BackupProgress,
) -> Result<(), StorageError> {
    if cancelled() {
        return Err(StorageError::BackupCancelled);
    }
    progress(item);
    if cancelled() {
        return Err(StorageError::BackupCancelled);
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<(BackupManifest, u64), StorageError> {
    let container_size = fs::metadata(path)?.len();
    if container_size > MAX_BACKUP_BYTES {
        return Err(StorageError::BackupTooLarge(container_size));
    }
    let mut file = BufReader::new(File::open(path)?);
    let mut magic = vec![0_u8; MAGIC.len()];
    file.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(StorageError::BackupFormat("invalid backup magic".into()));
    }
    let mut length = [0_u8; 4];
    file.read_exact(&mut length)?;
    let manifest_len = u32::from_le_bytes(length);
    if manifest_len > MAX_MANIFEST_BYTES {
        return Err(StorageError::BackupFormat("manifest is too large".into()));
    }
    let mut bytes = vec![0_u8; manifest_len as usize];
    file.read_exact(&mut bytes)?;
    let manifest = serde_json::from_slice(&bytes)?;
    Ok((manifest, container_size))
}

fn skip_container_header(file: &mut impl Read) -> Result<(), StorageError> {
    let mut magic = vec![0_u8; MAGIC.len()];
    file.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(StorageError::BackupFormat("invalid backup magic".into()));
    }
    let mut length = [0_u8; 4];
    file.read_exact(&mut length)?;
    let manifest_len = u32::from_le_bytes(length);
    if manifest_len > MAX_MANIFEST_BYTES {
        return Err(StorageError::BackupFormat("manifest is too large".into()));
    }
    io::copy(&mut file.take(manifest_len as u64), &mut io::sink())?;
    Ok(())
}

fn write_container(
    destination: &Path,
    sqlite_path: &Path,
    manifest: &BackupManifest,
) -> Result<(), StorageError> {
    let manifest_bytes = serde_json::to_vec(manifest)?;
    if manifest_bytes.len() > MAX_MANIFEST_BYTES as usize {
        return Err(StorageError::BackupFormat("manifest is too large".into()));
    }
    let mut output = BufWriter::new(File::create(destination)?);
    output.write_all(MAGIC)?;
    output.write_all(&(manifest_bytes.len() as u32).to_le_bytes())?;
    output.write_all(&manifest_bytes)?;
    let mut input = BufReader::new(File::open(sqlite_path)?);
    io::copy(&mut input, &mut output)?;
    output.flush()?;
    output.get_ref().sync_all()?;
    Ok(())
}

fn validate_sqlite(path: &Path, expected_schema: u32) -> Result<(), StorageError> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    validate_sqlite_connection(&connection, expected_schema)
}

fn validate_sqlite_connection(
    connection: &Connection,
    expected_schema: u32,
) -> Result<(), StorageError> {
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(StorageError::BackupFormat(format!(
            "SQLite integrity check failed: {integrity}"
        )));
    }
    let schema: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if schema != expected_schema {
        return Err(StorageError::BackupSchemaMismatch {
            expected: expected_schema,
            actual: schema,
        });
    }
    Ok(())
}

fn validate_manifest(manifest: &BackupManifest) -> Result<(), StorageError> {
    if manifest.format_version != BACKUP_FORMAT_VERSION {
        return Err(StorageError::BackupFormat(format!(
            "unsupported backup format version {}",
            manifest.format_version
        )));
    }
    if manifest.database_size_bytes == 0 || manifest.database_size_bytes > MAX_BACKUP_BYTES {
        return Err(StorageError::BackupTooLarge(manifest.database_size_bytes));
    }
    if manifest.checksum_sha256.len() != 64
        || !manifest
            .checksum_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(StorageError::BackupFormat(
            "invalid checksum metadata".into(),
        ));
    }
    Ok(())
}

fn object_summary(connection: &Connection) -> Result<Vec<BackupObjectSummary>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT type, COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' GROUP BY type ORDER BY type",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(BackupObjectSummary {
            object_type: row.get(0)?,
            count: row.get::<_, i64>(1)?.max(0) as u64,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn checkpoint(connection: &Connection) -> Result<(), StorageError> {
    let _: (i64, i64, i64) =
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, StorageError> {
    let mut input = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_digest(hasher.finalize()))
}

fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn progress_event(
    phase: BackupProgressPhase,
    completed: u64,
    total: Option<u64>,
    message: &str,
) -> BackupProgress {
    BackupProgress {
        phase,
        completed,
        total,
        message: message.into(),
    }
}

fn reject_existing_destination(path: &Path) -> Result<(), StorageError> {
    if path.exists() {
        return Err(StorageError::BackupDestinationExists(file_name(path)));
    }
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<(), StorageError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn temporary_sibling(path: &Path, suffix: &str) -> PathBuf {
    let stamp = now_unix_ms();
    let name = format!(
        ".{}.{}.{}.partial",
        file_name(path),
        std::process::id(),
        stamp
    );
    path.with_file_name(format!("{name}.{suffix}"))
}

fn remove_sqlite_sidecars(path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(PathBuf::from(format!("{}{}", path.display(), suffix)));
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "database-backup".into())
}

fn bounded_text(value: &str) -> String {
    value.trim().chars().take(128).collect()
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalDatabase;
    use std::path::PathBuf;

    fn paths(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "evohime-backup-{name}-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        (
            root.join("events.db"),
            root.join("backup.evohime"),
            root.join("pre-restore.evohime"),
        )
    }

    fn cleanup(paths: &(PathBuf, PathBuf, PathBuf)) {
        if let Some(root) = paths.0.parent() {
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn backup_rotation_drops_only_expired_evohime_containers() {
        let paths = paths("retention");
        let database = LocalDatabase::open(&paths.0).expect("database opens");
        database
            .create_backup(&paths.1, "1.0.0", |_| {})
            .expect("backup writes");
        let directory = paths.1.parent().expect("backup directory").to_path_buf();

        // A foreign file in the same directory is never ours to delete.
        let foreign = directory.join("notes.txt");
        fs::write(&foreign, b"user data").expect("foreign file writes");

        let created = LocalDatabase::preview_backup(&paths.1)
            .expect("preview reads")
            .created_at_unix_ms;
        let retention_ms = 7 * 24 * 60 * 60 * 1000;

        // Inside the retention window nothing is removed.
        assert!(LocalDatabase::purge_expired_backups(
            &directory,
            retention_ms,
            created + retention_ms - 1
        )
        .expect("sweep runs")
        .is_empty());
        assert!(paths.1.exists());

        // Once the container ages past the window it is rotated out.
        let removed = LocalDatabase::purge_expired_backups(
            &directory,
            retention_ms,
            created + retention_ms + 1,
        )
        .expect("sweep runs");
        assert_eq!(removed, vec!["backup.evohime".to_owned()]);
        assert!(!paths.1.exists());
        assert!(foreign.exists(), "unrelated files must survive rotation");

        // A missing directory is not an error: there is nothing to rotate.
        assert!(
            LocalDatabase::purge_expired_backups(directory.join("absent"), retention_ms, 0)
                .expect("missing directory is fine")
                .is_empty()
        );
        cleanup(&paths);
    }

    #[test]
    fn preview_reads_safe_manifest_without_database_rows() {
        let paths = paths("preview");
        let database = LocalDatabase::open(&paths.0).expect("database opens");
        database
            .connection()
            .execute("CREATE TABLE marker(value TEXT NOT NULL)", [])
            .expect("marker creates");
        database
            .connection()
            .execute("INSERT INTO marker VALUES ('not in preview')", [])
            .expect("marker writes");
        database
            .create_backup(&paths.1, "core-test", |_| {})
            .expect("backup creates");
        drop(database);

        let preview = LocalDatabase::preview_backup(&paths.1).expect("preview reads");
        assert_eq!(preview.format_version, BACKUP_FORMAT_VERSION);
        assert_eq!(preview.schema_version, crate::SCHEMA_VERSION);
        assert_eq!(preview.app_version, "core-test");
        assert_eq!(preview.source_name, "events.db");
        let serialized = serde_json::to_string(&preview).expect("preview serializes");
        assert!(!serialized.contains("not in preview"));
        assert_eq!(preview.checksum_sha256.len(), 64);
        cleanup(&paths);
    }

    #[test]
    fn cancellation_does_not_publish_partial_backup() {
        let paths = paths("cancel");
        let database = LocalDatabase::open(&paths.0).expect("database opens");
        let error = database
            .create_backup_with_cancel(&paths.1, "core-test", |_| {}, || true)
            .expect_err("backup must be cancelled");
        assert!(matches!(error, StorageError::BackupCancelled));
        assert!(!paths.1.exists());
        let partials = fs::read_dir(paths.1.parent().expect("parent"))
            .expect("parent reads")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains("partial"));
        assert!(!partials);
        cleanup(&paths);
    }

    #[test]
    fn checksum_failure_leaves_working_database_untouched() {
        let paths = paths("checksum");
        let database = LocalDatabase::open(&paths.0).expect("database opens");
        database
            .connection()
            .execute("CREATE TABLE marker(value TEXT NOT NULL)", [])
            .expect("marker creates");
        database
            .connection()
            .execute("INSERT INTO marker VALUES ('before')", [])
            .expect("marker writes");
        database
            .create_backup(&paths.1, "core-test", |_| {})
            .expect("backup creates");
        drop(database);
        let mut bytes = fs::read(&paths.1).expect("backup reads");
        let index = bytes.len() - 1;
        bytes[index] ^= 0x40;
        fs::write(&paths.1, bytes).expect("tampered backup writes");

        let mut database = LocalDatabase::open(&paths.0).expect("database reopens");
        let error = database
            .restore_backup(&paths.1, &paths.2, "core-test", |_| {})
            .expect_err("tampered backup rejects");
        assert!(matches!(error, StorageError::BackupChecksumMismatch { .. }));
        let marker: String = database
            .connection()
            .query_row("SELECT value FROM marker", [], |row| row.get(0))
            .expect("marker survives");
        assert_eq!(marker, "before");
        assert!(!paths.2.exists());
        cleanup(&paths);
    }

    #[test]
    fn restore_creates_safety_backup_and_replaces_database_state() {
        let paths = paths("restore");
        let mut database = LocalDatabase::open(&paths.0).expect("database opens");
        database
            .connection()
            .execute("CREATE TABLE marker(value TEXT NOT NULL)", [])
            .expect("marker creates");
        database
            .connection()
            .execute("INSERT INTO marker VALUES ('before')", [])
            .expect("marker writes");
        database
            .create_backup(&paths.1, "core-test", |_| {})
            .expect("backup creates");
        database
            .connection()
            .execute("UPDATE marker SET value = 'after'", [])
            .expect("marker changes");

        let result = database
            .restore_backup(&paths.1, &paths.2, "core-test", |_| {})
            .expect("restore succeeds");
        assert_eq!(result.safety_backup_name, "pre-restore.evohime");
        assert!(paths.2.exists());
        let marker: String = database
            .connection()
            .query_row("SELECT value FROM marker", [], |row| row.get(0))
            .expect("restored marker reads");
        assert_eq!(marker, "before");
        cleanup(&paths);
    }
}
