# 07. Voice pipeline и ambient audio

## Цель

Определить безопасные realtime-контракты для речи и ambient audio, сохранив
privacy-first поведение и текущую границу listener.

## Scope

- frame/event contracts для realtime pipeline;
- typed STT → LLM → TTS lifecycle;
- endpointing, interruption/barge-in, backpressure и cancellation;
- streaming transcript с segment/word timestamps;
- model manifest, preprocessing и quality fallback;
- permission, retention, deletion и bounded audio windows;
- optional offline speaker segmentation без вывода identity.

## Инварианты

- Текущий listener остаётся на `whisper.cpp`, пока отдельный PoC не докажет
  необходимость нового runtime.
- Ambient capture выключен без явного permission и имеет bounded retention.
- Speaker cluster не трактуется как личность пользователя.
- Audio/transcript events имеют provenance, consent и deletion semantics.
- Barge-in и cancellation не оставляют незавершённый активный stream.
- Voice worker получает capability-scoped session и не обходит Core policy.

## Тестовый контур

- 16 kHz preprocessing/format validation и quality fallback;
- endpointing, partial transcript и timestamp ordering;
- barge-in во время STT, LLM и TTS;
- backpressure, timeout, cancellation и worker crash;
- permission deny, retention expiry и forget;
- redaction transcript/audio metadata;
- CPU/GPU/memory budget и packaging smoke test.

## Критерии готовности

- realtime lifecycle полностью typed и replayable;
- privacy permission и deletion проверены end-to-end;
- stream bounded по времени, памяти и очереди;
- новый runtime не попадает в базовый package без PoC, license, resource и
  security review;
- качество измеряется через fixtures из 05.

## Зависимости

Требует 01–03. Для проверки качества использует 05; не зависит от browser или
vision.
