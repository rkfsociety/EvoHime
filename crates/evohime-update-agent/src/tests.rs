use super::{
    cleanup_failed_staging, copy_reader_bounded, is_github_api_url, is_github_release_asset_url,
    is_trusted_github_url, merge_installed_manifest_to, normalize_github_token, parse_json_body,
    read_installed_module_manifest, read_update_config, resolve_github_token_with,
    stream_file_hash, updater_bootstrap_script, updater_first_if_required, updater_http_client,
    validate_compatible_manifest, validate_runtime_manifest, write_staged_manifest,
    CompatibleComponent, CompatibleManifest, RuntimeReleaseEntry, RuntimeReleaseManifest,
    UpdateCandidate, UpdaterBootstrapPaths, UpdaterRequirement,
};
use sha2::Digest;
use std::{
    fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn json_response_reports_an_empty_body_without_a_raw_parser_error() {
    let error = parse_json_body::<serde_json::Value>("  \n", "manifest core").unwrap_err();
    assert_eq!(
        error,
        "updater: manifest core: GitHub вернул пустой ответ вместо JSON"
    );
}

#[test]
fn json_response_reports_invalid_json_with_its_purpose() {
    let error =
        parse_json_body::<serde_json::Value>("<html>", "список GitHub Release").unwrap_err();
    assert!(error.starts_with("updater: список GitHub Release: GitHub вернул некорректный JSON:"));
}

#[test]
fn ignores_a_flag_when_an_argument_value_is_missing() {
    let args = vec![
        "evohime-updater".into(),
        "--manifest".into(),
        "--available".into(),
    ];
    assert!(super::argument_value(&args, "--manifest").is_none());
    assert!(super::argument_value(&args, "--available").is_none());
}

#[test]
fn parses_optional_apply_wait_process_id() {
    let args = vec![
        "evohime-updater".into(),
        "--apply".into(),
        "--wait-pid".into(),
        "42".into(),
    ];
    assert_eq!(
        super::optional_process_id(&args, "--wait-pid").unwrap(),
        Some(42)
    );

    let invalid = vec!["--wait-pid".into(), "not-a-pid".into()];
    assert!(super::optional_process_id(&invalid, "--wait-pid").is_err());
}

#[test]
fn legacy_manifest_reader_accepts_a_utf8_bom() {
    let path = std::env::temp_dir().join(format!(
        "evohime-available-manifest-{}.json",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::write(
        &path,
        b"\xef\xbb\xbf[{\"id\":\"core\",\"version\":\"1.0.0\"}]",
    )
    .expect("write BOM-prefixed manifest");

    let available: Vec<super::ModuleRecord> =
        super::read(path.to_str().expect("temporary path is UTF-8"))
            .expect("BOM-prefixed manifest must parse");
    let _ = fs::remove_file(path);
    assert_eq!(available[0].id, "core");
}

#[test]
fn installed_manifest_reader_accepts_a_utf8_bom() {
    let root = std::env::temp_dir().join(format!(
        "evohime-installed-manifest-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary install directory");
    fs::write(
            root.join("evohime.components.json"),
            b"\xef\xbb\xbf{\"components\":[{\"id\":\"core\",\"version\":\"1.0.0\",\"dependencies\":null}]}",
        )
        .expect("write BOM-prefixed installed manifest");

    let manifest =
        read_installed_module_manifest(&root).expect("BOM-prefixed installed manifest must parse");
    let _ = fs::remove_dir_all(root);
    assert_eq!(manifest.components[0].id, "core");
}

#[test]
fn manifest_merge_fails_closed_on_corrupt_existing_manifest() {
    let root = std::env::temp_dir().join(format!(
        "evohime-merge-manifest-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary install directory");
    fs::write(root.join("evohime.components.json"), b"not-json").expect("write corrupt manifest");
    let destination = root.join("evohime.components.json.next");

    let error = merge_installed_manifest_to(&root, &[], &destination)
        .expect_err("corrupt manifest must not be replaced with an empty one");
    assert!(!error.is_empty());
    assert!(!destination.exists());
    fs::remove_dir_all(root).expect("remove temporary install directory");
}

#[test]
fn manifest_merge_rejects_duplicate_existing_components() {
    let root = std::env::temp_dir().join(format!(
        "evohime-duplicate-manifest-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary install directory");
    fs::write(
        root.join("evohime.components.json"),
        br#"{"components":[{"id":"core","version":"1.0.0"},{"id":"core","version":"1.0.0"}]}"#,
    )
    .expect("write duplicate manifest");
    let destination = root.join("evohime.components.json.next");

    let error = merge_installed_manifest_to(&root, &[], &destination)
        .expect_err("duplicate installed components must be rejected");
    assert!(error.contains("duplicate or invalid installed module id"));
    assert!(!destination.exists());
    fs::remove_dir_all(root).expect("remove temporary install directory");
}

#[test]
fn staged_manifest_keeps_unchanged_dependencies() {
    let root = std::env::temp_dir().join(format!(
        "evohime-staged-manifest-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary install directory");
    fs::write(
            root.join("evohime.components.json"),
            br#"{"components":[
                {"id":"core","version":"1.0.0","artifact":"core.exe","path":"core.exe","size":1,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","required":true,"restart":"core"}
            ]}"#,
        )
        .expect("write installed manifest");
    let update = UpdateCandidate {
        module: "shell-host".into(),
        installed: "1.0.0".into(),
        available: "1.1.0".into(),
        summary: String::new(),
        changes: vec![],
        dependencies: vec!["core".into()],
        restart: "shell".into(),
        artifact: "shell-host.zip".into(),
        size: 1,
        sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        download_url: String::new(),
    };
    let destination = root.join("staging").join("evohime.components.json");
    fs::create_dir_all(destination.parent().expect("staging parent")).unwrap();

    write_staged_manifest(&root, &destination, &[&update], None).expect("write marker");
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&destination).expect("read marker")).unwrap();
    evohime_tx::component_manifest::Manifest::parse(
        &fs::read(destination).expect("read transaction marker"),
    )
    .expect("staged marker matches transaction manifest schema");
    let components = value["components"].as_array().unwrap();
    assert_eq!(components.len(), 2);
    assert!(components.iter().any(|item| item["id"] == "core"));
    assert!(components
        .iter()
        .any(|item| { item["id"] == "shell-host" && item["dependencies"][0] == "core" }));
    fs::remove_dir_all(root).expect("remove temporary install directory");
}

#[test]
fn staged_manifest_strips_runtime_dependency_from_updated_listener() {
    let root = std::env::temp_dir().join(format!(
        "evohime-updated-listener-manifest-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary install directory");
    fs::write(
        root.join("evohime.components.json"),
        br#"{"components":[{"id":"core","version":"1.0.0","dependencies":[]}]}"#,
    )
    .expect("write installed manifest");
    let update = UpdateCandidate {
        module: "listener".into(),
        installed: "1.0.0".into(),
        available: "1.1.0".into(),
        summary: String::new(),
        changes: vec![],
        dependencies: vec!["core".into(), "listener-runtime".into()],
        restart: "listener".into(),
        artifact: "evohime-listener.exe".into(),
        size: 1,
        sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        download_url: String::new(),
    };
    let destination = root.join("staging").join("evohime.components.json");
    fs::create_dir_all(destination.parent().expect("staging parent")).unwrap();

    write_staged_manifest(&root, &destination, &[&update], None).expect("write marker");
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&destination).expect("read marker")).unwrap();
    let listener = value["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "listener")
        .expect("listener component");
    assert_eq!(listener["dependencies"], serde_json::json!(["core"]));
    fs::remove_dir_all(root).expect("remove temporary install directory");
}

#[test]
fn staged_manifest_repairs_legacy_listener_runtime_dependency() {
    let root = std::env::temp_dir().join(format!(
        "evohime-legacy-runtime-manifest-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary install directory");
    fs::write(
        root.join("evohime.components.json"),
        br#"{"components":[
                {"id":"core","version":"1.0.0","dependencies":[]},
                {"id":"listener","version":"1.0.0","dependencies":["core","listener-runtime"]}
            ]}"#,
    )
    .expect("write legacy installed manifest");
    let destination = root.join("staging").join("evohime.components.json");
    fs::create_dir_all(destination.parent().expect("staging parent")).unwrap();

    write_staged_manifest(&root, &destination, &[], None).expect("write marker");
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(destination).expect("read marker")).unwrap();
    let listener = value["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "listener")
        .expect("listener component");
    assert_eq!(listener["dependencies"], serde_json::json!(["core"]));

    let merged_destination = root.join("evohime.components.json.next");
    merge_installed_manifest_to(&root, &[], &merged_destination).expect("merge marker");
    let merged: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(merged_destination).expect("read merged marker"))
            .unwrap();
    let merged_listener = merged["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "listener")
        .expect("merged listener component");
    assert_eq!(merged_listener["dependencies"], serde_json::json!(["core"]));
    fs::remove_dir_all(root).expect("remove temporary install directory");
}

#[test]
fn staged_manifest_repairs_legacy_single_dependency_values() {
    let root = std::env::temp_dir().join(format!(
        "evohime-legacy-dependency-manifest-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after UNIX epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary install directory");
    fs::write(
        root.join("evohime.components.json"),
        br#"{"components":[
                {"id":"core","version":"1.0.0","dependencies":"supervisor"},
                {"id":"cli","version":"1.0.0","dependencies":"core"},
                {"id":"listener","version":"1.0.0","dependencies":null}
            ]}"#,
    )
    .expect("write legacy installed manifest");
    let destination = root.join("staging").join("evohime.components.json");
    fs::create_dir_all(destination.parent().expect("staging parent")).unwrap();

    write_staged_manifest(&root, &destination, &[], None).expect("write marker");
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(destination).expect("read marker")).unwrap();
    let components = value["components"].as_array().unwrap();
    let core = components
        .iter()
        .find(|item| item["id"] == "core")
        .expect("core component");
    let cli = components
        .iter()
        .find(|item| item["id"] == "cli")
        .expect("cli component");
    assert_eq!(core["dependencies"], serde_json::json!(["supervisor"]));
    assert_eq!(cli["dependencies"], serde_json::json!(["core"]));
    let listener = components
        .iter()
        .find(|item| item["id"] == "listener")
        .expect("listener component");
    assert_eq!(listener["dependencies"], serde_json::json!([]));
    fs::remove_dir_all(root).expect("remove temporary install directory");
}

#[test]
fn streams_shell_host_hash_and_reports_size() {
    let path = std::env::temp_dir().join(format!(
        "evohime-shell-hash-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::write(&path, b"shell-host").expect("write shell host");
    let (size, hash) = stream_file_hash(&path).expect("hash shell host");
    assert_eq!(size, 10);
    assert_eq!(hash, format!("{:x}", sha2::Sha256::digest(b"shell-host")));
    fs::remove_file(path).expect("remove shell host");
}

#[test]
fn bounded_archive_copy_rejects_expansion_over_limit() {
    let mut output = Vec::new();
    let error = copy_reader_bounded(&mut std::io::Cursor::new(b"1234"), &mut output, 3)
        .expect_err("archive expansion must be bounded");
    assert!(error.contains("лимит распаковки"));
    assert!(output.len() <= 3);
}

#[test]
fn failed_archive_extraction_removes_partial_destination() {
    let root = std::env::temp_dir().join(format!(
        "evohime-archive-cleanup-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("destination")).expect("create destination");
    fs::write(root.join("destination/partial"), b"partial").expect("write partial file");
    fs::write(root.join("broken.zip"), b"not a zip").expect("write broken archive");

    assert!(super::extract_ui_bundle(&root.join("broken.zip"), &root.join("destination")).is_err());
    assert!(!root.join("destination").exists());
    fs::remove_dir_all(root).expect("remove temporary archive directory");
}

#[test]
fn updater_package_requires_worker_and_electron_ui() {
    let root = std::env::temp_dir().join(format!(
        "evohime-updater-package-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary updater package directory");
    let archive_path = root.join("updater.zip");
    let file = fs::File::create(&archive_path).expect("create updater archive");
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    let mut worker = vec![0u8; 0x5a];
    worker[..2].copy_from_slice(b"MZ");
    worker[0x3c..0x40].copy_from_slice(&(0x40u32).to_le_bytes());
    worker[0x40..0x44].copy_from_slice(b"PE\0\0");
    worker[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes());
    worker[0x54..0x56].copy_from_slice(&2u16.to_le_bytes());
    worker[0x58..0x5a].copy_from_slice(&0x20bu16.to_le_bytes());
    for (name, content) in [
        ("updater/EvoHimeUpdater.exe", b"ui".as_slice()),
        ("updater/resources/app.asar", b"asar".as_slice()),
    ] {
        archive
            .start_file(name, options)
            .expect("start updater entry");
        archive.write_all(content).expect("write updater entry");
    }
    archive
        .start_file("evohime-updater.exe", options)
        .expect("start updater worker entry");
    archive
        .write_all(&worker)
        .expect("write updater worker entry");
    archive.finish().expect("finish updater archive");

    super::extract_updater_package(&archive_path, &root.join("destination"))
        .expect("complete updater package must extract");
    assert!(root.join("destination/evohime-updater.exe").is_file());
    assert!(root
        .join("destination/updater/EvoHimeUpdater.exe")
        .is_file());
    fs::remove_dir_all(root).expect("remove temporary updater package directory");
}

#[test]
fn archive_entry_count_is_bounded() {
    let root = std::env::temp_dir().join(format!(
        "evohime-archive-entries-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary archive directory");
    let archive_path = root.join("many-entries.zip");
    let file = fs::File::create(&archive_path).expect("create archive");
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for index in 0..=super::MAX_ARCHIVE_ENTRIES {
        archive
            .start_file(format!("entry-{index}"), options)
            .expect("start archive entry");
    }
    archive.finish().expect("finish archive");

    let error = super::extract_ui_bundle(&archive_path, &root.join("destination"))
        .expect_err("archives with too many entries must be rejected");
    assert!(error.contains("слишком много записей"));
    fs::remove_dir_all(root).expect("remove temporary archive directory");
}

#[test]
fn duplicate_archive_paths_are_rejected() {
    let root = std::env::temp_dir().join(format!(
        "evohime-archive-duplicate-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary archive directory");
    let archive_path = root.join("duplicate.zip");
    let file = fs::File::create(&archive_path).expect("create archive");
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    archive
        .start_file("index.html", options)
        .expect("start first entry");
    archive.write_all(b"first").expect("write first entry");
    archive
        .start_file("./index.html", options)
        .expect("start duplicate entry");
    archive.write_all(b"second").expect("write duplicate entry");
    archive.finish().expect("finish archive");

    let error = super::extract_ui_bundle(&archive_path, &root.join("destination"))
        .expect_err("duplicate archive paths must be rejected");
    assert!(error.contains("повторяющийся путь"));
    fs::remove_dir_all(root).expect("remove temporary archive directory");
}

#[test]
fn archive_paths_reject_windows_only_separators_and_streams() {
    assert!(super::normalize_archive_path(Path::new(r"assets\\app.js")).is_none());
    assert!(super::normalize_archive_path(Path::new("assets/app.js:stream")).is_none());
}

#[test]
fn failed_listener_runtime_update_removes_staging() {
    let root = std::env::temp_dir().join(format!(
        "evohime-runtime-cleanup-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    let staging = root.join("update-staging/listener-runtime");
    fs::create_dir_all(&staging).expect("create runtime staging");
    fs::write(staging.join("partial.bin"), b"partial").expect("write partial runtime");

    let result = cleanup_failed_staging(&staging, Err("download failed".to_owned()));
    assert!(result.is_err());
    assert!(!staging.exists());
    fs::remove_dir_all(root).expect("remove temporary runtime directory");
}

#[test]
fn successful_update_cleans_staging_unless_bootstrap_needs_it() {
    let root = std::env::temp_dir().join(format!(
        "evohime-staging-cleanup-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    let staging = root.join("update-staging");
    fs::create_dir_all(&staging).expect("create staging");
    super::cleanup_completed_staging(&staging, false);
    assert!(!staging.exists());

    fs::create_dir_all(&staging).expect("recreate staging");
    super::cleanup_completed_staging(&staging, true);
    assert!(staging.exists());
    fs::remove_dir_all(root).expect("remove temporary staging directory");
}

#[test]
fn bootstrap_failure_cleanup_removes_script_and_marker() {
    let root = std::env::temp_dir().join(format!(
        "evohime-bootstrap-cleanup-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create bootstrap state directory");
    let script = root.join("updater-bootstrap.cmd");
    let marker = root.join("updater-relaunch.pending");
    fs::write(&script, b"script").expect("write script");
    fs::write(&marker, b"pending").expect("write marker");

    super::cleanup_bootstrap_files(&script, &marker);
    assert!(!script.exists());
    assert!(!marker.exists());
    fs::remove_dir_all(root).expect("remove temporary bootstrap directory");
}

#[test]
fn manifest_merge_removes_temporary_file_after_rename_failure() {
    let root = std::env::temp_dir().join(format!(
        "evohime-merge-cleanup-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temporary install directory");
    fs::write(
        root.join("evohime.components.json"),
        br#"{"components":[]}"#,
    )
    .expect("write manifest");
    let destination = root.join("blocked.json");
    fs::create_dir(&destination).expect("create blocking destination");

    assert!(merge_installed_manifest_to(&root, &[], &destination).is_err());
    assert!(!root.join("blocked.json.tmp").exists());
    fs::remove_dir_all(root).expect("remove temporary install directory");
}

#[test]
fn compatible_manifest_accepts_nullable_string_arrays_from_github() {
    let manifest: CompatibleManifest = parse_json_body(
        r#"{
                "schema":"evohime.compatible-set.v1",
                "product":"EvoHime",
                "os":"windows",
                "architecture":"x64",
                "updater":{"minimum_version":"1.0.0","update_first":true},
                "components":[{
                    "id":"listener-runtime",
                    "version":"1.0.0",
                    "release_tag":"module-listener-runtime-v1.0.0",
                    "manifest_asset":"listener-runtime.json",
                    "artifact":null,
                    "size":0,
                    "sha256":"",
                    "dependencies":null,
                    "restart":"listener",
                    "changes":null
                }]
            }"#,
        "манифест совместимого комплекта",
    )
    .expect("nullable arrays must be accepted");

    assert!(manifest.components[0].dependencies.is_empty());
    assert!(manifest.components[0].changes.is_empty());
}

#[test]
fn compatible_manifest_binds_components_to_exact_release_tags() {
    let mut manifest = CompatibleManifest {
        schema: "evohime.compatible-set.v1".into(),
        product: "EvoHime".into(),
        os: "windows".into(),
        architecture: "x64".into(),
        updater: UpdaterRequirement {
            minimum_version: "1.0.0".into(),
            update_first: true,
        },
        components: vec![
            CompatibleComponent {
                id: "updater".into(),
                version: "1.1.0".into(),
                release_tag: "module-updater-v1.1.0".into(),
                manifest_asset: "updater.manifest.json".into(),
                artifact: Some("updater.zip".into()),
                size: 10,
                sha256: "a".repeat(64),
                dependencies: vec![],
                restart: "updater".into(),
                protocol: "desktop-ipc-v1".into(),
                summary: String::new(),
                changes: vec![],
            },
            CompatibleComponent {
                id: "core".into(),
                version: "1.0.0".into(),
                release_tag: "module-core-v1.0.0".into(),
                manifest_asset: "core.manifest.json".into(),
                artifact: Some("evohime-core.exe".into()),
                size: 10,
                sha256: "b".repeat(64),
                dependencies: vec!["updater".into()],
                restart: "core".into(),
                protocol: "desktop-ipc-v1".into(),
                summary: String::new(),
                changes: vec![],
            },
        ],
    };
    for (index, id) in super::MODULE_IDS.iter().enumerate() {
        if manifest
            .components
            .iter()
            .any(|component| component.id == *id)
        {
            continue;
        }
        let version = format!("1.0.{}", index + 1);
        manifest.components.push(CompatibleComponent {
            id: (*id).into(),
            version: version.clone(),
            release_tag: format!("module-{id}-v{version}"),
            manifest_asset: if *id == "listener-runtime" {
                "listener-runtime.json".into()
            } else {
                format!("{id}.manifest.json")
            },
            artifact: (*id != "listener-runtime").then(|| format!("{id}.bin")),
            size: if *id == "listener-runtime" { 0 } else { 10 },
            sha256: if *id == "listener-runtime" {
                String::new()
            } else {
                "c".repeat(64)
            },
            dependencies: vec![],
            restart: "module".into(),
            protocol: "desktop-ipc-v1".into(),
            summary: String::new(),
            changes: vec![],
        });
    }
    assert!(validate_compatible_manifest(&manifest).is_ok());

    let dependency_index = manifest
        .components
        .iter()
        .position(|component| component.id == "core")
        .expect("core component");
    manifest.components[dependency_index].dependencies = vec!["updater".into(), "updater".into()];
    assert!(validate_compatible_manifest(&manifest).is_err());
    manifest.components[dependency_index].dependencies = vec!["core".into()];
    assert!(validate_compatible_manifest(&manifest).is_err());
    manifest.components[dependency_index].dependencies = vec!["updater".into()];

    manifest.components[0].artifact = Some("core.exe:stream".into());
    assert!(validate_compatible_manifest(&manifest).is_err());
    manifest.components[0].artifact = Some("core.bin".into());
    manifest.components[0].manifest_asset = "core.manifest.json:stream".into();
    assert!(validate_compatible_manifest(&manifest).is_err());
    manifest.components[0].manifest_asset = "core.manifest.json".into();

    manifest.components[0].summary = "x".repeat(super::MAX_COMPATIBLE_SUMMARY_BYTES + 1);
    assert!(validate_compatible_manifest(&manifest).is_err());
    manifest.components[0].summary.clear();
    manifest.components[0].changes = vec!["x".repeat(super::MAX_COMPATIBLE_CHANGE_BYTES + 1)];
    assert!(validate_compatible_manifest(&manifest).is_err());
    manifest.components[0].changes.clear();
    manifest.components[0].dependencies =
        vec!["x".repeat(super::MAX_COMPATIBLE_DEPENDENCY_BYTES + 1)];
    assert!(validate_compatible_manifest(&manifest).is_err());
    manifest.components[0].dependencies.clear();

    manifest.components[0].size = super::MAX_UPDATE_ARTIFACT_BYTES + 1;
    assert!(validate_compatible_manifest(&manifest).is_err());
    manifest.components[0].size = 10;

    let mut invalid = manifest;
    invalid.components[1].release_tag = "module-supervisor-v1.0.0".into();
    assert!(validate_compatible_manifest(&invalid).is_err());
}

#[test]
fn oldest_base_updates_only_the_updater_first() {
    let updates = vec![
        UpdateCandidate {
            module: "core".into(),
            installed: "0.0.0".into(),
            available: "2.0.0".into(),
            summary: String::new(),
            changes: vec![],
            dependencies: vec![],
            restart: "core".into(),
            artifact: "evohime-core.exe".into(),
            size: 1,
            sha256: "a".repeat(64),
            download_url: "https://github.com/example/core".into(),
        },
        UpdateCandidate {
            module: "updater".into(),
            installed: "0.0.0".into(),
            available: "2.0.0".into(),
            summary: String::new(),
            changes: vec![],
            dependencies: vec![],
            restart: "updater".into(),
            artifact: "updater.zip".into(),
            size: 1,
            sha256: "b".repeat(64),
            download_url: "https://github.com/example/updater".into(),
        },
    ];
    let selected = updater_first_if_required(updates, "0.0.0", "1.0.0", true).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].module, "updater");
}

#[test]
fn update_config_accepts_the_bom_written_by_older_installers() {
    let path = std::env::temp_dir().join(format!(
        "evohime-update-config-{}.json",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::write(&path, b"\xef\xbb\xbf{\"enabled\":true}").expect("write update config");

    let config = read_update_config(&path).expect("read BOM-prefixed update config");
    let _ = fs::remove_file(&path);

    assert_eq!(config["enabled"], true);
}

#[test]
fn update_config_rejects_an_oversized_local_json_file() {
    let path = std::env::temp_dir().join(format!(
        "evohime-update-config-large-{}.json",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos()
    ));
    fs::write(&path, vec![b' '; super::MAX_LOCAL_JSON_BYTES + 1])
        .expect("write oversized update config");

    let error = read_update_config(&path).expect_err("oversized config must be rejected");
    let _ = fs::remove_file(&path);

    assert!(error.contains("read limit"));
}

#[test]
fn github_token_resolution_uses_the_documented_precedence() {
    let config = serde_json::json!({"githubToken": "config_token_1234567890"});
    let token = resolve_github_token_with(
        &config,
        |name| match name {
            "EVOHIME_UPDATE_GITHUB_TOKEN" => Some("explicit_token_1234567890".to_owned()),
            "GH_TOKEN" => Some("ambient_token_1234567890".to_owned()),
            _ => None,
        },
        || Some("gh_token_1234567890".to_owned()),
    );
    assert_eq!(token.as_deref(), Some("explicit_token_1234567890"));

    let token =
        resolve_github_token_with(&config, |_| None, || Some("gh_token_1234567890".to_owned()));
    assert_eq!(token.as_deref(), Some("config_token_1234567890"));

    let token = resolve_github_token_with(
        &serde_json::json!({}),
        |name| (name == "GH_TOKEN").then(|| "ambient_token_1234567890".to_owned()),
        || Some("gh_token_1234567890".to_owned()),
    );
    assert_eq!(token.as_deref(), Some("ambient_token_1234567890"));

    let token = resolve_github_token_with(
        &serde_json::json!({}),
        |_| None,
        || Some("gh_token_1234567890".to_owned()),
    );
    assert_eq!(token.as_deref(), Some("gh_token_1234567890"));
}

#[test]
fn github_token_normalization_rejects_invalid_values() {
    assert_eq!(normalize_github_token(Some(" too-short ")), None);
    assert_eq!(
        normalize_github_token(Some("token with spaces 1234567890")),
        None
    );
    assert_eq!(
        normalize_github_token(Some(" valid_token_1234567890 ")).as_deref(),
        Some("valid_token_1234567890")
    );
}

#[test]
fn github_authorization_is_limited_to_api_host() {
    assert!(is_github_api_url(
        "https://api.github.com/repos/example/project/releases"
    ));
    assert!(!is_github_api_url(
        "https://github.com/example/project/releases/download/v1/file.zip"
    ));
    assert!(!is_github_api_url(
        "http://api.github.com/repos/example/project"
    ));

    let client = updater_http_client(Some("token_1234567890".to_owned())).expect("client");
    let api_request = client
        .get("https://api.github.com/repos/example/project/releases")
        .build()
        .expect("API request");
    assert_eq!(
        api_request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer token_1234567890")
    );

    let asset_request = client
        .get("https://github.com/example/project/releases/download/v1/file.zip")
        .build()
        .expect("asset request");
    assert!(asset_request
        .headers()
        .get(reqwest::header::AUTHORIZATION)
        .is_none());
}

#[test]
fn release_asset_urls_are_limited_to_github_download_paths() {
    assert!(is_github_release_asset_url(
        "https://github.com/example/project/releases/download/v1/file.zip"
    ));
    assert!(!is_github_release_asset_url(
        "https://example.com/example/project/releases/download/v1/file.zip"
    ));
    assert!(!is_github_release_asset_url(
        "https://github.com/example/project/archive/v1/file.zip"
    ));
    assert!(!is_github_release_asset_url(
        "http://github.com/example/project/releases/download/v1/file.zip"
    ));
    assert!(!is_github_release_asset_url(
        "https://github.com:8443/example/project/releases/download/v1/file.zip"
    ));
    assert!(!is_github_release_asset_url(
        "https://user@github.com/example/project/releases/download/v1/file.zip"
    ));
}

#[test]
fn redirects_are_limited_to_trusted_github_hosts() {
    assert!(is_trusted_github_url(
        &"https://api.github.com/repos/example/project"
            .parse()
            .unwrap()
    ));
    assert!(is_trusted_github_url(
        &"https://release-assets.githubusercontent.com/file"
            .parse()
            .unwrap()
    ));
    assert!(is_trusted_github_url(
        &"https://release-assets.githubusercontent.com/file?X-Amz-Signature=signed"
            .parse()
            .unwrap()
    ));
    assert!(!is_trusted_github_url(
        &"https://github.com.evil.example/file".parse().unwrap()
    ));
    assert!(!is_trusted_github_url(
        &"http://github.com/file".parse().unwrap()
    ));
    assert!(!is_trusted_github_url(
        &"https://api.github.com/repos/example/project?redirect=evil"
            .parse()
            .unwrap()
    ));
}

#[test]
fn runtime_manifest_rejects_duplicate_file_names() {
    let entry = || RuntimeReleaseEntry {
        name: "models/base.bin".into(),
        size: 1,
        sha256: "a".repeat(64),
    };
    let manifest = RuntimeReleaseManifest {
        schema: 1,
        module: "listener-runtime".into(),
        version: "1.0.0".into(),
        abi: serde_json::json!({}),
        files: vec![entry()],
        models: vec![entry()],
    };
    let error = validate_runtime_manifest(&manifest)
        .expect_err("duplicate runtime file names must be rejected");
    assert!(error.contains("повторная запись"));

    let mut safe_manifest = manifest;
    safe_manifest.models[0].name = "models/other.bin".into();
    assert!(validate_runtime_manifest(&safe_manifest).is_ok());
    safe_manifest.files[0].name = "models/base.bin:stream".into();
    assert!(validate_runtime_manifest(&safe_manifest).is_err());
    safe_manifest.files[0].name = "models/".into();
    assert!(validate_runtime_manifest(&safe_manifest).is_err());
}

#[test]
fn updater_bootstrap_validates_new_worker_before_removing_backup() {
    let updater = Path::new(r"C:\Program Files\EvoHime\evohime-updater.exe");
    let updater_ui_dir = Path::new(r"C:\Program Files\EvoHime\updater");
    let legacy_updater_ui = Path::new(r"C:\Program Files\EvoHime\EvoHimeUpdater.exe");
    let staged =
        Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-staging\evohime-updater.exe.next");
    let staged_ui_dir = Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-staging\updater");
    let staged_package =
        Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-staging\updater.zip");
    let installed_package = Path::new(r"C:\Program Files\EvoHime\updater.zip");
    let backup =
        Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-state\updater-previous.exe");
    let ui_backup =
        Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-state\updater-previous");
    let package_backup = Path::new(
        r"C:\Users\Roman\AppData\Local\EvoHime\update-state\updater-package-previous.zip",
    );
    let legacy_backup =
        Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-state\updater-legacy-previous.exe");
    let manifest = Path::new(r"C:\Program Files\EvoHime\evohime.components.json");
    let manifest_next = Path::new(r"C:\Program Files\EvoHime\evohime.components.json.next");
    let manifest_backup =
        Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-state\components-previous.json");
    let marker =
        Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-state\updater-relaunch.pending");
    let paths = UpdaterBootstrapPaths {
        updater,
        updater_ui_dir,
        legacy_updater_ui,
        staged,
        staging_dir: Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-staging"),
        staged_ui_dir,
        staged_package,
        installed_package,
        backup,
        ui_backup,
        package_backup,
        legacy_backup,
        manifest,
        manifest_next,
        manifest_backup,
        marker,
        install_dir: Path::new(r"C:\Program Files\EvoHime"),
        wait_pid: Some(4242),
    };
    let script = updater_bootstrap_script(42, &paths);

    let verify = script
        .find("--check --install-dir")
        .expect("new worker check");
    let cleanup_backup = script
        .rfind("del /Q \"%BACKUP%\" \"%PACKAGE_BACKUP%\"")
        .expect("backup cleanup");
    assert!(verify < cleanup_backup);
    assert!(script.contains(":restore_manifest"));
    assert!(script.contains("if exist \"%MANIFEST%\" copy /Y \"%MANIFEST%\" \"%MANIFEST_BACKUP%\""));
    assert!(script.contains("if exist \"%MANIFEST%\" del /Q \"%MANIFEST%\""));
    assert!(script
        .contains("if exist \"%MANIFEST_BACKUP%\" move /Y \"%MANIFEST_BACKUP%\" \"%MANIFEST%\""));
    assert!(script.contains("move /Y \"%BACKUP%\" \"%UPDATER%\""));
    assert!(script.contains("%STAGED_UI_DIR%\\EvoHimeUpdater.exe"));
    assert!(script.contains("%UPDATER_UI_DIR%\\EvoHimeUpdater.exe"));
    assert!(script.contains("%STAGED_PACKAGE%"));
    assert!(script.contains("set \"COMMITTED=0\""));
    assert!(script.contains("set \"UI_PID=4242\""));
    assert!(script.contains(":wait_ui"));
    assert!(script.contains("PID eq %UI_PID%"));
    assert!(script.contains("if \"%COMMITTED%\"==\"1\" rmdir /S /Q \"%STAGING_DIR%\""));
    assert!(script.contains("Program Files"));
    assert!(script.contains("set \"UPDATER=C:\\Program Files\\EvoHime\\evohime-updater.exe\""));
    assert!(!script.contains("set \"UPDATER=\"C:\\Program Files"));
}
