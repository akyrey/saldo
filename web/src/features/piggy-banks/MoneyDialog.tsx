import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { errorMessage, fieldErrors } from "@/api/errors";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import type { Account } from "@/features/accounts/queries";
import { todayIso } from "@/lib/dates";
import { addPiggyBankEvent, type PiggyBank } from "./queries";
import { signedAmount, type Direction } from "./progress";

interface MoneyDialogProps {
  piggyBank: PiggyBank;
  direction: Direction;
  accounts: readonly Account[];
  onClose: () => void;
}

/** Adds money to or removes it from a piggy bank. */
export function MoneyDialog({ piggyBank, direction, accounts, onClose }: MoneyDialogProps) {
  const queryClient = useQueryClient();
  const linked = piggyBank.accounts;
  const [accountId, setAccountId] = useState(linked[0]?.account_id ?? "");
  const [amount, setAmount] = useState("");
  const [date, setDate] = useState(todayIso);
  const title = `${direction === "add" ? "Add money to" : "Remove money from"} ${piggyBank.name}`;
  const save = useMutation({
    mutationFn: () =>
      addPiggyBankEvent(piggyBank.id, {
        account_id: accountId || null,
        amount: signedAmount(direction, amount),
        date,
      }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["piggy-banks"] });
      onClose();
    },
  });
  const errors = fieldErrors(save.error);
  const nameOf = (id: string) => accounts.find((a) => a.id === id)?.name ?? id;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={title}
      className="fixed inset-0 z-20 flex items-end justify-center bg-black/40 p-4 sm:items-center"
    >
      <form
        className="flex w-full max-w-sm flex-col gap-3 rounded-lg border bg-background p-4 shadow-lg"
        onSubmit={(event) => {
          event.preventDefault();
          save.mutate();
        }}
      >
        <h2 className="font-semibold">{title}</h2>
        {linked.length > 1 ? (
          <Field id="money-account" label="Account" error={errors.account_id}>
            <NativeSelect
              id="money-account"
              value={accountId}
              onChange={(e) => setAccountId(e.target.value)}
            >
              {linked.map((a) => (
                <option key={a.account_id} value={a.account_id}>
                  {nameOf(a.account_id)} ({a.saved_amount} saved)
                </option>
              ))}
            </NativeSelect>
          </Field>
        ) : null}
        <Field id="money-amount" label={`Amount (${piggyBank.currency})`} error={errors.amount}>
          <Input
            id="money-amount"
            inputMode="decimal"
            autoComplete="off"
            required
            autoFocus
            placeholder="0.00"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            {...errorProps("money-amount", errors.amount)}
          />
        </Field>
        <Field id="money-date" label="Date" error={errors.date}>
          <Input
            id="money-date"
            type="date"
            required
            value={date}
            onChange={(e) => setDate(e.target.value)}
          />
        </Field>
        {save.error && Object.keys(errors).length === 0 ? (
          <Alert>{errorMessage(save.error)}</Alert>
        ) : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={save.isPending}>
            {direction === "add" ? "Add" : "Remove"}
          </Button>
        </div>
      </form>
    </div>
  );
}
