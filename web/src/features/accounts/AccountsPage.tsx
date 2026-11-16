import { useQuery } from "@tanstack/react-query";
import { Link, useSearch } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useState } from "react";
import { PageHeader } from "@/components/PageHeader";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { membersQuery } from "@/features/auth/queries";
import { useDebounced } from "@/lib/use-debounced";
import { AccountSearchResults, AccountSections, type SectionContext } from "./AccountSections";
import { currenciesQuery } from "./queries";

/** Wait this long after the last key press before searching. */
const SEARCH_DEBOUNCE_MS = 250;

export function AccountsPage() {
  const { scope } = useSearch({ from: "/app" });
  const [showArchived, setShowArchived] = useState(false);
  const [search, setSearch] = useState("");
  const q = useDebounced(search.trim(), SEARCH_DEBOUNCE_MS);
  const currencies = useQuery(currenciesQuery);
  const members = useQuery(membersQuery);

  const context: SectionContext = {
    scope,
    active: showArchived ? undefined : true,
    members: members.data,
    currencies: currencies.data,
  };

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
      <Input
        type="search"
        aria-label="Search accounts"
        placeholder="Search accounts"
        value={search}
        onChange={(event) => setSearch(event.target.value)}
        className="mb-4 sm:max-w-sm"
      />
      {q ? <AccountSearchResults q={q} context={context} /> : <AccountSections context={context} />}
    </div>
  );
}
