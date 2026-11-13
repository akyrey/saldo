import { describe, expect, it } from "vitest";
import {
  EMPTY_BUDGET_FORM,
  monthLabel,
  monthOf,
  monthRange,
  parseMonth,
  progress,
  shiftMonth,
  toBudgetRequest,
} from "./budget-model";

describe("months", () => {
  it("parses, shifts and spans months", () => {
    expect(monthOf("2026-11-13")).toBe("2026-11");
    expect(parseMonth("2026-11")).toBe("2026-11");
    expect(parseMonth("2026-13")).toBeUndefined();
    expect(parseMonth(5)).toBeUndefined();
    expect(shiftMonth("2026-12", 1)).toBe("2027-01");
    expect(shiftMonth("2026-01", -1)).toBe("2025-12");
    expect(monthRange("2024-02")).toEqual({ from: "2024-02-01", to: "2024-02-29" });
    expect(monthRange("2026-11")).toEqual({ from: "2026-11-01", to: "2026-11-30" });
    expect(monthLabel("2026-11", "en-US")).toBe("November 2026");
  });
});

describe("progress", () => {
  it("measures spending exactly and caps the bar", () => {
    expect(progress("0.10", "0.30", 2)).toEqual({ percent: 33, overspent: false });
    expect(progress("300.00", "300.00", 2)).toEqual({ percent: 100, overspent: false });
    expect(progress("450.00", "300.00", 2)).toEqual({ percent: 100, overspent: true });
    expect(progress("0.00", "0.00", 2)).toEqual({ percent: 0, overspent: false });
  });
});

describe("toBudgetRequest", () => {
  it("trims and leaves out an empty automatic limit", () => {
    expect(toBudgetRequest({ ...EMPTY_BUDGET_FORM, name: " Food ", notes: " " })).toEqual({
      name: "Food",
      active: true,
      notes: null,
      auto_budget: null,
    });
  });

  it("sends an automatic limit with a normalised amount", () => {
    const form = { ...EMPTY_BUDGET_FORM, name: "Fun", autoType: "rollover", autoAmount: "12,5" };
    expect(toBudgetRequest(form).auto_budget).toEqual({
      type: "rollover",
      period: "monthly",
      amount: "12.5",
    });
  });
});
