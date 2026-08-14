# Этап 05.3: Context isolation

Этап плана [05 Специализированные child workflows](05-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: этапы 01.1 (budget ребёнка) и 01.2 (scratchpad и artifact store);
этапы 02.2 и 02.3 — роль `researcher` без retrieval и planner не имеет своего
инструмента; этап 05.2 (состояния, в которых живёт контекст ребёнка).

Разблокирует: 05.4.

## Что этап отдаёт наружу

Изоляцию контекста между детьми и offload больших результатов.

## Содержание

- Child получает только selected context и свой scratchpad.
- Большие результаты offload в artifact store; parent получает summary + ids.
- Не передавать секреты соседнему child или role без policy grant.
- Reviewer видит diff/evidence, но не получает право менять код.

## Проверки

- секрет не переходит соседнему child или role без policy grant;
- reviewer не может изменить код, имея доступ к diff и evidence;
- большой результат уходит в artifact store, родитель получает summary и ids.

## Критерии готовности

- child не расширяет права родителя и не обходит approval;
- контекст одного ребёнка не протекает в другого.
