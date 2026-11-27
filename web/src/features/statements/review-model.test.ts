import { describe, expect, it } from "vitest";
import {
  actionCounts,
  findingLabel,
  newCounterpartyName,
  partColumns,
  proposalOf,
  reconcileSearch,
  statusLabel,
  summaryNotes,
} from "./review-model";

describe("statement review model", () => {
  it("names every line status and finding in plain words", () => {
    expect(statusLabel("new")).toBe("New");
    expect(statusLabel("duplicate")).toBe("Already imported");
    expect(statusLabel("possible_match")).toBe("Matches a transaction");
    expect(statusLabel("skipped_before")).toBe("Skipped before");
    expect(statusLabel("not_matched_reconciled")).toBe("Missing in a reconciled period");
    expect(statusLabel("something_else")).toBe("something_else");
    expect(findingLabel("duplicate_of")).toBe("Probably a duplicate");
    expect(findingLabel("identical")).toBe("Identical transactions");
    expect(findingLabel("not_on_statement")).toBe("Not on the statement");
  });

  it("counts what a commit will do", () => {
    const lines = [
      { action: "create" },
      { action: "create" },
      { action: "link" },
      { action: "skip" },
    ];
    expect(actionCounts(lines)).toEqual({ create: 2, link: 1, skip: 1 });
    expect(actionCounts([])).toEqual({ create: 0, link: 0, skip: 0 });
  });

  it("offers to reconcile from the closing balance only when the file had one", () => {
    expect(reconcileSearch({ closing_date: "2026-11-30", closing_balance: "120.50" })).toEqual({
      date: "2026-11-30",
      balance: "120.50",
    });
    expect(reconcileSearch({ closing_date: "2026-11-30", closing_balance: null })).toBeNull();
    expect(reconcileSearch({ closing_date: null, closing_balance: "1.00" })).toBeNull();
  });

  it("reads the part columns a mapping names", () => {
    const mapping = { part_columns: [["Meal Vouchers"], ["Shopping Vouchers", "Buoni"]] };
    expect(partColumns(mapping)).toEqual(["Meal Vouchers", "Shopping Vouchers"]);
    expect(partColumns({})).toEqual([]);
    expect(partColumns(null)).toEqual([]);
    expect(partColumns({ part_columns: "nope" })).toEqual([]);
  });

  it("reads the summary's notes defensively", () => {
    const finding = {
      kind: "not_on_statement",
      date: "2026-11-12",
      amount: "-3.00",
      description: "x",
    };
    expect(
      summaryNotes({
        findings: [finding, { kind: 5 }],
        unsupported: [{ row: 4, reason: "pending" }, "junk"],
        unsupported_total: 3,
      }),
    ).toEqual({ findings: [finding], unsupported: [{ row: 4, reason: "pending" }], more: 2 });
    expect(summaryNotes(null)).toEqual({ findings: [], unsupported: [], more: 0 });
    expect(summaryNotes({ findings: "nope", unsupported_total: "x" })).toEqual({
      findings: [],
      unsupported: [],
      more: 0,
    });
  });

  it("reads a proposal defensively", () => {
    const proposal = {
      source: "amazon",
      description: "Amazon: Moka",
      counterparty: "Amazon",
      notes: ["order", 3],
      items: [{ product: "Moka", amount_minor: 1500 }, { product: 1 }],
    };
    expect(proposalOf(proposal)).toEqual({
      ...proposal,
      notes: ["order"],
      items: [{ product: "Moka", amount_minor: 1500 }],
    });
    expect(proposalOf(null)).toBeNull();
    expect(proposalOf({ source: "amazon" })).toBeNull();
  });

  it("names a new counterparty after the line", () => {
    expect(newCounterpartyName({ counterparty_name: "Bar Sport", description: "POS 1" })).toBe(
      "Bar Sport",
    );
    expect(newCounterpartyName({ counterparty_name: "  ", description: "POS 1" })).toBe("POS 1");
    expect(newCounterpartyName({ counterparty_name: null, description: "POS 1" })).toBe("POS 1");
  });
});
