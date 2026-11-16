import { describe, expect, it } from "vitest";
import { split } from "@/test/fake-api";
import { groupByDay, groupTotals, splitFlow, signedAmount } from "./day-groups";
import type { TransactionGroup } from "./queries";

function group(id: string, splits: ReturnType<typeof split>[]): TransactionGroup {
  return { id, title: null, created_at: "", updated_at: "", splits } as TransactionGroup;
}

function on(date: string, s: ReturnType<typeof split>) {
  return { ...s, date };
}

describe("splitFlow", () => {
  it("uses the kind when there is no account perspective", () => {
    expect(splitFlow(split("s", "a1", "a2", "1.00", "withdrawal"))).toBe("out");
    expect(splitFlow(split("s", "a1", "a2", "1.00", "deposit"))).toBe("in");
    expect(splitFlow(split("s", "a1", "a2", "1.00", "transfer"))).toBe("neutral");
    expect(splitFlow(split("s", "a1", "a2", "1.00", "opening_balance"))).toBe("neutral");
  });

  it("uses the direction relative to the perspective account", () => {
    const transfer = split("s", "a1", "a2", "1.00", "transfer");
    expect(splitFlow(transfer, "a1")).toBe("out");
    expect(splitFlow(transfer, "a2")).toBe("in");
  });
});

describe("signedAmount", () => {
  it("negates money going out and keeps the rest", () => {
    expect(signedAmount("4.00", "out")).toBe("-4.00");
    expect(signedAmount("4.00", "in")).toBe("4.00");
    expect(signedAmount("4.00", "neutral")).toBe("4.00");
  });
});

describe("groupByDay", () => {
  const today = "2026-11-15";

  it("splits upcoming days from past ones, keeping the list's order", () => {
    const groups = [
      group("future", [on("2026-11-20", split("s1", "a1", "a2", "9.00"))]),
      group("today", [on("2026-11-15", split("s2", "a1", "a2", "1.00"))]),
      group("past1", [on("2026-11-10", split("s3", "a1", "a2", "2.00"))]),
      group("past2", [on("2026-11-10", split("s4", "a1", "a2", "3.00", "deposit"))]),
    ];
    const days = groupByDay(groups, today);
    expect(days.upcoming.map((d) => d.date)).toEqual(["2026-11-20"]);
    expect(days.past.map((d) => d.date)).toEqual(["2026-11-15", "2026-11-10"]);
    expect(days.past[1]?.groups.map((g) => g.id)).toEqual(["past1", "past2"]);
  });

  it("totals each day per currency, leaving neutral splits out", () => {
    const groups = [
      group("g1", [
        on("2026-11-10", split("s1", "a1", "a2", "2.50")),
        on("2026-11-10", split("s2", "a1", "a2", "1.00", "transfer")),
      ]),
      group("g2", [on("2026-11-10", split("s3", "a1", "a2", "10.00", "deposit"))]),
      group("g3", [{ ...on("2026-11-10", split("s4", "a1", "a2", "5")), currency: "JPY" }]),
    ];
    const [day] = groupByDay(groups, today).past;
    expect(day?.totals).toEqual([
      { currency: "EUR", amount: "7.50" },
      { currency: "JPY", amount: "-5" },
    ]);
  });

  it("totals the upcoming section as a whole", () => {
    const groups = [
      group("f1", [on("2026-11-20", split("s1", "a1", "a2", "9.00"))]),
      group("f2", [on("2026-11-21", split("s2", "a1", "a2", "1.00"))]),
    ];
    expect(groupByDay(groups, today).upcomingTotals).toEqual([
      { currency: "EUR", amount: "-10.00" },
    ]);
  });

  it("files a group under its latest split date", () => {
    const groups = [
      group("g", [
        on("2026-11-01", split("s1", "a1", "a2", "1.00")),
        on("2026-11-03", split("s2", "a1", "a2", "1.00")),
      ]),
    ];
    expect(groupByDay(groups, today).past.map((d) => d.date)).toEqual(["2026-11-03"]);
  });

  it("skips groups without splits", () => {
    expect(groupByDay([group("empty", [])], today).past).toEqual([]);
  });
});

describe("groupTotals", () => {
  it("nets the splits of one group per currency", () => {
    const g = group("g", [
      split("s1", "a1", "a2", "32.90"),
      split("s2", "a1", "a2", "129.00"),
      split("s3", "a1", "a2", "19.99", "deposit"),
    ]);
    expect(groupTotals(g)).toEqual([{ currency: "EUR", amount: "-141.91" }]);
  });

  it("sums a group of transfers without a sign", () => {
    const g = group("g", [
      split("s1", "a1", "a2", "100.00", "transfer"),
      split("s2", "a1", "a3", "50.50", "transfer"),
    ]);
    expect(groupTotals(g)).toEqual([{ currency: "EUR", amount: "150.50" }]);
  });

  it("follows the perspective account", () => {
    const g = group("g", [
      split("s1", "a1", "a2", "100.00", "transfer"),
      split("s2", "a3", "a1", "30.00", "transfer"),
    ]);
    expect(groupTotals(g, "a1")).toEqual([{ currency: "EUR", amount: "-70.00" }]);
  });
});
