import { describe, expect, it } from "vitest";
import { creatableType, sideTypes } from "./account-sides";

const asset = { type: "asset" };
const expense = { type: "expense" };
const revenue = { type: "revenue" };
const equity = { type: "equity", equity_role: "opening_balance" };

describe("sideTypes", () => {
  it("never offers expense accounts as a source or revenue accounts as a destination", () => {
    expect(sideTypes("source", undefined)).toEqual(["asset", "liability", "revenue", "equity"]);
    expect(sideTypes("destination", undefined)).toEqual([
      "asset",
      "liability",
      "expense",
      "equity",
    ]);
  });

  it("offers every partner of an asset or liability account", () => {
    expect(sideTypes("destination", asset)).toEqual(["asset", "liability", "expense", "equity"]);
    expect(sideTypes("source", { type: "liability" })).toEqual([
      "asset",
      "liability",
      "revenue",
      "equity",
    ]);
  });

  it("offers only own accounts opposite an expense, revenue or equity account", () => {
    expect(sideTypes("source", expense)).toEqual(["asset", "liability"]);
    expect(sideTypes("destination", revenue)).toEqual(["asset", "liability"]);
    expect(sideTypes("destination", equity)).toEqual(["asset", "liability"]);
  });

  it("narrows further to a template's kind", () => {
    expect(sideTypes("source", undefined, "withdrawal")).toEqual(["asset", "liability"]);
    expect(sideTypes("destination", asset, "withdrawal")).toEqual(["expense"]);
    expect(sideTypes("source", undefined, "deposit")).toEqual(["revenue"]);
    expect(sideTypes("destination", asset, "transfer")).toEqual(["asset", "liability"]);
  });
});

describe("creatableType", () => {
  it("creates revenue accounts as sources and expense accounts as destinations", () => {
    expect(creatableType("source", sideTypes("source", undefined))).toBe("revenue");
    expect(creatableType("destination", sideTypes("destination", asset))).toBe("expense");
  });

  it("creates nothing where the side does not take that type", () => {
    expect(creatableType("source", sideTypes("source", expense))).toBeUndefined();
    expect(creatableType("destination", sideTypes("destination", revenue))).toBeUndefined();
    expect(creatableType("source", sideTypes("source", undefined, "withdrawal"))).toBeUndefined();
  });
});
