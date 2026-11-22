# План 199.1 — Контракт `HiveCityMapSnapshot` и read-only сканер архитектуры в `evohime-hive`

Статус: active implementation contract. Этап следует после overview 199.0.

## Scope и изменяемые контракты
1. Добавить модуль `crates/evohime-hive/src/city_map.rs` (с полными `///`-доками публичных типов под `-D missing_docs` и без `unwrap`/`expect`/`panic` в production-коде).
2. Определить версионированный read-only контракт (`HIVE_CITY_MAP_SCHEMA_VERSION = 1`, `#[serde(deny_unknown_fields)]`; любое добавление/изменение поля требует `HIVE_CITY_MAP_SCHEMA_VERSION++`):
   - `HiveCityMapSnapshot`: `schema_version: u32`, `generated_at_ms: i64` (volatile-метка времени, **исключена** из хэшей), `revision_hash: String`, `capital: CityIsland`, `satellites: Vec<CityIsland>`.
   - **Детерминизм, сортировка и диапазоны полей:**
     - **Алгоритм стабильных хэшей (без внешних крипто-крейтов):** детерминированный **FNV-1a 128-bit** (`offset = 0x6c62272e07bb014262b821756295c58d`, `prime = 0x0000000001000000000000000000013b`) в lowercase hex: `CityIsland.id` для спутников — `"sat-<первые 16 hex-символов>"` (64 бита) от канонического нормализованного пути репозитория (без зависимости от порядка в `watched_repositories`); `structure_hash` и `revision_hash` — полные 32 hex-символа (128 бит). Для Столицы `id` фиксирован: `"capital-evohime"`.
     - `CityBuilding.id`: нормализованный относительный POSIX-путь модуля внутри репозитория (например, `"crates/evohime-hive"`), чтобы фоновые обновления не сбрасывали выделение в инспекторе.
     - `revision_hash` и `structure_hash`: вычисляются строго от канонического содержимого **без `generated_at_ms`**.
     - Каноническая сортировка всех коллекций: `satellites` по `(id, name)`, `districts` по `id`, `buildings` по `(district_id, id)`, `edges` по `(from_id, to_id, kind)` с дедупликацией и насыщением `weight`.
     - Типы и валидация схемы (без `f32`/`f64` артефактов JSON): `accent_hue: u16` (`0..360`), `weight: u16` (`1..=100`), `height_weight_bp: u16` (целочисленные базисные пункты `150..=1000`, где `150` = `0.15`, `1000` = `1.00`); `CityEdge.from_id` и `to_id` обязаны ссылаться на существующие `CityBuilding.id` того же острова; self-edges (`from_id == to_id`) запрещены. Межостровные мосты в `HiveCityMapSnapshot` v1 не моделируются как отдельные рёбра — UI строит визуальный мост «Столица ↔ спутник» детерминированно по наличию спутника в `satellites`.
   - `CityIsland`: `id`, `name`, `role` (`"capital"` | `"satellite"`), `source` (`"live_workspace"` | `"embedded_manifest"`), `status` (`"ready"` | `"unavailable"`), `reason: Option<CityIslandUnavailableReason>` (`None` при `ready`; `Some(...)` при `unavailable`: `"repo_not_found"` | `"repo_unreadable"` | `"permission_denied"` | `"invalid_manifest"` | `"scan_timeout"`), `ecosystem` (`"rust"` | `"node"` | `"python"` | `"go"` | `"dotnet"` | `"mixed"` | `"generic"`), `structure_hash`, `accent_hue`, `districts: Vec<CityDistrict>`, `buildings: Vec<CityBuilding>`, `edges: Vec<CityEdge>`, `total_files: u32`, `total_lines: u64`.
   - `CityDistrict`: `id`, `label`, `kind` (`"core"` | `"shell"` | `"memory"` | `"tools"` | `"voice"` | `"runtime"` | `"app"` | `"packages"` | `"docs"` | `"other"`).
   - `CityBuilding`: `id`, `name`, `district_id`, `kind` (`"citadel"` | `"service"` | `"library"` | `"ui"` | `"storage"` | `"gateway"` | `"worker"` | `"module"`), `relative_path`, `file_count: u32`, `line_count: u64`, `height_weight_bp: u16`, `language`.
   - `CityEdge`: `from_id`, `to_id`, `kind` (`"dependency"` | `"ipc"` | `"runtime"`), `weight: u16`.
3. Реализовать чистый read-only сканер структуры репозитория **без запуска внешних процессов** (`cargo`, `node`, `git` не вызываются) и без добавления новых production-зависимостей (проверено по `Cargo.toml`/`Cargo.lock`: `toml` в workspace отсутствует):
   - **Rust workspace / crate (строгое документированное подмножество `Cargo.toml`):** секции `[workspace]`, `[package]`, `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, `[target.*.dependencies]`; ключи `members = [...]` (включая многострочные массивы), `name = "..."`, `path = "..."`, `workspace = true` (как inline table, так и dotted key `foo.workspace = true`), корректный пропуск `#`-комментариев и строковых литералов. Обязательное покрытие unit-тестом на реальных `Cargo.toml` текущего репозитория EvoHime.
   - **Node / TS / Electron**: парсинг `package.json` через `serde_json` (`workspaces`, `dependencies`/`devDependencies` на внутренние пакеты, структура `src/main`, `src/preload`, `src/renderer` или `packages/*`, `apps/*`).
   - **Python / Go / .NET**: распознавание `pyproject.toml`, `go.mod`, `.sln`/`.csproj` и верхнеуровневых модулей.
   - **Универсальный fallback**: для любого прочего репозитория — группировка по значимым верхнеуровневым каталогам исходников (`src/*`, `app/*`, `lib/*`, `services/*` и т. д.).
   - **Столица Евы**: при валидном `eva_workspace_path` — живое сканирование реального checkout EvoHime (все workspace crates + `desktop/evohime-electron` + явные доменные группы и runtime/IPC-связи); при отсутствии `eva_workspace_path` — встроенный канонический fallback-манифест реальной архитектуры EvoHime (`source = "embedded_manifest"`).
4. Зафиксировать жёсткие лимиты, бюджеты сканирования и защиту путей:
   - структурные лимиты: до `12` городов-спутников, до `48` зданий на город, до `128` рёбер на город, глубина обхода не более `6`, максимум `512` записей на каталог (`MAX_ENTRIES_PER_DIR`), максимум `4_000` файлов на репозиторий, максимум `256 KiB` на читаемый манифест и `512 KiB` на читаемый текстовый файл для подсчёта строк (`MAX_SOURCE_FILE_BYTES`; бинарные файлы по расширению/NUL-байту пропускаются);
   - временные бюджеты и контракт при исчерпании: `MAX_REPO_SCAN_DURATION = 1500 ms` на один репозиторий, `MAX_TOTAL_SCAN_DURATION = 5000 ms` на весь архипелаг. Столица сканируется первой (при таймауте — `embedded_manifest`); далее спутники сканируются в каноническом порядке `id`, каждому выделяется `min(MAX_REPO_SCAN_DURATION, remaining_total_budget)`. Если `MAX_TOTAL_SCAN_DURATION` исчерпан, неуспевший спутник сохраняет свой предыдущий кэшированный остров (если он уже был в кэше), либо получает `status = "unavailable"`, `reason = Some("scan_timeout")` (если новый) и включается в валидный частичный снимок с отложенным досканированием на следующем тике;
   - жёсткий потолок сериализованного JSON-снимка в Rust — `MAX_CITY_MAP_PAYLOAD_BYTES = 256 KiB` (строго меньше клиентского лимита `512 KiB` и `MAX_FRAME_BYTES = 4 MiB`); при превышении применяется детерминированное усечение (сначала рёбра с наименьшим `weight`, затем второстепенные `module`-здания с наименьшим `line_count`; Столица приоритетнее спутников), крайний fallback — `embedded_manifest` Столицы. Усечение при полном номинальном заполнении (13 островов × до 48 зданий, до 128 рёбер на остров — суммарно ориентировочно 270–300 KiB) — ожидаемое штатное поведение, а не аномалия: тест на лимиты обязан покрывать этот худший случай;
   - пропуск служебных каталогов: `.git`, `target`, `node_modules`, `.evohime`, `dist`, `out`, `build`, `.venv`, `__pycache__` пропускаются на любой глубине; `bin` и `obj` пропускаются **только для `.NET`-проектов (рядом с `.csproj`/`.sln` или на верхнем уровне `.NET`-репозитория) и никогда не пропускаются внутри `src/`** (чтобы не потерять Rust `src/bin/daemon.rs`);
   - **Защита от symlink escape и Windows-нормализация:** симлинки пропускаются через `std::fs::symlink_metadata` / `file_type().is_symlink()` непосредственно перед спуском в каталог и перед открытием файла, плюс сверка канонического префикса с корнем репозитория (без претензии на kernel-level atomic `O_NOFOLLOW` против локальной TOCTOU-гонки ФС); Windows-пути нормализуются в POSIX-вид (`\` → `/`, снятие `\\?\`, запрет `..`);
   - **Санитизация всех строковых полей:** абсолютные пути ОС не попадают ни в `relative_path`, ни в `name`, `label`, `id` или `reason`; ошибки чтения маппятся только в enum `CityIslandUnavailableReason`.

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
- Unit-тесты в `crates/evohime-hive/src/city_map.rs` (на `tempfile::TempDir`, уже присутствует в `[dev-dependencies]` `crates/evohime-hive/Cargo.toml`):
  - детерминированное сканирование Rust workspace с внутренними зависимостями;
  - сканирование Node/TS монорепо и произвольного проекта (fallback);
  - проверка встроенного манифеста Столицы Евы и живого сканирования checkout;
  - проверка лимитов (усечение сверх лимита зданий/рёбер, лимит размера файла), включая худший случай: снимок с максимальным числом островов/зданий/рёбер укладывается в `MAX_CITY_MAP_PAYLOAD_BYTES` после детерминированного усечения;
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
