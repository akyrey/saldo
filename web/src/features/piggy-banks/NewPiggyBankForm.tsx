import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { errorMessage, fieldErrors } from "@/api/errors";
import { normalizeAmountInput } from "@/api/money";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { Account } from "@/features/accounts/queries";
import { createPiggyBank } from "./queries";

interface NewPiggyBankFormProps {
  /** Active asset accounts, the only ones a piggy bank can use. */
  accounts: readonly Account[];
  onDone: () => void;
}

/** Name, optional target and the linked accounts. */
export function NewPiggyBankForm({ accounts, onDone }: NewPiggyBankFormProps) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [target, setTarget] = useState("");
  const [targetDate, setTargetDate] = useState("");
  const [linked, setLinked] = useState<string[]>(() => accounts.slice(0, 1).map((a) => a.id));
  const create = useMutation({
    mutationFn: () =>
      createPiggyBank({
        name,
        target_amount: target.trim() === "" ? null : normalizeAmountInput(target),
        target_date: targetDate || null,
        account_ids: linked,
      }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["piggy-banks"] });
      onDone();
    },
  });
  const errors = fieldErrors(create.error);
  const toggle = (id: string) =>
    setLinked((current) =>
      current.includes(id) ? current.filter((x) => x !== id) : [...current, id],
    );

  return (
    <form
      aria-label="New piggy bank"
      className="mb-4 grid gap-3 rounded-lg border p-4 sm:grid-cols-3"
      onSubmit={(event) => {
        event.preventDefault();
        create.mutate();
      }}
    >
      <Field id="piggy-name" label="Name" error={errors.name}>
        <Input
          id="piggy-name"
          required
          value={name}
          onChange={(e) => setName(e.target.value)}
          {...errorProps("piggy-name", errors.name)}
        />
      </Field>
      <Field id="piggy-target" label="Target (optional)" error={errors.target_amount}>
        <Input
          id="piggy-target"
          inputMode="decimal"
          autoComplete="off"
          value={target}
          onChange={(e) => setTarget(e.target.value)}
          {...errorProps("piggy-target", errors.target_amount)}
        />
      </Field>
      <Field id="piggy-target-date" label="Target date (optional)" error={errors.target_date}>
        <Input
          id="piggy-target-date"
          type="date"
          value={targetDate}
          onChange={(e) => setTargetDate(e.target.value)}
        />
      </Field>
      <fieldset className="sm:col-span-3">
        <legend className="mb-1 text-sm font-medium">Saved in</legend>
        <div className="flex flex-wrap gap-3">
          {accounts.map((account) => (
            <label key={account.id} className="flex items-center gap-1.5 text-sm">
              <input
                type="checkbox"
                checked={linked.includes(account.id)}
                onChange={() => toggle(account.id)}
              />
              {account.name}
            </label>
          ))}
        </div>
        {errors.account_ids ? (
          <p className="mt-1 text-xs text-destructive">{errors.account_ids}</p>
        ) : null}
      </fieldset>
      {create.error && Object.keys(errors).length === 0 ? (
        <Alert className="sm:col-span-3">{errorMessage(create.error)}</Alert>
      ) : null}
      <div className="flex gap-2 sm:col-span-3">
        <Button type="submit" disabled={create.isPending}>
          Create piggy bank
        </Button>
        <Button type="button" variant="outline" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
