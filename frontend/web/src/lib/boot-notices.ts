export type BootNotice = {
  id: string;
  message: string;
};

let bootNoticeCounter = 0;

export function formatBootError(label: string, error: unknown) {
  const detail = error instanceof Error ? error.message : String(error);
  return `${label}: ${detail}`;
}

export function createBootNotice(message: string): BootNotice {
  bootNoticeCounter += 1;
  return {
    id: `boot-${bootNoticeCounter}-${Date.now()}`,
    message,
  };
}

export function appendBootNotice(current: BootNotice[], message: string) {
  if (current.some((item) => item.message === message)) {
    return current;
  }
  return [...current, createBootNotice(message)];
}
