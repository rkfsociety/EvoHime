# Permission Policy Rules — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Дать EvoHime декларативные правила разрешений с glob-паттернами, которые учитывают *содержимое* вызова инструмента (в первую очередь shell-команду), а не только категорию и путь.

**Architecture:** В `evohime-permissions` добавляется новый слой — упорядоченный набор `PolicyRule` (permission + glob-паттерн + режим), с семантикой «побеждает последнее совпавшее правило». Слой встраивается в существующий `check_scoped` с явным приоритетом: жёсткий `Deny` из правил не может быть перекрыт runtime-грантами, остальные режимы правил стоят ниже path grants и session overrides. `tool-runtime` начинает передавать в проверку нормализованную shell-команду, поэтому `rm *` и `git *` становятся различимыми политикой. Правила загружаются Core из `permissions.json` в data dir и переживают перезапуск через существующий snapshot-механизм.

**Tech Stack:** Rust 2021, tokio (`RwLock`), serde, futures-executor (в тестах `evohime-permissions`), tempfile (в тестах `evohime-core`).

**Источник идеи:** дизайн permission-системы opencode (`opencode.ai/docs/permissions`). Код оттуда не переносится — только модель правил. Перед любым заимствованием текста/кода проверить LICENSE репозитория.

## Global Constraints

- Продукт — native Windows: WinUI 3 + Rust Core + SQLite + named-pipe IPC. Ни веб-панель, ни HTTP-сервер не трогаем.
- Изменения в `crates/desktop-ipc/proto/evohime.desktop.proto` в этом плане **не выполняются**. Все новые поля — внутри уже существующих JSON-payload'ов либо только в Rust.
- Новые Rust-функции и исправления покрываются тестами (правило 3 из AGENTS.md).
- Каждая задача заканчивается task-only git-коммитом в текущей ветке `main`. Push — не выполняем.
- Перед заявлением о готовности задачи: свежий прогон тестов + `git diff --check`.
- Сравнение путей и команд — регистронезависимое (Windows), разделитель нормализуется в `/`.
- Существующие сериализованные структуры расширяются только через `#[serde(default)]`, чтобы старые снапшоты читались без миграции.

## File Structure

| Файл | Ответственность |
| --- | --- |
| `crates/permissions/src/pattern.rs` (создать) | Чистая функция glob-сопоставления `glob_match(pattern, value)`. Без зависимостей от движка. |
| `crates/permissions/src/policy.rs` (создать) | `PolicyRule`, `PolicyRuleSet`, разрешение «последнее совпавшее правило», дефолтный набор. |
| `crates/permissions/src/lib.rs` (изменить) | Хранение набора правил в `PermissionEngine`, встраивание в `check_scoped`, поле `command` в `PermissionCheck` и `ApprovalRequest`, правила в snapshot. |
| `crates/tool-runtime/src/registry.rs` (изменить) | Извлечение нормализованной команды из input и передача её в проверку и в approval. |
| `crates/evohime-core/src/permission_rules.rs` (создать) | Загрузка `permissions.json` из data dir, seed дефолтного файла, применение к движку. |
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

- [ ] **Step 1: Написать падающий тест**

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
}
```

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

Проверить `crates/permissions/Cargo.toml`. Если `serde_json` отсутствует в `[dev-dependencies]`, добавить туда, взяв версию из корневого workspace-манифеста (использовать `serde_json = { workspace = true }`, если остальные крейты объявляют зависимости так, иначе — ту же строку версии, что в `crates/tool-runtime/Cargo.toml`).

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
- Modify: `crates/permissions/src/lib.rs:73-76` (`PermissionCheck`), `:125-134` (поля `PermissionEngine`), `:151-170` (`new`), `:276-305` (`check_scoped`), `:113-120` (`PermissionScopesSnapshot`), `:456-493` (`export_scopes`/`import_scopes`)

**Interfaces:**
- Consumes: `PolicyRuleSet::resolve`, `PolicyRuleSet::defaults` из Task 2.
- Produces:
  - `PermissionCheck` получает поле `pub command: Option<&'a str>` (по умолчанию `None`, структура остаётся `Default`).
  - `PermissionEngine::set_policy_rules(&self, rules: PolicyRuleSet)`, `PermissionEngine::policy_rules(&self) -> PolicyRuleSet`.
  - `PermissionScopesSnapshot` получает поле `pub policy_rules: PolicyRuleSet` с `#[serde(default)]`.

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
    fn snapshot_roundtrip_preserves_policy_rules() {
        block_on(async {
            let engine = PermissionEngine::new();
            engine.set_policy_rules(PolicyRuleSet::defaults()).await;

            let snapshot = engine.export_scopes().await;
            let restored = PermissionEngine::new();
            restored.import_scopes(snapshot).await;

            assert_eq!(
                restored.policy_rules().await,
                PolicyRuleSet::defaults()
            );
        });
    }

    #[test]
    fn snapshot_without_policy_rules_field_still_parses() {
        let legacy = r#"{"session_overrides":[],"path_grants":[]}"#;
        let snapshot: PermissionScopesSnapshot =
            serde_json::from_str(legacy).expect("legacy snapshot must parse");
        assert!(snapshot.policy_rules.is_empty());
    }
```

Также обновить уже существующие тесты, которые конструируют `PermissionCheck { session_id, path }` литералом (`session_override_beats_global_mode`, `path_grant_allows_matching_prefix`, `grant_remembers_path_for_session`, `path_deny_overrides_session_allow`, `scopes_snapshot_roundtrip_preserves_grants`, `import_scopes_skips_expired_path_grants`), добавив в каждый литерал `command: None`.

- [ ] **Step 2: Запустить тесты и убедиться, что они падают**

Run: `cargo test -p evohime-permissions`
Expected: FAIL на компиляции — нет поля `command` в `PermissionCheck`, нет методов `set_policy_rules`/`policy_rules`, нет поля `policy_rules` в снапшоте.

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

Расширить снапшот:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionScopesSnapshot {
    #[serde(default)]
    pub session_overrides: Vec<SessionOverride>,
    #[serde(default)]
    pub path_grants: Vec<PathGrant>,
    #[serde(default)]
    pub policy_rules: PolicyRuleSet,
}
```

В `export_scopes` добавить `policy_rules: self.policy_rules().await,`. В начале `import_scopes` — `self.set_policy_rules(snapshot.policy_rules.clone()).await;` (перед разбором `session_overrides`, чтобы ранний `return` из-за пустых грантов не терял правила).

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

В `create_approval` (обёртка) передать `None` шестым аргументом. В `resolve_with_options` строка `tool_name: request.tool_name,` остаётся как есть — `ApprovalAuditEntry` не расширяем, команда уже видна через `scope`+`tool_name` в UI-слое и через `approval()`.

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

Затем прогнать `cargo check --workspace` и поправить все прочие места вызова `create_approval_scoped` и литеральной инициализации `PermissionCheck` / `ApprovalRequest` (ожидаются в `crates/tool-runtime/src/registry.rs` в `execute_after_approval` и в `crates/evohime-core/src/ipc_bridge.rs`), добавив `command: None` либо проброс уже вычисленной команды.

- [ ] **Step 4: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-permissions -p evohime-tool-runtime -p evohime-core`
Expected: PASS. Если `bootstrap_registers_filesystem_read` упал на количестве инструментов — значит был случайно затронут реестр; это регрессия, а не ожидаемое изменение.

- [ ] **Step 5: Коммит**

```bash
git add crates/permissions/src/lib.rs crates/tool-runtime/src/registry.rs crates/evohime-core/src/ipc_bridge.rs
git commit -m "feat(tools): match permission policy against shell commands"
```

---

### Task 5: Загрузка `permissions.json` из data dir

**Files:**
- Create: `crates/evohime-core/src/permission_rules.rs`
- Modify: `crates/evohime-core/src/lib.rs` (добавить `mod permission_rules;` рядом с остальными объявлениями модулей), `crates/evohime-core/src/main.rs:24` (применить правила после `ToolRegistry::bootstrap()`)
- Modify: `docs/architecture.md` (раздел про данные и диагностику — добавить строку про файл правил)

**Interfaces:**
- Consumes: `PolicyRuleSet`, `PolicyRule`, `PermissionEngine::set_policy_rules`.
- Produces:
  - `pub fn rules_path() -> PathBuf` — `<data_dir>/permissions.json`.
  - `pub fn load_rules_from(path: &Path) -> PolicyRuleSet` — читает файл; при отсутствии, пустом содержимом или ошибке парсинга возвращает `PolicyRuleSet::defaults()`.
  - `pub async fn apply_rules(permissions: &PermissionEngine)` — грузит из `rules_path()` и ставит в движок.

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

/// Read rules from `path`. A missing, empty, or malformed file falls back to
/// the built-in defaults — a typo in the config must not stop the agent.
pub fn load_rules_from(path: &Path) -> PolicyRuleSet {
    let Ok(contents) = fs::read_to_string(path) else {
        return PolicyRuleSet::defaults();
    };
    if contents.trim().is_empty() {
        return PolicyRuleSet::defaults();
    }
    match serde_json::from_str::<PolicyRuleSet>(&contents) {
        Ok(rules) => rules,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "permissions.json is malformed; falling back to default policy rules"
            );
            PolicyRuleSet::defaults()
        }
    }
}

pub async fn apply_rules(permissions: &PermissionEngine) {
    permissions
        .set_policy_rules(load_rules_from(&rules_path()))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_permissions::{Permission, PermissionMode};

    #[test]
    fn missing_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let rules = load_rules_from(&dir.path().join("permissions.json"));
        assert_eq!(rules, PolicyRuleSet::defaults());
    }

    #[test]
    fn malformed_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("permissions.json");
        fs::write(&path, "{ not json").expect("write");
        assert_eq!(load_rules_from(&path), PolicyRuleSet::defaults());
    }

    #[test]
    fn valid_file_is_parsed_in_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("permissions.json");
        fs::write(
            &path,
            r#"[
                { "permission": "shell_execute", "pattern": "*", "mode": "ask" },
                { "permission": "shell_execute", "pattern": "cargo *", "mode": "allow" },
                { "permission": "shell_execute", "pattern": "rm *", "mode": "deny" }
            ]"#,
        )
        .expect("write");

        let rules = load_rules_from(&path);
        assert_eq!(rules.rules().len(), 3);
        assert_eq!(
            rules.resolve(Permission::ShellExecute, "cargo test"),
            Some(PermissionMode::Allow)
        );
        assert_eq!(
            rules.resolve(Permission::ShellExecute, "rm -rf target"),
            Some(PermissionMode::Deny)
        );
    }

    #[test]
    fn empty_array_disables_all_rules() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("permissions.json");
        fs::write(&path, "[]").expect("write");
        assert!(load_rules_from(&path).is_empty());
    }
}
```

- [ ] **Step 2: Запустить тесты и убедиться, что они падают**

Run: `cargo test -p evohime-core permission_rules::`
Expected: FAIL — модуль не объявлен в `lib.rs`, тесты не собираются.

- [ ] **Step 3: Подключить модуль и вызвать при старте**

В `crates/evohime-core/src/lib.rs` добавить объявление рядом с существующими `mod`-строками:

```rust
pub mod permission_rules;
```

В `crates/evohime-core/src/main.rs` после строки 24 (`let tools = std::sync::Arc::new(evohime_tool_runtime::ToolRegistry::bootstrap());`) добавить:

```rust
    evohime_core::permission_rules::apply_rules(tools.permissions()).await;
```

`main` объявлен как `#[tokio::main] async fn main()`, поэтому `await` на этом месте допустим без дополнительных обвязок.

Проверить, что `tracing` и `tempfile` доступны крейту `evohime-core` (`tracing` — в `[dependencies]`, `tempfile` — в `[dev-dependencies]`); если чего-то нет, добавить так же, как это объявлено в `crates/tool-runtime/Cargo.toml`.

- [ ] **Step 4: Запустить тесты и убедиться, что они проходят**

Run: `cargo test -p evohime-core permission_rules::`
Expected: PASS — четыре теста зелёные.

Затем: `cargo check --workspace`
Expected: без ошибок.

- [ ] **Step 5: Задокументировать файл**

В `docs/architecture.md`, в списке данных/диагностики (рядом со строками про SQLite и логи), добавить пункт:

```markdown
- правила разрешений: `%LOCALAPPDATA%\EvoHime\permissions.json` (упорядоченный массив; побеждает последнее совпавшее правило; отсутствие файла = встроенные defaults).
```

- [ ] **Step 6: Коммит**

```bash
git add crates/evohime-core/src/permission_rules.rs crates/evohime-core/src/lib.rs crates/evohime-core/src/main.rs crates/evohime-core/Cargo.toml docs/architecture.md
git commit -m "feat(core): load permission policy rules from data dir"
```

---

### Task 6: Окно повторов вместо вечной блокировки вызова

**Files:**
- Modify: `crates/evohime-core/src/lib.rs:1957` (объявление `seen_tool_calls`), `:2072-2092` (логика дедупликации в цикле `ToolAgent`)

**Interfaces:**
- Consumes: ничего из предыдущих задач (независима от Tasks 1–5, но идёт последней, чтобы не конфликтовать по `lib.rs` с Task 5).
- Produces: изменённое поведение цикла агента; публичных сигнатур не меняет.

**Проблема:** сейчас `seen_tool_calls: HashSet` живёт весь таск, и `retain` вырезает любой вызов, который *когда-либо* уже встречался. Значит легитимный повтор — второй прогон `cargo test` после правки, повторное чтение файла после записи — молча удаляется до конца задачи, и агент получает подсказку «выбери другой шаг», хотя правильный шаг был именно этот.

**Решение:** ограничить дедупликацию скользящим окном последних `TOOL_CALL_HISTORY_WINDOW` вызовов. Повтор внутри окна — это петля, его по-прежнему вырезаем; повтор после окна — легитимная переработка, пропускаем.

- [ ] **Step 1: Написать падающий тест**

Добавить в `mod tests` в `crates/evohime-core/src/lib.rs` (рядом с прочими юнит-тестами файла):

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

Добавить в `crates/evohime-core/src/lib.rs` рядом с прочими вспомогательными типами файла:

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
- **`ApprovalAuditEntry.command`.** Аудит-запись не расширяем, чтобы не менять формат JSONL-журнала; команда доступна через `PermissionEngine::approval()`.
- **LSP-диагностика и файл инструкций проекта (AGENTS.md) из того же обзора opencode.** Это отдельные фичи этапа 4, каждая тянет свой план.
