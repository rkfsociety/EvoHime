# План 17.4. Финальный release audit

## Цель

Провести единую приёмку планов 07–16 и закрыть сопровождающий research-раздел
только после проверки фактического результата.

## Изменения

- Сверить каждый план с текущим кодом, схемами, `docs/architecture.md`,
  `docs/current-state.md`, tests и package contents.
- Выполнить fixture/replay matrix, Core/SQLite migration checks, supervisor
  recovery, IPC compatibility, policy/approval/cancellation, redaction и
  resource limits.
- Проверить optional backends и simulation в отключённом состоянии, включая
  typed fallback и отсутствие production side effects.
- Зафиксировать остаточные решения, known limitations, rollback command/path и
  release owner; не объявлять закрытым план без evidence.
- После успешного аудита перенести подтверждённый контракт/состояние в
  canonical docs и удалить только действительно завершённые plan files по
  правилам репозитория.

## Проверки

- полный clean-checkout release run с deterministic outputs;
- ссылки и номера планов, `git diff --check`, migration backup/restore;
- отсутствие unrestricted runtime, secret leakage, broken IPC или stale
  generation side effects;
- повторная проверка удалённых research sources и пустого каталога источников.

## Готово, когда

Все обязательные gates имеют свежие evidence, открытые решения либо закрыты,
либо явно блокируют release, а состояние репозитория и документация согласованы
с реально проверенным checkout.

