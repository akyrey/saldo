import { describe, expect, it } from "vitest";
import { emptySplit, type EditorState, type SplitRow } from "./editor-model";
import { touchesReconciled } from "./reconciled-guard";

function state(amount: string, extra: Partial<ReturnType<typeof emptySplit>> = {}): EditorState {
  return {
    title: "",
    splits: [
      {
        ...emptySplit("2026-11-05"),
        id: "s1",
        source_id: "a",
        destination_id: "b",
        amount,
        ...extra,
      },
      {
        ...emptySplit("2026-11-05"),
        id: "s2",
        source_id: "a",
        destination_id: "c",
        amount: "1.00",
      },
    ],
  };
}

describe("touchesReconciled", () => {
  const reconciled = new Set(["s1"]);
  const before = state("12.30");
  const [first, second] = before.splits as [SplitRow, SplitRow];

  it("ignores changes the bank does not see", () => {
    expect(
      touchesReconciled(before, state("12,30", { description: "Food", notes: "x" }), reconciled),
    ).toBe(false);
    const other = {
      ...before,
      splits: [first, { ...second, amount: "9.00" }],
    };
    expect(touchesReconciled(before, other, reconciled)).toBe(false);
  });

  it("notices a new amount, date or account, or a removed split", () => {
    expect(touchesReconciled(before, state("13.00"), reconciled)).toBe(true);
    expect(touchesReconciled(before, state("12.30", { date: "2026-11-06" }), reconciled)).toBe(
      true,
    );
    expect(touchesReconciled(before, state("12.30", { destination_id: "c" }), reconciled)).toBe(
      true,
    );
    expect(touchesReconciled(before, { ...before, splits: [second] }, reconciled)).toBe(true);
    expect(touchesReconciled(before, state("13.00"), new Set())).toBe(false);
  });
});
