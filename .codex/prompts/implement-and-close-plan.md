# Реализовать и закрыть implementation plan

Используй проектные skills из `.codex/skills/`.

0. `$evohime-plan-implementation` — главный оркестратор; он запускает маршрут ниже.

**Цель:** полностью реализовать и закрыть план №132.

**Маршрут — выполняй последовательно:**

1. `$evohime-repo-orientation` — sync, `main`, dirty tree, правила, `.codex`,
   память и источники истины.
2. `$evohime-plan-review` — найди canonical plan, ревьюй и исправляй его до
   устранения всех важных замечаний.
3. `$evohime-implementation-audit` — сверь каждый пункт с кодом, storage,
   runtime/recovery, IPC/UI, CI, packaging, security и docs.
4. Реализуй утверждённый план end-to-end: код, интеграция, тесты, CI, упаковка.
5. `$evohime-ci-evidence` — проверь workflow и доступные CI results.
6. `$evohime-module-release-routing` — определи source-модули и подними только
   их PATCH в `release-versions/`.
7. `$evohime-documentation-closure` — обнови `architecture`, `current-state`,
   `release-evidence`, индексы; удали все файлы плана; проверь ссылки.
8. `$evohime-static-verification` — выполни разрешённые статические проверки.
9. `$evohime-task-only-commit` — закоммить только изменения этой задачи.

**Ограничения:** новых веток не создавай; посторонние изменения сохрани; push
не выполняй. Локальные тесты, сборки, линтеры, smoke/E2E и другие project
checks запрещены — используй только статический анализ и CI evidence. Не
повышай версии незатронутых модулей; если source-модулей нет, зафиксируй, что
повышение не требуется.

**Готово только если:** plan reviewed, реализация полная, версии и docs
согласованы, plan-файлы удалены, устаревшие ссылки устранены, `git diff
--check` пройден и создан task-only commit.

**Финал:** укажи review result, изменения, модули и old → new версии,
разрешённые проверки, документы, удалённый plan, commit hash, ожидаемый
post-push module-router effect и ограничения.
