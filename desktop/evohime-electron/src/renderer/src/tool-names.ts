/**
 * Human-readable names for Core tools.
 *
 * The transcript is read by a person, not by an operator: `filesystem_read`
 * says nothing to someone watching the agent work. An unknown identifier falls
 * through unchanged, so a newly added tool shows up as itself rather than
 * disappearing behind a wrong label.
 */

const TOOL_LABELS: Record<string, string> = {
  'agent_run': 'Запускаю подзадачу',
  'analysis_run': 'Считаю во встроенном вычислителе',
  'browser_extract': 'Извлекаю данные со страницы',
  'browser_open': 'Открываю страницу',
  'browser_session_click': 'Нажимаю на странице',
  'browser_session_close': 'Закрываю вкладку',
  'browser_session_navigate': 'Перехожу по адресу',
  'browser_session_read': 'Читаю страницу',
  'browser_session_screenshot': 'Делаю снимок страницы',
  'browser_session_type': 'Ввожу текст на странице',
  'filesystem_list': 'Смотрю содержимое папки',
  'filesystem_patch': 'Правлю файл',
  'filesystem_read': 'Читаю файл',
  'filesystem_search': 'Ищу по файлам',
  'filesystem_write': 'Записываю файл',
  'git_commit': 'Создаю коммит',
  'git_diff': 'Смотрю изменения',
  'git_pull': 'Забираю изменения из репозитория',
  'git_push': 'Отправляю коммиты',
  'git_status': 'Проверяю состояние репозитория',
  'http_fetch': 'Запрашиваю данные по сети',
  'mcp_call': 'Обращаюсь к внешнему сервису',
  'memory_search': 'Ищу в памяти',
  'shell_execute': 'Выполняю команду'
}

export function toolLabel(tool: string): string {
  if (tool.startsWith('shell_execute: ')) {
    return `Выполняю команду: ${tool.slice('shell_execute: '.length)}`
  }
  return TOOL_LABELS[tool] ?? tool
}
