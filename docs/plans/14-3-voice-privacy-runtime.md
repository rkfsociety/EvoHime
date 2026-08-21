# 14-3 — Privacy, worker и resource limits

## Цель

Сохранить privacy-first capture и ограничить ресурсы voice worker.

## Изменения

1. Требовать microphone capability/consent до capture; pause, quiet hours,
   blocklist и revoke permission должны закрывать capture, а не просто
   фильтровать кадры.
2. Сохранить bounded audio windows, transcript retention, deletion/forget и
   metadata-only tombstones.
3. Передавать worker только capability-scoped session; audio/transcript
   provenance и deletion semantics не обходят Core.
4. Ввести CPU/GPU/memory/disk/latency/queue budgets и quality fallback для
   unsupported format или degraded engine.
5. Speaker clustering оставить optional и всегда `unverified`; identity
   inference запрещена.
6. Проверять runtime manifest, hashes, ABI, license и packaging до загрузки.

## Проверки

- permission deny/revoke, quiet hours и retention expiry;
- deletion/forget с отсутствием orphan transcript/projections;
- redaction transcript/audio metadata;
- CPU/GPU/memory/queue exhaustion;
- runtime manifest/hash/ABI/package rejection.

## Готово, когда

Без consent ничего не захватывается, после forget данные и projections
удаляются по контракту, worker bounded, а runtime trust проверен до загрузки.
