# Как участвовать в EvoHime

Спасибо за интерес к проекту. EvoHime — локальное Windows-приложение с
Electron shell, Rust Core, SQLite и supervisor. Перед изменением кода полезно
прочитать [`AGENTS.md`](AGENTS.md), [`docs/architecture.md`](docs/architecture.md),
[`docs/current-state.md`](docs/current-state.md) и [`SECURITY.md`](SECURITY.md).

## Сообщить об ошибке или предложить идею

- Для обычной ошибки используйте шаблон [Bug report](https://github.com/rkfsociety/EvoHime/issues/new?template=bug_report.yml).
- Для предложения используйте шаблон [Feature request](https://github.com/rkfsociety/EvoHime/issues/new?template=feature_request.yml).
- Уязвимости не публикуйте в issue: порядок сообщения описан в
  [`SECURITY.md`](SECURITY.md).

Перед отправкой проверьте, нет ли уже открытого issue с тем же вопросом. Не
прикладывайте API-ключи, токены, credentials, персональные данные, полные
логи с секретами или исходники приватных проектов.

## Локальная разработка

Для разработки нужны Windows 10 2004+ или Windows 11 x64, PowerShell 7+,
Rust MSVC toolchain и Node.js 22 LTS. Основные команды:

```powershell
pwsh -File .\start-dev.ps1
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
cargo check -p evohime-supervisor
```

Для Electron shell:

```powershell
cd desktop\evohime-electron
npm run bootstrap
npm run check:protocol
npm run typecheck
npm test
```

Real-Core E2E требует собранный Core. Полный acceptance-набор Rust, Electron,
native package и installer выполняется в GitHub Actions; локально выбирайте
объём проверки по области изменения.

## Требования к изменениям

- Core остаётся единственным владельцем runtime-состояния, SQLite, прав и
  эффектов; renderer получает только IPC-проекцию.
- Новые Rust-функции и исправления должны иметь соответствующие тесты.
- Изменения IPC требуют синхронного обновления Rust и Electron-сторон,
  generated bindings и regression tests.
- Изменения архитектуры и пользовательского поведения должны обновлять
  соответствующие канонические документы.
- Не включайте в commit секреты, локальные кэши, `target/`, `node_modules/`,
  абсолютные пути и временные артефакты.

Перед pull request проверьте `git diff --check` и убедитесь, что в diff нет
посторонних файлов.

## Pull request

Опишите проблему, решение и затронутые границы. Для изменений с observable
поведением добавьте способ проверки и укажите, какие проверки были запущены.
Если проверка оставлена GitHub Actions, напишите это явно. Если изменение
затрагивает миграции, IPC, упаковку или безопасность, отдельно опишите
совместимость, recovery/rollback и ограничения.

Pull request должен быть небольшим и проверяемым. Не смешивайте рефакторинг,
несвязанные форматирующие изменения и функциональное изменение без причины.
