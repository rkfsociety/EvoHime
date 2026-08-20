//! Читает соседние планы, на которые ссылается проверяемый, чтобы ревью и
//! правка видели инварианты соседних этапов.
//!
//! Живёт отдельно от `plan_review`, который намеренно не знает ни файловой
//! системы, ни часов: там остаются только границы и промпты, здесь — единственное
//! место, где ядро открывает файл плана по ссылке из другого плана.

/// Читает планы, на которые ссылается проверяемый, для промпта ревью и правки.
///
/// План этапа почти никогда не самодостаточен: инвариант соседнего этапа в нём
/// не повторён, а только упомянут ссылкой, и модель, не видя соседа, уверенно
/// переписывает план вразрез с ним. Обход идёт по ссылкам вширь на
/// `MAX_CONTEXT_DEPTH` шагов: одного шага мало, потому что этапы плана обычно
/// связаны не напрямую, а через обзорный файл.
///
/// Читает ядро, а не оболочка: оболочка и так прислала бы любой текст под
/// видом соседнего плана. Границы жёсткие — только `.md`, только относительные
/// ссылки, только внутри каталога исходного плана, и не больше
/// `MAX_CONTEXT_DOCUMENTS` файлов и `MAX_CONTEXT_BYTES` суммарно. Файл, который
/// не читается, молча пропускается: контекст улучшает правку, но его отсутствие
/// не повод отказать в ней.
pub async fn read_linked_plans(
    source_paths: &[String],
    source_markdown: &str,
) -> Vec<crate::plan_review::ContextDocument> {
    use crate::plan_review::{
        linked_plan_names, ContextDocument, MAX_CONTEXT_BYTES, MAX_CONTEXT_DEPTH,
        MAX_CONTEXT_DOCUMENTS,
    };

    let mut roots = Vec::new();
    for path in source_paths {
        let path = std::path::Path::new(path.trim());
        if path.as_os_str().is_empty() || !path.is_absolute() {
            continue;
        }
        let Some(directory) = path.parent().map(std::path::Path::to_path_buf) else {
            continue;
        };
        // Каталог берётся канонизированным один раз: сравнивать с ним
        // канонизированные пути соседей — единственный способ не пустить
        // симлинк за пределы каталога планов.
        let Ok(directory) = tokio::fs::canonicalize(&directory).await else {
            continue;
        };
        let visited = match tokio::fs::canonicalize(path).await {
            Ok(canonical) => canonical,
            Err(_) => continue,
        };
        roots.push((directory, visited));
    }
    if roots.is_empty() {
        return Vec::new();
    }

    let mut seen: Vec<std::path::PathBuf> = roots.iter().map(|(_, path)| path.clone()).collect();
    let mut documents: Vec<ContextDocument> = Vec::new();
    let mut total = 0usize;
    let mut frontier: Vec<(std::path::PathBuf, String)> = roots
        .iter()
        .map(|(directory, _)| (directory.clone(), source_markdown.to_string()))
        .collect();

    for _ in 0..MAX_CONTEXT_DEPTH {
        let mut next = Vec::new();
        for (directory, markdown) in &frontier {
            for name in linked_plan_names(markdown) {
                if documents.len() >= MAX_CONTEXT_DOCUMENTS || total >= MAX_CONTEXT_BYTES {
                    return documents;
                }
                let candidate = directory.join(&name);
                let Ok(canonical) = tokio::fs::canonicalize(&candidate).await else {
                    continue;
                };
                if !canonical.starts_with(directory) || seen.contains(&canonical) {
                    continue;
                }
                seen.push(canonical.clone());
                let Ok(text) = tokio::fs::read_to_string(&canonical).await else {
                    continue;
                };
                if text.trim().is_empty() || total + text.len() > MAX_CONTEXT_BYTES {
                    continue;
                }
                total += text.len();
                let file_name = canonical
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or(name);
                documents.push(ContextDocument {
                    file_name,
                    markdown: text.clone(),
                });
                next.push((directory.clone(), text));
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    documents
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "evohime-plan-context-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("plans")).expect("plans directory");
        root
    }

    /// Этапы плана связаны не напрямую, а через обзорный файл: одного шага по
    /// ссылкам не хватает, чтобы правка увидела инвариант соседнего этапа.
    #[tokio::test]
    async fn follows_links_two_steps_out() {
        let root = workspace("depth");
        let plans = root.join("plans");
        std::fs::write(plans.join("04-7.md"), "Этап плана [04](04-0.md).").expect("plan");
        std::fs::write(
            plans.join("04-0.md"),
            "Обзор: [контракт](04-1.md) и [хранение](04-2.md).",
        )
        .expect("overview");
        std::fs::write(plans.join("04-1.md"), "Хеш текста в лог не попадает.").expect("contract");
        std::fs::write(plans.join("04-2.md"), "Схема и retention.").expect("storage");

        let source = std::fs::read_to_string(plans.join("04-7.md")).expect("source");
        let documents = read_linked_plans(
            &[plans.join("04-7.md").to_string_lossy().to_string()],
            &source,
        )
        .await;

        let names: Vec<&str> = documents
            .iter()
            .map(|document| document.file_name.as_str())
            .collect();
        assert_eq!(names, vec!["04-0.md", "04-1.md", "04-2.md"]);
        assert!(documents[1].markdown.contains("Хеш текста"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Ссылка — это указание, какой файл открыть, поэтому всё, что уводит за
    /// пределы каталога планов, не должно быть прочитано вовсе.
    #[tokio::test]
    async fn refuses_to_leave_the_plan_directory() {
        let root = workspace("escape");
        let plans = root.join("plans");
        std::fs::write(root.join("secret.md"), "Секрет.").expect("secret");
        let source = concat!(
            "[вверх](../secret.md) [корень](/etc/passwd.md) [сеть](https://example.com/x.md)\n",
            "[нет такого](04-9.md)\n"
        );
        std::fs::write(plans.join("04-7.md"), source).expect("plan");

        let documents = read_linked_plans(
            &[plans.join("04-7.md").to_string_lossy().to_string()],
            source,
        )
        .await;

        assert!(documents.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Путь неизвестен — план пришёл перетаскиванием из источника без файловой
    /// системы: правка обязана работать и вслепую.
    #[tokio::test]
    async fn returns_nothing_without_an_absolute_path() {
        let documents = read_linked_plans(&[String::new()], "[сосед](04-1.md)").await;
        assert!(documents.is_empty());
        let documents = read_linked_plans(&["04-7.md".into()], "[сосед](04-1.md)").await;
        assert!(documents.is_empty());
    }

    /// Циклическая ссылка между планами — норма, а не ошибка: обход не должен
    /// ни зациклиться, ни прочитать сам исправляемый план вторым документом.
    #[tokio::test]
    async fn reads_each_plan_once_and_never_the_source() {
        let root = workspace("cycle");
        let plans = root.join("plans");
        std::fs::write(plans.join("04-7.md"), "[обзор](04-0.md)").expect("plan");
        std::fs::write(plans.join("04-0.md"), "[назад](04-7.md) [обзор](04-0.md)").expect("hub");

        let source = std::fs::read_to_string(plans.join("04-7.md")).expect("source");
        let documents = read_linked_plans(
            &[plans.join("04-7.md").to_string_lossy().to_string()],
            &source,
        )
        .await;

        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].file_name, "04-0.md");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Потолок держит промпт в пределах окна модели: лишние соседи
    /// отбрасываются, а правка всё равно происходит.
    #[tokio::test]
    async fn stops_at_the_document_ceiling() {
        let root = workspace("ceiling");
        let plans = root.join("plans");
        let links: String = (0..crate::plan_review::MAX_CONTEXT_DOCUMENTS + 3)
            .map(|index| {
                let name = format!("04-{index}.md");
                std::fs::write(plans.join(&name), format!("Этап {index}.")).expect("neighbour");
                format!("[сосед]({name})\n")
            })
            .collect();
        std::fs::write(plans.join("04-main.md"), &links).expect("plan");

        let documents = read_linked_plans(
            &[plans.join("04-main.md").to_string_lossy().to_string()],
            &links,
        )
        .await;

        assert_eq!(documents.len(), crate::plan_review::MAX_CONTEXT_DOCUMENTS);
        let _ = std::fs::remove_dir_all(&root);
    }
}
