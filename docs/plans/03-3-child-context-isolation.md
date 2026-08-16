# Этап 03.3: Context isolation

Этап плана [03 Специализированные child workflows](03-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: этап 03.2 — состояния, в которых живёт контекст ребёнка. Базовые
scratchpad, Artifact Store и workspace retrieval для роли `researcher` уже
реализованы; child budget и policy-grant enforcement добавляются в рамках
03.1–03.3.

Разблокирует: 03.4.

## Что этап отдаёт наружу

Изоляцию контекста между детьми и offload больших результатов.

## Содержание

- Child получает только selected context и свой scratchpad.
- Большие результаты offload в существующий content-addressed artifact store;
  parent получает summary + ids + hash.
- Locator проверяется по task namespace, hash и policy grant на каждом чтении.
  Reviewer получает только summary/evidence ids без полного артефакта, если
  отдельный grant не выдан coordinator.
- Не передавать секреты соседнему child или role без policy grant.
- Reviewer видит diff/evidence, но не получает право менять код.

## Проверки

- секрет не переходит соседнему child или role без policy grant;
- reviewer не может изменить код, имея доступ к diff и evidence;
- большой результат уходит в artifact store, родитель получает summary и ids.
- child не может прочитать locator другого task namespace без grant;
- path grant нельзя расширить через symlink, `..`, alternate stream или
  повторный tool call.

## Критерии готовности

- child не расширяет права родителя и не обходит approval;
- контекст одного ребёнка не протекает в другого.
