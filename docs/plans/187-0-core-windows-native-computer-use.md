# План 187.0 — Core-owned Windows native computer-use execution adapter

Статус: active implementation contract. Исходный issue #163 удаляется после сохранения плана; план не означает, что функциональность уже реализована.

## Цель

Расширить metadata-only native_computer_use_runtime до Core-owned Windows execution с exact window identity, fresh observation, UI Automation-first action, background-first delivery и отдельной postcondition verification.

## Основание checkout

native_computer_use_runtime.rs и его SQLite store сохраняют metadata-only contract. ToolRegistry, PolicyGate, approvals, ReceiptRuntime, EventJournal, Supervisor/Job Object, ArtifactStore и cross-modal grounding остаются owners полномочий, lifecycle и evidence.

## Граница

ToolRegistry → PolicyGate/approval/receipt → Core coordinator → ephemeral target/observation registry → Windows UIA/action adapter → effect readback/postcondition verifier → EventJournal/ledger → plans 184/185 and Benchmark Matrix.

## Изменяемые контракты

- Exact window target, ephemeral observation, one-use action binding, delivery mode, approval/receipt refs and postcondition result.
- Windows adapter/coordinator lifecycle, bounded IPC projection and GUI regression fixtures; no new authority owner.

## Recovery и rollback

After possible dispatch use UnknownOutcome and never retry blindly; invalidate observations on geometry, generation or session changes. Rollback disables action admission while preserving ledger evidence; screenshot/UIA payloads remain ephemeral by default.

## Этапы

- [Этап 1 — target/observation/action/effect contracts](./187-1-core-windows-native-computer-use.md)
- [Этап 2 — Windows UIA adapter, dispatch и recovery](./187-2-core-windows-native-computer-use.md)
- [Этап 3 — ToolRegistry/policy/approval/IPC projections](./187-3-core-windows-native-computer-use.md)
- [Этап 4 — GUI regressions, package acceptance и release evidence](./187-4-core-windows-native-computer-use.md)

## Зависимости

### Блокирующие

- ToolRegistry, PolicyGate, approval, ReceiptRuntime and EventJournal/effect ledger.
- Supervisor/helper lifecycle, authenticated IPC; ArtifactStore for explicitly retained short-lived screenshots.
- Планы 184 и 185 для required trace and GUI behavioral regression evidence.

### Опциональные

- Cross-modal grounding может предложить candidate refs, но не target/effect authority.
- Screenshot optional; UI Automation preferred for supported controls.

## Критерии готовности

- [ ] Target identity включает process start, exact HWND/window generation и app identity; ambiguous/recreated targets refused.
- [ ] Observation короткоживущий; pixel authority one-use и инвалидируется при geometry/DPI/generation change.
- [ ] Background path не меняет foreground/focus/cursor; foreground отдельный approved mode с recheck/restoration evidence.
- [ ] Dispatch не равен success; отдельный verifier различает Confirmed/Unverifiable/SuspectedNoop/UnknownOutcome; unknown mutation не retry-ится вслепую.
- [ ] Screenshots/UI tree/text/clipboard/selectors ephemeral по умолчанию; durable только bounded refs/hashes/reasons.
- [ ] Windows E2E доказывает exact target, focus preservation, crash/cancel/recovery; canonical docs/release evidence обновлены.

## Non-goals

- macOS/Linux/Android, public daemon/cloud control plane или второй browser stack.
- Unrestricted global input, auto foreground escalation, UIPI/UAC bypass или actuator acknowledgment как task success.

## Verification

Плановые gates: target/action contract tests → Windows UIA/background delivery fixtures → approval/receipt/trace/behavior integration → focused packaged Windows E2E for focus, recovery and session limits.

## Release evidence

Сохранять adapter/target-policy hashes, bounded action/effect/verifier outcomes, focus/restoration status, CI package evidence and reason codes; не сохранять screenshots, UI tree, typed text или clipboard без explicit artifact policy.

## Критерии выхода overview

- [ ] Exact target semantics, background refusal and foreground approval boundary согласованы до этапа 1.
- [ ] Plans 184/185 и Windows CI fixture strategy доступны до полного acceptance этапа.

## Исходная постановка

- [Issue #163 — Core-owned Windows native computer-use execution adapter](https://github.com/rkfsociety/EvoHime/issues/163); сохранённый номер исходной постановки, issue удаляется после публикации плана.
