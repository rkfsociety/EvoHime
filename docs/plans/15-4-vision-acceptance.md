# План 15.4. Приёмка vision и document worker

## Цель

Свести контракт, evidence и изоляцию в повторяемый набор acceptance и release
проверок.

## Изменения

- Добавить deterministic fixtures для изображений, коротких clips,
  многостраничных документов, OCR, cross-page вопросов и повреждённых входов.
- Зафиксировать benchmark через план 12: качество, latency, memory, размер
  evidence и поведение quality fallback сравниваются с versioned baseline.
- Проверить supervisor/Core recovery, IPC compatibility, cancellation,
  redaction, capability/approval и невозможность host action из output.
- Оформить release checklist для optional packaging, модели, лицензий,
  privacy/egress, временных файлов и обслуживания версий.
- Зафиксировать rollback: отключение backend, очистка staging/temp artifacts и
  безопасное возвращение к typed unsupported/unknown.

## Проверки

- полный fixture matrix на clean checkout без optional backend;
- budget, crash/restart, cancellation и replay прогон с диагностикой;
- security/privacy/licensing review и `git diff --check` для сопутствующих
  документов и схем;
- подтверждение, что benchmark evidence не попадает в production secrets или
  unrestricted logs.

## Готово, когда

Acceptance matrix зелёная, benchmark и ресурсные лимиты опубликованы, optional
worker можно включить и отключить без изменения базовых гарантий, а release
evidence подтверждает provenance, redaction, isolation и rollback.

