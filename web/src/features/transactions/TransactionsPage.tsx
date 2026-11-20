import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Button } from "@/components/ui/button";
import { accountsQuery } from "@/features/accounts/queries";
import { billsQuery } from "@/features/bills/queries";
import { budgetsQuery } from "@/features/budgets/queries";
import { categoriesQuery } from "@/features/categories/queries";
import { SavedViewChips } from "@/features/saved-views/SavedViewChips";
import { byId } from "@/features/shared/lookup";
import { pageCount } from "@/features/shared/pagination";
import { tagsQuery } from "@/features/tags/queries";
import { TemplatePicker } from "@/features/templates/TemplatePicker";
import { Pager } from "./Pager";
import { transactionsQuery } from "./queries";
import { TransactionFilters } from "./TransactionFilters";
import { TransactionTable } from "./TransactionTable";

export function TransactionsPage() {
  const search = useSearch({ from: "/app/transactions" });
  const { scope } = useSearch({ from: "/app" });
  const navigate = useNavigate({ from: "/transactions" });
  const { scope: _scope, ...filters } = search;
  const list = useQuery(transactionsQuery(filters, scope));
  const accounts = useQuery(accountsQuery());
  const categories = useQuery(categoriesQuery);
  const tags = useQuery(tagsQuery);
  const budgets = useQuery(budgetsQuery);
  const bills = useQuery(billsQuery);

  return (
    <div>
      <PageHeader
        title="Transactions"
        actions={
          <div className="flex flex-wrap items-center gap-2">
            {/* Phones get it on the new-transaction page instead, to keep the header short. */}
            <div className="hidden sm:block">
              <TemplatePicker />
            </div>
            <Button size="sm" asChild>
              <Link to="/transactions/new">
                <Plus /> New transaction
              </Link>
            </Button>
          </div>
        }
      />
      <SavedViewChips
        filters={filters}
        onApply={(next) => void navigate({ search: (prev) => ({ scope: prev.scope, ...next }) })}
      />
      <TransactionFilters
        key={filters.q ?? ""}
        value={filters}
        accounts={accounts.data ?? []}
        categories={categories.data ?? []}
        tags={tags.data ?? []}
        budgets={budgets.data ?? []}
        bills={bills.data ?? []}
        onChange={(changes) =>
          void navigate({ search: (prev) => ({ ...prev, ...changes, page: undefined }) })
        }
      />
      <QueryState isPending={list.isPending} error={list.error} />
      {list.data ? (
        <>
          <p className="mb-2 text-sm text-muted-foreground">{list.data.meta.total} transactions</p>
          <TransactionTable
            groups={list.data.data}
            accounts={byId(accounts.data)}
            categories={byId(categories.data)}
          />
          <Pager
            page={list.data.meta.page}
            pages={pageCount(list.data.meta)}
            onPage={(page) =>
              void navigate({ search: (prev) => ({ ...prev, page: page > 1 ? page : undefined }) })
            }
          />
        </>
      ) : null}
    </div>
  );
}
