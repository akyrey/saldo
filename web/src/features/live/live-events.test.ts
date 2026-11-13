import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { keysFor, parseNotice, startLiveEvents, type EventSourceLike } from "./live-events";

describe("keysFor", () => {
  it("maps known entities to the query keys they affect", () => {
    expect(keysFor("account")).toEqual([["accounts"], ["balances"], ["reports"]]);
    expect(keysFor("tag")).toEqual([["tags"], ["transactions"], ["reports"]]);
    expect(keysFor("household_member")).toEqual([["members"]]);
    expect(keysFor("household")).toEqual([["me"]]);
    expect(keysFor("webhook")).toEqual([["webhooks"]]);
    expect(keysFor("budget_limit")).toEqual([["budgets"], ["reports"]]);
    expect(keysFor("rule_group")).toEqual([["rules"]]);
    expect(keysFor("piggy_bank")).toEqual([["piggy-banks"]]);
    expect(keysFor("notification_channel")).toEqual([["notification-channels"]]);
  });

  it("refreshes what a new transaction changes elsewhere", () => {
    expect(keysFor("transaction")).toEqual(
      expect.arrayContaining([["budgets"], ["bills"], ["reports"]]),
    );
  });

  it("refreshes everything for unknown entities", () => {
    expect(keysFor("something_new")).toBe("all");
    expect(keysFor("import")).toBe("all");
  });
});

describe("parseNotice", () => {
  it("accepts the server's notice and rejects anything else", () => {
    const notice = { seq: 5, type: "tag.created", entity_type: "tag", entity_id: "t1" };
    expect(parseNotice(JSON.stringify(notice))).toEqual(notice);
    expect(parseNotice("not json")).toBeNull();
    expect(parseNotice(JSON.stringify({ seq: "5" }))).toBeNull();
  });
});

function lastSource(): FakeSource {
  if (!FakeSource.last) throw new Error("no EventSource was opened");
  return FakeSource.last;
}

class FakeSource implements EventSourceLike {
  static last: FakeSource | undefined;
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  closed = false;
  constructor(
    readonly url: string,
    readonly init?: EventSourceInit,
  ) {
    FakeSource.last = this;
  }
  close() {
    this.closed = true;
  }
  emit(entityType: string) {
    const data = JSON.stringify({
      seq: 1,
      type: `${entityType}.created`,
      entity_type: entityType,
      entity_id: "x",
    });
    this.onmessage?.(new MessageEvent("message", { data }));
  }
}

describe("startLiveEvents", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("batches notices into one round of invalidations", async () => {
    const client = new QueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries").mockResolvedValue();
    const stop = startLiveEvents(client, "/api/v1/events/stream", FakeSource);
    const source = lastSource();
    expect(source.url).toBe("/api/v1/events/stream");
    expect(source.init).toEqual({ withCredentials: true });

    source.emit("tag");
    source.emit("tag");
    source.emit("category");
    expect(invalidate).not.toHaveBeenCalled();
    await vi.runAllTimersAsync();
    const keys = invalidate.mock.calls.map(([filters]) => filters?.queryKey);
    expect(keys).toEqual([["tags"], ["transactions"], ["reports"], ["categories"]]);

    stop();
    expect(source.closed).toBe(true);
  });

  it("invalidates every query for an unknown entity", async () => {
    const client = new QueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries").mockResolvedValue();
    startLiveEvents(client, "/s", FakeSource);
    lastSource().emit("something_new");
    lastSource().emit("tag");
    await vi.runAllTimersAsync();
    expect(invalidate).toHaveBeenCalledOnce();
    expect(invalidate).toHaveBeenCalledWith();
  });

  it("does nothing where EventSource does not exist", () => {
    const stop = startLiveEvents(new QueryClient(), "/s", undefined);
    expect(() => stop()).not.toThrow();
  });
});
