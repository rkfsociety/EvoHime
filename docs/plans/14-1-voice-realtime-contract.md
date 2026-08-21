# 14-1 — Realtime voice contract

## Цель

Зафиксировать bounded frame/event contract для audio capture, STT, LLM и TTS.

## Изменения

1. Ввести typed frame metadata: stream/run/session IDs, sample rate, channels,
   format, sequence, timestamps, bounded duration и consent snapshot.
2. Описать STT segment/word timestamps, partial/final transcript, confidence,
   language и typed unknown/degraded states.
3. Описать LLM/TTS started, delta, completed, interrupted, cancelled, failed и
   provider-unavailable events.
4. Сохранить provenance, event linkage и redaction metadata без хранения PCM
   или raw credentials в durable journal.
5. Ввести bounded queue/backpressure contract и monotonic ordering.

## Проверки

- 16 kHz preprocessing/format validation;
- timestamp ordering и partial/final transition;
- frame/queue/payload bounds;
- schema round-trip, replay и malformed frame rejection.

## Готово, когда

Каждый stream event typed, ordered, cancellable и связан с consent, run и
execution provenance.
