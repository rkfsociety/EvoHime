# Runbook: добавление нового модуля без bootstrap-ловушки updater'а

## Суть проблемы (почему нельзя менять порядок)

`evohime-updater.exe` у уже установленных пользователей перед любым обновлением (включая самообновление updater'а) скачивает и валидирует `evohime.compatible.json` из релиза `compatibility`.

Если в `evohime.compatible.json` появляется новый модуль, которого нет в `MODULE_IDS` установленного у пользователя updater'а, и в этом updater'е нет forward-compatibility (пропуска неизвестных компонентов в `validate_compatible_manifest`), то:

1. Старый updater отклоняет `evohime.compatible.json` с ошибкой валидации.
2. Из-за этой ошибки updater не может дойти до шага выбора и скачивания новой версии **самого себя** (`updater`).
3. Возникает bootstrap-ловушка (deadlock): исправленный updater уже опубликован, но установленный у пользователя updater не может обновиться, потому что отвергает манифест, в котором описано это обновление.

Надёжного автоматического канала самообновления в обход `evohime.compatible.json` у старых версий updater'а нет (обновление минуя манифест возможно только вручную через `EvoHime-Setup.exe` или `scripts/repair-updater.ps1`).

## Правильный порядок действий при добавлении нового модуля

1. **Сначала выпустить новый `updater` с поддержкой модуля / forward-compatibility и обновить `installer` release.**
   - Добавить поддержку нового модуля (или убедиться, что в `validate_compatible_manifest` уже работает forward-compatibility: неизвестные `component.id` пропускаются без ошибки).
   - Поднять `release-versions/updater.txt` и `release-versions/installer.txt`, дождаться публикации нового `module-updater-v*` и пересборки `installer` (`EvoHime-Setup.exe` + `EvoHime-Setup.json`).
2. **Дождаться, пока updater с новой логикой окажется в актуальном релизе `installer`.**
   - Убедиться, что `EvoHime-Setup.json` в релизе `installer` указывает на коммит, в котором `evohime-update-agent` уже содержит forward-compatibility или новый модуль в `MODULE_IDS`.
   - Это гарантирует, что проверка `scripts/validate-manifest-bootstrap.ps1` в `.github/workflows/compatible-manifest.yml` пропустит новый манифест.
3. **Только после этого добавлять новый модуль в `compatible manifest`.**
   - Добавить модуль в `release-versions/<module>.txt`, `scripts/publish-compatible-manifest.ps1` (`$moduleIds`) и `.github/workflows/compatible-manifest.yml`.
   - CI запустит `scripts/validate-manifest-bootstrap.ps1` перед публикацией `evohime.compatible.json` и подтвердит, что установленный из `installer` updater сможет принять манифест и обновиться.

## Что делать, если bootstrap-ловушка уже сработала на клиентах

Если `evohime.compatible.json` с неизвестным модулем уже был опубликован раньше, чем обновился `installer`:

- Для новых и переустанавливаемых клиентов: пересобрать `installer` с актуальным `updater` (с forward-compatibility).
- Для уже установленных клиентов с застрявшим updater'ом:
  - скачать и запустить свежий `EvoHime-Setup.exe` из релиза `installer`, **либо**
  - выполнить скрипт восстановления `scripts/repair-updater.ps1`, который скачивает последний `module-updater-v*` напрямую из GitHub Releases, минуя `evohime.compatible.json`.
