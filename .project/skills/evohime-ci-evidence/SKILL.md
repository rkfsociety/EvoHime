---
name: evohime-ci-evidence
description: Проверить CI-конфигурацию и доступные GitHub Actions результаты EvoHime как внешний acceptance evidence без запуска локальных тестов или сборок.
---

# CI и release evidence

## Что читать

Сопоставь изменённые paths с `on.pull_request.paths`, `workflow_call`,
`workflow_dispatch`, job dependencies, `if`-условиями, artifacts, publish
steps и permissions в `.github/workflows/`. Для module routing дополнительно
прочитай `module-router.yml`, соответствующий module workflow, `module-native.yml`
и `scripts/module-release.ps1`; для installer — `windows.yml`, manifest и
compatibility workflow.

Если доступен GitHub CLI, используй только read-only запросы для текущего repo:
список workflow/run, commit SHA, conclusion, failed job/step и artifact/release
metadata. Не запускай `gh workflow run`, не перезапускай jobs, не комментируй PR
и не меняй issues/releases.

## Классификация evidence

- `PASS` — run относится к exact commit/path и завершён успешно;
- `FAIL` — относящийся run завершился ошибкой; найди root cause в логе/шаге;
- `PENDING` — run ещё выполняется;
- `UNAVAILABLE` — нет run, нет доступа или commit не опубликован;
- `STALE` — evidence относится к другому commit и не подтверждает текущий diff.

Не называй `UNAVAILABLE` или `STALE` успешным gate. Исторические записи в
`docs/release-evidence.md` сохраняй как историю и не выдавай за live status.
После локального commit без push CI для него обычно `UNAVAILABLE`; это не повод
нарушать запрет push.

## Результат

Сохрани в рабочем отчёте matrix workflow → trigger/path → relevant commit →
status → missing action. Для финального отчёта перечисли, что подтверждено CI,
что ожидает последующего push и какие workflow должен dispatch module router.
