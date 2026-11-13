import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { currenciesQuery, placesOf } from "@/features/accounts/queries";
import { todayIso } from "@/lib/dates";
import { BudgetFormFields } from "./BudgetFormFields";
import {
  EMPTY_BUDGET_FORM,
  monthLabel,
  monthOf,
  monthRange,
  shiftMonth,
  toBudgetRequest,
  type Month,
} from "./budget-model";
import { LimitProgress } from "./LimitProgress";
import { budgetsQuery, limitsQuery, type Budget } from "./queries";

/** One budget's limits in the month, as progress bars. */
function BudgetMonth({ budget, month }: { budget: Budget; month: Month }) {
  const { from, to } = monthRange(month);
  const limits = useQuery(limitsQuery(budget.id, from, to));
  const currencies = useQuery(currenciesQuery);
  return (
    <Card>
      <CardHeader className="pb-2 sm:pb-3">
        <CardTitle className="flex items-center justify-between gap-2 text-base">
          <Link
            to="/budgets/$budgetId"
            params={{ budgetId: budget.id }}
            className="hover:underline"
          >
            {budget.name}
          </Link>
          {budget.active ? null : (
            <span className="text-xs font-normal text-muted-foreground">inactive</span>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <QueryState isPending={limits.isPending} error={limits.error} />
        {limits.data?.length === 0 ? (
          <p className="text-sm text-muted-foreground">No limit in this month.</p>
        ) : null}
        {limits.data?.map((limit) => (
          <LimitProgress
            key={limit.id}
            limit={limit}
            places={placesOf(currencies.data, limit.currency)}
          />
        ))}
      </CardContent>
    </Card>
  );
}

/** Budgets with their progress in one month, and a form to add one. */
export function BudgetsPage() {
  const search = useSearch({ from: "/app/budgets" });
  const navigate = useNavigate({ from: "/budgets" });
  const month = search.month ?? monthOf(todayIso());
  const budgets = useQuery(budgetsQuery);
  const queryClient = useQueryClient();
  const [form, setForm] = useState(EMPTY_BUDGET_FORM);
  const create = useMutation({
    mutationFn: () => call(api.POST("/api/v1/budgets", { body: toBudgetRequest(form) })),
    onSuccess: async () => {
      setForm(EMPTY_BUDGET_FORM);
      await queryClient.invalidateQueries({ queryKey: ["budgets"] });
    },
  });
  const go = (delta: number) =>
    void navigate({ search: (prev) => ({ ...prev, month: shiftMonth(month, delta) }) });

  return (
    <div>
      <PageHeader
        title="Budgets"
        actions={
          <div className="flex items-center gap-1">
            <Button variant="ghost" size="icon" aria-label="Previous month" onClick={() => go(-1)}>
              <ChevronLeft />
            </Button>
            <span className="min-w-36 text-center text-sm font-medium" data-testid="budget-month">
              {monthLabel(month)}
            </span>
            <Button variant="ghost" size="icon" aria-label="Next month" onClick={() => go(1)}>
              <ChevronRight />
            </Button>
          </div>
        }
      />
      <form
        aria-label="New budget"
        className="mb-6 flex flex-col gap-3 rounded-lg border p-3 sm:p-4"
        onSubmit={(event) => {
          event.preventDefault();
          create.mutate();
        }}
      >
        <BudgetFormFields
          compact
          form={form}
          errors={fieldErrors(create.error)}
          onChange={(changes) => setForm((f) => ({ ...f, ...changes }))}
        />
        {create.error && Object.keys(fieldErrors(create.error)).length === 0 ? (
          <Alert>{errorMessage(create.error)}</Alert>
        ) : null}
        <div>
          <Button type="submit" disabled={create.isPending}>
            Add budget
          </Button>
        </div>
      </form>
      <QueryState isPending={budgets.isPending} error={budgets.error} />
      {budgets.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">No budgets yet.</p>
      ) : null}
      <div className="grid gap-4 md:grid-cols-2">
        {budgets.data?.map((budget) => (
          <BudgetMonth key={budget.id} budget={budget} month={month} />
        ))}
      </div>
    </div>
  );
}
