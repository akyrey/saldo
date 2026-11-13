import { describe, expect, it } from "vitest";
import { todayIso } from "./dates";

describe("todayIso", () => {
  it("formats the local date with padding", () => {
    expect(todayIso(new Date(2026, 0, 5, 23, 59))).toBe("2026-01-05");
  });
});
