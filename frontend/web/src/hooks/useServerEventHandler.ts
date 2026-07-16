import { useCallback, useRef } from "react";
import type { ServerEvent } from "../protocol";
import { applyServerEvent, type ServerEventHandlerContext } from "./applyServerEvent";

/** Keeps a stable event handler while always reading the latest context snapshot. */
export function useServerEventHandler(ctx: ServerEventHandlerContext) {
  const ctxRef = useRef(ctx);
  ctxRef.current = ctx;

  return useCallback((event: ServerEvent) => {
    applyServerEvent(event, ctxRef.current);
  }, []);
}
