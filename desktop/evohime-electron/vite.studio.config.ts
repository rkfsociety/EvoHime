import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import react from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'

/**
 * Dev-only конфигурация стенда Hive Studio (`npm run studio`).
 *
 * Открывается в обычном браузере, без Electron и Core. Служит две задачи:
 * 1) отдаёт dev-страницу `src/renderer/hive-studio.html` с горячей
 *    перезагрузкой сцены города Улья;
 * 2) проксирует наблюдательный control-pipe `evohime-hive-daemon`
 *    (`get_city_map`) на HTTP-эндпоинт `/hive-studio-api/city-map.json`,
 *    чтобы стенд мог показывать текущий город живого Улья в реальном
 *    времени (только чтение; работает, пока запущена EvoHime).
 *
 * Эта страница не входит в production-бандл: `electron.vite.config.ts`
 * перечисляет только `index.html` в rollup input.
 */
const here = fileURLToPath(new URL('.', import.meta.url))

function hiveDaemonObservationProxy(): Plugin {
  return {
    name: 'evohime-hive-studio-daemon-proxy',
    configureServer(server) {
      const respondCityMap = async (
        _request: unknown,
        response: { setHeader: (name: string, value: string) => void; end: (body: string) => void }
      ): Promise<void> => {
        // Dev-middleware выполняется в Node (конфигурация Vite), поэтому
        // импорт node:net в hive-daemon-client безопасен.
        const { fetchHiveCityMap } = await import('./src/main/hive-daemon-client')
        const outcome = await fetchHiveCityMap()
        response.setHeader('content-type', 'application/json; charset=utf-8')
        response.setHeader('cache-control', 'no-store')
        response.end(JSON.stringify(outcome))
      }
      server.middlewares.use('/hive-studio-api/city-map.json', (request, response) => {
        void respondCityMap(request, response)
      })
    }
  }
}

export default defineConfig({
  root: resolve(here, 'src/renderer'),
  publicDir: resolve(here, 'resources'),
  plugins: [react(), hiveDaemonObservationProxy()],
  resolve: {
    alias: { '@shared': resolve(here, 'src/shared') }
  },
  server: {
    port: 5178,
    // Без strictPort: если прошлый запуск стенда ещё держит порт, сервер
    // молча перейдёт на 5179/5180 и напечатает фактический адрес.
    strictPort: false
  }
})
