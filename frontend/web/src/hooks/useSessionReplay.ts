import { useCallback } from "react";
import type { HistoryItem, ServerEvent } from "../protocol";
import { apiClient } from "../api/client";

interface PaginatedHistoryResponse {
  items: HistoryItem[];
  next_cursor?: string;
  prev_cursor?: string;
  has_more: boolean;
  total_available: number;
}

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
          const response = await apiClient.get(
            `/api/sessions/${sessionId}/history?${new URLSearchParams({
              limit: "100",
              order: "asc",
              ...(currentCursor ? { cursor: currentCursor } : {}),
            }).toString()}`
          );

          const page: PaginatedHistoryResponse = await response.json();

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
