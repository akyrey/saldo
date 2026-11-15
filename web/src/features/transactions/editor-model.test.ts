import { describe, expect, it } from "vitest";
import type { ErrorDetail } from "@/api/errors";
import {
  addSplit,
  deriveKind,
  emptySplit,
  fromGroup,
  mapServerErrors,
  removeSplit,
  toRequest,
  updateSplit,
  type EditorState,
} from "./editor-model";
import type { TransactionGroup } from "./queries";

const asset = { type: "asset", equity_role: null };
const liability = { type: "liability", equity_role: null };
const expense = { type: "expense", equity_role: null };
const revenue = { type: "revenue", equity_role: null };
const opening = { type: "equity", equity_role: "opening_balance" };
const reconciliation = { type: "equity", equity_role: "reconciliation" };

describe("deriveKind", () => {
  it("follows the table in ARCHITECTURE.md 4.1", () => {
    expect(deriveKind(asset, expense)).toBe("withdrawal");
    expect(deriveKind(liability, expense)).toBe("withdrawal");
    expect(deriveKind(revenue, asset)).toBe("deposit");
    expect(deriveKind(revenue, liability)).toBe("deposit");
    expect(deriveKind(asset, liability)).toBe("transfer");
    expect(deriveKind(asset, asset)).toBe("transfer");
    expect(deriveKind(opening, asset)).toBe("opening_balance");
    expect(deriveKind(liability, opening)).toBe("opening_balance");
    expect(deriveKind(reconciliation, asset)).toBe("reconciliation");
    expect(deriveKind(asset, reconciliation)).toBe("reconciliation");
  });

  it("says invalid for pairs the server rejects", () => {
    expect(deriveKind(expense, asset)).toBe("invalid");
    expect(deriveKind(revenue, expense)).toBe("invalid");
    expect(deriveKind(opening, reconciliation)).toBe("invalid");
  });

  it("is undefined until both accounts are chosen", () => {
    expect(deriveKind(undefined, expense)).toBeUndefined();
    expect(deriveKind(asset, undefined)).toBeUndefined();
  });
});

describe("split rows", () => {
  it("adds a row that copies the previous date", () => {
    const state: EditorState = { title: "", splits: [emptySplit("2026-11-07")] };
    const next = addSplit(state, "2026-01-01");
    expect(next.splits).toHaveLength(2);
    expect(next.splits[1]?.date).toBe("2026-11-07");
    expect(next.splits[1]?.key).not.toBe(next.splits[0]?.key);
    expect(state.splits).toHaveLength(1);
  });

  it("updates one row without touching the others or the original", () => {
    const state: EditorState = {
      title: "",
      splits: [emptySplit("2026-11-07"), emptySplit("2026-11-07")],
    };
    const next = updateSplit(state, 1, { amount: "3" });
    expect(next.splits[1]?.amount).toBe("3");
    expect(next.splits[0]?.amount).toBe("");
    expect(state.splits[1]?.amount).toBe("");
  });

  it("never removes the last row", () => {
    const one: EditorState = { title: "", splits: [emptySplit("2026-11-07")] };
    expect(removeSplit(one, 0)).toBe(one);
    const two = addSplit(one, "2026-11-07");
    expect(removeSplit(two, 0).splits).toEqual([two.splits[1]]);
  });
});

describe("toRequest", () => {
  it("normalizes amounts and drops empty optional fields", () => {
    const state: EditorState = {
      title: "  Saturday shop ",
      splits: [
        {
          ...emptySplit("2026-11-07"),
          description: " Bread ",
          source_id: "a",
          destination_id: "b",
          amount: " 12,5 ",
          category_id: "c",
          budget_id: "b1",
          bill_id: "bill",
          tag_ids: ["t1"],
        },
        {
          ...emptySplit("2026-11-08"),
          description: "Fee",
          source_id: "d",
          destination_id: "b",
          amount: "1",
          foreign_amount: "2,00",
          foreign_currency: "USD",
        },
      ],
    };

    expect(toRequest(state)).toEqual({
      title: "Saturday shop",
      splits: [
        {
          date: "2026-11-07",
          description: "Bread",
          source_id: "a",
          destination_id: "b",
          amount: "12.5",
          category_id: "c",
          budget_id: "b1",
          bill_id: "bill",
          tag_ids: ["t1"],
          foreign_amount: null,
          foreign_currency: null,
          notes: null,
        },
        {
          date: "2026-11-08",
          description: "Fee",
          source_id: "d",
          destination_id: "b",
          amount: "1",
          category_id: null,
          budget_id: null,
          bill_id: null,
          tag_ids: [],
          foreign_amount: "2.00",
          foreign_currency: "USD",
          notes: null,
        },
      ],
    });
  });

  it("names the saved split a row continues, so it keeps its attachments", () => {
    const saved = { ...emptySplit("2026-11-07"), id: "s1", amount: "1" };
    const added = { ...emptySplit("2026-11-07"), amount: "2" };
    const [first, second] = toRequest({ title: "", splits: [saved, added] }).splits;
    expect(first?.id).toBe("s1");
    expect(second).not.toHaveProperty("id");
  });

  it("sends a null title when empty", () => {
    expect(toRequest({ title: " ", splits: [] }).title).toBeNull();
  });
});

describe("fromGroup", () => {
  it("loads a saved group into editable rows", () => {
    const group: TransactionGroup = {
      id: "g",
      title: null,
      created_at: "",
      updated_at: "",
      splits: [
        {
          id: "s1",
          position: 0,
          kind: "withdrawal",
          date: "2026-11-07",
          description: "Bread",
          source_account_id: "a",
          destination_account_id: "b",
          amount: "12.50",
          currency: "EUR",
          foreign_amount: null,
          foreign_currency: null,
          category_id: "c",
          budget_id: "b",
          tag_ids: ["t"],
          reconciled: false,
          notes: "fresh",
          meta: {},
          created_at: "",
          updated_at: "",
        },
      ],
    };

    const state = fromGroup(group);

    expect(state.title).toBe("");
    expect(state.splits[0]).toMatchObject({
      id: "s1",
      date: "2026-11-07",
      source_id: "a",
      destination_id: "b",
      amount: "12.50",
      foreign_amount: "",
      foreign_currency: "",
      category_id: "c",
      budget_id: "b",
      tag_ids: ["t"],
      notes: "fresh",
    });
  });
});

describe("mapServerErrors", () => {
  it("puts each detail on its row and field", () => {
    const details: ErrorDetail[] = [
      { field: "splits[1].amount", code: "amount_not_positive", message: "must be > 0" },
      { field: "splits[2]", code: "invalid_account_pair", message: "bad pair" },
      { field: "splits[0].foreign_amount", code: "x", message: "foreign" },
      { field: "title", code: "invalid", message: "too long" },
      { field: null, code: "other", message: "general" },
    ];

    const errors = mapServerErrors(details);

    expect(errors.rows[1]).toEqual({ amount: "must be > 0" });
    expect(errors.rows[2]).toEqual({ row: "bad pair" });
    expect(errors.rows[0]).toEqual({ foreign_amount: "foreign" });
    expect(errors.form).toEqual(["title: too long", "general"]);
  });

  it("keeps the first message per field", () => {
    const errors = mapServerErrors([
      { field: "splits[0].amount", code: "a", message: "first" },
      { field: "splits[0].amount", code: "b", message: "second" },
    ]);
    expect(errors.rows[0]).toEqual({ amount: "first" });
  });
});
