import type { ChatRecord, CoreEvent } from '@shared/api'

export function filterEventsForChat(
  events: readonly CoreEvent[],
  chat: Pick<ChatRecord, 'taskIds'> | null
): readonly CoreEvent[] {
  if (!chat) return []
  const taskIds = new Set(chat.taskIds)
  return events.filter((event) => event.taskId.length > 0 && taskIds.has(event.taskId))
}
