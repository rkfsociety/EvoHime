import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

export type TerminalEntry = { stream: "stdout" | "stderr" | "status"; text: string };

export function TerminalPanel({ entries }: { entries: TerminalEntry[] }) {
  const host = useRef<HTMLDivElement | null>(null);
  const terminal = useRef<Terminal | null>(null);
  useEffect(() => {
    if (!host.current) return;
    const instance = new Terminal({ convertEol: true, disableStdin: true, cursorBlink: false, theme: { background: "#050816" } });
    instance.open(host.current);
    terminal.current = instance;
    return () => { instance.dispose(); terminal.current = null; };
  }, []);
  useEffect(() => {
    const instance = terminal.current;
    if (!instance) return;
    instance.clear();
    for (const entry of entries) instance.writeln(entry.stream === "stderr" ? `\x1b[31m${entry.text}\x1b[0m` : entry.text);
  }, [entries]);
  return <div className="terminalPanel" ref={host} aria-label="Terminal output" />;
}
