import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import type { Interval } from "./report-model";

export type CashFlowReport = components["schemas"]["CashFlowReport"];
export type NetWorthReport = components["schemas"]["NetWorthReport"];
export type BreakdownReport = components["schemas"]["BreakdownReport"];
export type BudgetReport = components["schemas"]["BudgetReport"];

export interface ReportParams {
  from: string;
  to: string;
  interval: Interval;
  scope?: string;
}

const key = (kind: string, p: ReportParams) =>
  ["reports", kind, p.from, p.to, p.interval, p.scope ?? "household"] as const;

export const cashFlowQuery = (p: ReportParams) =>
  queryOptions({
    queryKey: key("cash-flow", p),
    queryFn: async () =>
      (await call(api.GET("/api/v1/reports/cash-flow", { params: { query: p } }))).data,
  });

export const netWorthQuery = (p: ReportParams) =>
  queryOptions({
    queryKey: key("net-worth", p),
    queryFn: async () =>
      (await call(api.GET("/api/v1/reports/net-worth", { params: { query: p } }))).data,
  });

export const byCategoryQuery = (p: ReportParams) =>
  queryOptions({
    queryKey: key("by-category", p),
    queryFn: async () =>
      (await call(api.GET("/api/v1/reports/by-category", { params: { query: p } }))).data,
  });

export const byTagQuery = (p: ReportParams) =>
  queryOptions({
    queryKey: key("by-tag", p),
    queryFn: async () =>
      (await call(api.GET("/api/v1/reports/by-tag", { params: { query: p } }))).data,
  });

export const budgetReportQuery = (p: ReportParams) =>
  queryOptions({
    queryKey: key("budget-vs-actual", p),
    queryFn: async () =>
      (await call(api.GET("/api/v1/reports/budget-vs-actual", { params: { query: p } }))).data,
  });

export type ForecastReport = components["schemas"]["ForecastReport"];

export interface ForecastParams {
  to: string;
  interval: "day" | "week" | "month";
  scope?: string;
  account_id?: string;
}

export const forecastQuery = (p: ForecastParams) =>
  queryOptions({
    queryKey: ["reports", "forecast", p.to, p.interval, p.scope ?? "household", p.account_id ?? ""],
    queryFn: async () =>
      (await call(api.GET("/api/v1/reports/forecast", { params: { query: p } }))).data,
  });
