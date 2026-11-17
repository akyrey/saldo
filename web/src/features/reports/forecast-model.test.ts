import { describe, expect, it } from "vitest";
import {
  addMonths,
  forecastRows,
  forecastSpan,
  groupByDate,
  isBelowZero,
  parseForecastSearch,
} from "./forecast-model";

describe("forecast search", () => {
  it("keeps only a known horizon and an account id", () => {
    expect(parseForecastSearch({ months: 6, account: "a1" })).toEqual({ months: 6, account: "a1" });
    expect(parseForecastSearch({ months: "12" })).toEqual({ months: 12, account: undefined });
    expect(parseForecastSearch({ months: 5, account: 3 })).toEqual({
      months: undefined,
      account: undefined,
    });
  });
});

describe("forecastSpan", () => {
  it("adds months, clamping the day, and picks an interval by length", () => {
    expect(addMonths("2026-11-30", 3)).toBe("2027-02-28");
    expect(addMonths("2026-01-31", 1)).toBe("2026-02-28");
    expect(forecastSpan("2026-11-17", 1)).toEqual({ to: "2026-12-17", interval: "day" });
    expect(forecastSpan("2026-11-17", 6)).toEqual({ to: "2027-05-17", interval: "week" });
    expect(forecastSpan("2026-11-17", 24)).toEqual({ to: "2028-11-17", interval: "month" });
  });
});

describe("forecast rows and items", () => {
  const currency = {
    currency: "EUR",
    start_balance: "100.00",
    points: [
      { date: "2026-11-30", balance: "75.50" },
      { date: "2026-12-31", balance: "-20.00" },
    ],
    lowest: { date: "2026-12-05", balance: "-20.00" },
    items: [
      { date: "2026-12-01", source: "bill", source_id: "b", title: "Rent", amount: "-80.00" },
      { date: "2026-12-01", source: "recurrence", source_id: "r", title: "Gym", amount: "-15.50" },
      { date: "2026-12-05", source: "transaction", source_id: "t", title: "TV", amount: "-4.50" },
    ],
  };

  it("plots the balances and keeps their exact text", () => {
    expect(forecastRows(currency, 2)).toEqual([
      { label: "2026-11-30", first: 75.5, second: 0, firstText: "75.50", secondText: "" },
      { label: "2026-12-31", first: -20, second: 0, firstText: "-20.00", secondText: "" },
    ]);
    expect(isBelowZero(currency.lowest.balance)).toBe(true);
    expect(isBelowZero("0.00")).toBe(false);
  });

  it("groups items by date in order", () => {
    const groups = groupByDate(currency.items);
    expect(groups.map((g) => [g.date, g.items.length])).toEqual([
      ["2026-12-01", 2],
      ["2026-12-05", 1],
    ]);
  });
});
