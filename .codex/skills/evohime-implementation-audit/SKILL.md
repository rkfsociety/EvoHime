---
name: evohime-implementation-audit
description: "Сопоставить реализацию EvoHime с каждым пунктом плана: код, storage, runtime recovery, IPC/UI, CI, packaging, security и документация."
---

# Аудит реализации по плану

## Метод

Построй таблицу requirement → implementation anchor → integration consumer →
verification evidence → documentation owner. Ищи фактические вызовы и wiring,
а не только типы, комментарии, заглушки или экспортированные API.

Проверь по затронутой области:

- Rust workspace crates, feature flags, migrations и владельца состояния;
- Core startup/shutdown, supervisor lifecycle, background workers, recovery,
  retry/idempotency и bounded resource behavior;
- canonical `desktop-ipc` proto, Rust server/client, Electron main/preload,
  generated TypeScript и renderer state projection;
- permissions, credentials, logs, redaction, input bounds и fail-closed paths;
- Git change-set/transaction boundaries, packaging scripts, artifact manifests,
  workflow path filters, module release и installer/compatibility consumers;
- tests как исходные контракты и CI jobs как исполняемые gates; при запрете
  локального тестирования не запускай ничего из этого списка;
- `architecture.md`, `current-state.md`, `release-evidence.md`, docs indexes и
  ссылки на plan-файлы.

## Типовые ложные завершения

Не принимай за реализацию: модель без persistence; storage без recovery;
runtime без startup wiring; IPC command без auth/sequence/consumer; UI без
Core-owned state; тест без workflow gate; workflow без artifact path; поднятую
версию без source change; документацию без соответствующего кода.

## Результат

Пометь каждый пункт `complete`, `partial`, `missing`, `blocked` или `not in
scope`, приложи точные пути и объясни, какие изменения нужны. Не меняй файлы,
если skill вызван как read-only audit; при полном executor workflow исправления
делаются после подтверждения пробела.
