# 13-3 — Browser lifecycle, artifacts и packaging

## Цель

Безопасно запускать, ограничивать и завершать browser backend в supervisor/Core
runtime.

## Изменения

1. Запускать browser process в bounded lifecycle с timeout, memory/size limits,
   cancellation token и supervisor recovery binding.
2. Закрывать context/tabs/process после normal completion, cancellation, crash
   и Core restart; orphan process считать typed diagnostic error.
3. Сохранять screenshots/traces/download references через bounded ArtifactStore,
   не превращая browser backend в durable state owner.
4. Проверить packaging, licensing, browser binary/resource manifest и отсутствие
   внешнего Node runtime в продукте.
5. Отключать capability при неполном manifest, unsupported binary или failed
   security check, не переходя в unrestricted fallback.

## Проверки

- startup/cleanup/crash/restart lifecycle;
- bounded resource exhaustion и cancellation;
- artifact retention/redaction;
- package smoke, license/resource verification и orphan process detection.

## Готово, когда

Browser backend не оставляет процессы или незакрытые contexts после любой
ошибки и не расширяет базовый Windows package невалидированными ресурсами.
