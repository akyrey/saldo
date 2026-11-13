import { describe, expect, it } from "vitest";
import {
  ACTION_KINDS,
  emptyRuleForm,
  fromRule,
  reorder,
  toRequest,
  TRIGGER_KINDS,
  valueKind,
} from "./rule-form";

const RULE = {
  id: "r1",
  rule_group_id: "g1",
  title: "Rent",
  description: null,
  trigger_on: "create",
  strict: true,
  stop_processing: false,
  active: true,
  sort_order: 2,
  triggers: [{ kind: "description_contains", value: "rent", negate: false }],
  actions: [{ kind: "link_to_bill", value: "b1", stop_processing: false, active: true }],
  created_at: "",
  updated_at: "",
};

describe("rule form", () => {
  it("knows which picker each keyword needs", () => {
    expect(valueKind("description_contains")).toBe("text");
    expect(valueKind("amount_more")).toBe("amount");
    expect(valueKind("category_is")).toBe("category");
    expect(valueKind("source_account_is")).toBe("account");
    expect(valueKind("transaction_type")).toBe("kind");
    expect(valueKind("has_no_tag")).toBe("none");
    expect(valueKind("link_to_bill")).toBe("bill");
    expect(valueKind("set_budget")).toBe("budget");
    expect(valueKind("remove_all_tags")).toBe("none");
    expect(valueKind("journal_id")).toBe("text");
    expect(TRIGGER_KINDS).toHaveLength(18);
    expect(ACTION_KINDS).toHaveLength(10);
  });

  it("starts with one empty trigger and action in the chosen group", () => {
    const form = emptyRuleForm("g1");
    expect(form.rule_group_id).toBe("g1");
    expect(form.triggers).toHaveLength(1);
    expect(form.actions).toHaveLength(1);
    expect(form.trigger_on).toBe("create");
  });

  it("round-trips a rule and drops values of keywords without one", () => {
    const form = fromRule(RULE);
    expect(toRequest(form)).toEqual({
      rule_group_id: "g1",
      title: "Rent",
      description: null,
      trigger_on: "create",
      strict: true,
      stop_processing: false,
      active: true,
      sort_order: 2,
      triggers: [{ kind: "description_contains", value: "rent", negate: false }],
      actions: [{ kind: "link_to_bill", value: "b1", stop_processing: false, active: true }],
    });
    const changed = {
      ...form,
      triggers: [{ kind: "has_no_tag", value: "leftover", negate: true }],
      actions: [{ kind: "set_notes", value: " note ", stop_processing: true, active: true }],
    };
    expect(toRequest(changed).triggers).toEqual([
      { kind: "has_no_tag", value: null, negate: true },
    ]);
    expect(toRequest(changed).actions[0]).toEqual({
      kind: "set_notes",
      value: "note",
      stop_processing: true,
      active: true,
    });
  });

  it("moves an item up or down and renumbers the order", () => {
    const items = [
      { id: "a", sort_order: 0 },
      { id: "b", sort_order: 1 },
      { id: "c", sort_order: 2 },
    ];
    expect(reorder(items, "c", -1).map((i) => [i.id, i.sort_order])).toEqual([
      ["a", 0],
      ["c", 1],
      ["b", 2],
    ]);
    expect(reorder(items, "a", -1)).toEqual(items);
    expect(reorder(items, "c", 1)).toEqual(items);
  });
});
