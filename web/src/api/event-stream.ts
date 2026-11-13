// Server-Sent Events over `fetch`, for the desktop app: the browser's
// `EventSource` cannot send `Authorization: Bearer`. It behaves like
// `EventSource` for what Saldo uses: `onmessage`, `close`, and reconnecting
// with `Last-Event-ID` so no event is missed.
import type { EventSourceLike } from "@/features/live/live-events";

export interface SseEvent {
  id: string | undefined;
  data: string;
}

/** Wait this long before reconnecting after the stream ends or fails. */
const RETRY_MS = 3_000;

/** Complete messages in `text`, plus the unfinished tail. Comments are skipped. */
export function parseEvents(text: string): { events: SseEvent[]; rest: string } {
  const blocks = text.replace(/\r\n/g, "\n").split("\n\n");
  const rest = blocks.pop() ?? "";
  const events: SseEvent[] = [];
  for (const block of blocks) {
    let id: string | undefined;
    const data: string[] = [];
    for (const line of block.split("\n")) {
      if (line.startsWith("id:")) id = line.slice(3).trim();
      else if (line.startsWith("data:")) data.push(line.slice(5).replace(/^ /, ""));
    }
    if (data.length > 0) events.push({ id, data: data.join("\n") });
  }
  return { events, rest };
}

export class FetchEventSource implements EventSourceLike {
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  private lastId: string | undefined;
  private closed = false;
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null;

  constructor(
    private readonly url: string,
    private readonly send: (request: Request) => Promise<Response>,
  ) {
    void this.run();
  }

  close(): void {
    this.closed = true;
    void this.reader?.cancel().catch(() => {});
  }

  private async run(): Promise<void> {
    while (!this.closed) {
      try {
        await this.readOnce();
      } catch {
        // Network error or server restart: try again after a pause.
      }
      if (this.closed) return;
      await new Promise((resolve) => setTimeout(resolve, RETRY_MS));
    }
  }

  private async readOnce(): Promise<void> {
    const headers = new Headers({ Accept: "text/event-stream" });
    if (this.lastId) headers.set("Last-Event-ID", this.lastId);
    // Resolved against the page, as the browser would (the desktop app then
    // rewrites it to the server's address).
    const url = new URL(this.url, globalThis.location.href);
    const response = await this.send(new Request(url, { headers }));
    if (!response.ok || !response.body) return;
    const reader = response.body.getReader();
    this.reader = reader;
    // Closed while the request was on its way: drop the answer.
    if (this.closed) return void reader.cancel().catch(() => {});
    const decoder = new TextDecoder();
    let buffer = "";
    for (;;) {
      const { done, value } = await reader.read();
      if (done) return;
      const parsed = parseEvents(buffer + decoder.decode(value, { stream: true }));
      buffer = parsed.rest;
      for (const event of parsed.events) this.deliver(event);
    }
  }

  private deliver(event: SseEvent): void {
    if (event.id) this.lastId = event.id;
    this.onmessage?.(new MessageEvent("message", { data: event.data, lastEventId: event.id }));
  }
}
