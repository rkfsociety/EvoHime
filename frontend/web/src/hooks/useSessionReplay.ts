import { useCallback } from "react";
import type { ServerEvent } from "../protocol";
import { sessionsApi } from "../api";

export function useSessionReplay() {
  const backfillHistory = useCallback(
    async (
      sessionId: string,
      cursor: string | undefined,
      onEvent: (event: ServerEvent) => void,
      maxPages = 10
    ): Promise<{ lastSequence: number; lastCursor?: string }> => {
      let currentCursor = cursor;
      let lastSequence = 0;
      let pageCount = 0;

      while (pageCount < maxPages) {
        try {
          const page = await sessionsApi.getSessionHistoryPaginated(
            sessionId,
            100,
            currentCursor,
            "asc"
          );

          for (const item of page.items) {
            onEvent(item.event);
            lastSequence = Math.max(lastSequence, item.sequence);
          }

          if (!page.has_more || !page.next_cursor) {
            break;
          }

          currentCursor = page.next_cursor;
          pageCount += 1;
        } catch (error) {
          console.error("Error backfilling history:", error);
          break;
        }
      }

      return { lastSequence, lastCursor: currentCursor };
    },
    []
  );

  return { backfillHistory };
}
