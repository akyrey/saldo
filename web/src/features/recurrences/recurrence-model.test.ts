import { describe, expect, it } from "vitest";
import {
  defaultMoment,
  describeRepetition,
  emptyRecurrence,
  fromRecurrence,
  toPreviewRequest,
  toRequest,
  updateRepetition,
  type Recurrence,
} from "./recurrence-model";

function first<T>(items: readonly T[]): T {
  const [item] = items;
  if (item === undefined) throw new Error("empty list");
  return item;
}

const rep = (kind: string, moment: string, skip = 0, weekend_policy = "keep") => ({
  kind,
  moment,
  skip,
  weekend_policy,
});

describe("describeRepetition", () => {
  it("reads each kind as a sentence", () => {
    expect(describeRepetition(rep("daily", ""))).toBe("Every day");
    expect(describeRepetition(rep("daily", "", 2))).toBe("Every 3 days");
    expect(describeRepetition(rep("weekly", "1"))).toBe("Every Monday");
    expect(describeRepetition(rep("weekly", "7", 1))).toBe("Every 2 weeks on Sunday");
    expect(describeRepetition(rep("monthly", "31"))).toBe("Monthly on day 31");
    expect(describeRepetition(rep("monthly", "2", 2))).toBe("Every 3 months on day 2");
    expect(describeRepetition(rep("nth_weekday", "2,3"))).toBe(
      "Every second Wednesday of the month",
    );
    expect(describeRepetition(rep("yearly", "03-15"))).toBe("Every year on 03-15");
    expect(describeRepetition(rep("yearly", "03-15", 1))).toBe("Every 2 years on 03-15");
  });

  it("mentions the weekend policy and survives odd input", () => {
    expect(describeRepetition(rep("monthly", "1", 0, "skip"))).toBe(
      "Monthly on day 1, not on weekends",
    );
    expect(describeRepetition(rep("monthly", "1", 0, "previous_friday"))).toBe(
      "Monthly on day 1, weekends move to Friday",
    );
    expect(describeRepetition(rep("monthly", "1", 0, "next_monday"))).toBe(
      "Monthly on day 1, weekends move to Monday",
    );
    expect(describeRepetition(rep("weekly", "9"))).toBe("Every ?");
    expect(describeRepetition(rep("hourly", ""))).toBe("hourly ");
  });
});

describe("defaultMoment", () => {
  it("derives a moment from the first date", () => {
    // 2026-11-13 is a Friday, the second Friday of November.
    expect(defaultMoment("daily", "2026-11-13")).toBe("");
    expect(defaultMoment("weekly", "2026-11-13")).toBe("5");
    expect(defaultMoment("monthly", "2026-11-13")).toBe("13");
    expect(defaultMoment("nth_weekday", "2026-11-13")).toBe("2,5");
    expect(defaultMoment("yearly", "2026-11-13")).toBe("11-13");
    expect(defaultMoment("monthly", "")).toBe("1");
  });
});

describe("requests", () => {
  it("builds the PUT/POST body with trimmed values and template splits", () => {
    const state = emptyRecurrence("2026-11-13");
    const filled = {
      ...state,
      title: " Rent ",
      max_occurrences: "12",
      repetitions: [{ ...first(state.repetitions), skip: "1" }],
      splits: [
        {
          ...first(state.splits),
          description: "Rent",
          source_id: "a",
          destination_id: "b",
          amount: "800,5",
          piggy_bank_id: "p",
        },
      ],
    };
    const body = toRequest(filled);
    expect(body).toMatchObject({
      title: "Rent",
      first_date: "2026-11-13",
      repeat_until: null,
      max_occurrences: 12,
      active: true,
      apply_rules: true,
      repetitions: [{ kind: "monthly", moment: "13", skip: 1, weekend_policy: "keep" }],
    });
    expect(body.splits[0]).toMatchObject({
      amount: "800.5",
      piggy_bank_id: "p",
      category_id: null,
    });
  });

  it("needs a first date and repetitions to preview", () => {
    const state = emptyRecurrence("2026-11-13");
    expect(toPreviewRequest(state)).toMatchObject({ first_date: "2026-11-13", count: 5 });
    expect(toPreviewRequest({ ...state, first_date: "" })).toBeNull();
    expect(toPreviewRequest({ ...state, repetitions: [] })).toBeNull();
  });

  it("changing the kind resets the moment from the first date", () => {
    const state = emptyRecurrence("2026-11-13");
    const weekly = updateRepetition(state, 0, { kind: "weekly" });
    expect(weekly.repetitions[0]?.moment).toBe("5");
    const moved = updateRepetition(weekly, 0, { moment: "1" });
    expect(moved.repetitions[0]?.moment).toBe("1");
    expect(state.repetitions[0]?.kind).toBe("monthly");
  });

  it("round-trips a stored recurrence", () => {
    const stored: Recurrence = {
      id: "r",
      title: "Rent",
      description: null,
      first_date: "2026-11-01",
      repeat_until: "2027-11-01",
      max_occurrences: null,
      apply_rules: false,
      active: true,
      notes: "n",
      fired_through: null,
      last_fired_date: null,
      repetitions: [rep("monthly", "1")],
      splits: [
        {
          position: 0,
          description: "Rent",
          source_account_id: "a",
          destination_account_id: "b",
          amount: "800.00",
          currency: "EUR",
          foreign_amount: null,
          foreign_currency: null,
          category_id: "c",
          budget_id: "bud",
          bill_id: "bill",
          piggy_bank_id: null,
          tag_ids: ["t"],
        },
      ],
      created_at: "",
      updated_at: "",
    };
    const body = toRequest(fromRecurrence(stored));
    expect(body).toMatchObject({
      title: "Rent",
      repeat_until: "2027-11-01",
      apply_rules: false,
      notes: "n",
      repetitions: [{ kind: "monthly", moment: "1", skip: 0 }],
    });
    expect(body.splits[0]).toMatchObject({
      source_id: "a",
      destination_id: "b",
      amount: "800.00",
      category_id: "c",
      budget_id: "bud",
      bill_id: "bill",
      tag_ids: ["t"],
    });
  });
});
