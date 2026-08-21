# План 15.2. OCR, provenance и visual evidence

## Цель

Сделать ответы vision/document worker проверяемыми на уровне страницы и кадра,
включая OCR, multilingual text и вопросы, связывающие несколько страниц.

## Изменения

- Ввести stable page/frame references с artifact id, номером страницы или
  timestamp/frame index, crop/region metadata и confidence.
- Представлять OCR как typed evidence, отделяя извлечённый текст от вывода
  модели и явно маркируя низкую уверенность.
- Определить page-aware aggregation для многостраничных вопросов: каждый
  использованный фрагмент обязан иметь provenance, а отсутствие evidence даёт
  unknown, а не догадку.
- Пропускать text/evidence через обычные policy, approval и redaction
  boundaries; секреты и PII не должны появляться в диагностике без разрешения.
- Сохранить компактные ссылки на evidence вместо неограниченного raw media в
  IPC, логах и durable state.

## Проверки

- OCR на Unicode и нескольких языках с ошибками чтения;
- page/frame selection, crop provenance и cross-page question fixtures;
- несовпадающие или отсутствующие ссылки, низкая confidence и typed unknown;
- PII/secret redaction в evidence, error и export paths;
- повторяемый результат на одном fixture при одинаковом backend version.

## Готово, когда

Каждый утверждающий visual ответ имеет проверяемые page/frame evidence links,
OCR и model output различимы, cross-page aggregation bounded, а отсутствие
доказательства никогда не превращается в уверенный факт.

