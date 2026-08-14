# Этап 02.3: Deterministic query planner и agentic loop

Этап плана [02 Локальный Agentic RAG](02-0-local-agentic-rag.md).

## Зависимости

Блокирующие: этап 02.2 — planner и checker работают поверх retrieval.

Разблокирует: 02.5 (отбор evidence для контекста) и 05.1 (роль `researcher`
возвращает evidence и unknowns).

## Что этап отдаёт наружу

Planner с валидируемым контрактом, evidence checker с versioned метриками и
bounded agentic loop.

## Содержание

Planner сначала выполняет локальный pre-check без LLM:

- очевидный path — path search;
- `?`, вопросительные слова или явные команды поиска — lexical search;
- один identifier — exact symbol search;
- иначе — lexical search с ограниченными terms.

Planner возвращает только валидированный JSON по фиксированной схеме:

```json
{
  "need_search": true,
  "strategy": "exact_symbol|lexical|path|metadata",
  "query": "...",
  "filters": {"path": null, "language": null},
  "reason": "...",
  "confidence": 0.0
}
```

Неизвестные поля, недопустимая strategy, пустой query и confidence вне `[0,1]`
отклоняются. LLM rewrite не используется для каждого запроса и не может
изменять scope workspace или security filters.

Agentic loop имеет одновременно hard limit итераций (default 2), wall-clock
timeout и token budget. Стратегии переписывания идут в порядке: exact
symbol/identifier, lexical expansion, path/type filter, затем optional
semantic strategy на этапе 02.4. Вся цепочка rewrite и причины остановки
пишутся в diagnostic log без секретного содержимого.

После каждого retrieval checker вычисляет минимум:

- term/identifier coverage для lexical query;
- наличие независимого evidence для утверждения;
- score threshold и diversity по документам;
- конфликт источников с явным `conflict=true`;
- hash freshness и sandbox validity.

Пороговые значения и формулы versioned/configurable, а не скрытая оценка LLM.
Если конфликт не разрешён deterministic metadata policy (сначала актуальность,
затем явно настроенный path priority), оба источника передаются модели с
пометкой конфликта; ответ не должен выбирать один молча.

При низком coverage planner делает ограниченное rewrite. После исчерпания
итераций, времени или evidence budget система сообщает «данных недостаточно» и
может запросить разрешение на более широкий поиск. UI получает bounded
streaming status: текущая стратегия, число найденных chunks, coverage,
rewrite и причина завершения; частота обновлений ограничена.

## Проверки

- query rewrite limit: loop останавливается по итерациям, времени и budget;
- невалидный planner output (неизвестное поле, пустой query, confidence вне
  диапазона) отклоняется;
- planner не может изменить scope workspace или security filters;
- unresolved conflict передаётся модели с пометкой, а не разрешается молча.

## Критерии готовности

- planner имеет валидируемый контракт, bounded loop и deterministic stop;
- checker использует явные versioned metrics и сообщает unresolved conflicts;
- агент не утверждает документальный факт без evidence или явно маркирует
  uncertainty.
