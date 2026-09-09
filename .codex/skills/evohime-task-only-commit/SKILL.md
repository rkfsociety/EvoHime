---
name: evohime-task-only-commit
description: Создать проверенный task-only git-коммит EvoHime в текущей main, сохранив чужие изменения и не выполняя push.
---

# Task-only commit

## Перед staging

Сохрани baseline dirty paths из начала задачи. После работы сравни
`git status --short`, `git diff --name-only` и `git diff --stat`; раздели файлы
на относящиеся к задаче и посторонние. Не используй `git add .`, `git add -A`,
`git commit -am`, reset или checkout, если это может захватить чужие изменения.

Особенно проверь, что в список попали новые `.codex/skills`, docs, plan
deletions, `release-versions/<module>.txt` и только нужные исходники/CI.

## Коммит

1. Точечно stage-ь разрешённые paths.
2. Проверь `git diff --cached --name-status`, `git diff --cached` и
   `git diff --cached --check`.
3. Убедись, что branch всё ещё `main`, upstream relation известен, секретов,
   кэшей и чужих изменений нет.
4. Создай понятный task-only commit с сообщением по результату, не меняя
   историю и не создавая ветку.
5. После commit проверь `git rev-parse HEAD`, `git show --stat --oneline HEAD`,
   `git status --short --branch` и сохранение посторонних dirty paths.

## Push boundary

Push здесь запрещён, если пользователь отдельно его не запросил. В финале
укажи commit hash и что `origin/main` может отставать; module router увидит
изменения только после последующего push. Если пользователь позже разрешит
push, используй проектную команду с `GIT_TERMINAL_PROMPT=0` и подтверждай remote
ref через `git ls-remote`, а не по одному выводу push.

## Результат

Выход — hash, branch, staged/committed paths, сохранённые посторонние изменения
и точная граница того, что не было отправлено на remote. Сам skill не запускает
tests/build/lint/smoke; перед ним должен быть завершён разрешённый static gate.
