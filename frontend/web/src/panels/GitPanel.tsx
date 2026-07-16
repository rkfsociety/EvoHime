import type { GitAction } from "../types";

type GitPanelProps = {
  branchLabel: string;
  changedCount: number;
  gitDiffPath: string | null;
  gitDiffPathInput: string;
  gitCommitMessage: string;
  gitRemote: string;
  gitBranch: string;
  gitAction: GitAction | null;
  gitActionNotice: string | null;
  gitStatus: string;
  gitDiff: string;
  selectedFilePath: string | null;
  onDiffPathInputChange: (value: string) => void;
  onCommitMessageChange: (value: string) => void;
  onRemoteChange: (value: string) => void;
  onBranchChange: (value: string) => void;
  onRefresh: (path?: string) => void;
  onUseSelectedFile: () => void;
  onGitAction: (action: GitAction) => void;
};

export function GitPanel({
  branchLabel,
  changedCount,
  gitDiffPath,
  gitDiffPathInput,
  gitCommitMessage,
  gitRemote,
  gitBranch,
  gitAction,
  gitActionNotice,
  gitStatus,
  gitDiff,
  selectedFilePath,
  onDiffPathInputChange,
  onCommitMessageChange,
  onRemoteChange,
  onBranchChange,
  onRefresh,
  onUseSelectedFile,
  onGitAction,
}: GitPanelProps) {
  return (
    <div className="gitPanel">
      <div className="panelToolbar">
        <div>
          <strong>Состояние репозитория</strong>
          <span>
            {branchLabel}
            {changedCount ? ` • изменено: ${changedCount}` : " • чисто"}
          </span>
        </div>
        <div className="toolbarActions">
          <button type="button" onClick={() => onRefresh(gitDiffPathInput || undefined)}>
            Обновить Гит
          </button>
        </div>
      </div>
      <div className="gitControls">
        <label>
          <span>Путь diff</span>
          <input
            value={gitDiffPathInput}
            onChange={(event) => onDiffPathInputChange(event.target.value)}
            placeholder="Корень репозитория или путь к файлу"
          />
        </label>
        <div className="gitControlButtons">
          <button type="button" onClick={() => onRefresh(gitDiffPathInput || undefined)}>
            Загрузить diff
          </button>
          <button type="button" onClick={onUseSelectedFile} disabled={!selectedFilePath}>
            Использовать выбранный файл
          </button>
        </div>
        <label>
          <span>Сообщение коммита</span>
          <input
            value={gitCommitMessage}
            onChange={(event) => onCommitMessageChange(event.target.value)}
            placeholder="Опиши изменение"
          />
        </label>
        <div className="gitRemoteFields">
          <label>
            <span>Удалённый репозиторий</span>
            <input value={gitRemote} onChange={(event) => onRemoteChange(event.target.value)} />
          </label>
          <label>
            <span>Ветка</span>
            <input
              value={gitBranch}
              onChange={(event) => onBranchChange(event.target.value)}
              placeholder="Текущая"
            />
          </label>
        </div>
        <div className="gitControlButtons">
          <button
            type="button"
            onClick={() => onGitAction("commit")}
            disabled={!gitCommitMessage.trim() || Boolean(gitAction)}
          >
            {gitAction === "commit" ? "Коммитим..." : "Коммит"}
          </button>
          <button type="button" onClick={() => onGitAction("pull")} disabled={Boolean(gitAction)}>
            {gitAction === "pull" ? "Забираем..." : "Забрать"}
          </button>
          <button type="button" onClick={() => onGitAction("push")} disabled={Boolean(gitAction)}>
            {gitAction === "push" ? "Отправляем..." : "Отправить"}
          </button>
        </div>
        {gitActionNotice ? <p className="gitActionNotice">{gitActionNotice}</p> : null}
      </div>
      <div className="gitSummary">
        <h3>Статус</h3>
        <pre>{gitStatus}</pre>
      </div>
      <div className="gitSummary">
        <h3>Изменения{gitDiffPath ? ` · ${gitDiffPath}` : ""}</h3>
        <pre className="gitDiffViewer">
          {(gitDiff || "Нет изменений").split("\n").map((line, index) => (
            <span
              className={
                line.startsWith("+") && !line.startsWith("+++")
                  ? "diffAdded"
                  : line.startsWith("-") && !line.startsWith("---")
                    ? "diffRemoved"
                    : line.startsWith("@@")
                      ? "diffContext"
                      : ""
              }
              key={`${index}-${line}`}
            >
              {line || " "}
            </span>
          ))}
        </pre>
      </div>
    </div>
  );
}
