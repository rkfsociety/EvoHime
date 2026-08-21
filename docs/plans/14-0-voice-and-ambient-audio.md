# План 14 — Voice pipeline и ambient audio

## Цель

Определить безопасные realtime-контракты для речи и ambient audio, сохранив
privacy-first поведение и текущую границу listener.

## Что уже есть в checkout

- whisper.cpp listener runtime и verified model manifest;
- bounded capture, resampling, VAD, segmentation и deduplication;
- authenticated listener pipe, microphone permission и ambient policy;
- transcript storage, retention, forget и memory/proactivity gates.

План 14 не заменяет whisper.cpp новым runtime без отдельного PoC и review.

## Границы

Входит: realtime frame/event contracts, typed STT→LLM→TTS lifecycle,
endpointing, barge-in, backpressure, streaming timestamps, model manifest,
quality fallback, consent/retention/deletion и optional offline speaker
segmentation без identity.

Не входит: continuous capture без permission, speaker identity, unbounded
audio storage, unrestricted worker access или новый runtime в базовом package.

## Зависимости

### Блокирующие

- планы 08–12 для ledger, policy, IPC, memory provenance и evaluation;
- текущие listener-contract, supervisor lifecycle, ambient storage и privacy
  permission.

### Опциональные

- TTS/LLM provider; без него STT и transcript lifecycle работают локально,
  а voice response возвращает typed `provider_unavailable`;
- offline speaker clustering; без него speaker остаётся `unverified`.

## Этапы

- [14-1 — realtime voice contract](14-1-voice-realtime-contract.md)
- [14-2 — STT/TTS lifecycle и interruption](14-2-voice-lifecycle.md)
- [14-3 — privacy, worker и resource limits](14-3-voice-privacy-runtime.md)
- [14-4 — acceptance и quality gate](14-4-voice-acceptance.md)

Порядок: 14-1 → 14-2 → 14-3 → 14-4.

## Готово, когда

Realtime lifecycle полностью typed/replayable, permission и deletion проверены
end-to-end, stream bounded по времени/памяти/очереди, а новый runtime не
попадает в package без PoC, license, resource и security review.
