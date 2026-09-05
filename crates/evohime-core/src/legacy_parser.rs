use evohime_model_gateway::NativeToolCall;
use std::borrow::Cow;

pub(crate) const LEGACY_TOOL_NAMES: &[&str] = &[
    "agent.run",
    "filesystem.list",
    "filesystem.read",
    "filesystem.search",
    "filesystem.write",
    "filesystem.patch",
    "shell.execute",
    "git.status",
    "git.diff",
    "git.commit",
    "git.pull",
    "git.push",
    "git.log",
    "git.show",
    "git.blame",
    "git.changed_files",
    "mcp.call",
    "memory.search",
    "browser.open",
    "browser.extract",
    "browser.session.navigate",
    "browser.session.read",
    "browser.session.click",
    "browser.session.screenshot",
    "browser.session.type",
    "browser.session.close",
    "http.fetch",
];

fn is_supported_tool_name(name: &str) -> bool {
    LEGACY_TOOL_NAMES.contains(&name)
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn attribute_value(tag: &str, attribute: &str) -> Option<String> {
    let marker = format!("{attribute}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('\"')? + start;
    Some(xml_unescape(&tag[start..end]))
}

/// Markers that begin a tool call a model printed as text instead of using the
/// provider's tool-call field.
const TOOL_CALL_MARKERS: [&str; 6] = [
    "<function_calls>",
    "<invoke",
    "<tool_call>",
    "<tool_use>",
    "```json",
    "```tool",
];

/// The part of a model reply a person should read.
///
/// Models without native tool calling print the call itself into the message,
/// so the prose ends where the first call begins. Sending the raw content to
/// the shell would put XML in the middle of the conversation.
pub fn visible_agent_text<'a>(content: &'a str) -> Cow<'a, str> {
    let cut = TOOL_CALL_MARKERS
        .iter()
        .filter_map(|marker| content.find(marker))
        .min()
        .unwrap_or(content.len());
    Cow::Borrowed(content[..cut].trim())
}

pub(crate) fn parse_legacy_function_calls(content: &str, iteration: usize) -> Vec<NativeToolCall> {
    let mut calls = Vec::new();
    let mut json_cursor = 0;
    while let Some(relative_start) = content[json_cursor..].find("<function_calls>") {
        let start = json_cursor + relative_start + "<function_calls>".len();
        let Some(relative_end) = content[start..].find("</function_calls>") else {
            break;
        };
        let end = start + relative_end;
        let body = content[start..end].trim();
        if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(body) {
            for item in items {
                let Some(name) = item
                    .get("tool_name")
                    .or_else(|| item.get("name"))
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                if !is_supported_tool_name(name) {
                    continue;
                }
                let arguments = item
                    .get("arguments")
                    .or_else(|| item.get("parameters"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                calls.push(NativeToolCall {
                    id: format!("legacy-json-{iteration}-{}", calls.len()),
                    name: name.to_string(),
                    arguments: arguments.to_string(),
                });
            }
        }
        json_cursor = end + "</function_calls>".len();
    }

    let mut cursor = 0;
    while let Some(relative_start) = content[cursor..].find("<invoke") {
        let start = cursor + relative_start;
        let Some(tag_end_relative) = content[start..].find('>') else {
            break;
        };
        let tag_end = start + tag_end_relative;
        let Some(name) = attribute_value(&content[start..=tag_end], "name") else {
            cursor = tag_end + 1;
            continue;
        };
        let supported = is_supported_tool_name(&name);
        let body_start = tag_end + 1;
        let Some(body_end_relative) = content[body_start..].find("</invoke>") else {
            break;
        };
        let body_end = body_start + body_end_relative;
        let body = &content[body_start..body_end];
        let mut arguments = serde_json::Map::new();
        let mut parameter_cursor = 0;
        while let Some(relative_parameter) = body[parameter_cursor..].find("<parameter") {
            let parameter_start = parameter_cursor + relative_parameter;
            let Some(parameter_tag_end_relative) = body[parameter_start..].find('>') else {
                break;
            };
            let parameter_tag_end = parameter_start + parameter_tag_end_relative;
            let tag = &body[parameter_start..=parameter_tag_end];
            let Some(parameter_name) = attribute_value(tag, "name") else {
                parameter_cursor = parameter_tag_end + 1;
                continue;
            };
            let value_start = parameter_tag_end + 1;
            let Some(value_end_relative) = body[value_start..].find("</parameter>") else {
                break;
            };
            let value_end = value_start + value_end_relative;
            arguments.insert(
                parameter_name,
                serde_json::Value::String(xml_unescape(body[value_start..value_end].trim())),
            );
            parameter_cursor = value_end + "</parameter>".len();
        }
        if supported {
            calls.push(NativeToolCall {
                id: format!("legacy-{iteration}-{}", calls.len()),
                name,
                arguments: serde_json::Value::Object(arguments).to_string(),
            });
        }
        cursor = body_end + "</invoke>".len();
    }
    calls
}

pub(crate) fn parse_natural_tool_intent(content: &str, iteration: usize) -> Option<NativeToolCall> {
    let lower = content.to_lowercase();
    let explicit_action = ["вызываю", "вызову", "вызвать", "запрашиваю"]
        .iter()
        .any(|marker| lower.contains(marker));
    let name = LEGACY_TOOL_NAMES
        .iter()
        .find(|candidate| content.contains(**candidate))
        .copied()?;

    if let Some(json_body) = content
        .split("```json")
        .nth(1)
        .and_then(|body| body.split("```").next())
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body.trim()).ok())
    {
        let arguments = json_body
            .get("arguments")
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or(json_body);
        let arguments = if name == "filesystem.search" || arguments.get("path").is_some() {
            arguments
        } else {
            serde_json::json!({ "path": "." })
        };
        return Some(NativeToolCall {
            id: format!("natural-{iteration}"),
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }

    if !explicit_action {
        return None;
    }

    let path = content
        .split('`')
        .nth(1)
        .filter(|value| !value.contains('.'))
        .unwrap_or(if name == "filesystem.list" { "." } else { "" });
    let arguments = match name {
        "filesystem.list" | "filesystem.read" => serde_json::json!({ "path": path }),
        "filesystem.search" => serde_json::json!({ "query": path }),
        _ => return None,
    };
    Some(NativeToolCall {
        id: format!("natural-{iteration}"),
        name: name.to_string(),
        arguments: arguments.to_string(),
    })
}

pub(crate) fn parse_tagged_tool_call(content: &str, iteration: usize) -> Option<NativeToolCall> {
    if let Some(start) = content.find("<tool_code>") {
        let body_start = start + "<tool_code>".len();
        let body_end = content[body_start..].find("</tool_code>")? + body_start;
        let wrapped = format!(
            "<tool_call>{}</tool_call>",
            content[body_start..body_end].trim()
        );
        return parse_tagged_tool_call(&wrapped, iteration);
    }
    if let (Some(name_start), Some(input_start)) =
        (content.find("<tool_name>"), content.find("<tool_input>"))
    {
        let name_start = name_start + "<tool_name>".len();
        let name_end = content[name_start..].find("</tool_name>")? + name_start;
        let input_start = input_start + "<tool_input>".len();
        let input_end = content[input_start..].find("</tool_input>")? + input_start;
        let name = content[name_start..name_end].trim();
        if !is_supported_tool_name(name) {
            return None;
        }
        let arguments =
            serde_json::from_str::<serde_json::Value>(content[input_start..input_end].trim())
                .ok()?;
        return Some(NativeToolCall {
            id: format!("tagged-{iteration}"),
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }
    let start_marker = "<tool_call>";
    let end_marker = "</tool_call>";
    let start = content.find(start_marker)? + start_marker.len();
    let end = content[start..].find(end_marker)? + start;
    let body = content[start..end].trim();
    let (name, arguments) = if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        let name = value.get("name")?.as_str()?.to_string();
        let arguments = value
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        (name, arguments)
    } else {
        let open = body.find('(')?;
        let name = body[..open].trim().to_string();
        let params = body[open + 1..].strip_suffix(')')?.trim();
        let mut arguments = serde_json::Map::new();
        for pair in params.split(',').filter(|pair| !pair.trim().is_empty()) {
            let (key, value) = pair.split_once('=')?;
            let value = value.trim().trim_matches('"').trim_matches('\'');
            arguments.insert(
                key.trim().to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
        (name, serde_json::Value::Object(arguments))
    };
    if !matches!(
        name.as_str(),
        "filesystem.list" | "filesystem.read" | "filesystem.search" | "git.status" | "git.diff"
    ) {
        return None;
    }
    Some(NativeToolCall {
        id: format!("tagged-{iteration}"),
        name,
        arguments: arguments.to_string(),
    })
}

pub(crate) fn parse_plain_tool_call(content: &str, iteration: usize) -> Option<NativeToolCall> {
    let name = LEGACY_TOOL_NAMES
        .iter()
        .find(|candidate| content.lines().any(|line| line.trim() == **candidate))
        .copied()?;
    let argument_keys = [
        "path",
        "query",
        "remote",
        "branch",
        "reference",
        "force",
        "command",
        "prompt",
        "model_route",
        "url",
        "method",
        "selector",
        "attribute",
        "text",
        "params",
        "max_steps",
        "timeout_ms",
        "max_chars",
        "limit",
        "max_count",
        "start_line",
        "end_line",
        "settle_ms",
        "full_page",
    ];
    let mut parsed_arguments = serde_json::Map::new();
    for (key, value) in content
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim(), value.trim()))
        .filter(|(key, _)| argument_keys.contains(key))
    {
        let json_value = match key {
            "force" | "full_page" => value
                .parse::<bool>()
                .map(serde_json::Value::Bool)
                .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
            "max_steps" | "timeout_ms" | "max_chars" | "limit" | "settle_ms" => value
                .parse::<u64>()
                .map(|number| serde_json::Value::Number(number.into()))
                .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
            "params" => serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
            _ => serde_json::Value::String(value.trim_matches(['"', '\'']).to_string()),
        };
        parsed_arguments.insert(key.to_string(), json_value);
    }
    if parsed_arguments.is_empty()
        && !name.starts_with("git.")
        && !matches!(name, "browser.session.read" | "browser.session.close")
    {
        return None;
    }
    let arguments = serde_json::Value::Object(parsed_arguments).to_string();
    Some(NativeToolCall {
        id: format!("plain-{iteration}"),
        name: name.to_string(),
        arguments: arguments.to_string(),
    })
}

pub(crate) fn parse_xml_named_tool_call(content: &str, iteration: usize) -> Option<NativeToolCall> {
    let name = LEGACY_TOOL_NAMES
        .iter()
        .find(|candidate| content.contains(&format!("<{}>", candidate)))
        .copied()?;
    let start_marker = format!("<{}>", name);
    let end_marker = format!("</{}>", name);
    let start = content.find(&start_marker)? + start_marker.len();
    let end = content[start..].find(&end_marker)? + start;
    let body = &content[start..end];
    if body.trim().is_empty() && matches!(name, "browser.session.read" | "browser.session.close") {
        return Some(NativeToolCall {
            id: format!("xml-{iteration}"),
            name: name.to_string(),
            arguments: "{}".to_string(),
        });
    }
    let parameter_start = body.find("<parameter")?;
    let tag_end = body[parameter_start..].find('>')? + parameter_start;
    let value_end = body[tag_end + 1..].find("</parameter>")? + tag_end + 1;
    let tag = &body[parameter_start..=tag_end];
    let parameter_name = attribute_value(tag, "name").or_else(|| {
        body[tag_end + 1..value_end]
            .split_once('>')
            .map(|(key, _)| key.trim().to_string())
    })?;
    let value = if tag.contains("name=") {
        body[tag_end + 1..value_end].trim().to_string()
    } else {
        body[tag_end + 1..value_end]
            .split_once('>')
            .map(|(_, value)| value.trim().to_string())?
    };
    Some(NativeToolCall {
        id: format!("xml-{iteration}"),
        name: name.to_string(),
        arguments: serde_json::json!({ parameter_name: value }).to_string(),
    })
}

pub(crate) fn strip_legacy_function_blocks(content: &str) -> String {
    let mut cleaned = String::with_capacity(content.len());
    let mut cursor = 0;
    while let Some(relative_start) = content[cursor..].find("<function_calls>") {
        let start = cursor + relative_start;
        cleaned.push_str(&content[cursor..start]);
        let block_start = start + "<function_calls>".len();
        let Some(relative_end) = content[block_start..].find("</function_calls>") else {
            break;
        };
        cursor = block_start + relative_end + "</function_calls>".len();
    }
    cleaned.push_str(&content[cursor..]);
    cleaned.trim().to_string()
}
