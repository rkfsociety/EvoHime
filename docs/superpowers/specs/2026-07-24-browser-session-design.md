# Visual browser agent loop: CDP session reuse

## Цель

Начать `7.100`: дать агенту настоящий браузер вместо one-shot HTTP-fetch. Сессия Chrome DevTools Protocol живёт между вызовами инструментов одной задачи: JS выполняется, состояние страницы (SPA-навигация, формы, cookies) сохраняется, агент может смотреть → действовать → смотреть снова.

## Выбранный подход

EvoHime подключается к **уже запущенному** браузеру через `EVOHIME_BROWSER_CDP_URL` (например, `http://127.0.0.1:9222` от `chrome --remote-debugging-port=9222`). Сервер не запускает и не бандлит Chrome: это честная граница — функция работает там, где оператор дал браузер, и полностью отсутствует иначе. CDP-клиент реализуется поверх websocket (`tokio-tungstenite`) без тяжёлых обвязок вроде chromiumoxide: нужный поднабор — четыре команды протокола.

Одна задача — одна вкладка. Реестр сессий в tool-runtime ключуется `task_id`: первый `browser.session.navigate` создаёт target через DevTools HTTP API и открывает websocket; последующие вызовы любой из session-инструментов переиспользуют его. Завершение — явный `browser.session.close` или вытеснение по капу.

## Инструменты первой волны

- `browser.session.navigate {url, timeout_ms?}` — навигация в persistent-вкладке задачи; ждёт `Page.loadEventFired` (с graceful-таймаутом для SPA); возвращает `url`, `title`, текстовый preview.
- `browser.session.read {max_chars?}` — текущее состояние страницы **без** повторной навигации: url, title, текст DOM. Смысл session reuse: страница могла измениться после JS/кликов.
- `browser.session.click {selector, settle_ms?}` — клик по `document.querySelector(selector)`; после клика короткий settle и возврат нового url/title/preview; ошибка, если селектор не найден.
- `browser.session.close {}` — закрыть вкладку и освободить сессию.

Все — permission `BrowserAccess`, как существующие browser tools.

## Безопасность и лимиты

- URL навигации проходит существующую SSRF-валидацию (`ssrf::assert_safe_http_url`); сам CDP endpoint — доверенная конфигурация оператора, как `EVOHIME_SYNC_URL`.
- Кап одновременных сессий (4): создание сверх капа вытесняет самую старую сессию с закрытием её вкладки.
- Idle-сессии старше 10 минут закрываются лениво при следующем обращении к реестру.
- Команда CDP имеет таймаут; зависший websocket не блокирует задачу дольше tool timeout.
- `Runtime.evaluate` используется только с фиксированными выражениями инструментов; произвольный JS от модели не принимается (`browser.session.eval` сознательно не вводится в этой волне).
- Текст страницы усечён по лимитам, как в `browser.open`.

## Реализационные границы

Вне волны: скриншоты (визуальный канал для мультимодальных моделей), ввод текста в формы, произвольный eval, автозапуск браузера сервером, пер-оператор изоляция браузера. Всё ложится поверх того же CDP-клиента и реестра.

## Проверка

- unit-тесты: framing команд, разбор ответов/событий, реестр (reuse per task, кап, idle-вытеснение), валидация конфигурации;
- integration-тесты с mock CDP: wiremock для DevTools HTTP API + локальный websocket-сервер, отвечающий на `Page.enable`/`Page.navigate`/`Runtime.evaluate` и шлющий `Page.loadEventFired` — без реального Chrome;
- полный Rust workspace test, Clippy, frontend build.

## Критерий готовности

При запущенном Chrome с `--remote-debugging-port` и заданном `EVOHIME_BROWSER_CDP_URL` агент в одной задаче выполняет `navigate → click → read`, и `read` видит состояние после клика в той же вкладке; без конфигурации инструменты возвращают понятную ошибку; mock-тесты проходят без Chrome.
