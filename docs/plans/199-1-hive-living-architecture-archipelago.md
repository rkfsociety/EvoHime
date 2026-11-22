# План 199.1 — Контракт `HiveCityMapSnapshot` и read-only сканер архитектуры в `evohime-hive`

Статус: active implementation contract. Этап следует после overview 199.0.

## Scope и изменяемые контракты
1. Добавить модуль `crates/evohime-hive/src/city_map.rs` (с полными `///`-доками публичных типов под `-D missing_docs` и без `unwrap`/`expect`/`panic` в production-коде).
2. Определить версионированный read-only контракт (`HIVE_CITY_MAP_SCHEMA_VERSION = 1`, `#[serde(deny_unknown_fields)]`):
   - `HiveCityMapSnapshot`: `schema_version`, `generated_at_ms`, `revision_hash`, `capital: CityIsland`, `satellites: Vec<CityIsland>`.
   - **Детерминизм идентификаторов и `revision_hash`:**
     - `CityIsland.id`: для Столицы фиксированный `"capital-evohime"`; для каждого спутника — стабильный детерминированный `"sat-<hex-digest>"` от канонического нормализованного пути репозитория (без зависимости от порядка в `watched_repositories` и без добавления новых внешних crypto-крейтов).
     - `CityBuilding.id`: нормализованный относительный POSIX-путь модуля внутри репозитория (например, `"crates/evohime-hive"`), чтобы фоновые обновления не сбрасывали выделение в инспекторе.
     - `revision_hash`: вычисляется строго от канонической топологии, состава и статусов островов **без учёта `generated_at_ms`**, поэтому неизменная структура даёт идентичный `revision_hash` между тиками.
   - `CityIsland`: `id`, `name`, `role` (`capital` | `satellite`), `source` (`live_workspace` | `embedded_manifest` | `unavailable`), `status` (`ready` | `unavailable`), `reason` (bounded диагностическая строка при `unavailable`), `ecosystem` (`rust` | `node` | `python` | `go` | `dotnet` | `mixed` | `generic`), `structure_hash`, `accent_hue`, `districts: Vec<CityDistrict>`, `buildings: Vec<CityBuilding>`, `edges: Vec<CityEdge>`, `total_files`, `total_lines`.
   - `CityDistrict`: `id`, `label`, `kind` (`core` | `shell` | `memory` | `tools` | `voice` | `runtime` | `app` | `packages` | `docs` | `other`).
   - `CityBuilding`: `id`, `name`, `district_id`, `kind` (`citadel` | `service` | `library` | `ui` | `storage` | `gateway` | `worker` | `module`), `relative_path`, `file_count`, `line_count`, `height_weight` (нормированный вес `0.15..=1.0`), `language`.
   - `CityEdge`: `from_id`, `to_id`, `kind` (`dependency` | `ipc` | `runtime`), `weight`.
3. Реализовать чистый read-only сканер структуры репозитория **без запуска внешних процессов** (`cargo`, `node`, `git` не вызываются):
   - **Rust workspace / crate**: парсинг корневого и вложенных `Cargo.toml` (`[workspace].members`, `[package].name`, `[dependencies]` с `path = ...` или совпадением имён workspace-членов).
   - **Node / TS / Electron**: парсинг `package.json` (`workspaces`, `dependencies`/`devDependencies` на внутренние пакеты, структура `src/main`, `src/preload`, `src/renderer` или `packages/*`, `apps/*`).
   - **Python / Go / .NET**: распознавание `pyproject.toml`, `go.mod`, `.sln`/`.csproj` и верхнеуровневых модулей.
   - **Универсальный fallback**: для любого прочего репозитория — группировка по значимым верхнеуровневым каталогам исходников (`src/*`, `app/*`, `lib/*`, `services/*` и т. д.).
   - **Столица Евы**: при валидном `eva_workspace_path` — живое сканирование реального checkout EvoHime (все workspace crates + `desktop/evohime-electron` + явные доменные группы и runtime/IPC-связи); при отсутствии `eva_workspace_path` — встроенный канонический fallback-манифест реальной архитектуры EvoHime (`source = "embedded_manifest"`).
4. Зафиксировать жёсткие лимиты и инварианты безопасности:
   - максимумы: до `12` городов-спутников, до `48` зданий на город, до `128` рёбер на город, глубина обхода не более `6`, максимум сканируемых файлов на репозиторий (`4_000`) и размер читаемого манифеста (`256 KiB`);
   - жёсткий потолок сериализованного JSON-снимка в Rust — `MAX_CITY_MAP_PAYLOAD_BYTES = 256 KiB` (строго меньше `MAX_FRAME_BYTES = 4 MiB` и меньше лимита приёма `512 KiB` в Electron main из этапа 199.3, с детерминированным усечением низкоприоритетных рёбер/узлов при приближении к потолку).
   - пропуск `.git`, `target`, `node_modules`, `.evohime`, `dist`, `out`, `build`, `.venv`, `__pycache__`, `bin`, `obj`;
   - канонизация корня и проверка каждого посещаемого пути, исключающая symlink escape и path traversal;
   - абсолютные локальные пути ОС не утекают в `CityBuilding.relative_path` (только нормализованные POSIX-подобные относительные пути); для острова хранится безопасное имя проекта и стабильный `id`.

## Зависимости
### Блокирующие
- `evohime-hive` (`Cargo.toml`, `lib.rs`, `settings.rs`) — без добавления новых production crate-зависимостей (используются уже подключённые `serde`, `serde_json`, `std::fs` и легковесный детерминированный парсер `Cargo.toml` / JSON).

### Опциональные
- Нет.

## Recovery и rollback
- Ошибка чтения отдельного файла/подкаталога пропускает проблемный элемент и не роняет построение острова.
- Несуществующий или невалидный каталог спутника формирует `CityIsland` со статусом `unavailable` и `reason`, не затрагивая Столицу и остальные спутники.
- Нечитаемый `eva_workspace_path` безопасно откатывается на `embedded_manifest` для Столицы.
- Rollback этапа: удаление `city_map.rs` и его экспорта из `evohime-hive/src/lib.rs`.

## Verification
- Unit-тесты в `crates/evohime-hive/src/city_map.rs` (на `tempfile::TempDir`):
  - детерминированное сканирование Rust workspace с внутренними зависимостями;
  - сканирование Node/TS монорепо и произвольного проекта (fallback);
  - проверка встроенного манифеста Столицы Евы и живого сканирования checkout;
  - проверка лимитов (усечение сверх лимита зданий/рёбер, лимит размера файла);
  - security-тесты: symlink escape за пределы корня, traversal, пропуск `.git`/`target`/`node_modules`;
  - проверка `serde` round-trip и `deny_unknown_fields`.
- Лёгкий локальный прогон: `cargo test -p evohime-hive city_map` (допустимо по правилу 4 `AGENTS.md`).

## Release evidence
- Схема `HiveCityMapSnapshot` v1 и фикстуры детерминированного вывода.
- Результаты unit/security-тестов модуля `city_map` и rustdoc-проверки в CI.

## Критерии выхода
- [ ] `HiveCityMapSnapshot` версионирован, детерминирован и документирован `///`.
- [ ] Сканер не вызывает внешние бинарники и не модифицирует ФС.
- [ ] Столица Евы работает и с `eva_workspace_path`, и через `embedded_manifest`.
- [ ] Пройдены unit- и security-тесты лимитов и границ путей.
