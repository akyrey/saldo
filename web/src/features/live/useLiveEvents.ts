import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { startLiveEvents } from "./live-events";

const STREAM_URL = `${import.meta.env.VITE_SALDO_API_URL ?? ""}/api/v1/events/stream`;

/** Keeps the cache fresh while the app is open (one stream per tab). */
export function useLiveEvents(): void {
  const queryClient = useQueryClient();
  useEffect(
    () =>
      startLiveEvents(
        queryClient,
        STREAM_URL,
        typeof EventSource === "undefined" ? undefined : EventSource,
      ),
    [queryClient],
  );
}
