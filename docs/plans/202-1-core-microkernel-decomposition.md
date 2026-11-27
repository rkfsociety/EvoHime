# План 202.1 — Единый контракт модулей, реестр сборки и `core-assets`

## Цель этапа
Построить фундамент микроядра до выноса прикладных подсистем: единый транспорт/контракт для всех модулей-сервисов, единый реестр модулей для CI/упаковки/апдейтера и вынос декларативных данных в пакет `core-assets`.

## Изменяемые контракты
1. **`crates/evohime-service-contract` (новый crate):**
   - Базовые запросы/ответы по локальному каналу (named pipe на Windows, Unix socket на Linux) с аутентификацией HMAC-SHA256.
   - Методы: `Ping` / `Health`, `DescribeCapabilities` (имена инструментов/операций, JSON Schema, требуемые `permissions`), `Invoke` (request/response), `StreamEvents` и `Shutdown`.
   - Единые коды ошибок (`Unavailable`, `Timeout`, `InvalidInput`, `PermissionDenied`, `Internal`).
2. **`ModuleRegistry` и `ModuleProxy` в `evohime-core`:**
   - Динамическая регистрация подключённых модулей в `ToolRegistry` / `CapabilityRegistry`.
   - Проверка `permissions` и аудит в ядре до отправки `Invoke` в модуль.
3. **Единый реестр модулей (`modules.manifest.json` или аналог в корне репозитория):**
   - Описывает `id`, `package`, `artifact`, `restart_scope`, `paths`, `dependencies`.
   - Используется в `module-router.yml`, `compatible-manifest.yml`, `scripts/module-release.ps1`, `scripts/native-package.ps1` и проверяется тестом согласованности.
4. **Модуль `core-assets`:**
   - Вынос статических системных промптов, шаблонов рецептов и декларативных профилей из `evohime-core` в версионированный `.zip`-пакет с SHA-256 проверкой и горячей перезагрузкой без перекомпиляции Rust.

## Зависимости
- Блокирующие: `crates/desktop-ipc`, `crates/permissions`, `crates/evohime-receipts`.
- Опциональные: План 195 (схемы capability descriptors).

## Recovery и rollback
- При отсутствии `core-assets` или ошибке валидации SHA-256 используется встроенный fallback или предыдущая валидная ревизия из backup-слоя `updater`.
- Недоступный внешний модуль переводит его capabilities в состояние `unavailable` (typed error), не роняя `evohime-core`.

## Проверка
- Unit-тесты контракта `evohime-service-contract` (handshake, timeout, bounded frames, schema validation).
- Smoke-проверка консистентности единого реестра модулей между CI, упаковкой и `evohime-updater`.
