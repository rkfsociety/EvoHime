import { useState } from "react";

export function CopyButton({ value }: { value: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
      setState("copied");
    } catch {
      setState("failed");
    }
  }

  return (
    <button type="button" className="copyCorrelationButton" onClick={() => void copy()} title="Скопировать correlation id">
      {state === "copied" ? "Скопировано" : state === "failed" ? "Ошибка копирования" : "Копировать id"}
    </button>
  );
}
