# План 172.1 — Recovery state и verified control-plane repair

Статус: этап 1 для [плана 172.0](./172-0-updater-recovery-and-self-healing.md).

## Зависимости

### Блокирующие

- План 172.0.
- `evohime-update-agent`, compatibility manifest и существующая SHA-256
  verification логика.
- Windows atomic file replacement и существующий update-state каталог.

### Опциональные

- Existing diagnostics projection; при отсутствии расширенного evidence
  результат должен оставаться bounded и redacted.

## Реализация

1. Ввести versioned bounded recovery journal в
   `%LOCALAPPDATA%\EvoHime\update-state` с фазами `prepared`, `downloaded`,
   `verified`, `replaced`, `self-tested`, `committed`, `rolled-back` и
   `manual-recovery`. Запись должна быть atomic, идемпотентной и пригодной для
   продолжения после завершения процесса или перезагрузки Windows.
2. Описать два слота одного updater-модуля: active
   `evohime-updater.exe` и persistent last-known-good recovery copy. При
   установке слоты инициализируются одним artifact; при последующих заменах
   старый подтверждённый active slot сохраняется до успешной проверки нового.
3. Добавить bounded `--self-test`, проверяющий executable header, версию,
   чтение конфигурации, доступность state/staging paths, parse текущего
   compatibility contract и отсутствие небезопасных путей. Self-test не
   запускает Core, не требует provider credentials и не меняет пользовательские
   данные.
4. Добавить в updater прямой control-plane repair только для
   `evohime-updater.exe` и `evohime-transaction.exe`: скачать artifact,
   проверить manifest/size/SHA-256/PE header, закрыть старый процесс при
   штатном gate, сделать backup и выполнить atomic replace. Повреждённый
   artifact никогда не становится active.
5. Переиспользовать существующие trusted GitHub HTTPS и compatibility
   restrictions. Не принимать URL из recovery journal, не писать токены или
   raw network responses в state/logs, ограничить размеры и количество
   recovery records.

## Критерии выхода

- [ ] Recovery schema versioned, bounded, atomic и fail-closed.
- [ ] Active/last-known-good slots имеют проверяемые version/hash metadata.
- [ ] Self-test не запускает продуктовые процессы и не раскрывает secrets.
- [ ] Updater может восстановить transaction worker без transaction worker.
- [ ] Любая ошибка replacement оставляет старую рабочую копию.
- [ ] Unit tests покрывают corruption, crash между фазами, replay,
  path traversal, hash/size/PE mismatch и отказ записи.

## Не входит

Полная native/UI transaction, renderer IPC, новый executable release module,
автоматическая отправка issue и изменение полного Inno installer кроме
минимального создания recovery slot.
