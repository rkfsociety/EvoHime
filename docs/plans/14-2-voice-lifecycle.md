# 14-2 — STT/TTS lifecycle и interruption

## Цель

Сделать endpointing, barge-in и cancellation корректными во всех стадиях
realtime voice pipeline.

## Изменения

1. Реализовать endpointing/VAD transitions с pre-roll, hangover и max segment
   limits, сохранив текущие listener limits.
2. Поддержать interruption/barge-in во время STT, LLM и TTS с одним terminal
   outcome и без зависших active streams.
3. Обработать backpressure, provider timeout, worker crash и cancellation с
   deterministic cleanup.
4. Синхронизировать transcript, model response и TTS receipts с execution
   ledger; streaming delta не заменяет durable result.
5. При provider/engine degradation возвращать typed unknown/degraded, а не
   выдавать неполный ответ за успешный.

## Проверки

- barge-in на каждой стадии;
- cancellation до старта, во время обработки и после результата;
- queue saturation/backpressure;
- worker crash и restart recovery;
- отсутствие незавершённого stream после любого terminal event.

## Готово, когда

Любое прерывание закрывает stream, сохраняет корректный terminal event и не
оставляет TTS/STT worker активным.
