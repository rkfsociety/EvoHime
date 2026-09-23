# План 183.2 — Foundational и protocol crates

## Изменить

1. Документировать и добавить репрезентативные doc-tests к согласованным в
   этапе 1 foundational/contracts crates: `context-budget`, `permissions`,
   `model-gateway`, `evohime-listener-contract`, `evohime-listener-audio`,
   `evohime-listener-ipc`, `desktop-ipc`, `evohime-cli-contract`,
   `evohime-cli-protocol`, `evohime-model-provenance`, `evohime-receipts` и
   `evohime-remote`.
2. В каждом crate проверить root exports, types, variants, fields, functions,
   methods, traits и associated items; объяснить bounds, serialization,
   invariants, error semantics и security-sensitive behavior по фактической
   реализации.
3. Для examples применять минимальные реальные типы и значения. Не добавлять
   новые production dependencies и не требовать внешней сети в doc-tests.
4. После каждой группы включать более строгий rustdoc lint только для области,
   где покрытие завершено; исключения должны быть точечными и обоснованными.

## Зависимости

### Блокирующие

- Завершённая inventory/матрица из этапа 183.1.

### Опциональные

- Изменения контрактов соседних активных планов учитываются только если они
  уже вошли в текущий checkout; не менять чужие незавершённые контракты.

## Проверка

Для каждого затронутого crate запускать `cargo test --doc -p <package>` и
`cargo doc --no-deps -p <package>` с согласованными warning gates. Просмотреть
генерируемые API страницы на корректные ссылки, читаемые signatures и отсутствие
потерянных re-exports.

## Rollback и evidence

Если пример выявляет неоднозначность документа, уточнить prose/example, не
ослаблять тест и не менять runtime-контракт без отдельного подтверждённого
требования.
