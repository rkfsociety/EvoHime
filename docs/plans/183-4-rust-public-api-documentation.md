# План 183.4 — Rustdoc gates, verification и closure

## Изменить

1. Ввести согласованный workspace CI gate для `cargo doc --workspace
   --no-deps --locked` и `cargo test --workspace --doc --locked` (или эквивалент
   workflow matrix, если target/platform особенности требуют разбиения).
2. Включить `missing_docs` для всех завершённых публичных library surfaces;
   оценить общий workspace enforcement по фактическому inventory. Исключения
   допустимы только узкие, локальные и снабжённые объяснением.
3. Проверить generated docs для каждого library target, intra-doc links,
   feature/platform configurations и совместимость с текущими CI runners.
4. Обновить `AGENTS.md`, `docs/README.md`, `docs/current-state.md` и
   `docs/release-evidence.md` только в той мере, в какой изменились обязательные
   команды, достигнутое покрытие и подтверждённые проверки. Удалить этапы плана
   после прохождения всех критериев.

## Зависимости

### Блокирующие

- Завершённые этапы 183.1–183.3; полное inventory без непокрытых внешних items.
- Все целевые workspace doc-tests и rustdoc gates проходят на поддерживаемых
  CI targets.

### Опциональные

- Дополнительные rendered-doc screenshots/release artifacts, если текущий CI
  умеет создавать их без нового сервиса или зависимости.

## Проверка

Свежий полный `cargo doc`/doc-test CI evidence, проверка rustdoc warnings и
внутренних ссылок, `pwsh -NoProfile -File scripts/documentation.tests.ps1`,
`git diff --check` и self-review scope/security. Не запускать локальные команды,
которые существенно дублируют обязательный CI без необходимости.

## Rollback и evidence

Если какой-либо target не может быть документационно собран из-за установленной
ограниченности среды, сохранить общий gate для доступных targets и завести
точно ограниченный CI matrix gate для оставшихся. Не отключать документационную
проверку всего workspace; указать недоступный target и причину в release evidence.
