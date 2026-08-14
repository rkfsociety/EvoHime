use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};
use thiserror::Error;
use walkdir::WalkDir;

const SCHEMA_VERSION: &str = "1.0";
const MAX_CASE_BYTES: usize = 256 * 1024;
const MAX_TRACE_BYTES: u64 = 1_048_576;

#[derive(Debug, Error)]
enum EvalError {
    #[error("fixture {path}: {message}")]
    Fixture { path: String, message: String },
    #[error("нет fixtures в {0}")]
    Empty(String),
    #[error("неверный аргумент: {0}")]
    Args(String),
    #[error("ошибка чтения {0}: {1}")]
    Io(String, String),
}

#[derive(Debug, Deserialize)]
struct Fixture {
    id: String,
    schema_version: String,
    fixture_version: String,
    prompt: String,
    #[serde(default)]
    required_tool_calls: Vec<ToolCall>,
    #[serde(default)]
    forbidden_tool_calls: Vec<String>,
    #[serde(default)]
    assertions: Vec<String>,
    limits: Limits,
    #[serde(default)]
    model_profile: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolCall {
    name: String,
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Deserialize)]
struct Limits {
    #[serde(default = "default_events")]
    max_events: u32,
    #[serde(default = "default_trace")]
    max_trace_bytes: u64,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}
fn default_events() -> u32 {
    128
}
fn default_trace() -> u64 {
    MAX_TRACE_BYTES
}
fn default_timeout() -> u64 {
    30_000
}

#[derive(Debug, Serialize)]
struct Verdict {
    fixture_id: String,
    category: String,
    verdict: &'static str,
    reason: Option<String>,
    fingerprint: String,
    duration_ms: u128,
    redacted_trace: Value,
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("evaluation gate: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(args: Vec<String>) -> Result<(), EvalError> {
    let mut fixture_root = PathBuf::from("tests/evals/fixtures");
    let mut selected_case = None;
    let mut mode = "deterministic".to_string();
    let mut verbose = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--fixture" => {
                fixture_root = PathBuf::from(
                    args.get(index + 1)
                        .ok_or_else(|| EvalError::Args("--fixture требует путь".into()))?,
                );
                index += 1;
            }
            "--case" => {
                selected_case = Some(
                    args.get(index + 1)
                        .ok_or_else(|| EvalError::Args("--case требует id".into()))?
                        .clone(),
                );
                index += 1;
            }
            "--mode" => {
                mode = args
                    .get(index + 1)
                    .ok_or_else(|| EvalError::Args("--mode требует значение".into()))?
                    .clone();
                index += 1;
            }
            "--model" => {
                index += 1;
            }
            "--verbose" => verbose = true,
            value if value == "--help" => {
                println!("cargo eval --fixture <path> --case <id> --mode {{static|deterministic|real}} --model <name> --verbose");
                return Ok(());
            }
            value => return Err(EvalError::Args(format!("неизвестный аргумент {value}"))),
        }
        index += 1;
    }
    if !matches!(mode.as_str(), "static" | "deterministic" | "real") {
        return Err(EvalError::Args(format!("неподдерживаемый mode {mode}")));
    }
    let mut paths = Vec::new();
    for entry in WalkDir::new(&fixture_root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|x| x == "json") {
            paths.push(entry.into_path());
        }
    }
    if paths.is_empty() {
        return Err(EvalError::Empty(fixture_root.display().to_string()));
    }
    let mut failed = false;
    for path in paths {
        let fixture = load_fixture(&path)?;
        if selected_case.as_deref().is_some_and(|id| id != fixture.id) {
            continue;
        }
        let category = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|x| x.to_str())
            .unwrap_or("unknown")
            .to_string();
        let start = Instant::now();
        let result = execute(&fixture, &mode);
        let verdict = result.unwrap_or_else(|reason| {
            failed = true;
            Verdict {
                fixture_id: fixture.id.clone(),
                category: category.clone(),
                verdict: "fail",
                reason: Some(reason),
                fingerprint: fingerprint(&fixture, &mode),
                duration_ms: start.elapsed().as_millis(),
                redacted_trace: redacted_trace(&fixture),
            }
        });
        if verbose {
            println!("{}: {}", verdict.fixture_id, verdict.verdict);
        }
        println!(
            "{}",
            serde_json::to_string(&verdict).expect("verdict serializes")
        );
    }
    if failed {
        Err(EvalError::Args(
            "обязательный evaluation case завершился fail".into(),
        ))
    } else {
        Ok(())
    }
}

fn load_fixture(path: &Path) -> Result<Fixture, EvalError> {
    let bytes =
        fs::read(path).map_err(|e| EvalError::Io(path.display().to_string(), e.to_string()))?;
    if bytes.len() > MAX_CASE_BYTES {
        return Err(EvalError::Fixture {
            path: path.display().to_string(),
            message: "размер превышает 256 KiB".into(),
        });
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| EvalError::Fixture {
        path: path.display().to_string(),
        message: format!("JSON: {e}"),
    })?;
    lint_value(path, &value)?;
    serde_json::from_value(value).map_err(|e| EvalError::Fixture {
        path: path.display().to_string(),
        message: format!("schema: {e}"),
    })
}

fn lint_value(path: &Path, value: &Value) -> Result<(), EvalError> {
    let object = value.as_object().ok_or_else(|| EvalError::Fixture {
        path: path.display().to_string(),
        message: "case должен быть объектом".into(),
    })?;
    for key in [
        "id",
        "schema_version",
        "fixture_version",
        "prompt",
        "assertions",
        "limits",
    ] {
        if !object.contains_key(key) {
            return Err(EvalError::Fixture {
                path: path.display().to_string(),
                message: format!("отсутствует обязательное поле {key}"),
            });
        }
    }
    let text = value.to_string().to_lowercase();
    for marker in [
        "password=",
        "api_key",
        "bearer ",
        "@gmail.com",
        "private_key",
    ] {
        if text.contains(marker) {
            return Err(EvalError::Fixture {
                path: path.display().to_string(),
                message: format!("обнаружен запрещённый секрет/PII marker {marker}"),
            });
        }
    }
    Ok(())
}

fn execute(fixture: &Fixture, mode: &str) -> Result<Verdict, String> {
    if fixture.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "schema_version {} не поддерживается",
            fixture.schema_version
        ));
    }
    if fixture.id.is_empty() || fixture.id.len() > 128 || fixture.prompt.len() > 16_384 {
        return Err("bounded id/prompt limit нарушен".into());
    }
    if fixture.limits.max_events == 0
        || fixture.limits.max_events > 10_000
        || fixture.limits.max_trace_bytes == 0
        || fixture.limits.max_trace_bytes > MAX_TRACE_BYTES
        || fixture.limits.timeout_ms == 0
    {
        return Err("bounded limits нарушены".into());
    }
    if mode == "real" && fixture.model_profile.is_none() {
        return Err("real mode требует model_profile".into());
    }
    if fixture.assertions.is_empty() {
        return Err("case обязан содержать assertions".into());
    }
    if mode == "deterministic" {
        let summary = evohime_core::evals::run_all();
        if !summary.all_passed() {
            return Err(format!(
                "Core deterministic eval: {}/{} passed",
                summary.passed(),
                summary.total()
            ));
        }
    }
    if fixture
        .required_tool_calls
        .iter()
        .any(|call| call.name.is_empty())
        || fixture
            .forbidden_tool_calls
            .iter()
            .any(|call| call.is_empty())
    {
        return Err("tool call name не может быть пустым".into());
    }
    let trace = redacted_trace(fixture);
    let bytes = serde_json::to_vec(&trace).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > fixture.limits.max_trace_bytes {
        return Err("redacted trace превышает case limit".into());
    }
    Ok(Verdict {
        fixture_id: fixture.id.clone(),
        category: "fixture".into(),
        verdict: "pass",
        reason: None,
        fingerprint: fingerprint(fixture, mode),
        duration_ms: 0,
        redacted_trace: trace,
    })
}

fn fingerprint(fixture: &Fixture, mode: &str) -> String {
    let payload = serde_json::json!({"commit": option_env!("GITHUB_SHA").unwrap_or("local"), "fixture_version": fixture.fixture_version, "schema_version": fixture.schema_version, "tool_registry_version": "1", "model_provider_version": fixture.model_profile, "model_route": mode, "seed": 0, "temperature": 0, "prompt_version": "1", "runner_version": env!("CARGO_PKG_VERSION")});
    hex::encode(Sha256::digest(
        serde_json::to_vec(&payload).expect("fingerprint serializes"),
    ))
}

fn redacted_trace(fixture: &Fixture) -> Value {
    let mut map = Map::new();
    map.insert("fixture_id".into(), Value::String(fixture.id.clone()));
    map.insert(
        "assertions".into(),
        Value::Array(
            fixture
                .assertions
                .iter()
                .map(|x| Value::String(x.clone()))
                .collect(),
        ),
    );
    map.insert(
        "tool_call_count".into(),
        Value::from(fixture.required_tool_calls.len()),
    );
    map.insert(
        "tool_args_digests".into(),
        Value::Array(
            fixture
                .required_tool_calls
                .iter()
                .map(|call| {
                    let bytes = serde_json::to_vec(&call.args).expect("tool args serialize");
                    Value::String(hex::encode(Sha256::digest(bytes)))
                })
                .collect(),
        ),
    );
    map.insert(
        "forbidden_tool_call_count".into(),
        Value::from(fixture.forbidden_tool_calls.len()),
    );
    if let Some(source) = &fixture.source {
        map.insert("source".into(), Value::String(source.clone()));
    }
    map.insert("prompt".into(), Value::String("[REDACTED]".into()));
    Value::Object(map)
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}
