# 06. Изолированный browser backend

## Цель

Добавить управляемый браузерный capability без превращения desktop UI в
неограниченный computer-use runtime.

## Scope

- отдельный BrowserContext на run;
- typed navigation, click, type, content, state и tabs receipts;
- locators/actionability вместо координатных кликов;
- accessibility/DOM snapshot как evidence;
- network policy, redirect/SSRF checks и ограничение egress;
- screenshot, trace и artifact references;
- lifecycle, packaging и cleanup browser process.

## Не входит

- browser extension;
- управление всем desktop по умолчанию;
- unrestricted host/network access;
- передача модели сырых credentials;
- подключение backend до отдельного packaging/security решения.

## Инварианты

- BrowserContext и credentials изолированы по run.
- Каждое действие проходит capability, scope, approval и cancellation.
- Redirect target проверяется повторно; localhost/private/internal targets
  запрещаются policy-слоем.
- Evidence имеет run/event provenance.
- Browser backend не становится источником durable state вне Core.

## Тестовый контур

- context isolation между runs;
- locator/actionability failure;
- navigation, redirect, download и timeout policy;
- SSRF/private IP/credential URL checks;
- cancellation и cleanup после crash;
- replay receipts без повторного side effect;
- packaging smoke test и отсутствие внешнего Node runtime в продукте.

## Критерии готовности

- browser capability включается явно и permission-gated;
- egress и filesystem scope проверяются до действия;
- trace/screenshot artifacts redacted и привязаны к event ledger;
- browser process завершается при run cancellation/supervisor recovery;
- отдельный security и packaging review пройден.

## Зависимости

Требует 01–03. Evaluation из 05 желательно использовать как release gate.
