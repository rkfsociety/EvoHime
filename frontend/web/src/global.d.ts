export {};

declare global {
  interface Window {
    /**
     * Токен сессии Launcher'а (раздел XV плана Installer/Launcher/Update).
     * Инъецируется static file server'ом Launcher'а прямо в index.html при
     * каждой раздаче — никогда не попадает в собранный JS-бандл на диске.
     * Остаётся плейсхолдером вне Launcher-управляемого запуска (обычный
     * `npm run dev`/прод-сборка без Launcher'а).
     */
    __EVOHIME_TOKEN__?: string;
  }
}
