import { describe, expect, it } from "vitest";
import { formatEventTypes, parseEventTypes } from "./event-types";

describe("event types", () => {
  it("splits on commas, spaces and new lines and drops duplicates", () => {
    expect(parseEventTypes(" transaction.*, bill.overdue\naccount.created  bill.overdue,")).toEqual(
      ["transaction.*", "bill.overdue", "account.created"],
    );
    expect(parseEventTypes("   ")).toEqual([]);
  });

  it("formats a list for the form", () => {
    expect(formatEventTypes(["*", "bill.*"])).toBe("*, bill.*");
  });
});
