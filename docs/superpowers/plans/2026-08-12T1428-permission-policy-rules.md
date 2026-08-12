# Permission Policy Rules — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Дать EvoHime декларативные правила разрешений с glob-паттернами, которые учитывают *содержимое* вызова инструмента (в первую очередь shell-команду), а не только категорию и путь.

**Architecture:** В `evohime-permissions` добавляется новый слой — упорядоченный набор `PolicyRule` (permission + glob-паттерн + режим), с семантикой «побеждает последнее совпавшее правило». Слой встраивается в существующий `check_scoped` с явным приоритетом: жёсткий `Deny` из правил не может быть перекрыт runtime-грантами, остальные режимы правил стоят ниже path grants и session overrides. `tool-runtime` начинает передавать в проверку нормализованную shell-команду, поэтому `rm *` и `git *` становятся различимыми политикой, а выданное одобрение перестаёт быть карт-бланшем: оно привязывается к тому вызову, который хозяин видел. Правила загружаются Core из `permissions.json` в data dir при старте; этот файл — единственный источник истины, никакого второго хранилища для них не заводится.

**Tech Stack:** Rust 2021, tokio (`RwLock`), serde, futures-executor (в тестах `evohime-permissions`), tempfile (в тестах `evohime-core`).

**Источник идеи:** дизайн permission-системы opencode (`opencode.ai/docs/permissions`). Код оттуда не переносится — только модель правил. Перед любым заимствованием текста/кода проверить LICENSE репозитория.

## Global Constraints

- Продукт — native Windows: WinUI 3 + Rust Core + SQLite + named-pipe IPC. Ни веб-панель, ни HTTP-сервер не трогаем.
- Изменения в `crates/desktop-ipc/proto/evohime.desktop.proto` в этом плане **не выполняются**. Все новые поля — внутри уже существующих JSON-payload'ов либо только в Rust.
- Новые Rust-функции и исправления покрываются тестами (правило 3 из AGENTS.md).
- Каждая задача заканчивается task-only git-коммитом в текущей ветке `main`. Push — не выполняем.
- Перед заявлением о готовности задачи: свежий прогон тестов + `git diff --check`.
- Сравнение путей и команд — регистронезависимое (Windows), разделитель нормализуется в `/`.
- Существующие сериализованные структуры (в этом плане — только `ApprovalRequest`) расширяются полями с `#[serde(default)]`, чтобы ранее записанный JSON читался без миграции.

## File Structure

| Файл | Ответственность |
| --- | --- |
| `crates/permissions/src/pattern.rs` (создать) | Чистая функция glob-сопоставления `glob_match(pattern, value)`. Без зависимостей от движка. |
| `crates/permissions/src/policy.rs` (создать) | `PolicyRule`, `PolicyRuleSet`, разрешение «последнее совпавшее правило», дефолтный набор. |
| `crates/permissions/src/lib.rs` (изменить) | Хранение набора правил в `PermissionEngine`, встраивание в `check_scoped`, поле `command` в `PermissionCheck` и `ApprovalRequest`. |
| `crates/tool-runtime/src/registry.rs` (изменить) | Извлечение нормализованной команды из input, передача её в проверку и в approval, привязка approval к конкретному вызову и повторная проверка запретов после подтверждения. |
| `crates/evohime-core/src/permission_rules.rs` (создать) | Чтение `permissions.json` из data dir и применение правил к движку. Файл не создаётся автоматически: его отсутствие означает встроенные defaults. |
| `crates/evohime-core/src/lib.rs` (изменить) | Вызов загрузчика при старте; окно повторов вместо «навсегда» в цикле агента. |

Тесты живут в `#[cfg(test)] mod tests` внутри тех же файлов — так устроен весь код в этих крейтах, отдельных `tests/` директорий не заводим.

---

### Task 1: Glob-сопоставление

**Files:**
- Create: `crates/permissions/src/pattern.rs`
- Modify: `crates/permissions/src/lib.rs:6` (добавить `mod pattern;` и реэкспорт)

**Interfaces:**
- Consumes: ничего.
- Produces: `pub fn glob_match(pattern: &str, value: &str) -> bool`.

**Семантика (зафиксирована намеренно, повторяет opencode):**
- `*` — ноль или больше любых символов, **включая `/`**. То есть `src/*.rs` совпадает и с `src/main.rs`, и с `src/tools/git.rs`. Это отличается от unix-glob и должно быть задокументировано в комментарии.
- `?` — ровно один любой символ.
- Сравнение регистронезависимое.
- Пустой паттерн совпадает только с пустой строкой.

- [ ] **Step 1: Написать падающие тесты**

Создать `crates/permissions/src/pattern.rs` с одним лишь тестовым модулем и объявлением функции:

```rust
//! Glob matching for permission policy rules.

/// Match `value` against a glob `pattern`.
///
/// `*` matches zero or more characters **including `/`**; `?` matches exactly
/// one character. Matching is case-insensitive because subjects are Windows
/// paths and shell commands.
pub fn glob_match(_pattern: &str, _value: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_across_separators() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(glob_match("src/*.rs", "src/tools/git.rs"));
        assert!(!glob_match("src/*.rs", "docs/main.rs"));
    }

    #[test]
    fn question_mark_matches_single_char() {
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
        assert!(!glob_match("a?c", "abbc"));
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(glob_match("*.ENV", "config/.env"));
        assert!(glob_match("git *", "GIT push"));
    }

    #[test]
    fn literal_and_empty_patterns() {
        assert!(glob_match("git push", "git push"));
        assert!(!glob_match("git push", "git pushx"));
        assert!(glob_match("", ""));
        assert!(!glob_match("", "x"));
    }

    #[test]
    fn trailing_star_matches_empty_tail() {
        assert!(glob_match("git*", "git"));
        assert!(glob_match("git *", "git "));
    }

    /// The exact patterns later tasks and the default rule set rely on.
    #[test]
    fn patterns_used_by_the_rule_sets() {
        assert!(glob_match("rm *", "rm -rf target"));
        assert!(glob_match("git *", "git status"));
        assert!(!glob_match("git *", "cargo test"));
        assert!(glob_match("git push*", "git push origin main"));
        assert!(glob_match("cargo *", "cargo --version"));
        assert!(glob_match("*.env", ".env"));
        assert!(glob_match("*.env", "backend/.env"));
        // `*.env` does NOT cover `.env.local` — that is why the default set
        // carries a second rule.
        assert!(!glob_match("*.env", "backend/.env.local"));
        assert!(glob_match("*.env.*", "backend/.env.local"));
        assert!(!glob_match("*.env", "src/main.rs"));
        assert!(!glob_match("*.env.*", "src/environment.rs"));
    }
}
```

Реализация из Step 3 вместе с этими тестами уже собрана и прогнана отдельным файлом через `rustc --test` — 6 тестов, все зелёные. То есть алгоритм проверен, а не только вычитан; при переносе в крейт менять его не нужно.

Добавить в `crates/permissions/src/lib.rs` сразу после строки `use serde::{Deserialize, Serialize};`:

```rust
mod pattern;

pub use pattern::glob_match;
```

- [ ] **Step 2: Запустить тест и убедиться, что он падает**

Run: `cargo test -p evohime-permissions pattern::`
Expected: FAIL — `star_matches_across_separators` падает на `assert!(glob_match("*", "anything"))`.

- [ ] **Step 3: Реализовать сопоставление**

Заменить тело `glob_match` в `crates/permissions/src/pattern.rs`:

```rust
pub fn glob_match(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let value: Vec<char> = value.to_lowercase().chars().collect();

    // Iterative backtracking: `star` remembers the last `*` position so a
    // failed branch can retry consuming one more character.
    let (mut p, mut v) = (0usize, 0usize);
    let (mut star, mut retry) = (None, 0usize);

    while v < value.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            retry = v;
            p += 1;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            retry += 1;
            v = retry;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}
```

Убрать `_` из имён параметров в сигнатуре.

- [ ] **Step 4: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-permissions`
Expected: PASS — все тесты `pattern::` зелёные, ранее существовавшие тесты крейта не сломаны.

- [ ] **Step 5: Коммит**

```bash
git add crates/permissions/src/pattern.rs crates/permissions/src/lib.rs
git commit -m "feat(permissions): add glob matcher for policy rules"
```

---

### Task 2: Набор правил политики

**Files:**
- Create: `crates/permissions/src/policy.rs`
- Modify: `crates/permissions/src/lib.rs` (добавить `mod policy;` и реэкспорт рядом с `mod pattern;`)

**Interfaces:**
- Consumes: `glob_match(pattern, value) -> bool` из Task 1.
- Produces:
  - `pub struct PolicyRule { pub permission: Permission, pub pattern: String, pub mode: PermissionMode }`
  - `pub struct PolicyRuleSet(Vec<PolicyRule>)` с `PolicyRuleSet::new(rules: Vec<PolicyRule>) -> Self`, `PolicyRuleSet::defaults() -> Self`, `PolicyRuleSet::rules(&self) -> &[PolicyRule]`, `PolicyRuleSet::resolve(&self, permission: Permission, subject: &str) -> Option<PermissionMode>`.

**Дефолтный набор** — намеренно минимальный, только то, что безопасно включить всем: запрет чтения `.env`-файлов. Ничего вроде `rm *` в дефолт не кладём: это решение владельца проекта, оно уезжает в пример конфига в Task 5.

- [ ] **Step 1: Написать падающие тесты**

Создать `crates/permissions/src/policy.rs`:

```rust
//! Ordered permission policy rules: the last matching rule wins.

use crate::{pattern::glob_match, Permission, PermissionMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub permission: Permission,
    /// Glob matched against the request subject: shell command for
    /// `ShellExecute`, normalized path or URL otherwise.
    pub pattern: String,
    pub mode: PermissionMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolicyRuleSet(Vec<PolicyRule>);

impl PolicyRuleSet {
    pub fn new(rules: Vec<PolicyRule>) -> Self {
        Self(rules)
    }

    /// Built-in hardening applied when the user has no rules of their own.
    pub fn defaults() -> Self {
        Self(vec![
            PolicyRule {
                permission: Permission::FilesystemRead,
                pattern: "*.env".into(),
                mode: PermissionMode::Deny,
            },
            PolicyRule {
                permission: Permission::FilesystemRead,
                pattern: "*.env.*".into(),
                mode: PermissionMode::Deny,
            },
        ])
    }

    pub fn rules(&self) -> &[PolicyRule] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Last matching rule wins, so later rules override earlier ones.
    pub fn resolve(&self, permission: Permission, subject: &str) -> Option<PermissionMode> {
        self.0
            .iter()
            .filter(|rule| rule.permission == permission && glob_match(&rule.pattern, subject))
            .next_back()
            .map(|rule| rule.mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(permission: Permission, pattern: &str, mode: PermissionMode) -> PolicyRule {
        PolicyRule {
            permission,
            pattern: pattern.into(),
            mode,
        }
    }

    #[test]
    fn last_matching_rule_wins() {
        let set = PolicyRuleSet::new(vec![
            rule(Permission::ShellExecute, "*", PermissionMode::Ask),
            rule(Permission::ShellExecute, "git *", PermissionMode::Allow),
            rule(Permission::ShellExecute, "git push*", PermissionMode::Deny),
        ]);
        assert_eq!(
            set.resolve(Permission::ShellExecute, "git status"),
            Some(PermissionMode::Allow)
        );
        assert_eq!(
            set.resolve(Permission::ShellExecute, "git push origin main"),
            Some(PermissionMode::Deny)
        );
        assert_eq!(
            set.resolve(Permission::ShellExecute, "cargo test"),
            Some(PermissionMode::Ask)
        );
    }

    #[test]
    fn rules_are_scoped_to_their_permission() {
        let set = PolicyRuleSet::new(vec![rule(
            Permission::ShellExecute,
            "*",
            PermissionMode::Deny,
        )]);
        assert_eq!(set.resolve(Permission::FilesystemWrite, "src/main.rs"), None);
    }

    #[test]
    fn no_match_returns_none() {
        let set = PolicyRuleSet::new(vec![rule(
            Permission::FilesystemWrite,
            "docs/*",
            PermissionMode::Allow,
        )]);
        assert_eq!(set.resolve(Permission::FilesystemWrite, "src/main.rs"), None);
    }

    #[test]
    fn defaults_deny_reading_env_files() {
        let set = PolicyRuleSet::defaults();
        assert_eq!(
            set.resolve(Permission::FilesystemRead, ".env"),
            Some(PermissionMode::Deny)
        );
        assert_eq!(
            set.resolve(Permission::FilesystemRead, "backend/.env.local"),
            Some(PermissionMode::Deny)
        );
        assert_eq!(set.resolve(Permission::FilesystemRead, "src/main.rs"), None);
    }

    #[test]
    fn ruleset_serde_roundtrip() {
        let set = PolicyRuleSet::defaults();
        let json = serde_json::to_string(&set).expect("serialize");
        assert!(json.starts_with('['), "transparent ruleset must be an array");
        let restored: PolicyRuleSet = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, set);
    }
}
```

Добавить в `crates/permissions/src/lib.rs` рядом с `mod pattern;`:

```rust
mod policy;

pub use policy::{PolicyRule, PolicyRuleSet};
```

- [ ] **Step 2: Запустить тесты и убедиться, что они падают**

Run: `cargo test -p evohime-permissions policy::`
Expected: FAIL на этапе компиляции — `serde_json` не подключён к крейту как dev-dependency.

- [ ] **Step 3: Добавить недостающую зависимость**

В `crates/permissions/Cargo.toml` крейт `serde_json` не объявлен вообще — ни в `[dependencies]`, ни в `[dev-dependencies]`. Добавить в `[dev-dependencies]` рядом с `futures-executor`:

```toml
serde_json = "1"
```

Версии в этом workspace задаются строкой, а не `workspace = true` (см. тот же манифест), поэтому пишем `"1"`.

- [ ] **Step 4: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-permissions`
Expected: PASS — все тесты `policy::` и `pattern::` зелёные.

- [ ] **Step 5: Коммит**

```bash
git add crates/permissions/src/policy.rs crates/permissions/src/lib.rs crates/permissions/Cargo.toml
git commit -m "feat(permissions): add ordered policy rule set"
```

---

### Task 3: Встроить правила в `check_scoped`

**Files:**
- Modify: `crates/permissions/src/lib.rs:73-76` (`PermissionCheck`), `:125-134` (поля `PermissionEngine`), `:151-170` (`new`), `:276-305` (`check_scoped`)

**Interfaces:**
- Consumes: `PolicyRuleSet::resolve` из Task 2.
- Produces:
  - `PermissionCheck` получает поле `pub command: Option<&'a str>` (по умолчанию `None`, структура остаётся `Default`).
  - `PermissionEngine::set_policy_rules(&self, rules: PolicyRuleSet)`, `PermissionEngine::policy_rules(&self) -> PolicyRuleSet`.

**Почему правила не кладутся в `PermissionScopesSnapshot`.** Соблазн есть — рядом лежит готовый `export_scopes`/`import_scopes`. Но: (а) у этой пары **нет ни одного вызывающего** во всём репозитории, кроме её собственных тестов, то есть persistence там сейчас мёртвый; (б) источник истины для правил — `permissions.json` (Task 6), и второе хранилище того же состояния создаёт реальный баг: `import_scopes` с пустым `policy_rules` затрёт правила, загруженные из файла при старте. Одно состояние — одно место хранения.

**Почему `PermissionEngine::new()` получает пустой набор, а не `defaults()`.** Встроенный запрет на чтение `.env` включается загрузчиком (Task 6), а не конструктором: иначе каждый юнит-тест и каждый вызывающий, собирающий движок вручную, молча получал бы политику, которую не просил, и существующий тест `default_policy_allows_read_and_asks_for_write` начал бы описывать неправду.

**Приоритет разрешения (документировать в doc-комментарии `check_scoped`):**

1. Правило политики со значением `Deny` — жёсткий запрет, не перекрывается ничем.
2. Path grant (session-scoped предпочтительнее глобального) — как сейчас.
3. Session permission mode — как сейчас.
4. Правило политики со значением `Allow` / `Ask`.
5. Глобальный режим — как сейчас.

Обоснование порядка: пункт 1 нужен, чтобы «запомнить путь» из approval-диалога не мог обойти явный запрет владельца проекта; пункты 2–3 остаются выше `Allow`/`Ask`-правил, потому что это осознанные runtime-решения пользователя по конкретной сессии.

**Subject правила:** `check.command.unwrap_or(check.path.unwrap_or("workspace"))` — команда важнее пути, потому что для `shell.execute` путь почти всегда `"workspace"` и ничего не различает.

- [ ] **Step 1: Написать падающие тесты**

Добавить в `mod tests` в конце `crates/permissions/src/lib.rs`:

```rust
    #[test]
    fn policy_rule_denies_matching_command() {
        block_on(async {
            let engine = PermissionEngine::new();
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![
                    PolicyRule {
                        permission: Permission::ShellExecute,
                        pattern: "*".into(),
                        mode: PermissionMode::Ask,
                    },
                    PolicyRule {
                        permission: Permission::ShellExecute,
                        pattern: "git *".into(),
                        mode: PermissionMode::Allow,
                    },
                    PolicyRule {
                        permission: Permission::ShellExecute,
                        pattern: "rm *".into(),
                        mode: PermissionMode::Deny,
                    },
                ]))
                .await;

            let check = |command: &'static str| PermissionCheck {
                session_id: None,
                path: Some("workspace"),
                command: Some(command),
            };

            assert_eq!(
                engine
                    .check_scoped(Permission::ShellExecute, &check("git status"))
                    .await,
                PermissionDecision::Allowed
            );
            assert_eq!(
                engine
                    .check_scoped(Permission::ShellExecute, &check("rm -rf target"))
                    .await,
                PermissionDecision::Denied
            );
            assert_eq!(
                engine
                    .check_scoped(Permission::ShellExecute, &check("cargo test"))
                    .await,
                PermissionDecision::NeedsApproval
            );
        });
    }

    #[test]
    fn policy_deny_beats_path_grant_and_session_mode() {
        block_on(async {
            let engine = PermissionEngine::new();
            let session = Uuid::new_v4();
            engine
                .set_session_mode(session, Permission::FilesystemRead, PermissionMode::Allow)
                .await;
            engine
                .set_path_grant(
                    Permission::FilesystemRead,
                    "backend",
                    PermissionMode::Allow,
                    Some(session),
                    None,
                )
                .await;
            engine.set_policy_rules(PolicyRuleSet::defaults()).await;

            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemRead,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("backend/.env"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Denied
            );
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemRead,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("backend/main.rs"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Allowed
            );
        });
    }

    #[test]
    fn path_grant_beats_policy_allow_and_ask() {
        block_on(async {
            let engine = PermissionEngine::new();
            let session = Uuid::new_v4();
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                    permission: Permission::FilesystemWrite,
                    pattern: "*".into(),
                    mode: PermissionMode::Ask,
                }]))
                .await;
            engine
                .set_path_grant(
                    Permission::FilesystemWrite,
                    "docs",
                    PermissionMode::Allow,
                    Some(session),
                    None,
                )
                .await;

            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("docs/plan.md"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Allowed
            );
        });
    }

    #[test]
    fn new_engine_starts_without_policy_rules() {
        block_on(async {
            // Built-in hardening arrives through the loader (Task 6), never
            // through the constructor.
            let engine = PermissionEngine::new();
            assert!(engine.policy_rules().await.is_empty());
            assert_eq!(
                engine.check(Permission::FilesystemRead).await,
                PermissionDecision::Allowed
            );
        });
    }
```

Также обновить уже существующие тесты, которые конструируют `PermissionCheck { session_id, path }` литералом (`session_override_beats_global_mode`, `path_grant_allows_matching_prefix`, `grant_remembers_path_for_session`, `path_deny_overrides_session_allow`, `scopes_snapshot_roundtrip_preserves_grants`, `import_scopes_skips_expired_path_grants`), добавив в каждый литерал `command: None`.

- [ ] **Step 2: Запустить тесты и убедиться, что они падают**

Run: `cargo test -p evohime-permissions`
Expected: FAIL на компиляции — нет поля `command` в `PermissionCheck` и нет методов `set_policy_rules` / `policy_rules`.

- [ ] **Step 3: Реализовать**

В `crates/permissions/src/lib.rs`:

Расширить `PermissionCheck`:

```rust
/// Context for a scoped permission check.
#[derive(Debug, Clone, Default)]
pub struct PermissionCheck<'a> {
    pub session_id: Option<Uuid>,
    pub path: Option<&'a str>,
    /// Normalized shell command, when the tool runs one. Policy rules match
    /// this instead of `path`, because shell paths carry no information.
    pub command: Option<&'a str>,
}
```

Добавить поле в движок (рядом с `path_grants`):

```rust
    policy_rules: Arc<RwLock<PolicyRuleSet>>,
```

и в `PermissionEngine::new()` — `policy_rules: Arc::new(RwLock::new(PolicyRuleSet::default())),`.

Добавить аксессоры рядом с `set_path_grant`:

```rust
    pub async fn set_policy_rules(&self, rules: PolicyRuleSet) {
        *self.policy_rules.write().await = rules;
    }

    pub async fn policy_rules(&self) -> PolicyRuleSet {
        self.policy_rules.read().await.clone()
    }
```

Переписать `check_scoped`:

```rust
    /// Resolve decision with policy rules, path grants, and session overrides.
    ///
    /// Priority (most specific first):
    /// 1. policy rule resolving to `Deny` — a hard block runtime grants cannot lift
    /// 2. matching path grant (session-scoped preferred over global)
    /// 3. session permission mode
    /// 4. policy rule resolving to `Allow` / `Ask`
    /// 5. global mode
    pub async fn check_scoped(
        &self,
        permission: Permission,
        check: &PermissionCheck<'_>,
    ) -> PermissionDecision {
        self.purge_expired_grants().await;

        let normalized_path = check.path.map(normalize_scope_path);
        let subject = check
            .command
            .map(str::to_string)
            .or_else(|| normalized_path.clone())
            .unwrap_or_else(|| "workspace".to_string());
        let policy_mode = self.policy_rules.read().await.resolve(permission, &subject);

        if policy_mode == Some(PermissionMode::Deny) {
            return PermissionDecision::Denied;
        }

        if let Some(path) = normalized_path {
            if let Some(mode) = self
                .find_path_mode(permission, &path, check.session_id)
                .await
            {
                return mode_to_decision(mode);
            }
        }

        if let Some(session_id) = check.session_id {
            if let Some(mode) = self
                .session_modes
                .read()
                .await
                .get(&(session_id, permission))
                .copied()
            {
                return mode_to_decision(mode);
            }
        }

        if let Some(mode) = policy_mode {
            return mode_to_decision(mode);
        }

        mode_to_decision(self.mode(permission).await)
    }
```

`PermissionScopesSnapshot`, `export_scopes` и `import_scopes` **не трогаем** — обоснование выше, в блоке Interfaces.

- [ ] **Step 4: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-permissions`
Expected: PASS — все тесты крейта зелёные, включая пять новых.

- [ ] **Step 5: Коммит**

```bash
git add crates/permissions/src/lib.rs
git commit -m "feat(permissions): resolve policy rules inside scoped checks"
```

---

### Task 4: Команда shell доходит до проверки и до approval

**Files:**
- Modify: `crates/tool-runtime/src/registry.rs:277-312` (блок проверки), `:462-476` (`scope_from_input`, рядом добавить `command_from_input`)
- Modify: `crates/permissions/src/lib.rs:43-53` (`ApprovalRequest`), `:318-355` (`create_approval_scoped`)

**Interfaces:**
- Consumes: `PermissionCheck.command` из Task 3.
- Produces:
  - `fn command_from_input(tool_name: &str, input: &Value) -> Option<String>` в `registry.rs` (private).
  - `ApprovalRequest` получает `#[serde(default, skip_serializing_if = "Option::is_none")] pub command: Option<String>`.
  - `PermissionEngine::create_approval_scoped` получает шестой параметр `command: Option<String>`.

**Зачем команда в `ApprovalRequest`, если UI и так её видит.** Событие `CoreEvent::ApprovalRequired` (`crates/evohime-core/src/lib.rs:2191-2199`) уже несёт целиком `input` вызова, поэтому панель подтверждения показывает команду и без наших правок — работы по UI здесь нет. Поле нужно не для отображения, а как **запись движка о том, что именно было одобрено**: Task 5 сравнивает её с тем, что реально пришло на исполнение. Без этого поля движок физически не может отличить подмену.

`ApprovalAuditEntry` при этом **не расширяем**: sink аудита (`attach_audit_sender`) в репозитории никем не вызывается, записи живут только в кольцевом буфере в памяти, и добавлять туда поле «на будущее» — то же самое спекулятивное хранилище, от которого мы отказались в Task 3.

**Нормализация команды:** инструмент `shell.execute` принимает либо `command: String`, либо `program: String` + `args: Vec<String>` (см. `crates/tool-runtime/src/tools/shell.rs:20-27`). Обе формы приводим к одной строке, схлопывая любые пробельные последовательности в один пробел, чтобы `git   push` и `git push` матчились одинаково.

- [ ] **Step 1: Написать падающие тесты**

Добавить в `mod tests` в `crates/tool-runtime/src/registry.rs`:

```rust
    #[test]
    fn command_from_input_normalizes_both_shell_forms() {
        assert_eq!(
            command_from_input(
                "shell.execute",
                &serde_json::json!({ "command": "git   push  origin main" })
            ),
            Some("git push origin main".to_string())
        );
        assert_eq!(
            command_from_input(
                "shell.execute",
                &serde_json::json!({ "program": "cargo", "args": ["test", "-p", "evohime-core"] })
            ),
            Some("cargo test -p evohime-core".to_string())
        );
        assert_eq!(
            command_from_input("filesystem.read", &serde_json::json!({ "path": "a.rs" })),
            None
        );
        assert_eq!(command_from_input("shell.execute", &serde_json::json!({})), None);
    }

    #[tokio::test]
    async fn policy_rule_denies_shell_command_before_execution() {
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::ShellExecute,
                evohime_permissions::PermissionMode::Allow,
            )
            .await;
        permissions
            .set_policy_rules(evohime_permissions::PolicyRuleSet::new(vec![
                evohime_permissions::PolicyRule {
                    permission: evohime_permissions::Permission::ShellExecute,
                    pattern: "rm *".into(),
                    mode: evohime_permissions::PermissionMode::Deny,
                },
            ]))
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: Some(Uuid::new_v4()),
            progress_tx: None,
        };

        let denied = registry
            .execute(
                &context,
                "shell.execute",
                serde_json::json!({ "command": "rm -rf target" }),
            )
            .await;
        assert!(matches!(denied, Err(ToolError::PermissionDenied(_))));

        let allowed = registry
            .execute(
                &context,
                "shell.execute",
                serde_json::json!({ "command": "cargo --version" }),
            )
            .await;
        assert!(!matches!(allowed, Err(ToolError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn shell_approval_carries_the_command() {
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::ShellExecute,
                evohime_permissions::PermissionMode::Ask,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: Some(Uuid::new_v4()),
            progress_tx: None,
        };

        let result = registry
            .execute(
                &context,
                "shell.execute",
                serde_json::json!({ "command": "cargo test" }),
            )
            .await;
        let approval_id = match result {
            Err(ToolError::NeedsApproval { approval_id, .. }) => approval_id,
            other => panic!("expected NeedsApproval, got {other:?}"),
        };
        let (request, _) = registry
            .permissions()
            .approval(approval_id)
            .await
            .expect("approval stored");
        assert_eq!(request.command.as_deref(), Some("cargo test"));
    }
```

Примечание для исполнителя: `ToolContext` (`crates/tool-runtime/src/registry.rs:39-45`) имеет ровно четыре поля — `workspace_root`, `task_id`, `session_id`, `progress_tx`; образец заполнения есть в тесте `ask_mode_creates_scoped_approval` (`crates/tool-runtime/src/registry.rs:746`).

- [ ] **Step 2: Запустить тесты и убедиться, что они падают**

Run: `cargo test -p evohime-tool-runtime`
Expected: FAIL на компиляции — нет `command_from_input`, нет поля `command` в `ApprovalRequest`.

- [ ] **Step 3: Реализовать**

В `crates/permissions/src/lib.rs` расширить `ApprovalRequest`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub task_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    pub tool_name: String,
    pub permission: Permission,
    /// Relative path, URL, or `"workspace"`.
    pub scope: String,
    /// Normalized shell command, when the approval gates one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}
```

Добавить параметр в `create_approval_scoped` (после `scope`) и проставить поле:

```rust
    pub async fn create_approval_scoped(
        &self,
        task_id: Uuid,
        session_id: Option<Uuid>,
        tool_name: impl Into<String>,
        permission: Permission,
        scope: impl Into<String>,
        command: Option<String>,
    ) -> ApprovalRequest {
        let request = ApprovalRequest {
            id: Uuid::new_v4(),
            task_id,
            session_id,
            tool_name: tool_name.into(),
            permission,
            scope: normalize_scope_path(scope.into()),
            command,
        };
```

В `create_approval` (обёртка, строка 314) передать `None` шестым аргументом. `ApprovalAuditEntry` и оба вызова `push_audit` остаются без изменений — обоснование выше, в блоке Interfaces.

В `crates/tool-runtime/src/registry.rs` добавить рядом с `scope_from_input`:

```rust
fn command_from_input(tool_name: &str, input: &Value) -> Option<String> {
    if tool_name != tools::shell::NAME {
        return None;
    }
    if let Some(command) = input.get("command").and_then(Value::as_str) {
        return normalize_command(command);
    }
    let program = input.get("program").and_then(Value::as_str)?;
    let args = input
        .get("args")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    normalize_command(&format!("{program} {args}"))
}

fn normalize_command(command: &str) -> Option<String> {
    let normalized = command.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}
```

Заменить блок проверки разрешений в `execute_with_cancellation`:

```rust
        let command = command_from_input(name, &input);
        for permission in definition.permissions {
            let scope = scope_from_input(name, &input);
            match self
                .permissions
                .check_scoped(
                    *permission,
                    &evohime_permissions::PermissionCheck {
                        session_id: ctx.session_id,
                        path: Some(scope.as_str()),
                        command: command.as_deref(),
                    },
                )
                .await
            {
                PermissionDecision::Allowed => {}
                PermissionDecision::Denied => return Err(ToolError::PermissionDenied(*permission)),
                PermissionDecision::NeedsApproval => {
                    let approval = self
                        .permissions
                        .create_approval_scoped(
                            ctx.task_id,
                            ctx.session_id,
                            name,
                            *permission,
                            scope,
                            command.clone(),
                        )
                        .await;
                    return Err(ToolError::NeedsApproval {
                        tool: name.to_string(),
                        permission: *permission,
                        scope: approval.scope,
                        approval_id: approval.id,
                        input: input.clone(),
                    });
                }
            }
        }
```

Затем прогнать `cargo check --workspace`. Полный список мест, которые сломаются от новой сигнатуры (проверено по репозиторию — других вызовов нет):

- `crates/permissions/src/lib.rs:314` — обёртка `create_approval`, передать шестым аргументом `None`;
- `crates/permissions/src/lib.rs:724` — тест `grant_remembers_path_for_session`, добавить `None`;
- `crates/tool-runtime/src/registry.rs:295` — единственный продуктовый вызов, правится кодом выше.

`crates/evohime-core/src/ipc_bridge.rs` `create_approval_scoped` не вызывает и правки не требует.

- [ ] **Step 4: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-permissions -p evohime-tool-runtime -p evohime-core`
Expected: PASS. Если `bootstrap_registers_filesystem_read` упал на количестве инструментов — значит был случайно затронут реестр; это регрессия, а не ожидаемое изменение.

- [ ] **Step 5: Коммит**

```bash
git add crates/permissions/src/lib.rs crates/tool-runtime/src/registry.rs
git commit -m "feat(tools): match permission policy against shell commands"
```

---

### Task 5: Привязать approval к конкретному вызову

**Files:**
- Modify: `crates/permissions/src/lib.rs` (новый метод `approval_matches` рядом с `approval`, строка 414)
- Modify: `crates/tool-runtime/src/registry.rs:362-382` (`execute_after_approval`)

**Interfaces:**
- Consumes: `command_from_input` и `ApprovalRequest.command` из Task 4.
- Produces: `PermissionEngine::approval_matches(&self, id: Uuid, tool_name: &str, scope: &str, command: Option<&str>) -> bool` — `true` только если approval существует, находится в состоянии `Granted` **и** описывает ровно этот вызов.

**Проблема (найдена при ревью плана).** `execute_after_approval` проверяет только то, что approval с таким `approval_id` находится в состоянии `Granted`, после чего исполняет **тот `input`, который передан в вызов**, а не тот, что был сохранён в approval. Ни сверки с одобренным вызовом, ни `check_scoped` на этом пути нет. Последствия:

- одобрение, выданное на `cargo test`, годится для исполнения чего угодно другого, если вызывающий подставит другой `input`;
- правило `Deny`, добавленное в `permissions.json` между запросом approval и его подтверждением, не сработает.

Без этой задачи вся политика из Tasks 1–4 обходится штатным потоком «агент попросил → хозяин подтвердил».

**Две отдельные защиты, обе нужны.** Сверка вызова с одобренным закрывает подмену целиком, включая команды, которых нет ни в одном правиле (`cargo test` → `cargo publish`). Повторная проверка `Deny` закрывает изменение политики между запросом и подтверждением, когда сам вызов не менялся. Ни одна из них не покрывает случай другой.

**Почему нормализация живёт в `evohime-permissions`.** `ApprovalRequest.scope` пропущен через приватную `normalize_scope_path` (`crates/permissions/src/lib.rs:572`), которая из крейта не экспортируется. Сравнивать снаружи означало бы дублировать её правила и однажды разойтись, поэтому сравнение выполняется методом самого движка.

- [ ] **Step 1: Написать падающие тесты**

Добавить в `mod tests` в `crates/tool-runtime/src/registry.rs`:

```rust
    #[tokio::test]
    async fn approval_cannot_be_reused_for_a_different_call() {
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::ShellExecute,
                evohime_permissions::PermissionMode::Ask,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: Some(Uuid::new_v4()),
            progress_tx: None,
        };

        let result = registry
            .execute(
                &context,
                "shell.execute",
                serde_json::json!({ "command": "cargo --version" }),
            )
            .await;
        let approval_id = match result {
            Err(ToolError::NeedsApproval { approval_id, .. }) => approval_id,
            other => panic!("expected NeedsApproval, got {other:?}"),
        };
        registry
            .permissions()
            .resolve(approval_id, true)
            .await
            .expect("granted");

        // No policy rule covers `cargo publish`; the substitution alone must
        // be enough to refuse.
        let substituted = registry
            .execute_after_approval(
                &context,
                "shell.execute",
                serde_json::json!({ "command": "cargo publish" }),
                approval_id,
                CancellationToken::new(),
            )
            .await;
        assert!(
            matches!(&substituted, Err(ToolError::Execution(message)) if message.contains("does not match")),
            "expected a mismatch refusal, got {substituted:?}"
        );

        // The call the owner actually approved is not refused as a mismatch.
        // Asserted negatively on purpose: whether `cargo` exists on PATH is
        // not what this test is about.
        let approved = registry
            .execute_after_approval(
                &context,
                "shell.execute",
                serde_json::json!({ "command": "cargo --version" }),
                approval_id,
                CancellationToken::new(),
            )
            .await;
        assert!(
            !matches!(&approved, Err(ToolError::Execution(message)) if message.contains("does not match")),
            "approved call must not be refused as a mismatch, got {approved:?}"
        );
    }

    #[tokio::test]
    async fn approved_call_cannot_execute_a_denied_command() {
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::ShellExecute,
                evohime_permissions::PermissionMode::Ask,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: Some(Uuid::new_v4()),
            progress_tx: None,
        };

        // The agent asks for a harmless command and the owner approves it.
        let result = registry
            .execute(
                &context,
                "shell.execute",
                serde_json::json!({ "command": "cargo --version" }),
            )
            .await;
        let approval_id = match result {
            Err(ToolError::NeedsApproval { approval_id, .. }) => approval_id,
            other => panic!("expected NeedsApproval, got {other:?}"),
        };
        registry
            .permissions()
            .resolve(approval_id, true)
            .await
            .expect("granted");

        // A deny rule lands after the approval was granted but before the call
        // actually runs. The call itself is unchanged, so the mismatch check
        // passes and the deny check is what must stop it.
        registry
            .permissions()
            .set_policy_rules(evohime_permissions::PolicyRuleSet::new(vec![
                evohime_permissions::PolicyRule {
                    permission: evohime_permissions::Permission::ShellExecute,
                    pattern: "cargo *".into(),
                    mode: evohime_permissions::PermissionMode::Deny,
                },
            ]))
            .await;

        let replayed = registry
            .execute_after_approval(
                &context,
                "shell.execute",
                serde_json::json!({ "command": "cargo --version" }),
                approval_id,
                CancellationToken::new(),
            )
            .await;
        assert!(matches!(replayed, Err(ToolError::PermissionDenied(_))));
    }
```

- [ ] **Step 2: Запустить тест и убедиться, что он падает**

Run: `cargo test -p evohime-tool-runtime approval`
Expected: FAIL — `approval_cannot_be_reused_for_a_different_call` не компилируется (нет `approval_matches`); после добавления метода он и `approved_call_cannot_execute_a_denied_command` падают по существу.

- [ ] **Step 3: Добавить сверку в движок разрешений**

В `crates/permissions/src/lib.rs`, рядом с методом `approval` (строка 414):

```rust
    /// True when `id` is a granted approval describing exactly this call.
    ///
    /// Normalization of `scope` happens here on purpose: the rules live in
    /// this crate and must not be duplicated by callers.
    pub async fn approval_matches(
        &self,
        id: Uuid,
        tool_name: &str,
        scope: &str,
        command: Option<&str>,
    ) -> bool {
        let approvals = self.approvals.read().await;
        let Some(record) = approvals.get(&id) else {
            return false;
        };
        record.state == ApprovalState::Granted
            && record.request.tool_name == tool_name
            && record.request.scope == normalize_scope_path(scope)
            && record.request.command.as_deref() == command
    }
```

**Попутно чиним порядок в `normalize_scope_path`** (`crates/permissions/src/lib.rs:572`). Сейчас там:

```rust
    path.as_ref()
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/")
```

Префикс `./` срезается **до** замены разделителей, поэтому windows-написание `.\src\main.rs` превращается в `./src/main.rs` и не совпадает с `src/main.rs`. Для одобрений это прямая дыра: одна и та же цель в двух написаниях выглядит как два разных scope. Поменять порядок:

```rust
fn normalize_scope_path(path: impl AsRef<str>) -> String {
    path.as_ref()
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}
```

И тест в `mod tests` того же файла — на ту самую нормализацию, ради которой метод живёт в этом крейте:

```rust
    #[test]
    fn scope_normalization_folds_windows_spelling() {
        assert_eq!(normalize_scope_path(".\\src\\main.rs"), "src/main.rs");
        assert_eq!(normalize_scope_path("./src/main.rs"), "src/main.rs");
        assert_eq!(normalize_scope_path("  src/main.rs "), "src/main.rs");
    }
```

```rust
    #[test]
    fn approval_matches_ignores_path_spelling_but_not_content() {
        block_on(async {
            let engine = PermissionEngine::new();
            let request = engine
                .create_approval_scoped(
                    Uuid::new_v4(),
                    None,
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    "src/main.rs",
                    None,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            // Same path, different spelling — still the approved call.
            assert!(
                engine
                    .approval_matches(request.id, "filesystem.write", ".\\src\\main.rs", None)
                    .await
            );
            // Different file, different tool, or an unexpected command — not.
            assert!(
                !engine
                    .approval_matches(request.id, "filesystem.write", "src/other.rs", None)
                    .await
            );
            assert!(
                !engine
                    .approval_matches(request.id, "filesystem.read", "src/main.rs", None)
                    .await
            );
            assert!(
                !engine
                    .approval_matches(request.id, "filesystem.write", "src/main.rs", Some("rm -rf"))
                    .await
            );
        });
    }
```

- [ ] **Step 4: Применить сверку и повторную проверку запретов**

В `crates/tool-runtime/src/registry.rs` заменить начало `execute_after_approval` (строки 370-382 — проверку `granted` и получение `definition`) на:

```rust
        // An approval authorizes one specific call, not the tool in general.
        let scope = scope_from_input(name, &input);
        let command = command_from_input(name, &input);
        if !self
            .permissions
            .approval_matches(approval_id, name, &scope, command.as_deref())
            .await
        {
            return Err(ToolError::Execution(
                "approval does not match this call".to_string(),
            ));
        }

        let definition = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;

        // Hard denials still apply: the policy may have changed between the
        // request and the confirmation. Only `Denied` is acted on — re-asking
        // would deadlock a flow that is already past its approval.
        for permission in definition.permissions {
            if self
                .permissions
                .check_scoped(
                    *permission,
                    &evohime_permissions::PermissionCheck {
                        session_id: ctx.session_id,
                        path: Some(scope.as_str()),
                        command: command.as_deref(),
                    },
                )
                .await
                == PermissionDecision::Denied
            {
                return Err(ToolError::PermissionDenied(*permission));
            }
        }
```

Сообщение `"approval is not granted"` исчезает: `approval_matches` возвращает `false` и для неподтверждённого approval тоже, а вызывающему в обоих случаях нужен один и тот же отказ. Проверить, что на это сообщение никто не полагается: `grep -rn "approval is not granted" --include=*.rs --include=*.cs .` — ожидается пусто после правки.

- [ ] **Step 5: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-permissions -p evohime-tool-runtime`
Expected: PASS — три новых теста зелёные, существующие approval-тесты (`ask_mode_creates_scoped_approval`, `approval_is_one_shot`, `grant_remembers_path_for_session`) не сломаны.

- [ ] **Step 6: Коммит**

```bash
git add crates/permissions/src/lib.rs crates/tool-runtime/src/registry.rs
git commit -m "fix(tools): bind approvals to the exact call they authorized"
```

---

### Task 6: Загрузка `permissions.json` из data dir

**Files:**
- Create: `crates/evohime-core/src/permission_rules.rs`
- Modify: `crates/evohime-core/src/lib.rs:626-627` (вставить `pub mod permission_rules;` между `pub mod observability;` и `pub mod plan;` — список объявлений там алфавитный), `crates/evohime-core/src/main.rs:24` (применить правила после `ToolRegistry::bootstrap()`)
- Modify: `docs/architecture.md` (раздел про данные и диагностику — добавить строку про файл правил)

**Interfaces:**
- Consumes: `PolicyRuleSet`, `PolicyRule`, `PermissionEngine::set_policy_rules`.
- Produces:
  - `pub fn rules_path() -> PathBuf` — `<data_dir>/permissions.json`.
  - `pub fn load_rules_from(path: &Path) -> Result<PolicyRuleSet, String>` — `Ok(defaults())` при отсутствующем или пустом файле, `Ok(rules)` при валидном, `Err(message)` при битом JSON.
  - `pub async fn apply_rules(permissions: &PermissionEngine)` — грузит из `rules_path()`, логирует ошибку разбора и в этом случае применяет `defaults()`.

**Про логирование:** в `evohime-core` нет ни `tracing`, ни `log` — крейт пишет структурный JSONL через `StructuredLogger` (`crates/evohime-core/src/logging.rs:11-45`), который уже реэкспортирован из корня крейта (`lib.rs:558`), поэтому обращаемся как `crate::StructuredLogger`. Новых зависимостей не добавляем. Разбор файла держим чистым (`Result`), чтобы тесты не трогали файловую систему логов.

**Про тесты:** `tempfile` в `[dev-dependencies]` крейта `evohime-core` отсутствует (там только `wiremock`). Вместо добавления зависимости повторяем идиому соседнего теста `crates/evohime-core/src/logging.rs:54` — уникальный путь внутри `std::env::temp_dir()` с уборкой за собой.

**Формат файла** (сознательно тот же порядок «последнее правило побеждает»):

```json
[
  { "permission": "shell_execute", "pattern": "*", "mode": "ask" },
  { "permission": "shell_execute", "pattern": "cargo *", "mode": "allow" },
  { "permission": "shell_execute", "pattern": "rm *", "mode": "deny" },
  { "permission": "filesystem_read", "pattern": "*.env", "mode": "deny" }
]
```

Отсутствующий файл — не ошибка: применяются `defaults()`. Битый файл — тоже не ошибка запуска (агент не должен падать из-за опечатки в конфиге), но факт обязан попасть в лог.

- [ ] **Step 1: Написать падающие тесты**

Создать `crates/evohime-core/src/permission_rules.rs`:

```rust
//! Loads declarative permission policy rules from the data directory.

use evohime_permissions::{PermissionEngine, PolicyRuleSet};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// `<data_dir>/permissions.json`.
pub fn rules_path() -> PathBuf {
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA").map(|path| PathBuf::from(path).join("EvoHime"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime"));
    data_dir.join("permissions.json")
}

/// Read rules from `path`.
///
/// A missing or empty file is not an error — it means "use the built-in
/// defaults". Malformed JSON returns `Err` with a human-readable message; the
/// caller logs it and still starts, because a typo in the config must not stop
/// the agent.
pub fn load_rules_from(path: &Path) -> Result<PolicyRuleSet, String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(PolicyRuleSet::defaults());
    };
    if contents.trim().is_empty() {
        return Ok(PolicyRuleSet::defaults());
    }
    serde_json::from_str::<PolicyRuleSet>(&contents).map_err(|error| error.to_string())
}

/// Log path for the malformed-config warning: `<data_dir>/logs/core.jsonl`.
fn core_log_path() -> PathBuf {
    let mut path = rules_path();
    path.pop();
    path.join("logs").join("core.jsonl")
}

pub async fn apply_rules(permissions: &PermissionEngine) {
    let path = rules_path();
    let rules = match load_rules_from(&path) {
        Ok(rules) => rules,
        Err(error) => {
            if let Ok(logger) = crate::StructuredLogger::open(core_log_path()) {
                let _ = logger.write(
                    "warn",
                    "permissions.rules.malformed",
                    serde_json::json!({
                        "path": path.display().to_string(),
                        "error": error,
                    }),
                );
            }
            PolicyRuleSet::defaults()
        }
    };
    permissions.set_policy_rules(rules).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_permissions::{Permission, PermissionMode};

    /// Unique scratch dir per test, mirroring `logging.rs` — `evohime-core`
    /// carries no `tempfile` dev-dependency.
    fn scratch_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "evohime-rules-{}-{label}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn missing_file_falls_back_to_defaults() {
        let dir = scratch_dir("missing");
        let rules = load_rules_from(&dir.join("permissions.json")).expect("missing file is ok");
        assert_eq!(rules, PolicyRuleSet::defaults());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_file_is_reported_as_error() {
        let dir = scratch_dir("malformed");
        let path = dir.join("permissions.json");
        fs::write(&path, "{ not json").expect("write");
        assert!(load_rules_from(&path).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn valid_file_is_parsed_in_order() {
        let dir = scratch_dir("valid");
        let path = dir.join("permissions.json");
        fs::write(
            &path,
            r#"[
                { "permission": "shell_execute", "pattern": "*", "mode": "ask" },
                { "permission": "shell_execute", "pattern": "cargo *", "mode": "allow" },
                { "permission": "shell_execute", "pattern": "rm *", "mode": "deny" }
            ]"#,
        )
        .expect("write");

        let rules = load_rules_from(&path).expect("valid file parses");
        assert_eq!(rules.rules().len(), 3);
        assert_eq!(
            rules.resolve(Permission::ShellExecute, "cargo test"),
            Some(PermissionMode::Allow)
        );
        assert_eq!(
            rules.resolve(Permission::ShellExecute, "rm -rf target"),
            Some(PermissionMode::Deny)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn empty_array_disables_all_rules() {
        let dir = scratch_dir("empty-array");
        let path = dir.join("permissions.json");
        fs::write(&path, "[]").expect("write");
        assert!(load_rules_from(&path).expect("parses").is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
```

- [ ] **Step 2: Запустить тесты и убедиться, что они падают**

Run: `cargo test -p evohime-core permission_rules::`
Expected: FAIL — модуль не объявлен в `lib.rs`, тесты не собираются.

- [ ] **Step 3: Подключить модуль и вызвать при старте**

В `crates/evohime-core/src/lib.rs` блок объявлений модулей (строки 618–635) отсортирован по алфавиту. Вставить между `pub mod observability;` и `pub mod plan;`:

```rust
pub mod permission_rules;
```

В `crates/evohime-core/src/main.rs` после строки 24 (`let tools = std::sync::Arc::new(evohime_tool_runtime::ToolRegistry::bootstrap());`) добавить:

```rust
    evohime_core::permission_rules::apply_rules(tools.permissions()).await;
```

`main` объявлен как `#[tokio::main] async fn main()`, поэтому `await` на этом месте допустим без дополнительных обвязок.

Новых записей в `crates/evohime-core/Cargo.toml` не требуется: `evohime-permissions` уже подключён (строка 17), `serde_json` — строка 22, логирование идёт через внутренний модуль `crate::logging`.

- [ ] **Step 4: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-core permission_rules::`
Expected: PASS — четыре теста зелёные.

Затем: `cargo check --workspace`
Expected: без ошибок.

- [ ] **Step 5: Задокументировать файл**

В `docs/architecture.md` раздел называется `## Данные и восстановление` (строка 26) и написан прозой, а не списком. Дописать в конец абзаца на строке 28 предложение в том же стиле:

```markdown
Правила разрешений читаются из `%LOCALAPPDATA%\EvoHime\permissions.json`: упорядоченный массив, побеждает последнее совпавшее правило; отсутствующий или пустой файл означает встроенный набор по умолчанию.
```

- [ ] **Step 6: Коммит**

```bash
git add crates/evohime-core/src/permission_rules.rs crates/evohime-core/src/lib.rs crates/evohime-core/src/main.rs docs/architecture.md
git commit -m "feat(core): load permission policy rules from data dir"
```

---

### Task 7: Окно повторов вместо вечной блокировки вызова

**Files:**
- Modify: `crates/evohime-core/src/lib.rs:1957` (объявление `seen_tool_calls`), `:2072-2092` (логика дедупликации в цикле `ToolAgent`)

**Interfaces:**
- Consumes: ничего из предыдущих задач (независима от Tasks 1–6, но идёт последней, чтобы не конфликтовать по `crates/evohime-core/src/lib.rs` с Task 6).
- Produces: изменённое поведение цикла агента; публичных сигнатур не меняет.

**Проблема:** сейчас `seen_tool_calls: HashSet` живёт весь таск, и `retain` вырезает любой вызов, который *когда-либо* уже встречался. Значит легитимный повтор — второй прогон `cargo test` после правки, повторное чтение файла после записи — молча удаляется до конца задачи, и агент получает подсказку «выбери другой шаг», хотя правильный шаг был именно этот.

**Решение:** ограничить дедупликацию скользящим окном последних `TOOL_CALL_HISTORY_WINDOW` вызовов. Повтор внутри окна — это петля, его по-прежнему вырезаем; повтор после окна — легитимная переработка, пропускаем.

- [ ] **Step 1: Написать падающий тест**

Тестовый модуль в `crates/evohime-core/src/lib.rs:4537` импортирует символы **явным списком**, а не через `use super::*`, поэтому сначала дописать тип в этот список:

```rust
    use super::{
        AgentRunError, CoreCommand, CoreEvent, CoreVersion, EventJournal, ModelAgent,
        RecentToolCalls, TaskCoordinator, TaskExecutor, ToolAgent,
    };
```

Затем добавить в тот же `mod tests` сам тест:

```rust
    #[test]
    fn recent_tool_calls_window_blocks_only_immediate_repeats() {
        let mut history = RecentToolCalls::new(3);
        assert!(history.remember("shell.execute:{\"command\":\"cargo test\"}"));
        assert!(!history.remember("shell.execute:{\"command\":\"cargo test\"}"));

        assert!(history.remember("filesystem.read:{\"path\":\"a.rs\"}"));
        assert!(history.remember("filesystem.write:{\"path\":\"a.rs\"}"));
        assert!(history.remember("filesystem.read:{\"path\":\"b.rs\"}"));

        // The first entry has now fallen out of the 3-slot window.
        assert!(history.remember("shell.execute:{\"command\":\"cargo test\"}"));
    }
```

- [ ] **Step 2: Запустить тест и убедиться, что он падает**

Run: `cargo test -p evohime-core recent_tool_calls_window`
Expected: FAIL на компиляции — `RecentToolCalls` не существует.

- [ ] **Step 3: Реализовать**

Добавить в `crates/evohime-core/src/lib.rs` на уровне корня крейта — непосредственно перед строкой 555 `mod ipc_bridge;`. `HashSet` там уже в области видимости (импорт на строке 593; порядок `use` относительно определения значения не имеет), а `VecDeque` не импортирован, поэтому он записан полным путём:

```rust
/// How many recent tool calls are checked for repetition. A repeat inside the
/// window is a loop; a repeat after it is legitimate rework (re-running tests
/// after an edit, re-reading a file after writing it).
const TOOL_CALL_HISTORY_WINDOW: usize = 6;

/// Bounded recency window over tool-call signatures.
struct RecentToolCalls {
    capacity: usize,
    order: std::collections::VecDeque<String>,
    present: HashSet<String>,
}

impl RecentToolCalls {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            order: std::collections::VecDeque::with_capacity(capacity),
            present: HashSet::new(),
        }
    }

    /// Record `signature`. Returns `false` when it is already in the window.
    fn remember(&mut self, signature: &str) -> bool {
        if self.present.contains(signature) {
            return false;
        }
        if self.order.len() == self.capacity {
            if let Some(evicted) = self.order.pop_front() {
                self.present.remove(&evicted);
            }
        }
        self.order.push_back(signature.to_string());
        self.present.insert(signature.to_string());
        true
    }
}
```

Заменить строку 1957 `let mut seen_tool_calls = HashSet::new();` на:

```rust
        let mut seen_tool_calls = RecentToolCalls::new(TOOL_CALL_HISTORY_WINDOW);
```

Заменить вызов внутри `retain` (строка 2077):

```rust
                let is_new = seen_tool_calls.remember(&format!("{}:{}", call.name, call.arguments));
```

Текст подсказки в `messages.push(...)` оставить без изменений — он по-прежнему верен для повтора внутри окна.

- [ ] **Step 4: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-core`
Expected: PASS — новый тест зелёный, существующие тесты цикла агента не сломаны.

- [ ] **Step 5: Коммит**

```bash
git add crates/evohime-core/src/lib.rs
git commit -m "fix(core): bound duplicate tool-call detection to a recent window"
```

---

## Финальная проверка

- [ ] `cargo test -p evohime-permissions -p evohime-tool-runtime -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc`
- [ ] `cargo check -p evohime-supervisor`
- [ ] `git diff --check`
- [ ] Ручной прогон: положить `permissions.json` с правилом `{"permission":"shell_execute","pattern":"rm *","mode":"deny"}` в `%LOCALAPPDATA%\EvoHime`, запустить `.\start-dev.ps1`, дать задачу, требующую `rm`, и убедиться, что вызов отклонён без диалога approval.

## Сознательно вне объёма

- **Отдельный `external_directory` permission из opencode.** У нас выход за пределы workspace уже блокируется песочницей (`crates/tool-runtime/src/sandbox.rs`, `resolve_existing` / `resolve_for_write`), отдельный слой разрешений дублировал бы её.
- **Редактирование правил из WinUI.** Требует изменений в `evohime.desktop.proto` и в policy-панели; делается отдельным планом, после того как формат правил устоится на практике. До тех пор источник истины — `permissions.json`.
- **Изменения в WinUI-панели подтверждений.** Не нужны: `CoreEvent::ApprovalRequired` уже несёт весь `input` вызова, поэтому команда доступна панели и без правок Core.
- **LSP-диагностика и файл инструкций проекта (AGENTS.md) из того же обзора opencode.** Это отдельные фичи этапа 4, каждая тянет свой план.

## Известные ограничения этой реализации

Их надо знать заранее, чтобы правило `*.env → deny` не создавало ложного чувства защищённости:

1. **`filesystem.search` обходит запрет по пути.** Инструмент требует того же `Permission::FilesystemRead` (`crates/tool-runtime/src/tools/search.rs:11`), но его scope — корень поиска, а не найденные файлы. Значит grep по workspace вернёт содержимое `.env`, хотя `filesystem.read` для него запрещён. Полное закрытие требует фильтрации результатов внутри самого инструмента — отдельная задача.
2. **Вложенные команды не разбираются.** Матчинг идёт по нормализованной строке, поэтому `rm -rf target` отклоняется, а `cmd /c rm -rf target` или `powershell -c "rm ..."` — нет. Правила стоит писать и на интерпретаторы (`cmd *`, `powershell *`), если это важно.
3. **Политика не покрывает пути внутри аргументов.** Для `shell.execute` subject — команда, а не файлы, которые она тронет; ограничение записи по путям остаётся за песочницей и `filesystem.*`.
