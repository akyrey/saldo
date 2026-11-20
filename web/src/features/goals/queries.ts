import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type Goal = components["schemas"]["NetWorthGoalView"];
export type CreateGoal = components["schemas"]["CreateGoalRequest"];
export type GoalPatch = components["schemas"]["GoalPatch"];
export type SafeToSpend = components["schemas"]["SafeToSpendView"];

export const goalsQuery = queryOptions({
  queryKey: ["goals"],
  queryFn: async () => (await call(api.GET("/api/v1/net-worth-goals"))).data,
});

export const safeToSpendQuery = (scope?: string) =>
  queryOptions({
    queryKey: ["reports", "safe-to-spend", scope ?? "household"],
    queryFn: async () =>
      (await call(api.GET("/api/v1/reports/safe-to-spend", { params: { query: { scope } } }))).data,
  });

export async function createGoal(body: CreateGoal): Promise<Goal> {
  return (await call(api.POST("/api/v1/net-worth-goals", { body }))).data;
}

export async function updateGoal(id: string, body: GoalPatch): Promise<Goal> {
  return (await call(api.PATCH("/api/v1/net-worth-goals/{id}", { params: { path: { id } }, body })))
    .data;
}

export async function deleteGoal(id: string): Promise<void> {
  await call(api.DELETE("/api/v1/net-worth-goals/{id}", { params: { path: { id } } }));
}

/** The labels of the safe-to-spend parts, in the order of the formula. */
export const PART_LABELS: Record<string, string> = {
  income_received: "Income received",
  income_planned: "Income still to come",
  spent: "Spent so far",
  bills: "Bills due",
  planned: "Planned payments",
  budgets: "Left in budgets",
  piggy_banks: "Piggy bank targets",
  savings_transfers: "Transfers to savings",
};

/** Parts that add to the figure; the rest subtract. */
export const ADDING_PARTS = new Set(["income_received", "income_planned"]);
