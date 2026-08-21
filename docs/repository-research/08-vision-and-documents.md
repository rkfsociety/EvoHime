# 08. Vision и document worker

## Цель

Добавить optional offline perception для разрешённых изображений, video clips
и документов, не расширяя базовый runtime до непрерывного наблюдения или
автоматических visual-agent действий.

## Scope

- bounded image/video/document input;
- visual budget и лимиты страниц, кадров и разрешения;
- page/frame provenance и evidence references;
- OCR и multilingual visual QA;
- page-aware answers для многостраничных документов;
- benchmark fixtures и quality fallback;
- worker isolation, cleanup и resource limits.

## Не входит

- continuous capture;
- автоматическое выполнение действий по одному visual output;
- тяжёлый Python/CUDA/PyTorch runtime в Electron package или Rust Core без
  отдельного решения;
- unrestricted access к камере, экрану, filesystem или network.

## Инварианты

- Worker получает только явно переданные artifacts и capability snapshot.
- Ответ ссылается на page/frame evidence и не считается фактом без provenance.
- Лимиты размера, страниц, кадров, памяти, времени и стоимости обязательны.
- Vision output проходит обычные policy, approval и redaction boundaries.
- Ошибка модели и низкая уверенность — typed unknown/diagnostic, а не команда.

## Тестовый контур

- page/frame selection и provenance;
- OCR/document evidence и cross-page questions;
- oversized input, unsupported format и budget exhaustion;
- worker crash, cancellation и cleanup;
- PII/secret redaction;
- deterministic benchmark через 05;
- packaging, licensing, privacy, egress и maintenance review.

## Критерии готовности

- worker изолирован и optional;
- все inputs/outputs bounded и provenance-aware;
- quality fallback и typed unknown реализованы;
- visual output не может напрямую вызвать host action;
- benchmark и ресурсные лимиты проходят release gate.

## Зависимости

Требует 01–03 и 05. Отдельное решение по worker process обязательно до
реализации.
