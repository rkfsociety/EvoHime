import type { ChatLine } from "../types";

let chatLineCounter = 0;

export function createChatLine(
  line: Omit<ChatLine, "id"> & { id?: string },
): ChatLine {
  if (line.id) {
    return line as ChatLine;
  }

  chatLineCounter += 1;
  const suffix =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `${Date.now()}-${chatLineCounter}`;

  return {
    ...line,
    id: `chat-${chatLineCounter}-${suffix}`,
  };
}
