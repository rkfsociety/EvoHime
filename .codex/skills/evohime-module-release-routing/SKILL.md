---
name: evohime-module-release-routing
description: Определить затронутые публикуемые модули EvoHime, правильно повысить только их patch-версии и проверить module-router/workflow gates.
---

# Module router и версии

## Сначала живые правила

Не полагайся на сохранённую карту модулей. После исходных изменений прочитай
`.github/workflows/module-router.yml`, все workflow, упомянутые его mapping-ом,
`release-versions/*.txt`, `scripts/module-release.ps1`, `scripts/native-package.ps1`
и связанные docs. Построй mapping module → source paths → version file →
workflow → artifact/tag/consumer.

В текущей архитектуре известны независимые releases `shell-host`, `ui-bundle`,
`core`, `supervisor`, `cli`, `analysis-worker`, `listener`,
`listener-runtime`, `transaction`, `verifier`, `updater`, а `installer` имеет
отдельный fixed release. Это только подсказка: добавление/переименование
модуля проверяй по живому router и workflow.

## Правило повышения

1. Получить список изменённых paths из полного diff и классифицировать их как
   source, test, docs, CI, packaging или release metadata.
2. Для каждого модуля доказать source-path ownership. Изменения только в
   документации, тестах, CI, workflow или release metadata сами по себе не
   требуют повышения module version.
3. Если source публикуемого модуля изменён, прочитать текущую строку
   `release-versions/<module>.txt`, проверить строгий `MAJOR.MINOR.PATCH` и
   увеличить только PATCH на 1. Не менять major/minor, product version,
   package version или незатронутые модули.
4. Если source модулей нет, явно записать «повышение версий не требуется».
5. Проверить, что version file входит в task-only commit и согласуется с
   workflow path filter и tag `module-<module>-v<semver>`. Не утверждать, что
   router уже запустился до push.

## Проверка post-push behavior

Для каждого изменённого source module опиши ожидаемое действие после push на
`main`: router увидит patch выше последнего module release, dispatch-ит ровно
соответствующий workflow, тот выполнит свои CI gates и при success опубликует
module release. Installer/compatibility запуск описывай только если живые
workflow подтверждают такую зависимость.

## Ограничения

Не публикуй релизы вручную, не запускай workflow и не меняй GitHub state без
прямого запроса. Не повышай версию из-за удобства или чтобы «разбудить» CI.
