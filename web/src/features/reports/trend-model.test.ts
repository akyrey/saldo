import { describe, expect, it } from "vitest";
import { formatBp, MONTH_NAMES, parseTrendSearch, yearRows } from "./trend-model";

describe("formatBp", () => {
  it("shows basis points as a percentage with two decimals", () => {
    expect(formatBp(2500)).toBe("25.00%");
    expect(formatBp(-3333)).toBe("-33.33%");
    expect(formatBp(7)).toBe("0.07%");
    expect(formatBp(-5)).toBe("-0.05%");
    expect(formatBp(null)).toBe("—");
    expect(formatBp(undefined)).toBe("—");
  });
});

describe("parseTrendSearch", () => {
  it("keeps valid trend parameters only", () => {
    expect(
      parseTrendSearch({ years: 4, metric: "savings", category_id: "none", months: "6" }),
    ).toEqual({ years: 4, metric: "savings", category_id: "none", months: 6 });
    expect(parseTrendSearch({ years: 9, metric: "profit", category_id: "x", months: 1 })).toEqual(
      {},
    );
  });
});

describe("yearRows", () => {
  it("lays months out as rows with one column per year", () => {
    const years = [
      { year: 2025, months: ["1.00", ...Array(11).fill("0.00")] },
      { year: 2026, months: ["2.50", null, ...Array(10).fill(null)] },
    ];
    const rows = yearRows(years, 2);
    expect(rows).toHaveLength(12);
    expect(rows[0]).toEqual({ label: MONTH_NAMES[0], 2025: 1, 2026: 2.5 });
    expect(rows[1]).toEqual({ label: MONTH_NAMES[1], 2025: 0, 2026: null });
  });
});
