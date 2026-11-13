import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { Archive, ArchiveRestore, Pencil, Trash2 } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Amount } from "@/components/Amount";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { membersQuery } from "@/features/auth/queries";
import { categoriesQuery } from "@/features/categories/queries";
import { byId } from "@/features/shared/lookup";
import { pageCount } from "@/features/shared/pagination";
import { Pager } from "@/features/transactions/Pager";
import { transactionsQuery } from "@/features/transactions/queries";
import { TransactionTable } from "@/features/transactions/TransactionTable";
import { ownerLabel, TYPE_LABELS } from "./grouping";
import { accountBalanceQuery, accountQuery, accountsQuery } from "./queries";

export function AccountDetailPage() {
  const { accountId } = useParams({ from: "/app/accounts/$accountId" });
  const { page } = useSearch({ from: "/app/accounts/$accountId" });
  const navigate = useNavigate({ from: "/accounts/$accountId" });
  const queryClient = useQueryClient();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const account = useQuery(accountQuery(accountId));
  const balance = useQuery(accountBalanceQuery(accountId));
  const members = useQuery(membersQuery);
  const accounts = useQuery(accountsQuery());
  const categories = useQuery(categoriesQuery);
  const transactions = useQuery(transactionsQuery({ account_id: accountId, page }));

  const invalidate = () =>
    Promise.all([
      queryClient.invalidateQueries({ queryKey: ["accounts"] }),
      queryClient.invalidateQueries({ queryKey: ["balances"] }),
    ]);
  const archive = useMutation({
    mutationFn: (active: boolean) =>
      call(
        api.PATCH("/api/v1/accounts/{id}", {
          params: { path: { id: accountId } },
          body: { active },
        }),
      ),
    onSuccess: invalidate,
  });
  const remove = useMutation({
    mutationFn: () =>
      call(api.DELETE("/api/v1/accounts/{id}", { params: { path: { id: accountId } } })),
    onSuccess: async () => {
      queryClient.removeQueries({ queryKey: ["accounts", "one", accountId] });
      await invalidate();
      await navigate({ to: "/accounts" });
    },
  });

  if (!account.data) return <QueryState isPending={account.isPending} error={account.error} />;
  const data = account.data;
  const isEquity = data.type === "equity";

  return (
    <div>
      <PageHeader
        title={data.name}
        actions={
          isEquity ? null : (
            <>
              <Button variant="outline" size="sm" asChild>
                <Link to="/accounts/$accountId/edit" params={{ accountId }}>
                  <Pencil /> Edit
                </Link>
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={archive.isPending}
                onClick={() => archive.mutate(!data.active)}
              >
                {data.active ? <Archive /> : <ArchiveRestore />}
                {data.active ? "Archive" : "Unarchive"}
              </Button>
              <Button
                variant={confirmDelete ? "destructive" : "outline"}
                size="sm"
                disabled={remove.isPending}
                onClick={() => (confirmDelete ? remove.mutate() : setConfirmDelete(true))}
              >
                <Trash2 /> {confirmDelete ? "Really delete?" : "Delete"}
              </Button>
            </>
          )
        }
      />
      {archive.error || remove.error ? (
        <Alert className="mb-4">{errorMessage(archive.error ?? remove.error)}</Alert>
      ) : null}
      <dl className="mb-6 grid grid-cols-2 gap-3 rounded-lg border p-4 text-sm sm:grid-cols-4">
        <div>
          <dt className="text-muted-foreground">Balance</dt>
          <dd className="text-lg font-semibold" data-testid="account-balance">
            {balance.data ? (
              <Amount value={balance.data.balance} currency={balance.data.currency} />
            ) : (
              "…"
            )}
          </dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Type</dt>
          <dd>{TYPE_LABELS[data.type] ?? data.type}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Owner</dt>
          <dd>{ownerLabel(data.owner_user_id, members.data)}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Status</dt>
          <dd>{data.active ? "Active" : <Badge>archived</Badge>}</dd>
        </div>
        {data.iban ? (
          <div className="col-span-2">
            <dt className="text-muted-foreground">IBAN</dt>
            <dd className="font-mono">{data.iban}</dd>
          </div>
        ) : null}
        {data.notes ? (
          <div className="col-span-2">
            <dt className="text-muted-foreground">Notes</dt>
            <dd className="whitespace-pre-wrap">{data.notes}</dd>
          </div>
        ) : null}
      </dl>
      <h2 className="mb-2 font-semibold">Transactions</h2>
      <QueryState isPending={transactions.isPending} error={transactions.error} />
      {transactions.data ? (
        <>
          <TransactionTable
            groups={transactions.data.data}
            accounts={byId(accounts.data)}
            categories={byId(categories.data)}
            perspectiveAccountId={accountId}
          />
          <Pager
            page={transactions.data.meta.page}
            pages={pageCount(transactions.data.meta)}
            onPage={(next) =>
              void navigate({ search: (prev) => ({ ...prev, page: next > 1 ? next : undefined }) })
            }
          />
        </>
      ) : null}
    </div>
  );
}
