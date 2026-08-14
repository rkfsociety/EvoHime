# Этап 01.3: Compression и pruning

Этап плана [01 Context Budget Manager](01-0-context-budget-manager.md).

## Зависимости

Блокирующие: этапы 01.1 (budget и `drop_reason`) и 01.2 (artifact store для
original items).

Разблокирует: никого — внутренний этап плана.

## Что этап отдаёт наружу

Ничего: сжатие и pruning видны только внутри сборки контекста.

## Содержание

- Перед моделью удалять дубликаты, старые tool outputs и записи с меньшим
  приоритетом.
- Разграничить причины удаления. `duplicate` — совпадение `content_hash` из
  01.1 (нормализованное содержимое), остаётся экземпляр с более высоким
  уровнем иерархии, при равенстве — более свежий. `superseded` — новая ревизия
  того же `parent_id`/ключа заменила запись по смыслу при другом содержимом.
  `expired` — истёк TTL или retention независимо от содержимого.
- При превышении `soft_limit` запускать отдельный bounded summarizer с
  собственным `summary_budget`, входным лимитом и запретом tool calls/retries.
  Если summarizer недоступен, превышает свой бюджет или возвращает invalid
  output, применять deterministic fallback без каскадного повторного запуска.
- Fallback удаляет сначала expired/duplicate/low-priority items, затем самые
  старые tool outputs, сохраняя system/policy/approval/user constraints,
  подтверждённые факты, числа, пути, отрицания и валидные пары tool-call/result.
  Нельзя резать середину сообщения или нарушать состояние незавершённого
  tool-call.
- Original items остаются source of truth в ledger/artifact store. Summary —
  только projection для текущего model call; сохранять связь
  `summary_id -> source_ids`, tokenizer/profile versions и возможность
  повторной сборки.
- Для system/instructions действует иерархия прав, а не простая recency:
  safety/hard-deny и approval policy > system instructions > явные ограничения
  пользователя > confirmed task decisions/facts > history/tool data >
  recovered/unverified. Новая запись не может понизить более высокий уровень.
  Для facts применять conflict detection и label `conflicting`, а не silent
  override; при существенном конфликте нужен пользовательский confirmation.
- Recency и trust работают только как тай-брейк внутри одного уровня иерархии:
  при равном уровне побеждает более доверенная запись, при равном trust — более
  свежая. Между уровнями свежесть не даёт преимущества, поэтому свежий tool
  output не вытесняет policy, system instruction или явное ограничение
  пользователя.
- Conflict detection первой версии детерминированный: совпадение ключа/сущности
  плюс расхождение по числам, путям, идентификаторам или отрицанию при разном
  `content_hash`. Semantic-детекция противоречий добавляется только после
  evaluation fixtures и измерения precision/recall; до этого неоднозначные
  случаи помечаются `conflicting` и решаются пользователем, а не эвристикой.

## Проверки

- unit-тесты конфликтного pruning и иерархии прав: новая запись не понижает
  более высокий уровень, recency решает исход только внутри уровня;
- разграничение `duplicate`/`superseded`/`expired` воспроизводимо: одинаковый
  `content_hash` даёт `duplicate`, новая ревизия того же ключа — `superseded`,
  истёкший TTL — `expired`;
- Core integration: большой tool output → offload → summary → replay;
- тесты сохранения чисел, путей, отрицаний, policy/permission rules и валидного
  tool-call state после compression;
- недоступный, превысивший бюджет или вернувший invalid output summarizer даёт
  deterministic fallback без каскадного повторного запуска;
- load tests для длинной истории и большого числа tool calls.

## Критерии готовности

- oversized history не ломает задачу, не вызывает неограниченное усечение и
  сохраняет минимально обязательный контекст;
- ledger и метрики показывают compression quality/ratio;
- summary восстановим до original items по `summary_id -> source_ids`.
