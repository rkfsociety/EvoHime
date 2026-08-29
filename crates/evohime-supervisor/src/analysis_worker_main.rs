//! Minimal allowlisted analysis worker process.
//!
//! It has no filesystem, network, shell or credential API. The supervisor
//! starts it with fixed arguments inside a separate Job Object; the protocol
//! below is intentionally pure and line-bounded for the first runtime slice.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
struct Request {
    request_id: String,
    operation: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct Response {
    request_id: String,
    status: &'static str,
    result: Option<serde_json::Value>,
    error_class: Option<&'static str>,
}

fn main() -> io::Result<()> {
    if std::env::args().any(|arg| arg == "--help") {
        println!("evohime-analysis-worker --protocol-version=1 --runtime=trusted-local-1");
        return Ok(());
    }
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line?;
        if line.len() > MAX_LINE_BYTES {
            write_response(
                &mut stdout,
                Response {
                    request_id: String::new(),
                    status: "error",
                    result: None,
                    error_class: Some("request_too_large"),
                },
            )?;
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => execute(request),
            Err(_) => Response {
                request_id: String::new(),
                status: "error",
                result: None,
                error_class: Some("invalid_request"),
            },
        };
        write_response(&mut stdout, response)?;
    }
    Ok(())
}

fn execute(request: Request) -> Response {
    let request_id = request.request_id;
    let result = match request.operation.as_str() {
        "json_parse" => Some(request.args),
        "json_select" => {
            let Some(value) = request.args.get("value") else {
                return Response {
                    request_id,
                    status: "error",
                    result: None,
                    error_class: Some("invalid_select"),
                };
            };
            let Some(path) = request
                .args
                .get("path")
                .and_then(serde_json::Value::as_array)
            else {
                return Response {
                    request_id,
                    status: "error",
                    result: None,
                    error_class: Some("invalid_select"),
                };
            };
            let mut selected = value;
            for segment in path {
                let Some(key) = segment.as_str() else {
                    return Response {
                        request_id,
                        status: "error",
                        result: None,
                        error_class: Some("invalid_select"),
                    };
                };
                let Some(next) = selected.get(key) else {
                    return Response {
                        request_id,
                        status: "error",
                        result: None,
                        error_class: Some("path_not_found"),
                    };
                };
                selected = next;
            }
            Some(selected.clone())
        }
        "csv_summary" => {
            let Some(text) = request.args.as_str() else {
                return Response {
                    request_id,
                    status: "error",
                    result: None,
                    error_class: Some("invalid_csv"),
                };
            };
            let mut lines = text.lines();
            Some(
                serde_json::json!({ "columns": lines.next().map_or(0, |line| line.split(',').count()), "rows": lines.count() }),
            )
        }
        "filesystem" | "network" | "shell" | "credentials" | "tool_request" | "artifact_read" => {
            return Response {
                request_id,
                status: "error",
                result: None,
                error_class: Some("host_request_required"),
            }
        }
        _ => {
            return Response {
                request_id,
                status: "error",
                result: None,
                error_class: Some("unsupported_operation"),
            }
        }
    };
    Response {
        request_id,
        status: "ok",
        result,
        error_class: None,
    }
}

fn write_response(writer: &mut impl Write, response: Response) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, &response).map_err(io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()
}
