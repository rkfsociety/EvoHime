export type RecordedError = {
  message: string;
  stack?: string;
  source: "window.onerror" | "unhandledrejection";
  at: string;
};

const MAX_RECORDED = 20;
const recentErrors: RecordedError[] = [];

function record(entry: RecordedError) {
  recentErrors.push(entry);
  if (recentErrors.length > MAX_RECORDED) {
    recentErrors.shift();
  }
  console.error(`[${entry.source}] ${entry.message}`, entry.stack ?? "");
}

export function getRecentErrors(): RecordedError[] {
  return [...recentErrors];
}

let installed = false;

/** Catches errors outside the React render tree (event handlers, async code, rejected promises). */
export function installGlobalErrorHandlers() {
  if (installed) {
    return;
  }
  installed = true;

  window.addEventListener("error", (event) => {
    record({
      message: event.message,
      stack: event.error instanceof Error ? event.error.stack : undefined,
      source: "window.onerror",
      at: new Date().toISOString(),
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    record({
      message: reason instanceof Error ? reason.message : String(reason),
      stack: reason instanceof Error ? reason.stack : undefined,
      source: "unhandledrejection",
      at: new Date().toISOString(),
    });
  });
}
