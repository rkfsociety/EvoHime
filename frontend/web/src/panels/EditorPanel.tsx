import Editor from "@monaco-editor/react";
import type { MutableRefObject } from "react";
import { formatFileSize } from "../lib/paths";
import type { SaveState } from "../types";

type EditorPanelProps = {
  selectedFilePath: string | null;
  selectedFileContent: string;
  selectedFileOriginal: string;
  selectedFileLanguage: string;
  selectedFileLoading: boolean;
  selectedFileNotice: string | null;
  saveState: SaveState;
  saveFileRef: MutableRefObject<() => void>;
  onContentChange: (value: string) => void;
  onReload: () => void;
  onSave: () => void;
};

export function EditorPanel({
  selectedFilePath,
  selectedFileContent,
  selectedFileOriginal,
  selectedFileLanguage,
  selectedFileLoading,
  selectedFileNotice,
  saveState,
  saveFileRef,
  onContentChange,
  onReload,
  onSave,
}: EditorPanelProps) {
  return (
    <div className="editorPanel">
      <div className="panelToolbar">
        <div>
          <strong>{selectedFilePath ?? "Файл не выбран"}</strong>
          <span>
            {selectedFileLoading
              ? "Загрузка..."
              : saveState === "saving"
                ? "Сохранение..."
                : saveState === "saved"
                  ? "Сохранено"
                  : selectedFilePath && selectedFileContent !== selectedFileOriginal
                    ? "Есть несохранённые изменения"
                    : "Готово"}
          </span>
        </div>
        <div className="toolbarActions">
          <button
            type="button"
            onClick={onReload}
            disabled={!selectedFilePath || selectedFileLoading}
          >
            Перезагрузить
          </button>
          <button
            type="button"
            onClick={onSave}
            disabled={!selectedFilePath || selectedFileContent === selectedFileOriginal}
          >
            Сохранить
          </button>
        </div>
      </div>
      {selectedFileNotice ? <div className="editorNotice">{selectedFileNotice}</div> : null}
      {selectedFilePath ? (
        <div className="editorMeta">
          <span>Язык: {selectedFileLanguage}</span>
          <span>Размер: {formatFileSize(selectedFileContent.length)}</span>
          <span>{selectedFileContent === selectedFileOriginal ? "Чисто" : "Есть изменения"}</span>
        </div>
      ) : null}
      {selectedFilePath ? (
        <Editor
          height="100%"
          theme="vs-dark"
          language={selectedFileLanguage}
          value={selectedFileContent}
          onChange={(value) => onContentChange(value ?? "")}
          onMount={(editor, monaco) => {
            editor.addAction({
              id: "evohime-save-file",
              label: "Сохранить файл",
              keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
              run: () => saveFileRef.current(),
            });
          }}
          options={{
            minimap: { enabled: false },
            fontSize: 14,
            automaticLayout: true,
            scrollBeyondLastLine: false,
            wordWrap: "on",
          }}
        />
      ) : (
        <div className="emptyPanelState">
          <strong>Выберите файл</strong>
          <span>Выбери файл во вкладке «Файлы», чтобы открыть его в редакторе Monaco.</span>
        </div>
      )}
    </div>
  );
}
