import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { BudgetFormFields } from "./BudgetFormFields";
import { EMPTY_BUDGET_FORM, toBudgetRequest, type BudgetForm } from "./budget-model";
import { LimitEditor } from "./LimitEditor";
import { budgetQuery, type Budget } from "./queries";

function formOf(budget: Budget): BudgetForm {
  return {
    ...EMPTY_BUDGET_FORM,
    name: budget.name,
    active: budget.active,
    notes: budget.notes ?? "",
    autoType: budget.auto_budget?.type ?? "",
    autoPeriod: budget.auto_budget?.period ?? "monthly",
    autoAmount: budget.auto_budget?.amount ?? "",
  };
}

function BudgetSettings({ budget }: { budget: Budget }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [form, setForm] = useState(() => formOf(budget));
  const save = useMutation({
    mutationFn: () =>
      call(
        api.PATCH("/api/v1/budgets/{id}", {
          params: { path: { id: budget.id } },
          body: toBudgetRequest(form),
        }),
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["budgets"] }),
  });
  const remove = useMutation({
    mutationFn: () =>
      call(api.DELETE("/api/v1/budgets/{id}", { params: { path: { id: budget.id } } })),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["budgets"] });
      void navigate({ to: "/budgets" });
    },
  });
  const errors = fieldErrors(save.error);
  const error =
    (save.error && Object.keys(errors).length === 0 ? save.error : null) ?? remove.error;
  return (
    <form
      aria-label="Budget settings"
      className="mb-6 flex flex-col gap-3 rounded-lg border p-3 sm:p-4"
      onSubmit={(event) => {
        event.preventDefault();
        save.mutate();
      }}
    >
      <BudgetFormFields
        form={form}
        errors={errors}
        onChange={(changes) => setForm((f) => ({ ...f, ...changes }))}
      />
      {error ? <Alert>{errorMessage(error)}</Alert> : null}
      {save.isSuccess ? <p className="text-sm text-muted-foreground">Saved.</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={save.isPending}>
          Save budget
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={remove.isPending}
          onClick={() => {
            if (window.confirm(`Delete ${budget.name}? Its transactions keep no budget.`)) {
              remove.mutate();
            }
          }}
        >
          <Trash2 /> Delete
        </Button>
      </div>
    </form>
  );
}

/** One budget: its settings and its limits. */
export function BudgetDetailPage() {
  const { budgetId } = useParams({ from: "/app/budgets/$budgetId" });
  const budget = useQuery(budgetQuery(budgetId));
  return (
    <div>
      <PageHeader
        title={budget.data?.name ?? "Budget"}
        actions={
          <Button variant="outline" asChild>
            <Link to="/budgets">All budgets</Link>
          </Button>
        }
      />
      <QueryState isPending={budget.isPending} error={budget.error} />
      {budget.data ? (
        <>
          <BudgetSettings key={budget.data.updated_at} budget={budget.data} />
          <LimitEditor budgetId={budget.data.id} />
        </>
      ) : null}
    </div>
  );
}
