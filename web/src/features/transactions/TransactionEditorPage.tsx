import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useRouter, useSearch } from "@tanstack/react-router";
import { Copy, Trash2 } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { todayIso } from "@/lib/dates";
import { templateQuery } from "@/features/templates/queries";
import { SaveAsTemplate } from "@/features/templates/SaveAsTemplate";
import { TemplatePicker } from "@/features/templates/TemplatePicker";
import { emptySplit, fromGroup, type EditorState } from "./editor-model";
import { duplicateOf, fromTemplate } from "./prefill";
import { transactionQuery } from "./queries";
import type { NewTransactionSearch } from "./search";
import { TransactionEditor } from "./TransactionEditor";

/** The editor's first state: blank, from a template, or a copy of a group. */
function usePrefill(search: NewTransactionSearch) {
  const [today] = useState(todayIso);
  const [blank] = useState<EditorState>(() => ({ title: "", splits: [emptySplit(today)] }));
  const template = useQuery({
    ...templateQuery(search.template ?? ""),
    enabled: search.template !== undefined,
  });
  const group = useQuery({
    ...transactionQuery(search.duplicate ?? ""),
    enabled: search.duplicate !== undefined,
  });
  if (search.template !== undefined) {
    const initial = template.data ? fromTemplate(template.data, today) : undefined;
    const title = template.data ? `New from “${template.data.name}”` : "New from template";
    return { initial, title, isPending: template.isPending, error: template.error };
  }
  if (search.duplicate !== undefined) {
    const initial = group.data ? duplicateOf(group.data, today) : undefined;
    return {
      initial,
      title: "Duplicate transaction",
      isPending: group.isPending,
      error: group.error,
    };
  }
  return { initial: blank, title: "New transaction", isPending: false, error: null };
}

export function NewTransactionPage() {
  const search = useSearch({ from: "/app/transactions/new" });
  const navigate = useNavigate();
  const prefill = usePrefill(search);
  return (
    <div>
      <PageHeader title={prefill.title} actions={<TemplatePicker value={search.template} />} />
      <QueryState isPending={prefill.isPending} error={prefill.error} />
      {prefill.initial ? (
        <TransactionEditor
          // A different template or copy starts a fresh editor.
          key={search.template ?? search.duplicate ?? "blank"}
          initial={prefill.initial}
          submitLabel="Create transaction"
          allowTemplate
          save={(body) => call(api.POST("/api/v1/transactions", { body }))}
          onSaved={() => void navigate({ to: "/transactions" })}
        />
      ) : null}
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
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" size="sm" asChild>
              <Link to="/transactions/new" search={{ duplicate: groupId }}>
                <Copy /> Duplicate
              </Link>
            </Button>
            {group.data ? (
              <SaveAsTemplate
                groupId={groupId}
                suggestion={group.data.title ?? group.data.splits[0]?.description ?? ""}
              />
            ) : null}
            <Button
              variant={confirming ? "destructive" : "outline"}
              size="sm"
              disabled={remove.isPending || !group.data}
              onClick={() => (confirming ? remove.mutate() : setConfirming(true))}
            >
              <Trash2 /> {confirming ? "Really delete?" : "Delete"}
            </Button>
          </div>
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
