import { describe, expect, it } from "vitest";
import {
  breakdownRows,
  budgetRows,
  cashFlowRows,
  defaultSpan,
  intervalLabel,
  isReportKind,
  netWorthRows,
  parseReportSearch,
  plotValue,
} from "./report-model";

describe("report search", () => {
  it("keeps only valid dates and intervals", () => {
    expect(parseReportSearch({ from: "2026-01-01", to: "nope", interval: "week" })).toEqual({
      from: "2026-01-01",
      to: undefined,
      interval: "week",
    });
    expect(parseReportSearch({ interval: "hour" }).interval).toBeUndefined();
    expect(isReportKind("cash-flow")).toBe(true);
    expect(isReportKind("pie")).toBe(false);
  });

  it("defaults to the twelve months up to today", () => {
    expect(defaultSpan("2026-11-13")).toEqual({ from: "2025-12-01", to: "2026-11-13" });
    expect(defaultSpan("2026-01-05")).toEqual({ from: "2025-02-01", to: "2026-01-05" });
  });
});

describe("chart rows", () => {
  it("plots decimal strings without parsing floats from text", () => {
    expect(plotValue("12.34", 2)).toBe(12.34);
    expect(plotValue("-5", 0)).toBe(-5);
    expect(plotValue("x", 2)).toBe(0);
    expect(intervalLabel("2026-11-01", "month")).toBe("2026-11");
    expect(intervalLabel("2026-01-01", "year")).toBe("2026");
    expect(intervalLabel("2026-11-09", "week")).toBe("2026-11-09");
  });

  it("builds rows for every report", () => {
    const flow = {
      currency: "EUR",
      income: "10.00",
      expenses: "4.00",
      net: "6.00",
      intervals: [
        { start: "2026-11-01", end: "2026-11-30", income: "10.00", expenses: "4.00", net: "6.00" },
      ],
    };
    expect(cashFlowRows(flow, "month", 2)).toEqual([
      { label: "2026-11", first: 10, second: 4, firstText: "10.00", secondText: "4.00" },
    ]);
    const worth = {
      currency: "EUR",
      opening: "1.00",
      intervals: [{ start: "2026-11-01", end: "2026-11-30", value: "3.00", change: "2.00" }],
    };
    expect(netWorthRows(worth, "month", 2)[0]?.first).toBe(3);
    const items = {
      currency: "EUR",
      items: [
        { id: "c", name: "Food", income: "0.00", expenses: "9.00", intervals: [] },
        { id: null, name: null, income: "5.00", expenses: "0.00", intervals: [] },
      ],
    };
    expect(breakdownRows(items, 2).map((r) => r.label)).toEqual(["Food"]);
    const budgets = {
      currency: "EUR",
      items: [{ budget_id: null, name: null, budgeted: "0.00", spent: "1.00", intervals: [] }],
    };
    expect(budgetRows(budgets, 2)[0]).toMatchObject({ label: "Without budget", second: 1 });
  });
});
