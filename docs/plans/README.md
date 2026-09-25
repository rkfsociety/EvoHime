# Планы реализации

Каталог `docs/plans/` содержит только незавершённые implementation contracts.
Обновлено: 2026-09-25. Наличие комплекта `NN-0` ... `NN-4` означает, что
направление ещё не закрыто; статус реализации не выводится из одного файла
плана.

## Источник статуса

Подтверждённое состояние checkout находится в [`../current-state.md`](../current-state.md),
архитектурные контракты — в [`../architecture.md`](../architecture.md), а
проверки поставки — в [`../release-evidence.md`](../release-evidence.md).
Эта страница является каталогом незавершённых направлений и не дублирует
таблицы реализации.

## Закрытые направления

Временные plan-файлы закрытых направлений удалены после переноса их контракта
и evidence в канонические документы. К закрытым относятся планы `01–174` и `183`
(включая `144`); планы `127–130` закрыты как MVP-контуры с явно сохранёнными
`unavailable` deployment/adapter gates. Их отсутствие из каталога не означает
отсутствие контракта: он находится в `architecture.md` и `current-state.md`.

## Активный каталог

Все перечисленные ниже направления имеют этапы `0–4`. Ссылки ведут на overview
этапа `0`; его блокирующие и опциональные зависимости являются источником
порядка реализации.

| План | Тема | Состояние |
| --- | --- | --- |
| 175 | [Memory Ingestion Integrity](175-0-memory-ingestion-integrity.md) | active; исторический источник issue #153 |
| 176 | [Guided Capability Recipes](176-0-guided-capability-recipes.md) | active; guided layer over closed recipe/workflow contracts; исторический источник issue #154 |
| 177 | [Core-owned Image Generation and Editing](177-0-core-image-generation-editing.md) | active; исторический источник issue #155 |
| 178 | [Core-owned Local Model Adaptation](178-0-core-local-model-adaptation.md) | active; исторический источник issue #156 |
| 179 | [Core-owned Prompt Strategy Resolver](179-0-core-prompt-strategy-resolver.md) | active; исторический источник issue #157 |
| 180 | [Core-owned A2A Bridge](180-0-core-a2a-bridge.md) | active; исторический источник issue #158 |
| 181 | [Core-owned Sensitive Egress Guardrails](181-0-core-sensitive-egress-guardrails.md) | active; исторический источник issue #159 |
| 184 | [Core-owned Execution-Trace Evaluation](184-0-core-execution-trace-evaluation.md) | active; исторический источник issue #160 |
| 185 | [Core-owned Behavioral Simulation Harness](185-0-core-behavioral-simulation-harness.md) | active; исторический источник issue #161; blocking dependency 184 |
| 186 | [Core-owned Local Typed-Decision Fast Path](186-0-core-local-typed-decision-fast-path.md) | active; исторический источник issue #162 |
| 187 | [Core-owned Windows Native Computer Use](187-0-core-windows-native-computer-use.md) | active; исторический источник issue #163; blocking dependencies 184–185 |
| 188 | [Core-owned Research Experiment Tree](188-0-core-research-experiment-tree.md) | active; исторический источник issue #164 |

Незавершённые numbered plans: 175–181 и 184–188.

Номера `149–172` и `182–183` являются закрытыми идентификаторами очереди.
Пропуск `144` намеренный: это закрытый план модульного обновления. Новая работа
получает следующий свободный номер только после проверки дубликатов и
зависимостей; следующий номер — `189`, текущий active catalog — `175–181` и
`184–188`.
Номера issues в активных планах — исторические идентификаторы постановок и не
являются текущим источником статуса или критерием закрытия.

## Формат этапов

Имя файла имеет формат `NN-M-slug.md`:

- `NN` — номер направления;
- `M = 0` — scope, контракт, ограничения и зависимости;
- `M = 1` — Core-контракт, schema и storage;
- `M = 2` — runtime-интеграция и recovery;
- `M = 3` — IPC, client projection и UI;
- `M = 4` — verification, release evidence и закрытие.

Этапы одного плана выполняются `0 → 1 → 2 → 3 → 4` после принятия overview.
Блокирующая зависимость от более позднего номера запрещена. Каждый этап
обязан разделять blocking и optional dependencies, изменяемые контракты,
recovery, verification, rollback и release evidence.

## Правило закрытия

План закрывается только после кода, integration, recovery, typed IPC/UI при
наличии, тестов, security/release evidence и обновления канонической
документации. После закрытия:

- контракт переносится в [`../architecture.md`](../architecture.md);
- фактический статус — в [`../current-state.md`](../current-state.md);
- проверки — в [`../release-evidence.md`](../release-evidence.md);
- временные файлы плана удаляются из этого каталога.

Старая ссылка на удалённый plan-файл не является доказательством незавершённой
работы: её нужно заменить ссылкой на канонический документ.

## Стыковка направлений

- `122` остаётся владельцем verification evidence для `124`, `135`, `136` и
  `140`;
- `134` предоставляет resource-pressure signals для runtime-направлений;
- `130` владеет lease/fencing, а `132` использует этот уже закрытый контракт;
- `131` и `137` не создают параллельные registries;
- `121`, `125`, `128` и `129` используют единый gateway/resolver;
- `138` не создаёт второй механизм обновлений и использует контракт `144`.

## Проверка

Перед реализацией overview и после каждого этапа сверяйте план с кодом,
`AGENTS.md`, [`../architecture.md`](../architecture.md), security policy и
release workflow. Минимальный gate: ссылки разрешаются, `git diff --check`
проходит, а критерии этапа подтверждены свежими тестами.
