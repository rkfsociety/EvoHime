use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const CONFIG_DIRECTORY: &str = "evohime";
const CONFIG_FILE: &str = "provider.env";
const MAX_CONFIG_BYTES: u64 = 16 * 1024;
const CONFIG_TEMPLATE: &str = concat!(
    "# EvoHime Linux CLI provider settings\n",
    "MODEL_PROVIDER=literouter\n",
    "LITEROUTER_API_KEY=\n",
    "EVOHIME_MODEL_TIMEOUT_SECS=0\n",
    "EVOHIME_TASK_TIMEOUT_SECONDS=0\n",
);
const ALLOWED_KEYS: &[&str] = &[
    "MODEL_PROVIDER",
    "LITEROUTER_API_KEY",
    "LITEROUTER_BASE_URL",
    "LITEROUTER_MODEL",
    "OLLAMA_BASE_URL",
    "OLLAMA_MODEL",
    "EVOHIME_MODEL_TIMEOUT_SECS",
    "EVOHIME_TASK_TIMEOUT_SECONDS",
];

pub(super) fn load() -> Result<(), String> {
    let path = config_path()?;
    let directory = path
        .parent()
        .ok_or_else(|| "provider_config_invalid: config path has no parent".to_string())?;
    prepare_private_directory(directory).map_err(|_| {
        format!(
            "provider_config_invalid: не удалось подготовить закрытый каталог настроек {}",
            directory.display()
        )
    })?;

    let values = read_private_config(&path).map_err(|error| {
        format!(
            "provider_config_invalid: не удалось прочитать {} ({error})",
            path.display()
        )
    })?;
    let values = match values {
        Some(values) => values,
        None => {
            write_config_template(&path).map_err(|_| {
                format!(
                    "provider_config_invalid: не удалось создать шаблон настроек {}",
                    path.display()
                )
            })?;
            read_private_config(&path)
                .map_err(|_| {
                    format!(
                        "provider_config_invalid: не удалось прочитать {}",
                        path.display()
                    )
                })?
                .unwrap_or_default()
        }
    };

    let present = std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .collect::<BTreeSet<_>>();
    for (name, value) in values_to_apply(values, &present) {
        std::env::set_var(name, value);
    }
    Ok(())
}

pub(super) fn setup_hint() -> Option<String> {
    let provider = std::env::var("MODEL_PROVIDER").unwrap_or_else(|_| "literouter".into());
    let path = config_path().ok()?;
    missing_key_hint(
        &provider,
        std::env::var("LITEROUTER_API_KEY").ok().as_deref(),
        &path,
    )
}

fn missing_key_hint(provider: &str, key: Option<&str>, path: &Path) -> Option<String> {
    let has_key = key.is_some_and(|value| !value.trim().is_empty());
    if !provider.eq_ignore_ascii_case("literouter") || has_key {
        return None;
    }
    Some(format!(
        "Ключ LiteRouter не настроен. Добавьте LITEROUTER_API_KEY=lr_… в {}",
        path.display()
    ))
}

fn config_path() -> Result<PathBuf, String> {
    let config_root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .filter(|path| path.is_absolute())
        .ok_or_else(|| {
            "provider_config_invalid: задайте абсолютный XDG_CONFIG_HOME или HOME".to_string()
        })?;
    Ok(config_root.join(CONFIG_DIRECTORY).join(CONFIG_FILE))
}

fn prepare_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "configuration path must be a real directory",
        ));
    }
    if metadata.mode() & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_config_template(path: &Path) -> io::Result<()> {
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => return Err(error),
    };
    file.write_all(CONFIG_TEMPLATE.as_bytes())?;
    file.sync_all()
}

fn read_private_config(path: &Path) -> io::Result<Option<BTreeMap<String, String>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "configuration must be a regular file, not a link",
        ));
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "configuration exceeds 16 KiB",
        ));
    }
    if metadata.mode() & 0o777 != 0o600 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }

    let file = File::open(path)?;
    let mut content = String::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut content)?;
    if content.len() as u64 > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "configuration exceeds 16 KiB",
        ));
    }
    parse_config(&content).map(Some).map_err(|line| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("ошибка синтаксиса в строке {line}"),
        )
    })
}

fn parse_config(content: &str) -> Result<BTreeMap<String, String>, usize> {
    let mut values = BTreeMap::new();
    for (line_number, line) in content.lines().enumerate() {
        let line_number = line_number + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (key, raw_value) = line.split_once('=').ok_or(line_number)?;
        let key = key.trim();
        if !ALLOWED_KEYS.contains(&key) {
            return Err(line_number);
        }
        let value = parse_value(raw_value.trim()).map_err(|()| line_number)?;
        if value.contains('\0') || value.contains('\r') || value.contains('\n') {
            return Err(line_number);
        }
        values.insert(key.to_owned(), value);
    }
    Ok(values)
}

fn parse_value(raw: &str) -> Result<String, ()> {
    let bytes = raw.as_bytes();
    let first = bytes.first().copied();
    let last = bytes.last().copied();
    match (first, last) {
        (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"')) if raw.len() >= 2 => {
            Ok(raw[1..raw.len() - 1].to_owned())
        }
        (Some(b'\'' | b'"'), _) | (_, Some(b'\'' | b'"')) => Err(()),
        _ => Ok(raw.to_owned()),
    }
}

fn values_to_apply(
    mut values: BTreeMap<String, String>,
    environment_names: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    values
        .entry("EVOHIME_MODEL_TIMEOUT_SECS".to_owned())
        .or_insert_with(|| "0".to_owned());
    values
        .entry("EVOHIME_TASK_TIMEOUT_SECONDS".to_owned())
        .or_insert_with(|| "0".to_owned());

    values
        .into_iter()
        .filter(|(name, _)| !environment_names.contains(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn parses_provider_values_without_expanding_or_logging_secrets() {
        let values = parse_config(
            "# local CLI config\nexport MODEL_PROVIDER=literouter\nLITEROUTER_API_KEY='lr-secret-value'\nLITEROUTER_MODEL=deepseek:free\n",
        )
        .expect("provider config parses");

        assert_eq!(
            values.get("MODEL_PROVIDER").map(String::as_str),
            Some("literouter")
        );
        assert_eq!(
            values.get("LITEROUTER_API_KEY").map(String::as_str),
            Some("lr-secret-value")
        );
        assert_eq!(
            values.get("LITEROUTER_MODEL").map(String::as_str),
            Some("deepseek:free")
        );
    }

    #[test]
    fn rejects_unsupported_or_multiline_values() {
        assert!(parse_config("PATH=/tmp\n").is_err());
        assert!(parse_config("LITEROUTER_API_KEY='unterminated\n").is_err());
        assert!(parse_config("LITEROUTER_API_KEY=bad\0value\n").is_err());
    }

    #[test]
    fn process_environment_takes_precedence_over_file() {
        let values = BTreeMap::from([
            ("MODEL_PROVIDER".to_owned(), "literouter".to_owned()),
            ("LITEROUTER_API_KEY".to_owned(), "file-key".to_owned()),
            ("EVOHIME_MODEL_TIMEOUT_SECS".to_owned(), "30".to_owned()),
        ]);
        let present = BTreeSet::from([
            "LITEROUTER_API_KEY".to_owned(),
            "EVOHIME_MODEL_TIMEOUT_SECS".to_owned(),
        ]);
        let applied = values_to_apply(values, &present);

        assert_eq!(applied.len(), 2);
        assert_eq!(
            applied.get("MODEL_PROVIDER").map(String::as_str),
            Some("literouter")
        );
        assert!(!applied.contains_key("LITEROUTER_API_KEY"));
        assert!(!applied.contains_key("EVOHIME_MODEL_TIMEOUT_SECS"));
        assert_eq!(
            applied
                .get("EVOHIME_TASK_TIMEOUT_SECONDS")
                .map(String::as_str),
            Some("0")
        );
    }

    #[test]
    fn missing_timeout_values_default_to_unlimited() {
        let applied = values_to_apply(BTreeMap::new(), &BTreeSet::new());

        assert_eq!(
            applied
                .get("EVOHIME_MODEL_TIMEOUT_SECS")
                .map(String::as_str),
            Some("0")
        );
        assert_eq!(
            applied
                .get("EVOHIME_TASK_TIMEOUT_SECONDS")
                .map(String::as_str),
            Some("0")
        );
    }

    #[test]
    fn explicit_timeout_values_in_config_are_preserved() {
        let values = BTreeMap::from([("EVOHIME_MODEL_TIMEOUT_SECS".to_owned(), "30".to_owned())]);
        let applied = values_to_apply(values, &BTreeSet::new());

        assert_eq!(
            applied
                .get("EVOHIME_MODEL_TIMEOUT_SECS")
                .map(String::as_str),
            Some("30")
        );
        assert_eq!(
            applied
                .get("EVOHIME_TASK_TIMEOUT_SECONDS")
                .map(String::as_str),
            Some("0")
        );
    }

    #[test]
    fn setup_hint_points_to_user_config_without_exposing_a_key() {
        let path = Path::new("/home/test/.config/evohime/provider.env");

        let hint = missing_key_hint("literouter", None, path).expect("missing-key hint");
        assert!(hint.contains(path.to_str().expect("UTF-8 test path")));
        assert!(hint.contains("LITEROUTER_API_KEY=lr_…"));
        assert!(missing_key_hint("ollama", None, path).is_none());
        assert!(missing_key_hint("literouter", Some("lr-test"), path).is_none());
    }

    #[test]
    fn tightens_config_permissions_before_reading() {
        let root = test_directory("config-mode");
        let directory = root.join("evohime");
        prepare_private_directory(&directory).expect("private config directory");
        let path = directory.join(CONFIG_FILE);
        fs::write(&path, "LITEROUTER_API_KEY=test\n").expect("write config");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("loosen mode");

        let values = read_private_config(&path).expect("private config read");
        assert_eq!(
            values
                .and_then(|values| values.get("LITEROUTER_API_KEY").cloned())
                .as_deref(),
            Some("test")
        );
        assert_eq!(
            fs::metadata(&directory).expect("directory metadata").mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).expect("file metadata").mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn creates_private_first_run_template() {
        let root = test_directory("config-template");
        let directory = root.join("evohime");
        prepare_private_directory(&directory).expect("private config directory");
        let path = directory.join(CONFIG_FILE);

        write_config_template(&path).expect("create config template");
        let values = read_private_config(&path)
            .expect("read config template")
            .expect("template exists");
        assert_eq!(
            values.get("MODEL_PROVIDER").map(String::as_str),
            Some("literouter")
        );
        assert_eq!(
            values.get("LITEROUTER_API_KEY").map(String::as_str),
            Some("")
        );
        assert_eq!(
            values.get("EVOHIME_MODEL_TIMEOUT_SECS").map(String::as_str),
            Some("0")
        );
        assert_eq!(
            values
                .get("EVOHIME_TASK_TIMEOUT_SECONDS")
                .map(String::as_str),
            Some("0")
        );
        assert_eq!(
            fs::metadata(&path).expect("template metadata").mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_linked_config_files() {
        let root = test_directory("config-link");
        let target = root.join("target");
        let linked = root.join(CONFIG_FILE);
        fs::write(&target, "LITEROUTER_API_KEY=test\n").expect("write target");
        symlink(&target, &linked).expect("create symlink");

        assert_eq!(
            read_private_config(&linked).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let _ = fs::remove_dir_all(root);
    }

    fn test_directory(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "evohime-cli-provider-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("temporary test directory");
        path
    }
}
