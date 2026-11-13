import { useQuery } from "@tanstack/react-query";
import { Link, useSearch } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useState } from "react";
import { Amount } from "@/components/Amount";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { membersQuery } from "@/features/auth/queries";
import { groupAccounts, ownerLabel, TYPE_LABELS } from "./grouping";
import { accountsQuery, balancesQuery, currenciesQuery } from "./queries";

export function AccountsPage() {
  const { scope } = useSearch({ from: "/app" });
  const [showArchived, setShowArchived] = useState(false);
  const accounts = useQuery(accountsQuery(scope));
  const balances = useQuery(balancesQuery(scope));
  const currencies = useQuery(currenciesQuery);
  const members = useQuery(membersQuery);

  const visible = (accounts.data ?? []).filter((a) => showArchived || a.active);
  const groups = groupAccounts(visible, balances.data ?? new Map(), currencies.data);

  return (
    <div>
      <PageHeader
        title="Accounts"
        actions={
          <>
            <Button variant="outline" size="sm" onClick={() => setShowArchived((v) => !v)}>
              {showArchived ? "Hide archived" : "Show archived"}
            </Button>
            <Button size="sm" asChild>
              <Link to="/accounts/new">
                <Plus /> New account
              </Link>
            </Button>
          </>
        }
      />
      <QueryState isPending={accounts.isPending} error={accounts.error ?? balances.error} />
      {accounts.data && groups.length === 0 ? (
        <p className="text-sm text-muted-foreground">No accounts yet.</p>
      ) : null}
      <div className="grid gap-4 lg:grid-cols-2">
        {groups.map((group) => (
          <Card key={group.type} data-testid={`group-${group.type}`}>
            <CardHeader className="flex-row items-baseline justify-between">
              <CardTitle>{TYPE_LABELS[group.type] ?? group.type}</CardTitle>
              <div className="flex flex-col items-end text-sm">
                {group.totals.map((total) => (
                  <Amount key={total.currency} value={total.amount} currency={total.currency} />
                ))}
              </div>
            </CardHeader>
            <CardContent>
              <ul className="divide-y">
                {group.accounts.map((account) => {
                  const balance = balances.data?.get(account.id);
                  return (
                    <li key={account.id} className="flex items-center justify-between gap-2 py-2">
                      <div className="min-w-0">
                        <Link
                          to="/accounts/$accountId"
                          params={{ accountId: account.id }}
                          className="font-medium hover:underline"
                        >
                          {account.name}
                        </Link>
                        <div className="flex gap-1 text-xs text-muted-foreground">
                          {ownerLabel(account.owner_user_id, members.data)}
                          {account.active ? null : <Badge>archived</Badge>}
                        </div>
                      </div>
                      {balance ? (
                        <Amount
                          value={balance.balance}
                          currency={balance.currency}
                          className="text-sm"
                          data-testid={`balance-${account.name}`}
                        />
                      ) : null}
                    </li>
                  );
                })}
              </ul>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}
