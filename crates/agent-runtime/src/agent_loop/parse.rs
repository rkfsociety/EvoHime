//! Plan / tool-call text parsing helpers.
use evohime_protocol::PlanStep;
use serde_json::Value;

pub fn parse_plan(raw: &str) -> Vec<PlanStep> {
    let normalized = unwrap_code_fence(raw);
    if normalized.is_empty() {
        return default_plan();
    }

    if let Some(plan) = parse_plan_json(&normalized) {
        return normalize_plan(plan);
    }
    if let Some(tool_calls) = parse_model_tool_calls(&normalized) {
        return tool_calls;
    }

    let source_lines = normalized.lines().collect::<Vec<_>>();
    let mut logical_lines = Vec::new();
    let mut index = 0;
    while index < source_lines.len() {
        let line = source_lines[index];
        let is_write_step = line.contains("filesystem.write") || line.contains("filesystem.patch");
        if is_write_step {
            let mut combined = line.to_string();
            let mut cursor = index + 1;
            let mut saw_code_fence = false;
            while cursor < source_lines.len() {
                let next = source_lines[cursor];
                combined.push('\n');
                combined.push_str(next);
                if next.trim_start().starts_with("```") {
                    if saw_code_fence {
                        cursor += 1;
                        break;
                    }
                    saw_code_fence = true;
                }
                cursor += 1;
                if cursor > index + 12 && !saw_code_fence {
                    break;
                }
            }
            logical_lines.push(combined);
            index = cursor;
        } else {
            logical_lines.push(line.to_string());
            index += 1;
        }
    }

    let parsed: Vec<PlanStep> = logical_lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| parse_plan_line(index, line))
        .collect();

    if parsed.is_empty() {
        parse_model_tool_calls(&normalized).unwrap_or_else(default_plan)
    } else {
        normalize_plan(parsed)
    }
}

pub(crate) const REGISTERED_TOOLS: &[&str] = &[
    "filesystem.read",
    "filesystem.list",
    "filesystem.search",
    "filesystem.write",
    "filesystem.patch",
    "shell.execute",
    "git.status",
    "git.diff",
    "git.commit",
    "git.pull",
    "git.push",
    "browser.open",
    "browser.extract",
    "mcp.call",
    "memory.search",
    "agent.run",
    "worker.run",
    "assistant.reply",
];
pub(crate) fn parse_markup_tool_calls(raw: &str) -> Option<Vec<PlanStep>> {
    let mut steps = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = raw[cursor..].find("<invoke name=\"") {
        let start = cursor + relative_start + "<invoke name=\"".len();
        let Some(relative_name_end) = raw[start..].find('\"') else {
            break;
        };
        let name_end = relative_name_end + start;
        let tool_name = &raw[start..name_end];
        let Some(relative_body_start) = raw[name_end..].find('>') else {
            break;
        };
        let body_start = relative_body_start + name_end + 1;
        let Some(relative_body_end) = raw[body_start..].find("</invoke>") else {
            break;
        };
        let body_end = relative_body_end + body_start;
        let body = &raw[body_start..body_end];
        let path = markup_parameter(body, "path");
        let content = markup_parameter(body, "content").or_else(|| markup_parameter(body, "patch"));
        let description = match (path, content) {
            (Some(path), Some(content)) => format!("path: {path}\n```\n{content}\n```"),
            (Some(path), None) => format!("path: {path}"),
            (None, _) => body.trim().to_string(),
        };
        steps.push(PlanStep {
            id: format!("step-{}", steps.len() + 1),
            tool_name: tool_name.to_string(),
            description,
            depends_on: Vec::new(),
        });
        cursor = body_end + "</invoke>".len();
    }
    (!steps.is_empty()).then(|| normalize_plan(steps))
}

pub(crate) fn parse_model_tool_calls(raw: &str) -> Option<Vec<PlanStep>> {
    parse_markup_tool_calls(raw)
        .or_else(|| parse_json_tool_calls(raw))
        .or_else(|| parse_tagged_tool_calls(raw))
}

pub(crate) fn parse_json_tool_calls(raw: &str) -> Option<Vec<PlanStep>> {
    let mut candidates = extract_json_blocks(raw);
    if candidates.is_empty() && serde_json::from_str::<Value>(raw.trim()).is_ok() {
        candidates.push(raw.trim().to_string());
    }
    let mut steps = Vec::new();

    for candidate in candidates {
        let values = match serde_json::from_str::<Value>(&candidate) {
            Ok(Value::Array(values)) => values,
            Ok(value) => vec![value],
            Err(_) => continue,
        };

        for value in values {
            let Some(object) = value.as_object() else {
                continue;
            };
            let Some(type_name) = object.get("type").and_then(Value::as_str) else {
                continue;
            };
            let (tool_name, input) = if type_name == "tool.call" {
                let Some(tool_name) = object.get("tool").and_then(Value::as_str) else {
                    continue;
                };
                let Some(input) = object.get("input").and_then(Value::as_object) else {
                    continue;
                };
                (tool_name.to_string(), input.clone())
            } else if is_supported_tool(type_name) {
                let input = object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "type")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                (type_name.to_string(), input)
            } else {
                continue;
            };

            let description = tool_call_description(&tool_name, &input);
            steps.push(PlanStep {
                id: format!("step-{}", steps.len() + 1),
                tool_name,
                description,
                depends_on: Vec::new(),
            });
        }
    }

    (!steps.is_empty()).then(|| normalize_plan(steps))
}

pub(crate) fn is_supported_tool(tool_name: &str) -> bool {
    REGISTERED_TOOLS
        .iter()
        .any(|name| *name == tool_name && *name != "assistant.reply")
}

pub(crate) fn parse_tagged_tool_calls(raw: &str) -> Option<Vec<PlanStep>> {
    let mut steps = Vec::new();
    let mut cursor = 0;
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    while let Some(relative_start) = raw[cursor..].find(OPEN) {
        let body_start = cursor + relative_start + OPEN.len();
        let Some(relative_end) = raw[body_start..].find(CLOSE) else {
            break;
        };
        let body_end = relative_end + body_start;
        let Ok(value) = serde_json::from_str::<Value>(raw[body_start..body_end].trim()) else {
            cursor = body_end + CLOSE.len();
            continue;
        };
        let Some(object) = value.as_object() else {
            cursor = body_end + CLOSE.len();
            continue;
        };
        let Some(tool_name) = object
            .get("tool")
            .and_then(Value::as_str)
            .or_else(|| object.get("type").and_then(Value::as_str))
        else {
            cursor = body_end + CLOSE.len();
            continue;
        };
        let input = object
            .get("input")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(|| {
                object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "type" && key.as_str() != "tool")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            });
        let description = tool_call_description(tool_name, &input);
        steps.push(PlanStep {
            id: format!("step-{}", steps.len() + 1),
            tool_name: tool_name.to_string(),
            description,
            depends_on: Vec::new(),
        });
        cursor = body_end + CLOSE.len();
    }

    (!steps.is_empty()).then(|| normalize_plan(steps))
}

pub(crate) fn tool_call_description(
    tool_name: &str,
    input: &serde_json::Map<String, Value>,
) -> String {
    match tool_name {
        "filesystem.write" => format!(
            "path: {}\n```\n{}\n```",
            input
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            input
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
        "filesystem.patch" => format!(
            "path: {}\n```\n{}\n```",
            input
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            input
                .get("patch")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
        _ => serde_json::to_string(input).unwrap_or_default(),
    }
}

pub(crate) fn extract_json_blocks(raw: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = raw[cursor..].find("```") {
        let start = cursor + relative_start;
        let Some(line_end) = raw[start..].find('\n') else {
            break;
        };
        let body_start = start + line_end + 1;
        let Some(relative_end) = raw[body_start..].find("```") else {
            break;
        };
        let language = raw[start + 3..start + line_end].trim();
        if language.is_empty() || language.eq_ignore_ascii_case("json") {
            blocks.push(
                raw[body_start..body_start + relative_end]
                    .trim()
                    .to_string(),
            );
        }
        cursor = body_start + relative_end + 3;
    }
    blocks
}

pub(crate) fn markup_parameter(body: &str, name: &str) -> Option<String> {
    let marker = format!("<parameter name=\"{name}\">");
    let start = body.find(&marker)? + marker.len();
    let end = body[start..].find("</parameter>")? + start;
    let value = body[start..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub(crate) fn extract_code_block(value: &str) -> Option<String> {
    let start = value.find("```")?;
    let body_start = value[start..].find('\n').map(|offset| start + offset + 1)?;
    let end = value[body_start..].find("```")? + body_start;
    let body = &value[body_start..end];
    (!body.is_empty()).then(|| body.to_string())
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PlanEnvelope {
    #[serde(default)]
    steps: Vec<PlanStepDraft>,
    #[serde(default)]
    plan: Vec<PlanStepDraft>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PlanStepDraft {
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) tool_name: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) depends_on: Vec<String>,
}

pub fn parse_plan_json(raw: &str) -> Option<Vec<PlanStep>> {
    serde_json::from_str::<Vec<PlanStep>>(raw).ok().or_else(|| {
        serde_json::from_str::<PlanEnvelope>(raw)
            .ok()
            .and_then(|envelope| {
                let drafts = if !envelope.steps.is_empty() {
                    envelope.steps
                } else {
                    envelope.plan
                };
                (!drafts.is_empty()).then(|| {
                    drafts
                        .into_iter()
                        .enumerate()
                        .map(|(index, draft)| PlanStep {
                            id: draft.id.unwrap_or_else(|| format!("step-{}", index + 1)),
                            tool_name: draft
                                .tool_name
                                .unwrap_or_else(|| "assistant.reply".to_string()),
                            description: draft
                                .description
                                .unwrap_or_else(|| format!("Execute step-{}", index + 1)),
                            depends_on: draft
                                .depends_on
                                .into_iter()
                                .filter(|dependency| !dependency.trim().is_empty())
                                .collect(),
                        })
                        .collect()
                })
            })
    })
}

pub(crate) fn unwrap_code_fence(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let mut lines = trimmed.lines();
    let first = lines.next().unwrap_or_default();
    if !first.starts_with("```") {
        return trimmed.to_string();
    }

    let body = lines
        .take_while(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");

    body.trim().to_string()
}
pub(crate) fn format_plan(plan: &[PlanStep]) -> String {
    plan.iter()
        .map(|step| {
            if step.depends_on.is_empty() {
                format!("{}: {} ({})", step.id, step.description, step.tool_name)
            } else {
                format!(
                    "{}: {} ({}) depends on {}",
                    step.id,
                    step.description,
                    step.tool_name,
                    step.depends_on.join(", ")
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn default_plan() -> Vec<PlanStep> {
    vec![
        PlanStep {
            id: "step-1".to_string(),
            tool_name: "filesystem.list".to_string(),
            description: "List workspace root `.`".to_string(),
            depends_on: Vec::new(),
        },
        PlanStep {
            id: "step-2".to_string(),
            tool_name: "assistant.reply".to_string(),
            description: "Write the response using the collected context".to_string(),
            depends_on: vec!["step-1".to_string()],
        },
    ]
}

pub(crate) fn normalize_plan(mut plan: Vec<PlanStep>) -> Vec<PlanStep> {
    if plan.is_empty() {
        return default_plan();
    }

    for (index, step) in plan.iter_mut().enumerate() {
        if step.id.trim().is_empty() {
            step.id = format!("step-{}", index + 1);
        }
        if step.tool_name.trim().is_empty() {
            step.tool_name = "assistant.reply".to_string();
        }
        if step.description.trim().is_empty() {
            step.description = format!("Execute {}", step.tool_name);
        }
        step.depends_on
            .retain(|dependency| !dependency.trim().is_empty());
    }

    plan
}

pub fn parse_plan_line(index: usize, line: &str) -> Option<PlanStep> {
    let mut text = line.trim().trim_matches('*').trim();
    if text.is_empty() {
        return None;
    }

    while let Some(stripped) = text.strip_prefix(|ch: char| ch == '-' || ch == '*' || ch == '•') {
        text = stripped.trim_start();
    }

    if let Some((numbered, rest)) = text.split_once('.') {
        if numbered.chars().all(|ch| ch.is_ascii_digit()) {
            text = rest.trim_start();
        }
    }

    if text.is_empty() {
        return None;
    }

    let (body, depends_on) = split_dependencies(text);
    let (tool_name, description) = extract_tool_and_description(body, index);

    // Plain prose from a model is not a plan step. The old fallback mapped
    // every unrecognised line to assistant.reply, producing dozens of fake
    // steps when the model ignored the JSON-only planning instruction.
    if !REGISTERED_TOOLS.contains(&tool_name.as_str()) {
        return None;
    }
    if tool_name == "assistant.reply" && !body.trim_start().starts_with("assistant.reply") {
        return None;
    }

    Some(PlanStep {
        id: format!("step-{}", index + 1),
        tool_name,
        description,
        depends_on,
    })
}

pub(crate) fn split_dependencies(text: &str) -> (&str, Vec<String>) {
    let lowered = text.to_lowercase();
    for marker in ["depends on", "after"] {
        if let Some(position) = lowered.find(marker) {
            let head = text[..position].trim().trim_end_matches([':', '-', ' ']);
            let tail = text[position + marker.len()..].trim();
            let depends_on = tail
                .split([',', ';'])
                .map(|item| item.trim().trim_start_matches("step ").trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
            return (head, depends_on);
        }
    }
    (text, Vec::new())
}

pub(crate) fn extract_tool_and_description(text: &str, index: usize) -> (String, String) {
    for tool_name in REGISTERED_TOOLS {
        if let Some(position) = text.find(tool_name) {
            let prefix = text[..position].trim();
            if prefix.starts_with("step-") || prefix.starts_with("**step-") {
                return (
                    (*tool_name).to_string(),
                    text[position + tool_name.len()..]
                        .trim_start_matches(['*', ':', '-', ' ', '\n'])
                        .to_string(),
                );
            }
        }
    }

    for separator in ["|", ":", " - ", " => "] {
        if let Some((left, right)) = text.split_once(separator) {
            let left = left.trim();
            let right = right.trim();
            if left.contains('.') || left.contains('_') || left == "assistant.reply" {
                return (left.to_string(), right.to_string());
            }
            if right.contains('.') && !left.contains('.') && index == 0 {
                return (right.to_string(), left.to_string());
            }
        }
    }

    let lower = text.to_lowercase();
    for tool_name in REGISTERED_TOOLS {
        if lower.starts_with(tool_name) {
            return (
                (*tool_name).to_string(),
                text[tool_name.len()..]
                    .trim_start_matches([':', '-', ' '])
                    .to_string(),
            );
        }
    }

    ("assistant.reply".to_string(), text.to_string())
}
