import { useQueries } from "@tanstack/react-query";
import { Minus, Plus } from "lucide-react";
import { useState } from "react";
import { Amount } from "@/components/Amount";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Button } from "@/components/ui/button";
import {
  accountsQuery,
  currenciesQuery,
  placesOf,
  type Account,
} from "@/features/accounts/queries";
import { MoneyDialog } from "./MoneyDialog";
import { NewPiggyBankForm } from "./NewPiggyBankForm";
import { progressPercent, type Direction } from "./progress";
import { piggyBanksQuery, type PiggyBank } from "./queries";

function ProgressBar({ percent, label }: { percent: number; label: string }) {
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent}
      className="h-2 overflow-hidden rounded-full bg-muted"
    >
      <div className="h-full bg-primary" style={{ width: `${percent}%` }} />
    </div>
  );
}

interface CardProps {
  piggyBank: PiggyBank;
  places: number;
  accounts: readonly Account[];
  onMoney: (direction: Direction) => void;
}

function PiggyBankCard({ piggyBank, places, accounts, onMoney }: CardProps) {
  const percent = progressPercent(piggyBank.saved_amount, piggyBank.target_amount, places);
  const names = piggyBank.accounts
    .map((a) => accounts.find((account) => account.id === a.account_id)?.name ?? a.account_id)
    .join(", ");
  return (
    <li className="flex flex-col gap-2 rounded-lg border p-4" aria-label={piggyBank.name}>
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="font-medium">{piggyBank.name}</h2>
        <p className="text-sm">
          <Amount value={piggyBank.saved_amount} currency={piggyBank.currency} />
          {piggyBank.target_amount ? (
            <>
              {" of "}
              <Amount value={piggyBank.target_amount} currency={piggyBank.currency} />
            </>
          ) : null}
        </p>
      </div>
      {percent !== null ? (
        <ProgressBar percent={percent} label={`${piggyBank.name} progress`} />
      ) : null}
      <p className="text-xs text-muted-foreground">
        {percent !== null ? `${percent}% · ` : ""}
        {piggyBank.target_date ? `by ${piggyBank.target_date} · ` : ""}
        in {names}
      </p>
      <div className="flex gap-2">
        <Button size="sm" variant="outline" onClick={() => onMoney("add")}>
          <Plus /> Add money
        </Button>
        <Button size="sm" variant="outline" onClick={() => onMoney("remove")}>
          <Minus /> Remove money
        </Button>
      </div>
    </li>
  );
}

export function PiggyBanksPage() {
  const [piggyBanks, accounts, currencies] = useQueries({
    queries: [piggyBanksQuery, accountsQuery(), currenciesQuery],
  });
  const [creating, setCreating] = useState(false);
  const [money, setMoney] = useState<{ id: string; direction: Direction } | null>(null);
  const error = piggyBanks.error ?? accounts.error;
  const assets = (accounts.data ?? []).filter((a) => a.type === "asset" && a.active);
  const selected = piggyBanks.data?.find((p) => p.id === money?.id);

  return (
    <div>
      <PageHeader
        title="Piggy banks"
        actions={
          creating ? null : (
            <Button size="sm" onClick={() => setCreating(true)} disabled={!accounts.data}>
              <Plus /> New piggy bank
            </Button>
          )
        }
      />
      {creating ? <NewPiggyBankForm accounts={assets} onDone={() => setCreating(false)} /> : null}
      <QueryState isPending={piggyBanks.isPending} error={error} />
      {piggyBanks.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">No piggy banks yet.</p>
      ) : null}
      <ul className="grid gap-3 md:grid-cols-2">
        {piggyBanks.data?.map((piggyBank) => (
          <PiggyBankCard
            key={piggyBank.id}
            piggyBank={piggyBank}
            places={placesOf(currencies.data, piggyBank.currency)}
            accounts={accounts.data ?? []}
            onMoney={(direction) => setMoney({ id: piggyBank.id, direction })}
          />
        ))}
      </ul>
      {selected && money ? (
        <MoneyDialog
          piggyBank={selected}
          direction={money.direction}
          accounts={accounts.data ?? []}
          onClose={() => setMoney(null)}
        />
      ) : null}
    </div>
  );
}
