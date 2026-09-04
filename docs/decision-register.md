# EvoHime — реестр решений

Обновлено: 2026-09-04.

Канонический реестр решений текущего desktop-цикла. Здесь нет секретов,
provider credentials или обещаний, не подтверждённых кодом. `accepted` означает,
что решение зафиксировано реализацией и текущей архитектурой; `open` требует
отдельного решения или integration work.

## Accepted decisions

| ID | Решение | Владелец | Evidence |
| --- | --- | --- | --- |
| D-IPC-01 | Core — единственный executor и source of truth; renderer получает только typed IPC projection | Core | `docs/architecture.md`, authenticated named-pipe tests |
| D-SQL-01 | SQLite migrations additive и transactional; backup создаётся до blocking migration | Storage | `evohime-local-storage`, migration tests |
| D-AUTO-01 | Automation не использует lease workflow; у scheduler собственные fenced runtime и durable events | Core automation | automation runtime/store tests |
| D-AUTO-02 | Simulation допускает только fake-provider effects; host effects fail closed | Core automation | simulation tests, eval fixtures |
| D-OPT-01 | Отсутствующий browser/voice/vision adapter — typed `unavailable` без production side effect | Capability owner | `architecture.md`, optional adapter tests |
| D-RES-01 | Базовый пакет local-only: без cloud control plane, public HTTP, external telemetry и mandatory GPU | Release | `AGENTS.md`, `SECURITY.md` |
| D-LIC-01 | License/attribution inventory хранится в Git как metadata и не является runtime secret storage | Release | `docs/licenses/` |
| D-SIGN-01 | Authenticode signing вне текущего release scope; trust root — manifest/hash evidence | Release | `architecture.md`, `release-evidence.md` |
| D-REPAIR-01 | Self-repair запускается пользователем; provider/model обязательны, diagnose/commit/push/restart подтверждаются отдельно | Repair/update | `repair-service.ts`, Electron repair tests |
| D-UPDATE-01 | Backup удерживается до authenticated Core health marker; timeout вызывает rollback | Repair/update | `evohime-updater`, health-marker tests |
| D-UI-01 | Основная навигация короткая; технические панели находятся в collapsed `Интерфейс разработчика` | Desktop shell | `App.tsx`, operations/sidebar tests |
| D-MODEL-01 | API model selection действует со следующего Core-запроса; смена API-профиля и Codex model restart Core | Provider/shell | `ModelPicker`, `CodexService`, shell-bridge tests |
| D-RELEASE-01 | Поставка выполняется одним постоянным full installer-релизом `installer` | Release | `installer/release-notes.md`, Windows workflow |
| D-RELEASE-02 | Component manifest и selective update остаются предложением плана 144 до его полного закрытия | Release | `docs/plans/144-*` |
| D-REL-21 | Electron diagnostics — bounded redacted projection; recovery, approvals, backup/restore и effects остаются Core-owned | Reliability | `diagnostic-bundle.ts`, recovery projection tests |

## Закрытые acceptance records

| ID | Решение | Evidence |
| --- | --- | --- |
| O-AUTO-01 | Scheduler имеет timezone/missed-tick policy, durable cursor, additive IPC и focused gates | plan 18 evidence |
| O-AUTO-02 | Archive/restore использует checksum, identity validation, bounded restore и retention sweep | automation store tests |
| O-LIC-01 | Cargo/npm license inventory и hash verification проходят CI gate | `docs/licenses/` |
| O-REPAIR-01 | Isolated user-triggered repair, protected paths, отдельные commit/push gates и health-gated rollback подтверждены | repair/update tests |

## Dependency graph

Закрытые планы 01–117 не являются текущей очередью и представлены только
перенесёнными контрактами. Основная незавершённая последовательность:

`118 → 119 → 120 → 121 → 122 → 123 → 124 → 125 → 126 → 127 → 128 → 129 →
130 → 131 → 132 → 133 → 134 → 135 → 136 → 137 → 138 → 139 → 140 → 141 →
142 → 143 → 144`.

Планы с optional adapters подключаются через fail-closed boundaries и не должны
становиться обязательной зависимостью базового Windows-пакета. Любая блокирующая
ссылка на более поздний номер — ошибка, которую нужно исправить до реализации.

## Open questions

| ID | Вопрос | Когда закрывать |
| --- | --- | --- |
| O-RELEASE-01 | Какой минимальный component graph и compatibility policy нужен для selective update | В plan 144.1 |
| O-RELEASE-02 | Как разделить package/recovery evidence без ослабления current full-installer rollback | В plan 144.2–144.4 |
| O-COMPAT-01 | Нужен ли отдельный informative ARM64/Insider release job | При изменении release scope |

## Resource and contract budgets

| Ресурс | Ограничение | Владелец |
| --- | --- | --- |
| Automation input | 64 KiB | Core contract |
| Automation activities | 64 | Core contract |
| Automation command queue | 256 pending | Core runtime |
| Automation progress | 1024 coalesced entries | Core runtime |
| Provider call | 120 s deadline, максимум 2 retry attempts | Core runtime |
| Snapshot | 1 MiB, 64 на run | Simulation/storage |
| Durable history | 256 events на run | Acceptance |
| Archive | 10,000 runs / 30 days | Release gate |
| Simulation | fake provider only; no host/network/process/IPC | Core simulation |

При изменении schema или IPC в том же task-only коммите обновляются owner,
version, migration, rollback note и focused compatibility test.

## Правило использования

Реестр не заменяет исходный код или `current-state.md`. Если решение изменено,
сначала обновите реализацию и тесты, затем этот файл, архитектурный контракт и
release evidence. Непринятые варианты остаются в plan-файлах и не выдаются за
часть продукта.
