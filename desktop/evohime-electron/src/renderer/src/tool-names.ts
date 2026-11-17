/**
 * Human-readable names for Core tools.
 *
 * The transcript is read by a person, not by an operator: `filesystem_read`
 * says nothing to someone watching the agent work. An unknown identifier falls
 * through unchanged, so a newly added tool shows up as itself rather than
 * disappearing behind a wrong label.
 *
 * Transcripts recorded before the ids were renamed still carry the dotted
 * spelling, so the lookup normalizes the separator instead of losing the label
 * for history.
 */

const TOOL_LABELS: Record<string, string> = {
  agent_run: 'Запускаю подзадачу',
  analysis_run: 'Считаю во встроенном вычислителе',
  browser_extract: 'Извлекаю данные со страницы',
  browser_open: 'Открываю страницу',
  browser_session_click: 'Нажимаю на странице',
  browser_session_close: 'Закрываю вкладку',
  browser_session_navigate: 'Перехожу по адресу',
  browser_session_read: 'Читаю страницу',
  browser_session_screenshot: 'Делаю снимок страницы',
  browser_session_type: 'Ввожу текст на странице',
  filesystem_list: 'Смотрю содержимое папки',
  filesystem_patch: 'Правлю файл',
  filesystem_read: 'Читаю файл',
  filesystem_search: 'Ищу по файлам',
  filesystem_write: 'Записываю файл',
  git_commit: 'Создаю коммит',
  git_diff: 'Смотрю изменения',
  git_pull: 'Забираю изменения из репозитория',
  git_push: 'Отправляю коммиты',
  git_status: 'Проверяю состояние репозитория',
  http_fetch: 'Запрашиваю данные по сети',
  mcp_call: 'Обращаюсь к внешнему сервису',
  memory_search: 'Ищу в памяти',
  shell_execute: 'Выполняю команду'
}

const SHELL_COMMAND_PREFIX = 'shell_execute: '

/**
 * Normalizes only the tool id, never the command text after `: `.
 *
 * A transcript may contain `shell.execute: pwsh -File check.ps1`; rewriting
 * every dot would corrupt the file name inside the command.
 */
function canonicalToolName(tool: string): string {
  const separator = tool.indexOf(': ')
  if (separator === -1) return tool.replace(/\./g, '_')
  const id = tool.slice(0, separator).replace(/\./g, '_')
  return `${id}${tool.slice(separator)}`
}

export function toolLabel(tool: string): string {
  const canonical = canonicalToolName(tool)
  if (canonical.startsWith(SHELL_COMMAND_PREFIX)) {
    return `Выполняю команду: ${canonical.slice(SHELL_COMMAND_PREFIX.length)}`
  }
  return TOOL_LABELS[canonical] ?? tool
}
