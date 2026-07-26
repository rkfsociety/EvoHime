# EvoHime

Браузерная платформа для AI-агентов. Интерфейс работает в браузере, сервер — на Rust, данные хранятся в PostgreSQL.

## Быстрый запуск на Windows

Первичная настройка:

```powershell
.\scripts\setup-local.ps1 -InstallPostgres -ApplyMigrations
```

Запуск локального стека:

```powershell
.\start-dev.ps1
```

После запуска:

- веб-интерфейс: http://localhost:5173;
- проверка API: http://localhost:3000/health.

## Разработка

```powershell
# фронтенд
Set-Location frontend/web
npm install
npm run dev

# сервер
cargo run -p evohime-server
```

Переменные окружения находятся в `.env.example`. Для работы модели LiteRouter укажите `LITEROUTER_API_KEY` и, при необходимости, `LITEROUTER_MODEL`.

## Документация

Подробная документация, архитектура, настройка, roadmap и инструкции для разработчиков находятся в [Wiki проекта](https://github.com/rkfsociety/EvoHime/wiki).

Исходные документы в репозитории:

- [текущий статус](docs/current-state.md);
- [план разработки](docs/development-plan.md);
- [roadmap](docs/roadmap.md);
- [архитектура](docs/architecture.md);
- [инструкции для AI-агентов](AGENTS.md).
