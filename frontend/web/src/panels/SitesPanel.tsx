export function SitesPanel({
  siteSearch,
  onSiteSearchChange,
}: {
  siteSearch: string;
  onSiteSearchChange: (value: string) => void;
}) {
  return (
    <div className="sitesPage">
      <section className="sitesHero">
        <div>
          <h3>Сайты</h3>
          <p>Превратите свои идеи в готовые сайты.</p>
        </div>
      </section>

      <div className="sitesSearchRow">
        <label className="sitesSearch">
          <span className="sitesSearchIcon" aria-hidden="true">
            ⌕
          </span>
          <input
            value={siteSearch}
            onChange={(event) => onSiteSearchChange(event.target.value)}
            placeholder="Поиск сайтов"
            aria-label="Поиск сайтов"
          />
        </label>
      </div>

      <div className="sitesBody">
        <div className="sitesEmptyState">
          <div className="sitesEmptyIcon" aria-hidden="true">
            ▢
          </div>
          <strong>Сайтов пока нет</strong>
          <button type="button" className="sitesCreateButton">
            Создать новый сайт
          </button>
        </div>
      </div>
    </div>
  );
}
