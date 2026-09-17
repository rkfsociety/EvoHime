Это маленький сетевой bootstrap-установщик EvoHime. Он содержит только
updater UI, update-agent и transaction worker, после запуска получает точный
совместимый комплект module releases через fixed `compatibility` manifest и
проверяет SHA-256 каждого артефакта.

Для полного восстановления без загрузки модулей используйте полный установщик
из fixed release `installer`:
https://github.com/rkfsociety/EvoHime/releases/tag/installer

Bootstrap не трогает пользовательские данные в `%LOCALAPPDATA%\EvoHime` и не
запускает shell до успешного применения полного комплекта. На первом экране
кнопка запуска передаёт управление тому же verified module apply, если shell
ещё отсутствует.
