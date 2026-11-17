import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Undo2 } from "lucide-react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Amount } from "@/components/Amount";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { reconciliationsQuery } from "./queries";

const AFFECTED = [["reconciliations"], ["transactions"], ["balances"], ["accounts"], ["reports"]];

/** "Reconciled up to …" and the account's reconciliations; the latest can be undone. */
export function ReconciliationHistory({ accountId }: { accountId: string }) {
  const queryClient = useQueryClient();
  const history = useQuery(reconciliationsQuery(accountId));
  const undo = useMutation({
    mutationFn: (id: string) =>
      call(api.DELETE("/api/v1/reconciliations/{id}", { params: { path: { id } } })),
    onSuccess: () =>
      Promise.all(AFFECTED.map((queryKey) => queryClient.invalidateQueries({ queryKey }))),
  });
  const items = history.data?.data ?? [];
  const [latest] = items;
  if (!latest) return null;
  return (
    <section aria-label="Reconciliations" className="mb-6 flex flex-col gap-2">
      <h2 className="font-semibold">Reconciled up to {latest.statement_date}</h2>
      {undo.error ? <Alert>{errorMessage(undo.error)}</Alert> : null}
      <ul className="divide-y rounded-md border text-sm">
        {items.map((r) => (
          <li key={r.id} className="flex flex-wrap items-center justify-between gap-2 px-3 py-2">
            <span>
              {r.statement_date}: <Amount value={r.statement_balance} currency={r.currency} />
              <span className="text-muted-foreground"> · {r.split_ids.length} transactions</span>
              {r.difference_group_id ? (
                <span className="text-muted-foreground">
                  {" "}
                  · difference <Amount value={r.difference} currency={r.currency} />
                </span>
              ) : null}
            </span>
            {r.id === latest.id ? (
              <Button
                variant="ghost"
                size="sm"
                disabled={undo.isPending}
                onClick={() => undo.mutate(r.id)}
              >
                <Undo2 /> Undo
              </Button>
            ) : null}
          </li>
        ))}
      </ul>
    </section>
  );
}
