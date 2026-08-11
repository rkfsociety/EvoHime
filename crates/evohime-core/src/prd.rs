use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrdDiagnostic {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedPrdTask {
    pub source_ref: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedPrd {
    pub origin: String,
    pub version: String,
    pub source_text: String,
    pub tasks: Vec<ParsedPrdTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrdParseResult {
    pub document: Option<ParsedPrd>,
    pub diagnostics: Vec<PrdDiagnostic>,
}

pub fn parse_markdown_prd(source_text: &str, origin: &str, version: &str) -> PrdParseResult {
    let mut diagnostics = Vec::new();
    let mut tasks = Vec::new();
    let mut current: Option<ParsedPrdTask> = None;
    let mut description = Vec::new();

    for (index, line) in source_text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("##") {
            let title = title.trim_start();
            if title.trim().is_empty() {
                diagnostics.push(PrdDiagnostic {
                    line: line_number,
                    message: "заголовок задачи не может быть пустым".into(),
                });
                continue;
            }
            finish_task(&mut tasks, &mut current, &mut description);
            current = Some(ParsedPrdTask {
                source_ref: format!("{origin}#L{line_number}"),
                title: title.trim().into(),
                description: String::new(),
                acceptance_criteria: Vec::new(),
            });
            continue;
        }

        if let Some(criteria) = trimmed
            .strip_prefix("- [ ]")
            .or_else(|| trimmed.strip_prefix("- [x]"))
            .or_else(|| trimmed.strip_prefix("- [X]"))
        {
            if let Some(task) = current.as_mut() {
                let criteria = criteria.trim();
                if criteria.is_empty() {
                    diagnostics.push(PrdDiagnostic {
                        line: line_number,
                        message: "критерий приемки не может быть пустым".into(),
                    });
                } else {
                    task.acceptance_criteria.push(criteria.into());
                }
            } else {
                diagnostics.push(PrdDiagnostic {
                    line: line_number,
                    message: "критерий приемки находится вне задачи".into(),
                });
            }
            continue;
        }

        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            if current.is_some() {
                description.push(trimmed);
            }
        }
    }
    finish_task(&mut tasks, &mut current, &mut description);

    if tasks.is_empty() {
        diagnostics.push(PrdDiagnostic {
            line: 1,
            message: "PRD не содержит задач уровня ##".into(),
        });
    }

    PrdParseResult {
        document: (!tasks.is_empty() && diagnostics.is_empty()).then_some(ParsedPrd {
            origin: origin.into(),
            version: version.into(),
            source_text: source_text.into(),
            tasks,
        }),
        diagnostics,
    }
}

fn finish_task(
    tasks: &mut Vec<ParsedPrdTask>,
    current: &mut Option<ParsedPrdTask>,
    description: &mut Vec<&str>,
) {
    if let Some(mut task) = current.take() {
        task.description = description.join("\n");
        description.clear();
        tasks.push(task);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_markdown_prd;

    #[test]
    fn preserves_source_and_extracts_tasks_and_acceptance() {
        let source = "# Plan\n\n## First task\nDo the thing.\n- [ ] Tests pass\n";
        let result = parse_markdown_prd(source, "prd.md", "v1");
        let document = result.document.expect("valid PRD");
        assert_eq!(document.source_text, source);
        assert_eq!(document.origin, "prd.md");
        assert_eq!(document.version, "v1");
        assert_eq!(document.tasks[0].source_ref, "prd.md#L3");
        assert_eq!(document.tasks[0].acceptance_criteria, ["Tests pass"]);
    }

    #[test]
    fn reports_checklist_before_a_task() {
        let result = parse_markdown_prd("- [ ] orphan\n", "prd.md", "v1");
        assert!(result.document.is_none());
        assert!(result.diagnostics.iter().any(|item| item.message.contains("вне задачи")));
    }

    #[test]
    fn reports_empty_checklist_and_missing_tasks() {
        let result = parse_markdown_prd("# Header\n- [ ]\n", "prd.md", "v1");
        assert_eq!(result.diagnostics.len(), 2);
        assert!(result.document.is_none());
    }

    #[test]
    fn reports_malformed_empty_task_heading() {
        let result = parse_markdown_prd("## \n", "prd.md", "v1");
        assert!(result.document.is_none());
        assert!(result.diagnostics.iter().any(|item| item.message.contains("пустым")));
    }
}
