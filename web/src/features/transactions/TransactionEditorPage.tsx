import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams, useRouter } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { todayIso } from "@/lib/dates";
import { emptySplit, fromGroup } from "./editor-model";
import { transactionQuery } from "./queries";
import { TransactionEditor } from "./TransactionEditor";

export function NewTransactionPage() {
  const navigate = useNavigate();
  const [initial] = useState(() => ({ title: "", splits: [emptySplit(todayIso())] }));
  return (
    <div>
      <PageHeader title="New transaction" />
      <TransactionEditor
        initial={initial}
        submitLabel="Create transaction"
        save={(body) => call(api.POST("/api/v1/transactions", { body }))}
        onSaved={() => void navigate({ to: "/transactions" })}
      />
    </div>
  );
}

export function EditTransactionPage() {
  const { groupId } = useParams({ from: "/app/transactions/$groupId/edit" });
  const router = useRouter();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const group = useQuery(transactionQuery(groupId));
  const [confirming, setConfirming] = useState(false);
  const remove = useMutation({
    mutationFn: () =>
      call(api.DELETE("/api/v1/transactions/{id}", { params: { path: { id: groupId } } })),
    onSuccess: async () => {
      queryClient.removeQueries({ queryKey: ["transactions", "one", groupId] });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["transactions"] }),
        queryClient.invalidateQueries({ queryKey: ["balances"] }),
      ]);
      await navigate({ to: "/transactions" });
    },
  });

  return (
    <div>
      <PageHeader
        title="Edit transaction"
        actions={
          <Button
            variant={confirming ? "destructive" : "outline"}
            size="sm"
            disabled={remove.isPending || !group.data}
            onClick={() => (confirming ? remove.mutate() : setConfirming(true))}
          >
            <Trash2 /> {confirming ? "Really delete?" : "Delete"}
          </Button>
        }
      />
      {remove.error ? <Alert className="mb-4">{errorMessage(remove.error)}</Alert> : null}
      <QueryState isPending={group.isPending} error={group.error} />
      {group.data ? (
        <TransactionEditor
          key={group.data.updated_at}
          initial={fromGroup(group.data)}
          submitLabel="Save changes"
          save={(body) =>
            call(api.PUT("/api/v1/transactions/{id}", { params: { path: { id: groupId } }, body }))
          }
          onSaved={() => router.history.back()}
        />
      ) : null}
    </div>
  );
}
