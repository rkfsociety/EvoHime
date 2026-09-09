---
name: evohime-repo-orientation
description: "Выполнить обязательный read-only baseline EvoHime перед планированием или изменениями: sync, ветка, dirty tree, правила, память и источники истины."
---

# Baseline репозитория EvoHime

## Порядок

До любых изменений прочитай актуальные корневые и вложенные `AGENTS.md`,
`.codex/README.md`, содержимое `.codex/skills/`, `docs/README.md`,
`docs/plans/README.md`, `docs/architecture.md`, `docs/current-state.md`,
`docs/release-evidence.md`, `SECURITY.md` и документы, на которые они ссылаются
для текущей области.

Сними read-only baseline:

- `git status --short --branch`, `git branch --show-current`, `git rev-parse
  HEAD`;
- наличие upstream и сравнение `HEAD...origin/main` через
  `git rev-list --left-right --count`; при необходимости только обнови
  remote-tracking refs безопасным fetch, но не делай merge/rebase/pull;
- полный список dirty paths с классификацией «до задачи / относится к задаче»;
- наличие `.codex` и Git-tracked проектных инструкций;
- текущий каталог незавершённых планов и точный комплект этапов целевого плана.

## Правила решения

Если дерево dirty, сохрани исходный список и не перезаписывай чужие изменения.
Если локальная ветка ahead/behind/diverged, не исправляй это автоматически:
зафиксируй commit IDs и риск смешения истории, продолжая только если задача
безопасно выполняется в текущем checkout. Новую ветку не создавай.

Источники истины при конфликте: код и тесты → `current-state.md` →
`architecture.md` → `development-plan.md` → `roadmap.md`. При явном запрете
локального тестирования тесты можно анализировать по исходникам и CI, но нельзя
запускать.

## Результат

Верни короткую baseline-запись: branch, HEAD, remote relation, dirty paths,
прочитанные правила/документы, целевой plan комплект и ограничения. Не меняй
файлы и не создавай commit этим skill.
