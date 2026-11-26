import { describe, expect, it } from "vitest";
import {
  budgetMonthChoices,
  cardProgress,
  hasDifference,
  monthLabel,
  payingAccounts,
  settlementStateLabel,
  shiftMonth,
  shortMonth,
} from "./credit-card-model";

describe("credit card model", () => {
  it("measures the spent share in basis points without floats for amounts", () => {
    expect(cardProgress("125.00", "500.00", 2)).toEqual({ percent: 25, overspent: false });
    expect(cardProgress("0.01", "3.00", 2)).toEqual({ percent: 0.33, overspent: false });
    expect(cardProgress("600.00", "500.00", 2)).toEqual({ percent: 100, overspent: true });
    expect(cardProgress("-20.00", "500.00", 2)).toEqual({ percent: 0, overspent: false });
    expect(cardProgress("1.00", "0.00", 2)).toEqual({ percent: 100, overspent: true });
    expect(cardProgress("0.00", "0.00", 2)).toEqual({ percent: 0, overspent: false });
  });

  it("names settlement states and months", () => {
    expect(settlementStateLabel("synced")).toBe("updates until then");
    expect(settlementStateLabel("frozen")).toBe("booked");
    expect(settlementStateLabel("detached")).toBe("managed by hand");
    expect(settlementStateLabel("other")).toBe("other");
    expect(monthLabel("2026-11")).toBe("November 2026");
    expect(shortMonth("2026-10")).toBe("Oct");
    expect(monthLabel("nope")).toBe("nope");
  });

  it("shifts months across years", () => {
    expect(shiftMonth("2026-12", 1)).toBe("2027-01");
    expect(shiftMonth("2027-01", -1)).toBe("2026-12");
    expect(shiftMonth("2026-05", 0)).toBe("2026-05");
    expect(budgetMonthChoices("2026-11", 3)).toEqual(["2026-11", "2026-12", "2027-01"]);
  });

  it("shows a difference only when it is not zero", () => {
    expect(hasDifference(null, 2)).toBe(false);
    expect(hasDifference("0.00", 2)).toBe(false);
    expect(hasDifference("-5.00", 2)).toBe(true);
  });
});

describe("paying accounts", () => {
  const account = (id: string, extra: Record<string, unknown> = {}) =>
    ({ id, name: id, type: "asset", active: true, currency: "EUR", ...extra }) as never;
  it("offers active asset accounts in the card's currency, not the card", () => {
    const card = account("card", { asset_role: "credit_card" });
    const all = [
      card,
      account("checking"),
      account("old", { active: false }),
      account("usd", { currency: "USD" }),
      account("shop", { type: "expense" }),
    ];
    expect(payingAccounts(all, card).map((a) => a.id)).toEqual(["checking"]);
  });
});
