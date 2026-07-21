import { useCallback, useEffect, useState } from "react";
import { sitesApi, type Site, type SiteStatusFilter } from "../api";

type SitesPanelProps = {
  siteSearch: string;
  onSiteSearchChange: (value: string) => void;
  workspacePath: string;
};

export function SitesPanel({ siteSearch, onSiteSearchChange, workspacePath }: SitesPanelProps) {
  const [sites, setSites] = useState<Site[]>([]);
  const [statusFilter, setStatusFilter] = useState<SiteStatusFilter>("all");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [description, setDescription] = useState("");

  const reload = useCallback(() => {
    if (!workspacePath) {
      setSites([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    void sitesApi
      .listSites(workspacePath, { q: siteSearch, status: statusFilter })
      .then((items) => {
        setSites(items);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [workspacePath, siteSearch, statusFilter]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      reload();
    }, siteSearch.trim() ? 250 : 0);
    return () => window.clearTimeout(timer);
  }, [reload, siteSearch]);

  async function create() {
    setError(null);
    try {
      const site = await sitesApi.createSite(workspacePath, {
        name,
        slug,
        description,
        status: "draft",
      });
      if (
        statusFilter === "all" ||
        statusFilter === site.status
      ) {
        const q = siteSearch.trim().toLowerCase();
        const haystack = `${site.name} ${site.slug} ${site.description}`.toLowerCase();
        if (!q || haystack.includes(q)) {
          setSites((current) => [site, ...current]);
        }
      }
      setCreating(false);
      setName("");
      setSlug("");
      setDescription("");
    } catch (e) {
      setError(String(e));
    }
  }

  async function remove(id: string) {
    try {
      await sitesApi.deleteSite(workspacePath, id);
      setSites((current) => current.filter((site) => site.id !== id));
    } catch (e) {
      setError(String(e));
    }
  }

  async function publish(id: string) {
    setError(null);
    try {
      const site = await sitesApi.publishSite(workspacePath, id);
      if (statusFilter === "draft") {
        setSites((current) => current.filter((item) => item.id !== site.id));
        return;
      }
      setSites((current) => current.map((item) => (item.id === site.id ? site : item)));
    } catch (e) {
      setError(String(e));
    }
  }

  function openPreview(id: string) {
    window.open(sitesApi.previewUrl(workspacePath, id), "_blank", "noopener,noreferrer");
  }

  const hasFilters = siteSearch.trim().length > 0 || statusFilter !== "all";
  const emptyTitle = hasFilters ? "Ничего не найдено" : "Сайтов пока нет";
  const emptyHint = hasFilters
    ? "Попробуй другой запрос или сбрось фильтр статуса."
    : "Создай первый сайт для этого workspace.";

  return (
    <div className="sitesPage">
      <section className="sitesHero">
        <div>
          <h3>Сайты</h3>
          <p>Управляйте сайтами выбранного workspace.</p>
        </div>
        <div className="sitesMeta">
          <strong>{loading ? "…" : `${sites.length}`}</strong>
          <span>{hasFilters ? "совпадений" : "сайтов"}</span>
        </div>
      </section>

      <div className="sitesSearchRow">
        <label className="sitesSearch">
          <span className="sitesSearchIcon" aria-hidden="true">
            ⌕
          </span>
          <input
            value={siteSearch}
            onChange={(e) => onSiteSearchChange(e.target.value)}
            placeholder="Поиск по названию, slug или описанию"
            aria-label="Поиск сайтов"
          />
        </label>
        <button type="button" className="sitesCreateButton" onClick={() => setCreating(true)}>
          Создать сайт
        </button>
      </div>

      <div className="sitesTabs" role="tablist" aria-label="Фильтр статуса сайтов">
        {(
          [
            ["all", "Все"],
            ["draft", "Черновики"],
            ["published", "Опубликованные"],
          ] as const
        ).map(([value, label]) => (
          <button
            key={value}
            type="button"
            role="tab"
            aria-selected={statusFilter === value}
            className={statusFilter === value ? "sitesTab active" : "sitesTab"}
            onClick={() => setStatusFilter(value)}
          >
            {label}
          </button>
        ))}
      </div>

      {error ? <p role="alert">{error}</p> : null}

      {creating ? (
        <form
          className="sitesCreateForm"
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
        >
          <input
            required
            maxLength={120}
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Название"
            aria-label="Название сайта"
          />
          <input
            required
            pattern="[a-z0-9-]+"
            maxLength={80}
            value={slug}
            onChange={(e) => setSlug(e.target.value)}
            placeholder="slug"
            aria-label="Slug сайта"
          />
          <textarea
            maxLength={4000}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Описание"
            aria-label="Описание сайта"
          />
          <button type="submit">Сохранить</button>
          <button type="button" onClick={() => setCreating(false)}>
            Отмена
          </button>
        </form>
      ) : null}

      <div className="sitesBody">
        {loading ? (
          <p>Загрузка…</p>
        ) : sites.length === 0 ? (
          <div className="sitesEmptyState">
            <div className="sitesEmptyIcon" aria-hidden="true">
              □
            </div>
            <strong>{emptyTitle}</strong>
            <p>{emptyHint}</p>
          </div>
        ) : (
          <div className="sitesGrid">
            {sites.map((site) => (
              <article className="siteCard" key={site.id}>
                <strong>{site.name}</strong>
                <span>{site.slug}</span>
                <p>{site.description || "Без описания"}</p>
                <small>{site.status === "published" ? "Опубликован" : "Черновик"}</small>
                <div className="siteCardActions">
                  <button type="button" onClick={() => openPreview(site.id)}>
                    Предпросмотр
                  </button>
                  <button
                    type="button"
                    disabled={site.status === "published"}
                    onClick={() => void publish(site.id)}
                  >
                    Опубликовать
                  </button>
                  <button type="button" onClick={() => void remove(site.id)}>
                    Удалить
                  </button>
                </div>
              </article>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
