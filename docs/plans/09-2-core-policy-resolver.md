# 09-2 — Core resolver и operation checks

## Цель

Собрать hard checks filesystem, network, tool и resource limits в одном
Core-owned policy gate. Это общий путь вызова, а не новая библиотека, которую
можно обойти альтернативным adapter runtime.

## Зависимости

### Блокирующие

- 09-1 для `CapabilitySnapshotV1`, effective action binding и typed outcomes;
- план 08 для durable action/terminal linkage;
- текущие `ToolRegistry`, `WorkspaceSandbox`, network capability/SSRF checks,
  supervisor boundary и cancellation token.

### Опциональные

- будущие adapters из планов 10, 13–15 подключаются через тот же gate; пока
  adapter не зарегистрирован, его вызов возвращает `unavailable` без попытки
  выполнить fallback напрямую.

## Core policy gate

Ввести Core-owned resolver с двумя явно различимыми операциями:

1. `preflight`: нормализует canonical call, выбирает permission/scope и
   строит bounded preview/decision;
2. `recheck_before_effect`: заново проверяет тот же snapshot, action, tool,
   permission, scope, input hash, policy version, approval state и resource
   budget непосредственно перед side effect.

Renderer, planner, workflow coordinator и adapter могут дать hint, но ни один
hint не является решением. Все зарегистрированные paths — built-in tools,
terminal, workflow adapters, MCP/browser и provider/worker dispatch — должны
вызывать gate до dispatch; прямой вызов tool implementation из Core обходом
gate запрещается тестом/контрактом.

## Filesystem и workspace

- workspace anchor формируется Core из выбранного workspace и имеет stable
  identity; relative path разрешается только относительно него;
- absolute path допускается только если после Windows normalization остаётся
  внутри разрешённого anchor или явного уже существующего scoped grant;
  drive-relative, UNC/device paths, `..` escape и невалидная normalization
  отклоняются;
- каждый сегмент проверяется на symlink/reparse traversal, а перед открытием
  и для mutation повторно проверяются canonical location и file identity;
  write/delete используют verified parent handle/temporary atomic path, чтобы
  rename/reparse/TOCTOU не превращали preflight в доступ вне scope;
- protected paths и patterns (credentials, `.env*`, private keys, runtime
  secrets и т. п.) задаются versioned policy. Исключение для конкретного
  builtin tool должно быть явным и более узким, а не глобальным wildcard;
- operation type проверяется отдельно: read не становится write через
  аргумент, git read не получает git write, а shell cwd/argv не расширяет
  workspace scope.

## Network, providers и resources

- URI scheme, host, port, method и payload проходят allow/deny policy до
  connect;
- DNS разрешается и проверяется по всем адресам; loopback, private,
  link-local, metadata и internal targets deny-ятся по умолчанию, если
  versioned route policy явно не разрешает их;
- каждый redirect заново проходит тот же resolver: запрещены смена scheme,
  host/port policy и обход через DNS rebinding; действует bounded hop count;
- timeout, input/output bytes, concurrent calls, process lifetime, provider
  token/cost budget и cancellation проверяются до dispatch и при каждом
  bounded continuation;
- adapter получает capability-scoped session и opaque secret refs. Secret
  value может быть выдан только supervisor/Core boundary по purpose; он не
  передаётся renderer, prompt, argv или обычному log.

## Outcomes и cancellation

Resolver возвращает decision из 09-1 с bounded `reason_code`, а не свободную
строку ошибки. `denied` означает hard policy refusal, `unavailable` —
зарегистрированный capability/adapter отсутствует, `policy_error` — контракт
не удалось безопасно проверить. Cancellation до dispatch не создаёт effect;
cancellation во время процесса передаётся в существующий tool/supervisor
path, а результат с неизвестным эффектом фиксируется как `unknown_outcome`,
не как успешная mutation и не как blind retry.

## Проверки

- absolute/relative/drive-relative/UNC paths, traversal, case/normalization,
  symlink/reparse, rename race, protected path и operation mismatch;
- HTTP/private/internal target, all-address DNS, redirects, scheme/port
  changes, DNS rebinding, payload/timeout limits;
- preflight allow → mutation, then recheck denial after policy/scope/input/
  snapshot drift with zero sentinel side effect;
- timeout/cancellation до запуска, во время запуска и после результата;
- budget/concurrency exhaustion and unavailable adapter are distinguished from
  deny/policy error;
- worker/provider secret-ref boundary and direct adapter bypass tests.

## Готово, когда

Ни один внешний side effect не проходит мимо одного Core policy gate; path,
redirect, snapshot или resource drift блокирует dispatch; policy errors,
denied, unavailable, expired и cancelled остаются различимыми и durable
связываются с action/receipt плана 08.
