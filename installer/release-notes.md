# EvoHime — единственный web installer

`EvoHime-Setup.exe` — единственный установщик EvoHime. Он содержит только
самостоятельный updater UI, Rust update-agent с transaction engine и bootstrap
marker. После запуска updater получает fixed `compatibility` manifest,
скачивает точные module releases, проверяет SHA-256 и устанавливает полный
клиент.

**Постоянная ссылка:**
https://github.com/rkfsociety/EvoHime/releases/tag/installer

Установщик используется и для первой установки, и для восстановления уже
установленного клиента. Второго offline/full installer нет: для работы нужен
доступ к GitHub Release проекта.

## Поведение

- устанавливается один updater и создаётся один ярлык `EvoHime`;
- shell, Core и supervisor скачиваются после проверки compatibility manifest;
- пользовательские данные `%LOCALAPPDATA%\EvoHime` не удаляются;
- старый клиент закрывается, а обновление выполняется через verified staging,
  backup и rollback;
- после успешного health-check запускается обычная Eva.

## Публикация и проверки

Изменение версии `release-versions/installer.txt` запускает быстрый
`bootstrap-installer.yml`. Workflow собирает только web installer из
опубликованного updater module, проверяет marker/content gates и заменяет
постоянный release `installer`. Module releases и compatibility manifest
публикуются отдельными workflow; полный native package acceptance не является
вторым установщиком.

Требования: Windows 10 2004+ или Windows 11 x64 и доступ к GitHub.
