# Evaluation catalog и smoke gates

Локальный deterministic gate запускается из корня репозитория:

```powershell
.\scripts\eval-gate.tests.ps1
.\scripts\security-eval-gate.tests.ps1
cargo eval --fixture tests/evals/fixtures --case tool-use-001 --mode deterministic --verbose
```

Каталог `tests/evals/` содержит синтетические versioned fixtures, JSON Schema и
пороговые значения. Runner проверяет bounded limits, обязательные поля,
запрещённые secret/PII markers, запускает существующие Core deterministic evals
и печатает JSONL verdict с fingerprint и redacted trace. `fail`, `blocked`,
`no_verdict`, `flaky` и неразрешённый `skipped` не являются pass.

`static` предназначен для cassette fixtures, `deterministic` — для быстрых PR
и Gate B, `real` допускается только в model-dependent/nightly jobs и требует
`model_profile`. Smoke packaged runtime остаётся отдельным Gate C и не заменяет
evaluation или Gate S.

Каждый новый feature/regression fixture должен содержать happy-path и
failure/edge-path, уникальный `id`/`fixture_version`, synthetic data и одну
локальную команду воспроизведения. Артефакт CI сохраняется как redacted JSONL в
`artifacts/eval-gate/summary.jsonl`.
