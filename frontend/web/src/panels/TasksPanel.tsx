import type { ChatSessionSummary } from "../types";
import {
  formatSessionPreview,
  formatSessionTimestamp,
  formatSessionTitle,
} from "../lib/format";

type TasksPanelProps = {
  chatSessions: ChatSessionSummary[];
  activeSessionId: string | null;
  onNewChat: () => void;
  onOpenSession: (chat: ChatSessionSummary) => void;
};

export function TasksPanel({
  chatSessions,
  activeSessionId,
  onNewChat,
  onOpenSession,
}: TasksPanelProps) {
  return (
    <div className="standaloneChatsPanel">
      <div className="panelToolbar">
        <div>
          <strong>Чаты без проектов</strong>
          <span>Личные задачи и разговоры, не привязанные к проекту</span>
        </div>
        <button type="button" onClick={onNewChat}>Новый чат</button>
      </div>
      {chatSessions.length > 0 ? (
        <div className="standaloneChatList">
          {chatSessions.map((chat, index) => (
            <button
              key={chat.session_id}
              type="button"
              className={chat.session_id === activeSessionId ? "standaloneChatItem active" : "standaloneChatItem"}
              onClick={() => onOpenSession(chat)}
            >
              <span className="standaloneChatIcon">⊕</span>
              <span className="standaloneChatDetails">
                <strong>{formatSessionTitle(chat, index)}</strong>
                <span>{formatSessionPreview(chat)}</span>
              </span>
              <time dateTime={chat.last_message_at ?? chat.created_at}>
                {formatSessionTimestamp(chat.last_message_at ?? chat.created_at)}
              </time>
            </button>
          ))}
        </div>
      ) : (
        <div className="emptyPanelState">
          <strong>Пока нет чатов без проекта</strong>
          <span>Создай новый чат, и он появится здесь.</span>
        </div>
      )}
    </div>
  );
}
