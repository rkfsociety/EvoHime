# 13-2 — Browser network policy и evidence

## Цель

Ограничить browser egress и сделать DOM/accessibility/screenshot evidence
проверяемым и provenance-aware.

## Изменения

1. Проверять исходный URL, redirect target, DNS/private IP и credential URL
   непосредственно перед каждым navigation/fetch.
2. Запретить localhost/private/internal targets и неразрешённые domains;
   downloads и external redirects получать отдельный typed policy outcome.
3. Добавить bounded DOM/accessibility snapshot, page state, tabs и artifact
   references с run/event provenance.
4. Redact credentials, cookies, authorization headers, secrets и PII до
   durable storage, receipt и IPC projection.
5. Привязать browser evidence к execution ledger и memory/RAG citation path;
   stale page evidence не подтверждает факт.

## Проверки

- SSRF/private IP/redirect/credential URL fixtures;
- navigation, download, timeout и egress denial;
- DOM/accessibility evidence bounds и redaction;
- screenshot/trace artifact provenance и stale evidence;
- cancellation во время network operation.

## Готово, когда

Каждый внешний browser target проверен повторно, а evidence можно связать с
конкретной страницей, событием и permission snapshot без утечки credentials.
