# План 172.4 — Verification, release evidence и закрытие

Статус: этап 4 для [плана 172.0](./172-0-updater-recovery-and-self-healing.md).

## Зависимости

### Блокирующие

- План 172.0 и завершённые этапы 172.1–172.3.
- Windows module workflows для updater и transaction.
- Existing package/installer/rollback acceptance contracts.
- Security policy для trusted release assets, secrets, paths и process
  boundaries.

### Опциональные

- Полный Windows installer acceptance как подтверждение совместимости
  первоначальной установки.
- CI timing evidence и fault-injection artifacts.

## Реализация

1. Добавить Rust tests для recovery journal, A/B slots, self-test,
   transaction-worker repair, atomic replace, interrupted phases, rollback,
   crash-loop и bounded retry/hash verification.
2. Добавить Electron tests для preflight launch, stale/corrupt status,
   recovery projection, retry idempotency, UI fallback и отсутствия silent
   `ready` после failure.
3. Добавить Windows package smoke, проверяющий наличие двух слотов одного
   updater artifact, корректный shortcut entrypoint, запуск без консоли,
   repair transaction worker и сохранение user data. Не запускать проверки на
   установленном рабочем клиенте.
4. Проверить, что изменение updater/transaction исходников маршрутизируется в
   module-only workflows и не требует `windows.yml` full installer для
   исправления control-plane. Полный installer оставить manual/initial
   recovery fallback.
5. Выполнить разрешённые focused checks, `git diff --check`, protocol/typecheck
   при изменении IPC, Rust format/clippy/tests в CI и release evidence с
   bounded redacted failure cases.
6. После фактической реализации перенести контракт в
   `docs/architecture.md`, подтверждённый статус в `docs/current-state.md`,
   проверки и rollback matrix в `docs/release-evidence.md`, обновить
   `installer/release-notes.md`, удалить комплект `172-0 ... 172-4` и
   проверить устаревшие ссылки.

## Критерии выхода

- [ ] Все failure matrix cases имеют reproducible test/evidence.
- [ ] Module-only updater repair подтверждён CI без полного installer build.
- [ ] Package/shortcut/slot/rollback smoke проходит на Windows runner.
- [ ] Security gates подтверждают trusted URL, hash, path, process и secret
  boundaries.
- [ ] Canonical docs и release evidence не утверждают непроверенное состояние.
- [ ] `git diff --check` и документационные ссылки проходят.
- [ ] После переноса контракта полный plan-комплект удалён.

## Не входит

Публикация нового web-инсталлятора, новый модуль/канал, изменение product
version без требования release routing и push, не относящийся к текущей
задаче.
