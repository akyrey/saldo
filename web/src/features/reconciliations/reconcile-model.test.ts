import { describe, expect, it } from "vitest";
import { parseReconcileSearch, tally } from "./reconcile-model";

const SPLITS = [
  { id: "a", group_id: "g1", date: "2026-11-01", description: "Salary", amount: "2000.00" },
  { id: "b", group_id: "g2", date: "2026-11-03", description: "Shop", amount: "-50.00" },
  { id: "c", group_id: "g3", date: "2026-11-05", description: "Shop", amount: "-30.00" },
];

describe("tally", () => {
  it("adds the selected splits to the cleared balance and compares with the statement", () => {
    const result = tally("100.00", SPLITS, new Set(["a", "b"]), "2045,5", 2);
    expect(result).toEqual({ cleared: "2050.00", difference: "-4.50", balanced: false });
    expect(tally("100.00", SPLITS, new Set(["a", "b", "c"]), "2020", 2)).toEqual({
      cleared: "2020.00",
      difference: "0.00",
      balanced: true,
    });
  });

  it("has no difference while the statement balance is not a number", () => {
    expect(tally("0.00", SPLITS, new Set(), "", 2)).toEqual({
      cleared: "0.00",
      difference: null,
      balanced: false,
    });
  });
});

describe("parseReconcileSearch", () => {
  it("keeps a date and a balance from the URL", () => {
    expect(parseReconcileSearch({ date: "2026-11-10", balance: "12.50" })).toEqual({
      date: "2026-11-10",
      balance: "12.50",
    });
    expect(parseReconcileSearch({ date: "soon", balance: 2050 })).toEqual({
      date: undefined,
      balance: "2050",
    });
    expect(parseReconcileSearch({ balance: { x: 1 } }).balance).toBeUndefined();
  });
});
