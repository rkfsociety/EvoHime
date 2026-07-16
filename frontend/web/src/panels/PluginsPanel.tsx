export function PluginsPanel() {
  return (
    <div className="pluginsPage">
      <section className="pluginsHero">
        <div>
          <h3>Плагины</h3>
          <p>Каталог плагинов пока не подключён.</p>
        </div>
      </section>

      <div className="pluginsBody">
        <section className="pluginsCatalog pluginsCatalogEmpty">
          <div className="pluginsSectionHeader">
            <h4>Каталог</h4>
          </div>
          <div className="pluginsInstalledEmpty">
            <strong>Каталог ещё не настроен</strong>
            <p>Здесь появятся реальные плагины после подключения источника каталога.</p>
          </div>
        </section>
        <section className="pluginsInstalled">
          <div className="pluginsSectionHeader">
            <h4>Установленные</h4>
          </div>
          <div className="pluginsInstalledList">
            <div className="pluginsInstalledEmpty">
              <strong>Пока нет плагинов</strong>
              <p>Установленные плагины появятся здесь.</p>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
