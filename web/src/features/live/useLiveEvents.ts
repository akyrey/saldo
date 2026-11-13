import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { sendRequest } from "@/api/client";
import { FetchEventSource } from "@/api/event-stream";
import { platform } from "@/platform";
import { alertBill } from "./bill-alerts";
import { type EventNotice, type EventSourceFactory, startLiveEvents } from "./live-events";

const STREAM_PATH = "/api/v1/events/stream";
const STREAM_URL = `${import.meta.env.VITE_SALDO_API_URL ?? ""}${STREAM_PATH}`;

/** The desktop app's stream: `fetch` with the bearer token (see event-stream.ts). */
class DesktopEventSource extends FetchEventSource {
  constructor(url: string) {
    super(url, sendRequest);
  }
}

/** Which `EventSource` to use here, if any. */
function sourceFactory(): EventSourceFactory | undefined {
  if (platform().kind === "desktop") return DesktopEventSource;
  return typeof EventSource === "undefined" ? undefined : EventSource;
}

/** Keeps the cache fresh while the app is open (one stream per tab). */
export function useLiveEvents(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    const desktop = platform().kind === "desktop";
    const onNotice = (notice: EventNotice) => {
      // A failed lookup only loses one notification; the stream carries on.
      alertBill(notice).catch(() => {});
    };
    return startLiveEvents(
      queryClient,
      desktop ? STREAM_PATH : STREAM_URL,
      sourceFactory(),
      onNotice,
    );
  }, [queryClient]);
}
