import { classifyDiffLine } from "../lib/diff";

type DiffViewerProps = {
  diff: string;
  emptyText?: string;
};

export function DiffViewer({ diff, emptyText = "Нет изменений" }: DiffViewerProps) {
  return (
    <pre className="diffViewer">
      {(diff || emptyText).split("\n").map((line, index) => (
        <span className={classifyDiffLine(line)} key={`${index}-${line}`}>
          {line || " "}
        </span>
      ))}
    </pre>
  );
}
