# План 191.3 — доставка skills агенту и bounded projection

Статус: active implementation contract. Этап следует после overview 191.0 и
предыдущих этапов этого направления.

## Scope и изменяемые контракты

Довести существующий Core-owned `SkillRegistry` до model-facing использования,
не создавая второй registry и не превращая skill в расширение полномочий.

1. **Discovery.** Core строит bounded `SkillCatalogV1` для текущего workspace и
   передаёт модели только metadata trusted-записей: `skill_id`, `name`,
   `description`, `version`, усечённый до существующего лимита. Тело,
   `references` и `source_ref` в промпт не попадают. Список ограничен
   `MAX_SKILLS` и отбирается детерминированно тем же правилом, что и каталог
   инструментов этапа 191.2.
2. **Progressive disclosure.** Загрузка тела выполняется отдельным Core-owned
   шагом в агентском цикле по `skill_id` через `SkillRegistry::load`, а не через
   UI-команду. Шаг возвращает bounded content (`MAX_SKILL_BYTES`), `content_hash`,
   `version` и provenance; при `invalid`, `untrusted`, `quarantined`,
   `SensitiveContent`, `StaleContent` или `CapabilityEscalation` — typed отказ с
   кодом из `SkillRegistryError`, без частичной выдачи тела.
3. **Grant и нагрузка.** Доступные скиллы пересекаются с родительским grant, как
   в `SkillRegistry::effective_permissions`. Загруженное тело учитывается
   context-budget runtime как отдельная секция с собственным лимитом; отсутствие
   заголовка или превышение лимита не роняет общий prompt, а даёт typed
   диагностику.
4. **Task bridge.** `core_coordinator_tasks.rs:335` передаёт реальный
   `skill_context` вместо `&[]`, заполняя его только metadata отобранных
   trusted-скиллов. Порядок секций, зафиксированный тестом
   `workspace.rs:484-516`, сохраняется.
5. **Trace.** Durable trace получает `skill_id`, `version`, `content_hash` и
   `cache_hit` (как уже делает `ipc_bridge_goals_skills.rs:515-527` для UI-пути).
   Тело, `references`, prompts и secrets не сохраняются.

## Зависимости

### Блокирующие

Существующие `SkillRegistry`, `skill_trust_pipeline` и SQLite schema 48 как
единственный источник trust-решения; `context_budget` как владелец лимитов;
`EventJournal` как владелец trace. Этапы 191.1–191.2 задают форму каталога и
профиля промпта.

### Опциональные

План 184 может потреблять skill-provenance в trace evaluation. Расширение
`desktop-ipc` не требуется: существующие `ListSkills`/`LoadSkill`/
`LoadSkillReference` остаются UI-путём и не меняются.

## Recovery и rollback

Skill остаётся untrusted-инструкцией: загрузка не выдаёт permissions, не
запускает скрипты и не выполняет install/network hooks. Ошибка загрузки на
середине turn не отменяет уже выполненные шаги и не подменяется другим
skill-пакетом. После перезапуска Core кэш тел пуст, а durable trace сохраняет
только идентичность. Rollback: отключение секции skills возвращает текущее
поведение (`skill_context: &[]`) без миграции схемы и без удаления trust-записей.

## Verification

Unit: невалидный/quarantined/oversized/secret-shaped skill не выдаёт тело;
`content_hash` не совпадает с диском → `StaleContent`; grant-эскалация даёт
`CapabilityEscalation`. Integration: агентский turn с metadata-only discovery и
явной загрузкой; `skill_context` в task bridge заполняется и остаётся
детерминированно упорядоченным; trace содержит только id/version/hash.
Security: тело skill не попадает в системный промпт и в trace; prompt-injection
маркеры в metadata по-прежнему дают `invalid`.

## Release evidence

Зафиксировать версию skill schema, формат секции промпта, лимиты тела и
references, результат negative security тестов, focused test/CI evidence и
подтверждение, что secondary registry не создан. Плановые проверки не выдавать
за выполненные.

## Критерии выхода

- [ ] Модель видит metadata trusted-скиллов и загружает тело только явным
      шагом с typed отказом.
- [ ] Тело скилла не попадает ни в промпт по умолчанию, ни в durable trace.
- [ ] Grant-пересечение и trust-гейт не ослаблены.
- [ ] Task bridge получает реальный `skill_context`.
- [ ] Внутренние ссылки разрешаются, `git diff --check` проходит.
