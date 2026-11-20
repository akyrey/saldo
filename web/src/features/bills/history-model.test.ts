import { describe, expect, it } from "vitest";
import { epochOf, historyChart, timeTicks, type BillHistory } from "./history-model";

function payment(
  id: string,
  date: string,
  amount: string,
  currency = "EUR",
  due: string | null = null,
) {
  return {
    split_id: id,
    group_id: `g${id}`,
    date,
    description: `Gym ${date}`,
    amount,
    currency,
    source: { id: "a1", name: "Checking" },
    destination: { id: "a2", name: "Gym" },
    due_date: due,
  };
}

function due(date: string, status: string) {
  return {
    date,
    status,
    period_id: status === "scheduled" ? null : `p${date}`,
    paid_split_id: null,
  };
}

const HISTORY: BillHistory = {
  bill_id: "b1",
  currency: "EUR",
  range: { from: "2026-09-02", to: "2027-03-02" },
  amount_min: "39.00",
  amount_max: "45.00",
  due_dates: [
    due("2026-09-02", "overdue"),
    due("2026-10-02", "paid"),
    due("2026-11-02", "paid"),
    due("2026-12-02", "open"),
    due("2027-03-02", "scheduled"),
  ],
  payments: [
    payment("s3", "2026-11-03", "45.00", "EUR", "2026-11-02"),
    payment("s2", "2026-10-15", "50.00", "USD"),
    payment("s1", "2026-10-03", "42.50", "EUR", "2026-10-02"),
  ],
  summary: {
    count: 2,
    total: "87.50",
    average: "43.75",
    min: "42.50",
    max: "45.00",
    other_currency_count: 1,
  },
  truncated: false,
};

describe("historyChart", () => {
  it("plots payments in the bill's currency oldest first, keeping the exact strings", () => {
    const chart = historyChart(HISTORY, 2, "2026-11-13");
    expect(chart.payments.map((p) => [p.date, p.y, p.amount, p.dueDate])).toEqual([
      ["2026-10-03", 42.5, "42.50", "2026-10-02"],
      ["2026-11-03", 45, "45.00", "2026-11-02"],
    ]);
    expect(chart.band).toEqual({ min: 39, max: 45 });
    expect(chart.domain).toEqual([epochOf("2026-09-02"), epochOf("2027-03-02")]);
    expect(chart.today).toBe(epochOf("2026-11-13"));
  });

  it("groups due dates by status and leaves out empty statuses", () => {
    const chart = historyChart(HISTORY, 2, "2026-11-13");
    expect(chart.dues.map((s) => [s.status, s.points.map((p) => p.date)])).toEqual([
      ["paid", ["2026-10-02", "2026-11-02"]],
      ["open", ["2026-12-02"]],
      ["overdue", ["2026-09-02"]],
      ["scheduled", ["2027-03-02"]],
    ]);
  });

  it("drops the today marker outside the span", () => {
    expect(historyChart(HISTORY, 2, "2030-01-01").today).toBeNull();
  });
});

describe("timeTicks", () => {
  it("ticks each month on short spans", () => {
    const { ticks, unit } = timeTicks("2026-09-02", "2027-01-02");
    expect(unit).toBe("month");
    expect(ticks).toEqual(["2026-10-01", "2026-11-01", "2026-12-01", "2027-01-01"].map(epochOf));
  });

  it("ticks every few months to stay readable", () => {
    const { ticks } = timeTicks("2024-01-01", "2026-12-31");
    expect(ticks.length).toBeLessThanOrEqual(12);
    expect(ticks[0]).toBe(epochOf("2024-01-01"));
    expect(ticks[1]).toBe(epochOf("2024-04-01"));
  });

  it("ticks on January firsts for long spans", () => {
    const { ticks, unit } = timeTicks("2015-06-10", "2026-11-13");
    expect(unit).toBe("year");
    expect(ticks[0]).toBe(epochOf("2016-01-01"));
    expect(ticks.length).toBeLessThanOrEqual(12);
    expect(ticks.every((t) => new Date(t).getUTCMonth() === 0)).toBe(true);
  });
});
