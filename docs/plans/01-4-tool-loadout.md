# Этап 01.4: Tool loadout

Этап плана [01 Context Budget Manager](01-0-context-budget-manager.md).

## Зависимости

Блокирующие: этап 01.1 — loadout ограничен `tool_schema_reserve` из профиля.

Опциональная: semantic tool selection разрешается только после evaluation
catalog; до него работает deterministic intent router, и этап выполним
полностью.

Разблокирует: никого — внутренний этап плана.

## Что этап отдаёт наружу

Ничего: registry остаётся в Core, наружу уходит только сам loadout в model
call.

## Содержание

- Разделить инструменты на обязательные, read-only и mutation groups.
- Сначала использовать детерминированный intent router; semantic selection
  добавлять только после появления evaluation fixtures.
- Обязательные инструменты всегда входят в loadout и имеют отдельный
  `mandatory_schema_reserve`; при неопределённом intent использовать безопасный
  read-only fallback loadout.
- Передавать модели только небольшой релевантный набор schemas с лимитом
  `tool_schema_reserve`, сохраняя полный registry в Core. Semantic selection
  разрешать только после evaluation catalog и измерений precision/recall.
- Никогда не скрывать permission/approval semantics у выбранного инструмента.
- Вызов инструмента вне loadout Core отклоняет до эффекта с bounded diagnostic
  `loadout_miss`; автоматический fallback разрешён только для явно разрешённой
  read-only замены.

## Проверки

- E2E фиксирует prompt, model/profile/tokenizer versions, registry snapshot,
  memory snapshot и timestamp policy; одинаковый fixture даёт одинаковый
  loadout без flaky-зависимости от wall clock;
- неопределённый intent даёт безопасный read-only fallback loadout, а не пустой
  или произвольный набор;
- security test: out-of-loadout mutation блокируется до эффекта с диагностикой
  `loadout_miss`.

## Критерии готовности

- tool schemas передаются только по loadout, но Core всё равно проверяет вызов;
- permission/approval semantics выбранного инструмента остаются видимыми;
- ledger и метрики показывают loadout misses.
