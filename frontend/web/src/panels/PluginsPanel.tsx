import { useCallback, useEffect, useMemo, useState } from "react";
import {
  installPlugin,
  listCatalog,
  listPluginSkills,
  listPlugins,
  uninstallPlugin,
  updatePlugin,
  type CatalogGroup,
  type CatalogPlugin,
  type InstalledPlugin,
  type PluginCatalogResponse,
  type PluginSkillSummary,
} from "../api/plugins";

type PluginAction = "install" | "update" | "uninstall";

function trustLabel(level: string | undefined) {
  switch (level) {
    case "official":
      return "Официальный";
    case "curated":
      return "Кураторский";
    case "community":
      return "Сообщество";
    default:
      return "Не проверен";
  }
}

function pluginNeedsUpdate(plugin: CatalogPlugin) {
  if (!plugin.installed || !plugin.installable) {
    return false;
  }
  if (!plugin.installed_version || !plugin.version) {
    return true;
  }
  return plugin.installed_version !== plugin.version;
}

export function PluginsPanel() {
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [catalog, setCatalog] = useState<PluginCatalogResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<{ name: string; action: PluginAction } | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [catalogQuery, setCatalogQuery] = useState("");
  const [activeGroup, setActiveGroup] = useState("all");
  const [expandedSkillsPluginId, setExpandedSkillsPluginId] = useState<string | null>(null);
  const [skillsByPlugin, setSkillsByPlugin] = useState<Record<string, PluginSkillSummary[]>>({});
  const [skillsLoadingId, setSkillsLoadingId] = useState<string | null>(null);
  const [skillsError, setSkillsError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [installed, remote] = await Promise.all([listPlugins(), listCatalog()]);
      setPlugins(installed);
      setCatalog(remote);
      setActiveGroup((current) => {
        if (current === "all") {
          return current;
        }
        return remote.groups?.some((group) => group.id === current) ? current : "all";
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось загрузить плагины");
      setPlugins([]);
      setCatalog(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function runPluginAction(name: string, action: PluginAction) {
    if (pendingAction) {
      return;
    }
    setPendingAction({ name, action });
    setActionError(null);
    try {
      if (action === "install") {
        await installPlugin(name);
      } else if (action === "update") {
        await updatePlugin(name);
      } else {
        await uninstallPlugin(name);
        if (expandedSkillsPluginId === name) {
          setExpandedSkillsPluginId(null);
        }
        setSkillsByPlugin((current) => {
          const next = { ...current };
          delete next[name];
          return next;
        });
      }
      await refresh();
    } catch (err: unknown) {
      setActionError(err instanceof Error ? err.message : "Операция с плагином не удалась");
    } finally {
      setPendingAction(null);
    }
  }

  async function toggleSkills(plugin: InstalledPlugin) {
    if (expandedSkillsPluginId === plugin.id) {
      setExpandedSkillsPluginId(null);
      setSkillsError(null);
      return;
    }

    setExpandedSkillsPluginId(plugin.id);
    setSkillsError(null);
    if (skillsByPlugin[plugin.id]) {
      return;
    }

    setSkillsLoadingId(plugin.id);
    try {
      const skills = await listPluginSkills(plugin.id);
      setSkillsByPlugin((current) => ({ ...current, [plugin.id]: skills }));
    } catch (err: unknown) {
      setSkillsError(err instanceof Error ? err.message : "Не удалось загрузить skills");
    } finally {
      setSkillsLoadingId(null);
    }
  }

  const catalogPlugins = catalog?.plugins ?? [];
  const groups: CatalogGroup[] = catalog?.groups ?? [];

  const filteredCatalog = useMemo(() => {
    const query = catalogQuery.trim().toLowerCase();
    return catalogPlugins.filter((plugin) => {
      if (activeGroup !== "all" && plugin.group !== activeGroup) {
        return false;
      }
      if (!query) {
        return true;
      }
      const haystack =
        `${plugin.name} ${plugin.description} ${plugin.version} ${plugin.category} ${plugin.group}`.toLowerCase();
      return haystack.includes(query);
    });
  }, [activeGroup, catalogPlugins, catalogQuery]);

  const groupedSections = useMemo(() => {
    if (activeGroup !== "all") {
      return [
        {
          id: activeGroup,
          label: groups.find((group) => group.id === activeGroup)?.label ?? activeGroup,
          plugins: filteredCatalog,
        },
      ];
    }
    const byGroup = new Map<string, CatalogPlugin[]>();
    for (const plugin of filteredCatalog) {
      const bucket = byGroup.get(plugin.group) ?? [];
      bucket.push(plugin);
      byGroup.set(plugin.group, bucket);
    }
    const ordered = groups
      .map((group) => ({
        id: group.id,
        label: group.label,
        plugins: byGroup.get(group.id) ?? [],
      }))
      .filter((section) => section.plugins.length > 0);
    const known = new Set(ordered.map((section) => section.id));
    for (const [id, pluginsInGroup] of byGroup) {
      if (!known.has(id) && pluginsInGroup.length > 0) {
        ordered.push({ id, label: id, plugins: pluginsInGroup });
      }
    }
    return ordered;
  }, [activeGroup, filteredCatalog, groups]);

  const sourceCount = catalog?.sources?.length ?? (catalog ? 1 : 0);
  const heroText = loading
    ? "Загружаю каталог и установленные плагины…"
    : error
      ? error
      : catalog
        ? `${catalog.marketplace}: ${catalogPlugins.length} плагинов, ${groups.length} категорий, ${sourceCount} OSS-источников. Установлено: ${plugins.length}.`
        : "Каталог плагинов пока не подключён.";

  function renderCatalogActions(plugin: CatalogPlugin) {
    const busy = pendingAction?.name === plugin.name;
    if (!plugin.installed) {
      return (
        <button
          type="button"
          className="pluginInstallButton"
          disabled={!plugin.installable || Boolean(pendingAction)}
          onClick={() => void runPluginAction(plugin.name, "install")}
        >
          {busy && pendingAction?.action === "install" ? "Ставлю…" : "Установить"}
        </button>
      );
    }

    return (
      <div className="pluginCardActions">
        {plugin.installable && pluginNeedsUpdate(plugin) ? (
          <button
            type="button"
            className="pluginUpdateButton"
            disabled={Boolean(pendingAction)}
            onClick={() => void runPluginAction(plugin.name, "update")}
          >
            {busy && pendingAction?.action === "update" ? "Обновляю…" : "Обновить"}
          </button>
        ) : null}
        <button
          type="button"
          className="pluginUninstallButton"
          disabled={Boolean(pendingAction)}
          onClick={() => void runPluginAction(plugin.name, "uninstall")}
        >
          {busy && pendingAction?.action === "uninstall" ? "Удаляю…" : "Удалить"}
        </button>
      </div>
    );
  }

  function renderInstalledActions(plugin: InstalledPlugin) {
    const catalogEntry = catalogPlugins.find(
      (entry) => entry.name === plugin.id || entry.name === plugin.name,
    );
    const busy = pendingAction?.name === plugin.id || pendingAction?.name === plugin.name;
    const actionName = catalogEntry?.name ?? plugin.id;

    return (
      <div className="pluginCardActions">
        <button
          type="button"
          className="pluginSkillsButton"
          disabled={Boolean(pendingAction)}
          onClick={() => void toggleSkills(plugin)}
          aria-expanded={expandedSkillsPluginId === plugin.id}
        >
          {expandedSkillsPluginId === plugin.id ? "Скрыть skills" : "Skills"}
        </button>
        {catalogEntry?.installable && (catalogEntry ? pluginNeedsUpdate(catalogEntry) : false) ? (
          <button
            type="button"
            className="pluginUpdateButton"
            disabled={Boolean(pendingAction)}
            onClick={() => void runPluginAction(actionName, "update")}
          >
            {busy && pendingAction?.action === "update" ? "Обновляю…" : "Обновить"}
          </button>
        ) : null}
        <button
          type="button"
          className="pluginUninstallButton"
          disabled={Boolean(pendingAction)}
          onClick={() => void runPluginAction(plugin.id, "uninstall")}
        >
          {busy && pendingAction?.action === "uninstall" ? "Удаляю…" : "Удалить"}
        </button>
      </div>
    );
  }

  return (
    <div className="pluginsPage">
      <section className="pluginsHero">
        <div>
          <h3>Плагины</h3>
          <p>{heroText}</p>
          {actionError ? <p className="pluginsInstallError">{actionError}</p> : null}
        </div>
      </section>

      <div className="pluginsBody">
        <section className={catalogPlugins.length ? "pluginsCatalog" : "pluginsCatalog pluginsCatalogEmpty"}>
          <div className="pluginsSectionHeader">
            <h4>Каталог</h4>
          </div>

          {!loading && catalogPlugins.length > 0 ? (
            <>
              <div className="pluginsSwitcherRow">
                <div className="pluginsTabs" role="tablist" aria-label="Категории плагинов">
                  <button
                    type="button"
                    className={`pluginsTab${activeGroup === "all" ? " active" : ""}`}
                    onClick={() => setActiveGroup("all")}
                  >
                    Все ({catalogPlugins.length})
                  </button>
                  {groups.map((group) => (
                    <button
                      key={group.id}
                      type="button"
                      className={`pluginsTab${activeGroup === group.id ? " active" : ""}`}
                      onClick={() => setActiveGroup(group.id)}
                    >
                      {group.label} ({group.count})
                    </button>
                  ))}
                </div>
              </div>
              <label className="pluginsSearch">
                <span>Поиск по каталогу</span>
                <input
                  value={catalogQuery}
                  onChange={(event) => setCatalogQuery(event.target.value)}
                  placeholder="имя, описание или категория"
                  aria-label="Поиск по каталогу плагинов"
                />
              </label>
            </>
          ) : null}

          {loading ? (
            <div className="pluginsInstalledEmpty">
              <strong>Загрузка каталога…</strong>
              <p>Тяну несколько OSS marketplace.</p>
            </div>
          ) : catalogPlugins.length === 0 ? (
            <div className="pluginsInstalledEmpty">
              <strong>Каталог пуст</strong>
              <p>Источники не вернули плагины.</p>
            </div>
          ) : filteredCatalog.length === 0 ? (
            <div className="pluginsInstalledEmpty">
              <strong>Ничего не найдено</strong>
              <p>Смени категорию или поисковый запрос.</p>
            </div>
          ) : (
            <div className="pluginsGroupedList">
              {groupedSections.map((section) => (
                <section key={section.id} className="pluginsGroupSection">
                  <div className="pluginsGroupHeader">
                    <h5>{section.label}</h5>
                    <span>{section.plugins.length}</span>
                  </div>
                  <div className="pluginsGrid">
                    {section.plugins.map((plugin) => (
                      <article key={plugin.name} className="pluginCard">
                        <div className="pluginIcon" aria-hidden="true">
                          ◌
                        </div>
                        <div className="pluginBody">
                          <div className="pluginTopRow">
                            <strong>{plugin.name}</strong>
                            <small>v{plugin.version || "?"}</small>
                          </div>
                          <p>{plugin.description || "Без описания."}</p>
                          <div className="pluginMetaRow">
                            <span className="pluginCategoryChip">{plugin.category || plugin.group}</span>
                            <span
                              className={`pluginTrustChip pluginTrust-${plugin.trust?.level ?? "unverified"}`}
                              title={plugin.trust?.reasons?.join("; ") ?? ""}
                            >
                              {trustLabel(plugin.trust?.level)} · {plugin.trust?.score ?? 0}
                            </span>
                            <small>
                              {plugin.installed
                                ? `Установлен${plugin.installed_version ? ` · v${plugin.installed_version}` : ""}`
                                : plugin.installable
                                  ? "Доступен для установки"
                                  : "Источник не поддерживается"}
                            </small>
                          </div>
                        </div>
                        {renderCatalogActions(plugin)}
                      </article>
                    ))}
                  </div>
                </section>
              ))}
            </div>
          )}
        </section>

        <section className="pluginsInstalled">
          <div className="pluginsSectionHeader">
            <h4>Установленные</h4>
          </div>
          <div className="pluginsInstalledList">
            {loading ? (
              <div className="pluginsInstalledEmpty">
                <strong>Загрузка…</strong>
                <p>Смотрю `.evohime/plugins`.</p>
              </div>
            ) : plugins.length === 0 ? (
              <div className="pluginsInstalledEmpty">
                <strong>Пока нет плагинов</strong>
                <p>Установленные плагины появятся здесь.</p>
              </div>
            ) : (
              <div className="pluginsGrid pluginsGridCompact">
                {plugins.map((plugin) => (
                    <article key={plugin.id} className="pluginCard pluginCardCompact pluginCardInstalled">
                      <div className="pluginIcon" aria-hidden="true">
                        ◌
                      </div>
                      <div className="pluginBody">
                        <div className="pluginTopRow">
                          <strong>{plugin.display_name || plugin.name}</strong>
                          <small>v{plugin.version || "?"}</small>
                        </div>
                        <p>{plugin.description || "Без описания."}</p>
                        <small>
                          {plugin.skills_count} skills · {plugin.path}
                        </small>
                        {expandedSkillsPluginId === plugin.id ? (
                          <div className="pluginSkillBrowser" aria-label={`Skills плагина ${plugin.display_name || plugin.name}`}>
                            {skillsLoadingId === plugin.id ? (
                              <p className="pluginSkillBrowserMeta">Загружаю skills…</p>
                            ) : skillsError ? (
                              <p className="pluginsInstallError">{skillsError}</p>
                            ) : (skillsByPlugin[plugin.id] ?? []).length === 0 ? (
                              <p className="pluginSkillBrowserMeta">Skills не найдены.</p>
                            ) : (
                              <ul className="pluginSkillList">
                                {(skillsByPlugin[plugin.id] ?? []).map((skill) => (
                                  <li key={skill.name} className="pluginSkillItem">
                                    <strong>{skill.name}</strong>
                                    {skill.preview ? <pre>{skill.preview}</pre> : <p className="pluginSkillBrowserMeta">SKILL.md пуст.</p>}
                                  </li>
                                ))}
                              </ul>
                            )}
                          </div>
                        ) : null}
                      </div>
                      {renderInstalledActions(plugin)}
                    </article>
                ))}
              </div>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
