import { afterEach, describe, expect, it, vi } from "vitest";
import { FetchEventSource, parseEvents } from "./event-stream";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("parseEvents", () => {
  it("splits complete messages and keeps the rest for the next chunk", () => {
    const { events, rest } = parseEvents(
      ': keep-alive\n\nid: 7\ndata: {"seq":7}\n\nid: 8\ndata: {"se',
    );
    expect(events).toEqual([{ id: "7", data: '{"seq":7}' }]);
    expect(rest).toBe('id: 8\ndata: {"se');
  });

  it("joins multi-line data and accepts CRLF", () => {
    const { events } = parseEvents("data: a\r\ndata: b\r\n\r\n");
    expect(events).toEqual([{ id: undefined, data: "a\nb" }]);
  });
});

function streamOf(chunks: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  });
}

describe("FetchEventSource", () => {
  it("delivers messages and resumes after the last ID when the stream ends", async () => {
    vi.useFakeTimers();
    const requests: Request[] = [];
    const responses = [
      new Response(streamOf(['id: 1\ndata: {"seq":1}\n\n'])),
      new Response(streamOf(['id: 2\ndata: {"seq":2}\n\n'])),
    ];
    const send = vi.fn(async (request: Request) => {
      requests.push(request);
      return responses.shift() ?? new Promise<Response>(() => {});
    });
    const seen: string[] = [];

    const source = new FetchEventSource("/api/v1/events/stream", send);
    source.onmessage = (event) => seen.push(event.data);
    await vi.runAllTimersAsync();

    expect(seen).toEqual(['{"seq":1}', '{"seq":2}']);
    expect(requests[0]?.headers.get("Last-Event-ID")).toBeNull();
    expect(requests[1]?.headers.get("Last-Event-ID")).toBe("1");
    expect(requests[2]?.headers.get("Last-Event-ID")).toBe("2");
    source.close();
  });

  it("stops for good after close", async () => {
    vi.useFakeTimers();
    const send = vi.fn(async () => new Response(streamOf([])));
    const source = new FetchEventSource("/s", send);
    source.close();
    await vi.runAllTimersAsync();
    expect(send.mock.calls.length).toBeLessThanOrEqual(1);
  });
});
