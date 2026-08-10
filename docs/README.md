# Документация EvoHime

## Канонические документы

- `README.md` — установка, запуск разработки и пользовательский релиз;
- `docs/architecture.md` — native Windows architecture и IPC;
- `docs/current-state.md` — фактическое текущее состояние;
- `docs/development-plan.md` — текущий план реализации;
- `AGENTS.md` — правила работы с репозиторием;
- `SECURITY.md` — security boundary локального клиента;
- `docs/providers/literouter.md` — настройка model provider для Core.

## Пользовательская модель

Продукт — один локальный Windows EXE-клиент. Пользователь скачивает `EvoHime-Setup.exe`, устанавливает приложение и запускает один ярлык `EvoHime`. Короткое имя агента — **Ева**. `evohime-core.exe` и `evohime-supervisor.exe` являются скрытыми внутренними компонентами runtime.

## Рабочие правила

Для разработки используйте `.\start-dev.ps1`, native package tests, WinUI tests и Windows CI. Установщик и пользовательский запуск работают через `EvoHime.exe`.

Веб-панель полностью выведена из продукта. `start-dev.ps1` — это native launcher: он собирает пакет и открывает WinUI-клиент `EvoHime.exe`; клиент сам запускает единственный скрытый supervisor, а supervisor — Core. `-SkipBuild` допустим только при наличии готового `.evohime-native\windows-x64`.
