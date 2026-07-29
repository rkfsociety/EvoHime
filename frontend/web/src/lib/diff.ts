export type DiffLineClass = "diffAdded" | "diffRemoved" | "diffContext" | "";

export function classifyDiffLine(line: string): DiffLineClass {
  if (line.startsWith("+") && !line.startsWith("+++")) {
    return "diffAdded";
  }
  if (line.startsWith("-") && !line.startsWith("---")) {
    return "diffRemoved";
  }
  if (line.startsWith("@@")) {
    return "diffContext";
  }
  return "";
}
