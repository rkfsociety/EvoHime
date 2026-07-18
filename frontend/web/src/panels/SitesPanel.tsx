import { useEffect, useMemo, useState } from "react";
import { sitesApi, type Site } from "../api";

export function SitesPanel({ siteSearch, onSiteSearchChange, workspacePath }: { siteSearch: string; onSiteSearchChange: (value: string) => void; workspacePath: string }) {
  const [sites, setSites] = useState<Site[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [description, setDescription] = useState("");
  const reload = () => { setLoading(true); void sitesApi.listSites(workspacePath).then(setSites).catch((e) => setError(String(e))).finally(() => setLoading(false)); };
  useEffect(() => { reload(); }, [workspacePath]);
  const visible = useMemo(() => { const q = siteSearch.trim().toLowerCase(); return q ? sites.filter((site) => `${site.name} ${site.slug} ${site.description}`.toLowerCase().includes(q)) : sites; }, [siteSearch, sites]);
  async function create() { setError(null); try { const site = await sitesApi.createSite(workspacePath, { name, slug, description, status: "draft" }); setSites((current) => [site, ...current]); setCreating(false); setName(""); setSlug(""); setDescription(""); } catch (e) { setError(String(e)); } }
  async function remove(id: string) { try { await sitesApi.deleteSite(workspacePath, id); setSites((current) => current.filter((site) => site.id !== id)); } catch (e) { setError(String(e)); } }
  async function publish(id: string) { setError(null); try { const site = await sitesApi.publishSite(workspacePath, id); setSites((current) => current.map((item) => item.id === site.id ? site : item)); } catch (e) { setError(String(e)); } }
  function openPreview(id: string) { window.open(sitesApi.previewUrl(workspacePath, id), "_blank", "noopener,noreferrer"); }
 return <div className="sitesPage"><section className="sitesHero"><div><h3>Сайты</h3><p>Управляйте сайтами выбранного workspace.</p></div></section><div className="sitesSearchRow"><label className="sitesSearch"><span className="sitesSearchIcon" aria-hidden="true">⌕</span><input value={siteSearch} onChange={(e) => onSiteSearchChange(e.target.value)} placeholder="Поиск сайтов" aria-label="Поиск сайтов" /></label><button type="button" className="sitesCreateButton" onClick={() => setCreating(true)}>Создать сайт</button></div>{error && <p role="alert">{error}</p>}{creating && <form className="sitesCreateForm" onSubmit={(e) => { e.preventDefault(); void create(); }}><input required maxLength={120} value={name} onChange={(e) => setName(e.target.value)} placeholder="Название" aria-label="Название сайта" /><input required pattern="[a-z0-9-]+" maxLength={80} value={slug} onChange={(e) => setSlug(e.target.value)} placeholder="slug" aria-label="Slug сайта" /><textarea maxLength={4000} value={description} onChange={(e) => setDescription(e.target.value)} placeholder="Описание" aria-label="Описание сайта" /><button type="submit">Сохранить</button><button type="button" onClick={() => setCreating(false)}>Отмена</button></form>}<div className="sitesBody">{loading ? <p>Загрузка…</p> : visible.length === 0 ? <div className="sitesEmptyState"><div className="sitesEmptyIcon" aria-hidden="true">□</div><strong>Сайтов пока нет</strong></div> : <div className="sitesGrid">{visible.map((site) => <article className="siteCard" key={site.id}><strong>{site.name}</strong><span>{site.slug}</span><p>{site.description || "Без описания"}</p><small>{site.status}</small><div><button type="button" onClick={() => openPreview(site.id)}>Предпросмотр</button><button type="button" disabled={site.status === "published"} onClick={() => void publish(site.id)}>Опубликовать</button><button type="button" onClick={() => void remove(site.id)}>Удалить</button></div></article>)}</div>}</div></div>;
}
