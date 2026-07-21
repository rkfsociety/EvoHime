import type { KeyboardEvent } from "react";
import type {
  ChatSessionSummary,
  McpServerConfig,
  ModelConfig,
  ModelRouteDraft,
  PermissionAuditEntry,
  PermissionMode,
  PermissionScopes,
  PermissionSettings,
  SettingsTab,
  ToolDefinition,
} from "../types";
import { formatSessionPreview, formatSessionTitle } from "../lib/format";
import { reconcileModelForBilling } from "../lib/modelBilling";
import { LocalAuthSettingsSection } from "./LocalAuthSettingsSection";
import { MetricsSettingsSection } from "./MetricsSettingsSection";
import { WorkerSettingsSection } from "./WorkerSettingsSection";

type SettingsPanelProps = {
  settingsTab: SettingsTab;
  onSettingsTabChange: (tab: SettingsTab) => void;
  modelConfig: ModelConfig | null;
  modelConfigError: string | null;
  modelSaving: boolean;
  modelNotice: string | null;
  activeModelRoute: ModelRouteDraft | null;
  activeModelRouteIndex: number;
  orchestratorRoute: ModelRouteDraft | null;
  orchestratorRouteIndex: number;
  orchestratorModels: string[];
  onUpdateModelDraft: (index: number, patch: Partial<ModelRouteDraft>) => void;
  onSaveModelConfig: () => void;
  permissionSettings: PermissionSettings;
  permissionAudit: PermissionAuditEntry[];
  permissionScopes: PermissionScopes | null;
  onUpdatePermission: (name: string, mode: PermissionMode) => void;
  mcpServers: McpServerConfig[];
  mcpServersError: string | null;
  mcpServersNotice: string | null;
  mcpServersSaving: boolean;
  onAddMcpServer: () => void;
  onSaveMcpServers: () => void;
  onUpdateMcpServer: (index: number, patch: Partial<McpServerConfig>) => void;
  onRemoveMcpServer: (index: number) => void;
  toolCatalog: ToolDefinition[];
  toolCatalogError: string | null;
  archivedChats: ChatSessionSummary[];
  deletingSessionId: string | null;
  onRestoreSession: (chat: ChatSessionSummary) => void;
  onDeleteSession: (chat: ChatSessionSummary) => void;
};

const SETTINGS_TABS: Array<[SettingsTab, string, string]> = [
  ["model", "Модель", "Провайдер и маршруты"],
  ["permissions", "Разрешения", "Доступ инструментов"],
  ["mcp", "MCP", "Подключённые серверы"],
  ["tools", "Инструменты", "Каталог и таймауты"],
  ["worker", "Worker", "Статус и jobs"],
  ["metrics", "Metrics", "Pipeline snapshot"],
  ["archive", "Архив", "Скрытые чаты"],
];

function settingsTabId(tab: SettingsTab) {
  return `settings-tab-${tab}`;
}

function settingsTabPanelId(tab: SettingsTab) {
  return `settings-tabpanel-${tab}`;
}

function focusSettingsTab(tab: SettingsTab) {
  document.getElementById(settingsTabId(tab))?.focus();
}

export function SettingsPanel({
  settingsTab,
  onSettingsTabChange,
  modelConfig,
  modelConfigError,
  modelSaving,
  modelNotice,
  activeModelRoute,
  activeModelRouteIndex,
  orchestratorRoute,
  orchestratorRouteIndex,
  orchestratorModels,
  onUpdateModelDraft,
  onSaveModelConfig,
  permissionSettings,
  permissionAudit,
  permissionScopes,
  onUpdatePermission,
  mcpServers,
  mcpServersError,
  mcpServersNotice,
  mcpServersSaving,
  onAddMcpServer,
  onSaveMcpServers,
  onUpdateMcpServer,
  onRemoveMcpServer,
  toolCatalog,
  toolCatalogError,
  archivedChats,
  deletingSessionId,
  onRestoreSession,
  onDeleteSession,
}: SettingsPanelProps) {
  function handleTabKeyDown(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    const tabIds = SETTINGS_TABS.map(([id]) => id);
    let nextIndex: number | null = null;

    if (event.key === "ArrowRight") {
      nextIndex = (index + 1) % tabIds.length;
    } else if (event.key === "ArrowLeft") {
      nextIndex = (index - 1 + tabIds.length) % tabIds.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = tabIds.length - 1;
    }

    if (nextIndex === null) {
      return;
    }

    event.preventDefault();
    const nextTab = tabIds[nextIndex];
    onSettingsTabChange(nextTab);
    focusSettingsTab(nextTab);
  }

  return (
    <div className="settingsPanel">
      <nav className="settingsTabs" role="tablist" aria-label="Разделы настроек">
        {SETTINGS_TABS.map(([id, label, hint], index) => (
          <button
            key={id}
            type="button"
            id={settingsTabId(id)}
            className={settingsTab === id ? "settingsTab active" : "settingsTab"}
            onClick={() => onSettingsTabChange(id)}
            onKeyDown={(event) => handleTabKeyDown(event, index)}
            aria-selected={settingsTab === id}
            aria-controls={settingsTabPanelId(id)}
            role="tab"
            tabIndex={settingsTab === id ? 0 : -1}
          >
            <strong>{label}</strong>
            <span>{hint}</span>
          </button>
        ))}
      </nav>

      <div
        className="settingsTabContent"
        role="tabpanel"
        id={settingsTabPanelId(settingsTab)}
        aria-labelledby={settingsTabId(settingsTab)}
        tabIndex={0}
      >
        {settingsTab === "model" ? (
          <section className="settingsSection">
            <h3>Провайдер модели</h3>
            {modelConfigError ? <p className="settingsError">{modelConfigError}</p> : null}
            {modelConfig ? (
              <div className="modelSettingsEditor">
                <div className="settingsInlineBar">
                  <span className={modelConfig.configured ? "modelStatus configured" : "modelStatus"}>
                    {modelSaving ? "Сохранение..." : modelConfig.configured ? "Агент готов" : "Нужен API-ключ"}
                  </span>
                </div>
                {activeModelRoute ? (
                  <div className="modelProviderForm">
                    <label>
                      <span>Провайдер</span>
                      <select
                        value={activeModelRoute.provider}
                        onChange={(event) => {
                          const provider = event.target.value;
                          onUpdateModelDraft(activeModelRouteIndex, {
                            provider,
                            base_url: provider === "literouter" ? "https://api.literouter.com/v1" : "https://api.openai.com/v1",
                            model: provider === "literouter" ? "deepseek:free" : "gpt-4o-mini",
                            billing_mode: provider === "literouter" ? "free" : "paid",
                          });
                        }}
                      >
                        <option value="literouter">LiteRouter</option>
                        <option value="openai-compatible">OpenAI-compatible</option>
                      </select>
                    </label>
                    {activeModelRoute.provider === "literouter" ? (
                      <label>
                        <span>Режим LiteRouter</span>
                        <select
                          value={activeModelRoute.billing_mode}
                          onChange={(event) => {
                            const billing_mode = event.target.value as "free" | "paid";
                            const candidates = [
                              activeModelRoute.model,
                              ...(modelConfig?.routes.find((route) => route.name === activeModelRoute.name)
                                ?.available_models ?? []),
                              ...(modelConfig?.available_models ?? []),
                            ];
                            onUpdateModelDraft(activeModelRouteIndex, {
                              billing_mode,
                              model: reconcileModelForBilling(
                                activeModelRoute.model,
                                billing_mode,
                                candidates,
                              ),
                            });
                          }}
                        >
                          <option value="free">Бесплатный — только :free</option>
                          <option value="paid">Платный — без :free</option>
                        </select>
                      </label>
                    ) : null}
                    <label className="modelProviderKey">
                      <span>API-ключ</span>
                      <input
                        type="password"
                        value={activeModelRoute.api_key}
                        onChange={(event) => onUpdateModelDraft(activeModelRouteIndex, { api_key: event.target.value })}
                        onBlur={onSaveModelConfig}
                        placeholder={activeModelRoute.configured ? "Ключ сохранён — оставь пустым" : "Введи API-ключ"}
                      />
                      {activeModelRoute.configured && !activeModelRoute.api_key ? (
                        <small className="modelKeyStatus">Ключ сохранён</small>
                      ) : null}
                    </label>
                  </div>
                ) : null}
                <p className="settingsHint">Изменения сохраняются автоматически.</p>
                {modelNotice ? (
                  <p className={modelNotice.startsWith("Настройки") ? "settingsHint" : "settingsError"}>{modelNotice}</p>
                ) : null}
                <LocalAuthSettingsSection />
                {orchestratorRoute ? (
                  <section className="orchestratorSettings">
                    <div>
                      <h3>Оркестратор</h3>
                      <p className="settingsHint">Модель, которая строит план действий перед выполнением задачи.</p>
                    </div>
                    <div className="modelProviderForm">
                      <label>
                        <span>Провайдер</span>
                        <select
                          value={orchestratorRoute.provider}
                          onChange={(event) => {
                            const provider = event.target.value;
                            onUpdateModelDraft(orchestratorRouteIndex, {
                              provider,
                              base_url: provider === "literouter" ? "https://api.literouter.com/v1" : "https://api.openai.com/v1",
                              model: provider === "literouter" ? "deepseek:free" : "gpt-4o-mini",
                              billing_mode: provider === "literouter" ? "free" : "paid",
                            });
                          }}
                        >
                          <option value="literouter">LiteRouter</option>
                          <option value="openai-compatible">OpenAI-compatible</option>
                        </select>
                      </label>
                      <label>
                        <span>Модель</span>
                        <select
                          value={orchestratorRoute.model}
                          onChange={(event) => onUpdateModelDraft(orchestratorRouteIndex, { model: event.target.value })}
                        >
                          {[orchestratorRoute.model, ...orchestratorModels]
                            .filter((model, index, models) => model && models.indexOf(model) === index)
                            .map((model) => (
                              <option key={model} value={model}>
                                {model}
                              </option>
                            ))}
                        </select>
                      </label>
                    </div>
                  </section>
                ) : null}
              </div>
            ) : (
              <p>Загрузка конфигурации модели...</p>
            )}
          </section>
        ) : null}

        {settingsTab === "permissions" ? (
          <section className="settingsSection">
            <h3>Разрешения инструментов</h3>
            <div className="permissionList">
              {Object.entries(permissionSettings).map(([name, value]) => (
                <label key={name}>
                  <span>{name}</span>
                  <select
                    value={value.mode}
                    onChange={(event) => onUpdatePermission(name, event.target.value as PermissionMode)}
                  >
                    <option value="ask">спрашивать</option>
                    <option value="allow">разрешать</option>
                    <option value="deny">запрещать</option>
                  </select>
                </label>
              ))}
            </div>

            <h3>Временные path grants</h3>
            {permissionScopes?.path_grants?.length ? (
              <ul className="permissionAuditList">
                {permissionScopes.path_grants.map((grant) => (
                  <li key={`${grant.permission}-${grant.path}-${grant.session_id ?? "global"}`}>
                    <strong>{grant.permission}</strong> · {grant.mode} · <code>{grant.path}</code>
                    {grant.session_id ? ` · session ${grant.session_id.slice(0, 8)}` : " · global"}
                  </li>
                ))}
              </ul>
            ) : (
              <p className="settingsHint">Пока нет активных path grants.</p>
            )}

            <h3>Аудит approvals</h3>
            {permissionAudit.length ? (
              <ul className="permissionAuditList">
                {permissionAudit
                  .slice()
                  .reverse()
                  .slice(0, 20)
                  .map((entry) => (
                    <li key={`${entry.approval_id}-${entry.decision}-${entry.at_ms}`}>
                      <strong>{entry.decision}</strong> · {entry.tool_name} ·{" "}
                      <code>{entry.scope}</code>
                      {entry.remembered_path ? " · путь запомнен" : ""}
                    </li>
                  ))}
              </ul>
            ) : (
              <p className="settingsHint">Пока нет записей аудита.</p>
            )}
          </section>
        ) : null}

        {settingsTab === "mcp" ? (
          <section className="settingsSection">
            <div className="settingsHeaderRow">
              <div>
                <h3>MCP-серверы</h3>
                <p className="settingsHint">
                  Эти конечные точки редактируются в памяти и управляют интерфейсом MCP.
                </p>
              </div>
              <div className="toolbarActions">
                <button type="button" onClick={onAddMcpServer}>
                  Добавить сервер
                </button>
                <button type="button" onClick={onSaveMcpServers} disabled={mcpServersSaving}>
                  {mcpServersSaving ? "Сохранение..." : "Сохранить серверы"}
                </button>
              </div>
            </div>
            {mcpServersError ? <p className="settingsError">{mcpServersError}</p> : null}
            {mcpServersNotice ? <p className="settingsHint">{mcpServersNotice}</p> : null}
            <div className="mcpServerList">
              {mcpServers.length === 0 ? (
                <div className="emptyState">
                  <strong>Пока нет MCP-серверов</strong>
                  <p>Добавь первый сервер, чтобы держать общие точки доступа под рукой для агентов.</p>
                </div>
              ) : null}
              {mcpServers.map((server, index) => (
                <article className="mcpServerCard" key={`${server.name || "server"}-${index}`}>
                  <div className="mcpServerRow">
                    <label>
                      <span>Название</span>
                      <input
                        value={server.name}
                        onChange={(event) => onUpdateMcpServer(index, { name: event.target.value })}
                        placeholder="docs"
                      />
                    </label>
                    <label>
                      <span>Ссылка</span>
                      <input
                        value={server.url}
                        onChange={(event) => onUpdateMcpServer(index, { url: event.target.value })}
                        placeholder="https://example.com/rpc"
                      />
                    </label>
                  </div>
                  <label className="mcpServerDescription">
                    <span>Описание</span>
                    <input
                      value={server.description ?? ""}
                      onChange={(event) => onUpdateMcpServer(index, { description: event.target.value })}
                      placeholder="Необязательная заметка"
                    />
                  </label>
                  <div className="mcpServerFooter">
                    <label className="toggleRow">
                      <input
                        type="checkbox"
                        checked={server.enabled}
                        onChange={(event) => onUpdateMcpServer(index, { enabled: event.target.checked })}
                      />
                      <span>Включён</span>
                    </label>
                    <button type="button" onClick={() => onRemoveMcpServer(index)}>
                      Удалить
                    </button>
                  </div>
                </article>
              ))}
            </div>
          </section>
        ) : null}

        {settingsTab === "tools" ? (
          <section className="settingsSection">
            <h3>Каталог инструментов</h3>
            {toolCatalogError ? <p className="settingsError">{toolCatalogError}</p> : null}
            <div className="toolCatalog">
              {toolCatalog.map((tool) => (
                <article className="toolCard" key={tool.name}>
                  <strong>{tool.name}</strong>
                  <p>{tool.description}</p>
                  <small>{tool.permissions.join(", ") || "нет разрешений"}</small>
                  <span>Таймаут {tool.timeout_ms} мс</span>
                </article>
              ))}
            </div>
          </section>
        ) : null}

        {settingsTab === "worker" ? <WorkerSettingsSection /> : null}

        {settingsTab === "metrics" ? <MetricsSettingsSection /> : null}

        {settingsTab === "archive" ? (
          <section className="settingsSection">
            <h3>Архивированные чаты</h3>
            <p className="settingsHint">
              Архивация скрывает чат из левого бара. Здесь его можно восстановить или удалить окончательно.
            </p>
            <div className="archivedChatList">
              {archivedChats.length === 0 ? (
                <div className="emptyState">
                  <strong>Архив пуст</strong>
                  <p>Архивированные чаты появятся здесь.</p>
                </div>
              ) : (
                archivedChats.map((chat, index) => (
                  <article className="archivedChatItem" key={chat.session_id}>
                    <div>
                      <strong>{formatSessionTitle(chat, index)}</strong>
                      <span>{formatSessionPreview(chat)}</span>
                    </div>
                    <div className="archivedChatActions">
                      <button
                        type="button"
                        className="archivedChatRestoreButton"
                        onClick={() => onRestoreSession(chat)}
                        disabled={deletingSessionId === chat.session_id}
                        aria-label={`Восстановить чат ${formatSessionTitle(chat, index)}`}
                      >
                        {deletingSessionId === chat.session_id ? "..." : "Восстановить"}
                      </button>
                      <button
                        type="button"
                        className="archivedChatDeleteButton"
                        onClick={() => onDeleteSession(chat)}
                        disabled={deletingSessionId === chat.session_id}
                        aria-label={`Удалить чат ${formatSessionTitle(chat, index)} навсегда`}
                      >
                        {deletingSessionId === chat.session_id ? "..." : "Удалить навсегда"}
                      </button>
                    </div>
                  </article>
                ))
              )}
            </div>
          </section>
        ) : null}
      </div>
    </div>
  );
}
