import type { ReactNode } from "react";

type FilesPanelProps = {
  rootEntryCount: number;
  newFilePath: string;
  newFileContent: string;
  onNewFilePathChange: (value: string) => void;
  onNewFileContentChange: (value: string) => void;
  onRefreshTree: () => void;
  onCreateFile: () => void;
  fileTree: ReactNode;
};

export function FilesPanel({
  rootEntryCount,
  newFilePath,
  newFileContent,
  onNewFilePathChange,
  onNewFileContentChange,
  onRefreshTree,
  onCreateFile,
  fileTree,
}: FilesPanelProps) {
  return (
    <div className="filesPanel">
      <div className="panelToolbar">
        <div>
          <strong>Дерево рабочего пространства</strong>
          <span>{rootEntryCount} элементов в корне</span>
        </div>
        <button type="button" onClick={onRefreshTree}>Обновить дерево</button>
      </div>
      <div className="createFileForm">
        <input
          value={newFilePath}
          onChange={(event) => onNewFilePathChange(event.target.value)}
          placeholder="путь/до/нового-файла.ts"
          aria-label="Путь нового файла"
        />
        <input
          value={newFileContent}
          onChange={(event) => onNewFileContentChange(event.target.value)}
          placeholder="Начальное содержимое"
          aria-label="Содержимое нового файла"
        />
        <button type="button" onClick={onCreateFile} disabled={!newFilePath.trim()}>
          Создать файл
        </button>
      </div>
      <div className="fileTree">{fileTree}</div>
    </div>
  );
}
