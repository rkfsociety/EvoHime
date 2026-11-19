# План 197.2 — Runtime-интеграция: ModelCatalog per-route и опциональный git worktree

Статус: active implementation contract. Этап следует после 197.1.

## Scope и изменяемые контракты
1. **`ModelCatalog` и проверки доступности по выбранному провайдеру:**
   - Обновить обработчик `ModelCatalog` в Core
     (`ipc_bridge_ambient_model_and_protocol.rs` / `ipc_bridge_core_commands.rs`),
     чтобы запрос каталога принимал опциональные `provider` и `profile_id` и
     обращался к соответствующему маршруту в `ModelGateway`, не требуя смены
     глобального `default_route`.
   - Кэш каталога моделей (`ProviderCatalogCache`) ключуется по
     `(provider, profile_id, base_url)`, чтобы параллельные запросы из разных
     чатов не перезаписывали кэш друг друга.
2. **Опциональная файловая изоляция (`git worktree` песочница):**
   - Подключить режим `isolation_mode` (`shared_workspace` по умолчанию /
     `isolated_worktree` для пишущих задач в git-репозитории) в
     `core_coordinator_routing.rs` с опорой на существующие контракты
     `taskWorktreeIsolation` и `agent_git_change_sets`.
   - При `isolated_worktree` Core создаёт для задачи/чата выделенный
     `git worktree` в служебном каталоге (не внутри дерева исходников),
     подставляет его как `workspace_root` для инструментов этой задачи и
     сохраняет привязку в `EventJournal`.
   - По завершении задачи пользователь получает diff / summary изменений
     worktree и может применить их в основную ветку или отбросить, после чего
     worktree корректно удаляется (`git worktree remove` + prune).
3. **Recovery незавершённых worktree:**
   - При старте/восстановлении Core проверяет «осиротевшие» worktree от
     прерванных задач: если задача в терминальном статусе или отменена без
     применения, работа не теряется молча (фиксируется в журнале), а
     пользователь может применить или очистить worktree.

## Зависимости
### Блокирующие
Этап 197.1 (`ModelGateway` multi-route, `SessionBinding`);
`crates/evohime-core/src/core_coordinator_routing.rs`;
`crates/evohime-core/src/agent_git_change_sets.rs`.

### Опциональные
Если проект не является git-репозиторием или пользователь выбрал
`shared_workspace`, задача выполняется напрямую в `workspace_path` без
создания worktree.

## Recovery и rollback
- Если создание `git worktree` не удалось (например, dirty index с конфликтом
  или не-git папка), задача не падает молча в общую папку при явно
  запрошенном `isolated_worktree`, а возвращает типизированную ошибку с
  предложением переключиться в `shared_workspace`.
- Rollback — отключение `isolated_worktree` (остаётся `shared_workspace`).

## Verification
- Интеграционный тест Core: параллельный запуск `ModelCatalog` для двух
  разных маршрутов без взаимного загрязнения кэша.
- Тест жизненного цикла `isolated_worktree`: две параллельные задачи меняют
  один и тот же файл в разных worktree без конфликта в основном рабочем
  дереве, затем корректная очистка worktree.
