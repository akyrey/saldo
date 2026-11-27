import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { useState } from "react";
import { explain } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { accountsQuery } from "@/features/accounts/queries";
import { IMPORT_AFFECTS, importAction, importQuery, type LinePatch, updateLine } from "./queries";
import { ReviewLines } from "./ReviewLines";
import { ReviewNotes } from "./ReviewNotes";
import { actionCounts, reconcileSearch } from "./review-model";

/** Review a statement's lines, then commit, discard or later revert it. */
export function StatementReviewPage() {
  const { importId } = useParams({ from: "/app/statement-imports/$importId" });
  const queryClient = useQueryClient();
  const detail = useQuery(importQuery(importId));
  const accounts = useQuery(accountsQuery()).data ?? [];
  const [confirm, setConfirm] = useState<"discard" | "revert" | null>(null);
  const refresh = () =>
    Promise.all(IMPORT_AFFECTS.map((queryKey) => queryClient.invalidateQueries({ queryKey })));
  const act = useMutation({
    mutationFn: (action: "commit" | "discard" | "revert") => importAction(importId, action),
    onSuccess: async () => {
      setConfirm(null);
      await refresh();
    },
  });
  const edit = useMutation({
    mutationFn: ({ position, patch }: { position: number; patch: LinePatch }) =>
      updateLine(importId, position, patch),
    // An old commit error no longer applies once a line changes.
    onMutate: () => act.reset(),
    onSuccess: (line) => {
      // Show the saved line at once, so the select does not snap back.
      queryClient.setQueryData(importQuery(importId).queryKey, (old) =>
        old
          ? { ...old, lines: old.lines.map((l) => (l.position === line.position ? line : l)) }
          : old,
      );
    },
  });
  const run = (action: "commit" | "discard" | "revert") => {
    edit.reset();
    act.mutate(action);
  };
  const busy = act.isPending || edit.isPending;

  if (!detail.data) return <QueryState isPending={detail.isPending} error={detail.error} />;
  const { import: view, lines } = detail.data;
  const draft = view.status === "draft";
  const counts = actionCounts(lines);
  const reconcile = view.status === "committed" ? reconcileSearch(view) : null;
  const accountName = accounts.find((a) => a.id === view.account_id)?.name ?? "the account";
  // Compared as text: amounts never go through floats.
  const gap = view.gap && !/^-?0+(\.0+)?$/.test(view.gap) ? view.gap : null;
  const error = edit.error ?? act.error;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={`Statement ${view.filename}`}
        actions={
          <Button variant="outline" size="sm" asChild>
            <Link to="/accounts/$accountId" params={{ accountId: view.account_id }}>
              Back to {accountName}
            </Link>
          </Button>
        }
      />
      <p className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
        <Badge>{view.status}</Badge>
        {view.opening_balance ? <span>Opening {view.opening_balance}</span> : null}
        {view.closing_balance ? (
          <span>
            Closing {view.closing_balance} on {view.closing_date}
          </span>
        ) : null}
      </p>
      {gap ? (
        <Alert>
          The file&apos;s balances do not add up (off by {gap} {view.currency}): it may be missing
          lines or contain pending ones.
        </Alert>
      ) : null}
      <ReviewLines
        lines={lines}
        currency={view.currency}
        accountId={view.account_id}
        accounts={accounts}
        editable={draft && !act.isPending}
        onChange={(position, patch) => edit.mutate({ position, patch })}
      />
      <ReviewNotes summary={view.summary} currency={view.currency} />
      {error ? <Alert>{explain(error)}</Alert> : null}
      <div className="flex flex-wrap items-center gap-2">
        {draft ? (
          <>
            <Button disabled={busy} onClick={() => run("commit")}>
              Commit: {counts.create} to create, {counts.link} to link, {counts.skip} to skip
            </Button>
            <Button
              variant={confirm === "discard" ? "destructive" : "outline"}
              disabled={busy}
              onBlur={() => setConfirm(null)}
              onClick={() => (confirm === "discard" ? run("discard") : setConfirm("discard"))}
            >
              {confirm === "discard" ? "Really discard?" : "Discard"}
            </Button>
          </>
        ) : null}
        {reconcile ? (
          <Button asChild>
            <Link
              to="/accounts/$accountId/reconcile"
              params={{ accountId: view.account_id }}
              search={reconcile}
            >
              Reconcile now
            </Link>
          </Button>
        ) : null}
        {view.status === "committed" ? (
          <Button
            variant={confirm === "revert" ? "destructive" : "outline"}
            disabled={busy}
            onBlur={() => setConfirm(null)}
            onClick={() => (confirm === "revert" ? run("revert") : setConfirm("revert"))}
          >
            {confirm === "revert" ? "Really revert?" : "Revert import"}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
