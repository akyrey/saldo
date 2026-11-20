import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouteContext } from "@tanstack/react-router";
import { type FormEvent, useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Field } from "@/components/Field";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";

/** The household's large-transaction amount (owner only); empty turns the alert off. */
export function LargeTransactionCard() {
  const { me } = useRouteContext({ from: "/app" });
  const queryClient = useQueryClient();
  const current = me.household.large_transaction_amount ?? "";
  const [text, setText] = useState(current);
  const save = useMutation({
    mutationFn: (amount: string | null) =>
      call(api.PATCH("/api/v1/household", { body: { large_transaction_amount: amount } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["me"] }),
  });
  if (me.role !== "owner") return null;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const amount = text.trim().replace(",", ".");
    save.mutate(amount === "" ? null : amount);
  };
  return (
    <Card>
      <CardHeader>
        <CardTitle>Large transactions</CardTitle>
        <CardDescription>
          Channels with “Large transaction” chosen hear about every split at or above this amount (
          {me.household.primary_currency}). Leave it empty to turn the alert off.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={submit} className="flex items-end gap-2">
          <Field id="large-amount" label="Amount" className="max-w-40">
            <Input
              id="large-amount"
              inputMode="decimal"
              placeholder="Off"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </Field>
          <Button type="submit" disabled={save.isPending}>
            Save
          </Button>
        </form>
        {save.isSuccess ? (
          <p className="mt-2 text-sm" role="status">
            Saved.
          </p>
        ) : null}
        {save.error ? <Alert className="mt-2">{errorMessage(save.error)}</Alert> : null}
      </CardContent>
    </Card>
  );
}
