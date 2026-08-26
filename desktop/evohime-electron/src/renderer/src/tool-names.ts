/**
 * Human-readable names for Core tools.
 *
 * The transcript is read by a person, not by an operator: `filesystem.read`
 * says nothing to someone watching the agent work. An unknown identifier falls
 * through unchanged, so a newly added tool shows up as itself rather than
 * disappearing behind a wrong label.
 */

const TOOL_LABELS: Record<string, string> = {
  'agent.run': 'Запускаю подзадачу',
  'browser.extract': 'Извлекаю данные со страницы',
  'browser.open': 'Открываю страницу',
  'browser.session.click': 'Нажимаю на странице',
  'browser.session.close': 'Закрываю вкладку',
  'browser.session.navigate': 'Перехожу по адресу',
  'browser.session.read': 'Читаю страницу',
  'browser.session.screenshot': 'Делаю снимок страницы',
  'browser.session.type': 'Ввожу текст на странице',
  'filesystem.list': 'Смотрю содержимое папки',
  'filesystem.patch': 'Правлю файл',
  'filesystem.read': 'Читаю файл',
  'filesystem.search': 'Ищу по файлам',
  'filesystem.write': 'Записываю файл',
  'git.commit': 'Создаю коммит',
  'git.diff': 'Смотрю изменения',
  'git.pull': 'Забираю изменения из репозитория',
  'git.push': 'Отправляю коммиты',
  'git.status': 'Проверяю состояние репозитория',
  'http.fetch': 'Запрашиваю данные по сети',
  'mcp.call': 'Обращаюсь к внешнему сервису',
  'memory.search': 'Ищу в памяти',
  'shell.execute': 'Выполняю команду'
}

export function toolLabel(tool: string): string {
  if (tool.startsWith('shell.execute: ')) {
    return `Выполняю команду: ${tool.slice('shell.execute: '.length)}`
  }
  return TOOL_LABELS[tool] ?? tool
}
