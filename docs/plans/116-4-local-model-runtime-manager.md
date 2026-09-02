# План 116.4 — Local Model Runtime Manager: verification, release-evidence и закрытие

Статус: этап 4 для [плана 116.0](./116-0-local-model-runtime-manager.md); после [плана 116.3](./116-3-local-model-runtime-manager.md).

## Цель

Подтвердить все criteria issue #96 свежими воспроизводимыми тестами, security
evidence и redacted release record. До этого направление не считается реализованным.

## Зависимости

### Блокирующие

- План 116.3 — полный Core/runtime/IPC/UI vertical slice.
- Project release-evidence, architecture/current-state/development-plan и security gates.

### Опциональные

- Benchmark, support-bundle и cache integrations расширяют evidence, но не заменяют
  базовые contract/runtime/security tests.

## Матрица проверки

- [ ] CPU-only, sufficient/insufficient VRAM, multiple adapters, missing driver/runtime,
  incomplete data → conservative Unknown и invalidated fit cache.
- [ ] Verified download, wrong hash, interrupted/cancelled staging, disk full, atomic
  promotion и immutable new revision.
- [ ] Successful start/load/probe, executable/version mismatch, OOM, startup timeout,
  crash isolation, process-tree cleanup, active-call eviction block, idle unload и
  stale restart.
- [ ] ModelProfile registration, capability propagation, active safe context,
  resilience participation и strict selection preservation.
- [ ] Bootstrap available before preferred, background continuation, failed preferred,
  safe next-call switch, Manual no-switch и NewConversationsOnly snapshot.
- [ ] Remote-code/custom executable rejection, no credentials, untrusted endpoint
  rejection, renderer forgery rejection, path escape rejection и metadata-only logs.
- [ ] IPC auth, bounds, replay/resync, duplicate/stale/idempotency and UI projection.

## Обязательные gates

1. Focused Core contract/storage/runtime/recovery/security tests с migration backup,
   rollback и fault injection.
2. Rust `cargo fmt --all -- --check`, relevant `cargo clippy --locked ... -- -D warnings`,
   full affected Rust tests и desktop IPC tests.
3. Electron `npm run check:protocol`, `npm run typecheck`, focused manager tests,
   `npm test`, production build/bundle checks и native package smoke при изменении
   packaging/runtime delivery.
4. `git diff --check`, redaction scan, dependency/license/provenance review и
   recovery/disable procedure. Evidence содержит commit, schema/protocol versions,
   test IDs, hashes, typed outcomes и omission summary, но не credentials, raw
   prompts/outputs, absolute paths, PII или model bytes.

## Release-evidence и закрытие

После подтверждения всех criteria перенести фактический контракт в
`docs/architecture.md`, состояние и test totals в `docs/current-state.md`, порядок
и release gates в `docs/development-plan.md`/`docs/release-evidence.md`. Зафиксировать
rollback: отключение manager оставляет existing external profiles работоспособными,
а unknown runtime/artifact outcome не объявляется успехом и не повторяется вслепую.

После полного закрытия направления удалить комплект `116-0` … `116-4` согласно
`docs/plans/README.md`; до этого stage-файлы сохраняются для трассировки.

## Definition of Done

- [ ] Все criteria матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты либо есть явно принятый typed degradation.
- [ ] Ссылки, schema/tags, package/runtime versions и фактические пути сверены с checkout.
- [ ] Release bundle redacted, rollback и recovery procedure записаны.

## Связанный issue

- [issue #96](https://github.com/rkfsociety/EvoHime/issues/96)

