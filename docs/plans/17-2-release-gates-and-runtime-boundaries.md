# План 17.2. Release gates и границы базового runtime

## Цель

Превратить общие инварианты EvoHime в повторяемые gates для каждого нового
контракта, worker и automation feature.

## Изменения

- Проверять Core/SQLite durable ownership, stable IDs, atomic writes,
  versioned schemas, contract tests и sequence replay после reconnect.
- Требовать capability, scope, approval/policy, cancellation, timeout, budget,
  size и concurrency checks для каждого внешнего действия.
- Проверять typed error/rejection/timeout/unknown model output, redaction
  secrets/PII/sensitive output и отсутствие model authority над host resources.
- Запретить public HTTP, cloud control plane, второй execution runtime,
  unrestricted browser/desktop access и production effects из benchmark/
  simulation.
- Подтверждать, что renderer остаётся projection/control layer и не получает
  прямой доступ к workspace, SQLite, provider или инструментам.

## Проверки

- contract, migration, reconnect/replay и crash/recovery fixtures;
- негативные tests на policy bypass, host action, egress и secret leakage;
- bounded resource/concurrency/retention checks;
- clean package inspection для случайных Python/Node/model assets и public
  endpoints.

## Готово, когда

Новый компонент проходит единый gate без исключений “только для dev”, а
неподдерживаемая capability безопасно деградирует и не расширяет базовый
runtime.

