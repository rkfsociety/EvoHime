//! Tool-call text parsing helpers.

pub(crate) fn extract_code_block(value: &str) -> Option<String> {
    let start = value.find("```")?;
    let body_start = value[start..].find('\n').map(|offset| start + offset + 1)?;
    let end = value[body_start..].find("```")? + body_start;
    let body = &value[body_start..end];
    (!body.is_empty()).then(|| body.to_string())
}
