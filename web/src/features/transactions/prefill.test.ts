import { describe, expect, it } from "vitest";
import type { Template } from "@/features/templates/queries";
import { emptySplit } from "./editor-model";
import type { TransactionGroup } from "./queries";
import {
  accountTypesFor,
  duplicateOf,
  fromTemplate,
  templateKindOf,
  toTemplateRequest,
} from "./prefill";

function template(extra: Partial<Template> = {}): Template {
  return {
    id: "t1",
    name: "Coffee",
    kind: "withdrawal",
    title: null,
    splits: [],
    sort_order: 0,
    created_at: "",
    updated_at: "",
    ...extra,
  };
}

describe("fromTemplate", () => {
  it("fills what the template has and leaves the rest empty, dated today", () => {
    const state = fromTemplate(
      template({
        title: "Morning",
        splits: [
          { description: "Coffee", source_id: "a1", amount: "2.50", tag_ids: ["t"], bill_id: "b" },
        ],
      }),
      "2026-11-13",
    );
    expect(state.title).toBe("Morning");
    expect(state.kind).toBe("withdrawal");
    const [row] = state.splits;
    expect(row).toMatchObject({
      date: "2026-11-13",
      description: "Coffee",
      source_id: "a1",
      destination_id: "",
      amount: "2.50",
      category_id: "",
      bill_id: "b",
      tag_ids: ["t"],
    });
  });

  it("gives a template without splits one empty row", () => {
    const state = fromTemplate(template({ kind: "transfer" }), "2026-11-13");
    expect(state.splits).toHaveLength(1);
    expect(state.splits[0]?.description).toBe("");
    expect(state.kind).toBe("transfer");
  });

  it("ignores a kind it does not know", () => {
    expect(fromTemplate(template({ kind: "other" }), "2026-11-13").kind).toBeUndefined();
  });
});

describe("duplicateOf", () => {
  it("copies every split with today's date", () => {
    const split = {
      id: "s1",
      position: 0,
      kind: "withdrawal",
      date: "2026-10-01",
      description: "Bread",
      source_account_id: "a1",
      destination_account_id: "e1",
      amount: "3.20",
      currency: "EUR",
      tag_ids: [],
      reconciled: true,
      meta: {},
      created_at: "",
      updated_at: "",
    };
    const group: TransactionGroup = {
      id: "g1",
      title: "Shop",
      created_at: "",
      updated_at: "",
      splits: [split, { ...split, id: "s2", amount: "1.00" }],
    };
    const state = duplicateOf(group, "2026-11-13");
    expect(state.title).toBe("Shop");
    expect(state.splits.map((s) => [s.date, s.amount])).toEqual([
      ["2026-11-13", "3.20"],
      ["2026-11-13", "1.00"],
    ]);
    expect(state.splits[0]?.key).not.toBe(state.splits[1]?.key);
    // A copy is new: its splits continue nothing, so attachments stay behind.
    expect(state.splits.map((s) => s.id)).toEqual([undefined, undefined]);
  });
});

describe("accountTypesFor", () => {
  it("offers the sides that form the kind", () => {
    expect(accountTypesFor("deposit")).toEqual({
      source: ["revenue"],
      destination: ["asset", "liability"],
    });
    expect(accountTypesFor(undefined)).toBeUndefined();
  });
});

describe("toTemplateRequest", () => {
  it("keeps what is filled in, leaves the rest null and drops dates", () => {
    const state = fromTemplate(template({ splits: [{ tag_ids: [] }] }), "2026-11-13");
    const filled = {
      ...state,
      title: " ",
      splits: state.splits.map((row) => ({ ...row, description: " Tea ", amount: "1,5" })),
    };
    const body = toTemplateRequest(filled, " Tea ", "withdrawal");
    expect(body.name).toBe("Tea");
    expect(body.title).toBeNull();
    expect(body.splits?.[0]).toMatchObject({
      description: "Tea",
      amount: "1.5",
      source_id: null,
      tag_ids: [],
    });
    expect(body.splits?.[0]).not.toHaveProperty("date");
  });
});

describe("templateKindOf", () => {
  const accounts = [
    { id: "a", type: "asset" },
    { id: "r", type: "revenue" },
  ];
  it("prefers the editor's hint, then the accounts, then withdrawal", () => {
    const state = fromTemplate(template({ kind: "transfer" }), "2026-11-13");
    expect(templateKindOf(state, accounts)).toBe("transfer");
    const row = { ...emptySplit("2026-11-13"), source_id: "r", destination_id: "a" };
    expect(templateKindOf({ title: "", splits: [row] }, accounts)).toBe("deposit");
    expect(templateKindOf({ title: "", splits: [] }, accounts)).toBe("withdrawal");
  });
});
