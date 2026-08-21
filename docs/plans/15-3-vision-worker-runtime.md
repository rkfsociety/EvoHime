# План 15.3. Изоляция и runtime worker

## Цель

Запустить optional vision backend так, чтобы сбой модели, зависание, утечка
ресурсов или неподдерживаемый формат не нарушали Core, Electron и policy
границы.

## Изменения

- Выбрать отдельный worker process или иной изолированный boundary и описать
  его launch context, capabilities, IPC frame limits и отсутствие host action
  authority.
- Реализовать cancellation, deadline, memory/CPU/disk/network limits,
  temporary artifact scope и гарантированный cleanup после success, error,
  timeout или crash.
- Ограничить backend allowlist и packaging surface; модель/движок не должен
  незаметно добавлять Python, CUDA, filesystem или network dependency в базовый
  Electron/Rust runtime.
- Возвращать typed unsupported/backend-failed/timeout/low-confidence errors с
  quality fallback, не маскируя их пустым ответом.
- Добавить redacted diagnostics, health state и безопасное удаление временных
  media artifacts.

## Проверки

- worker crash, cancellation, timeout, repeated launch и cleanup;
- превышение каждого resource budget и запрет выхода из artifact scope;
- отсутствие прямого действия через visual output и корректная revalidation
  capability/policy на границе Core;
- optional backend отсутствует, повреждён или имеет неверную версию;
- packaging, licensing, privacy, egress и maintenance review.

## Готово, когда

Worker можно отключить без потери базового runtime, каждый запуск bounded и
recoverable, временные данные очищаются, а backend и его зависимости проходят
явный security/licensing/packaging gate.

