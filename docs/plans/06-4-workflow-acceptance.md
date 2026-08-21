# План 06-4 — Evaluation, security, observability и закрытие плана

## Цель

Доказать, что workflow orchestration пригодна для поставляемой Евы, и после
полного выполнения перенести контракт в каноническую документацию и удалить
временные файлы плана.

## Зависимости

### Блокирующие

- [06-3](06-3-workflow-desktop.md);
- существующий evaluation catalog `tests/evals/` со скриптами
  `scripts/eval-gate.tests.ps1`, `scripts/security-eval-gate.tests.ps1` и
  deterministic evals в `crates/evohime-core/src/evals.rs`
  ([`../evaluations.md`](../evaluations.md));
- полный набор Rust, Electron, IPC и packaging checks из `AGENTS.md`, включая
  `scripts/native-package.tests.ps1`.

### Опциональные

- дополнительные локальные модели и embeddings. Их отсутствие не блокирует
  закрытие: обязательные fixtures используют deterministic provider/fallback.

## Evaluation matrix

1. Валидный sequential workflow выполняется в ожидаемом порядке.
2. Diamond graph даёт bounded fan-out и deterministic fan-in.
3. `AND` ждёт все ветки, `OR` принимает только объявленные route events.
4. Child output schema и provenance проверяются до fan-in.
5. Retry не повторяет non-retryable error и не превышает budget.
6. Approval, cancellation, timeout и restart дают правильные terminal states.
7. Crash до/после dispatch marker не приводит к blind retry.
8. Renderer получает projection, но не raw prompt, secret, unrestricted child
   context или произвольный tool result.
9. MCP server identity, host, tool name и capability grants не могут быть
   подменены model output или входом renderer; host вне allowlist, redirect за
   его пределы и неподдержанный transport отклоняются до запуска.
10. Context Provider freshness, source identity, path scope и evidence provenance
    проверяются до включения данных в model context; stale/unavailable source
    не превращается в уверенный ответ.
11. Попытки cycle, route injection, grant escalation, nested child, path escape,
    unbounded loop и oversized payload отклоняются до эффекта.
12. Старые IPC-клиенты продолжают работать с additive workflow protocol.
13. Block schema fixture с невалидным обязательным входом не запускает узел;
    явная failure-ветвь продолжает только разрешённый fallback, а неподключённая
    ошибка блокирует downstream.
14. Изменение template/block version во время активного запуска не меняет его
    graph snapshot; расписание сохраняет bounded input snapshot, а
    неподдержанное календарное правило даёт typed `unsupported_schedule`
    вместо молчаливого пропуска запуска.

## Документация и закрытие

- перенести подтверждённый workflow contract и runtime invariants в
  `docs/architecture.md`;
- перенести фактическое состояние, шаблоны и результаты проверок в
  `docs/current-state.md`;
- обновить `docs/features/task-dependency-graphs.md`: он описывает граф
  зависимостей work items, а не workflow orchestration, поэтому обязан явно
  разделить два контура и сослаться на канонический workflow contract;
- обновить `docs/plans/README.md` и `docs/development-plan.md`: убрать строки
  и порядок этапов плана 06 и перевести его в раздел реализованного;
- заменить в `docs/plans/07-0-superagi-inspired-tooling.md` и
  `docs/plans/07-1-tool-manifest-contract.md` ссылки на 06-1 и 06-3 ссылками на
  `docs/architecture.md`, иначе удаление файлов плана оставит битые ссылки;
- проверить внутренние ссылки, `git diff --check`, generated protocol и
  отсутствие CAMEL/AutoGPT/Python/Docker как runtime-зависимости;
- проверить, что prompts, responses, workspace text и credentials не уходят в
  observability export без явного opt-in; локальный redacted trace остаётся
  authoritative projection поверх receipts/provenance;
- удалить `docs/plans/06-*.md` только после полного acceptance и task-only
  commit.

## Готово, когда

Все обязательные eval/security/packaging проверки зелёные, current-state и
architecture отражают код, план удалён по правилам репозитория, а опубликованный
commit можно воспроизвести на `main`.
