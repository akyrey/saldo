import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { useState } from "react";
import { ApiError, errorMessage } from "@/api/errors";
import { sumAmounts, normalizeAmountInput } from "@/api/money";
import { Field } from "@/components/Field";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  accountsQuery,
  currenciesQuery,
  placesOf,
  type Account,
} from "@/features/accounts/queries";
import { categoriesQuery } from "@/features/categories/queries";
import { tagsQuery } from "@/features/tags/queries";
import { todayIso } from "@/lib/dates";
import {
  addSplit,
  mapServerErrors,
  removeSplit,
  toRequest,
  updateSplit,
  type EditorErrors,
  type EditorState,
} from "./editor-model";
import type { TransactionRequest } from "./queries";
import { SplitRowFields } from "./SplitRowFields";

const NO_ERRORS: EditorErrors = { rows: {}, form: [] };

interface TransactionEditorProps {
  initial: EditorState;
  submitLabel: string;
  save: (body: TransactionRequest) => Promise<unknown>;
  onSaved: () => void;
}

/** The currency the amount of `row` is in: the asset or liability side's. */
function rowCurrency(accounts: readonly Account[], sourceId: string, destinationId: string) {
  const sides = [sourceId, destinationId].map((id) => accounts.find((a) => a.id === id));
  return sides.find((a) => a && (a.type === "asset" || a.type === "liability"))?.currency;
}

/** A group with one or more splits; each split has its own accounts. */
export function TransactionEditor({ initial, submitLabel, save, onSaved }: TransactionEditorProps) {
  const queryClient = useQueryClient();
  const [state, setState] = useState(initial);
  const [errors, setErrors] = useState<EditorErrors>(NO_ERRORS);
  const [accounts, categories, tags, currencies] = useQueries({
    queries: [accountsQuery(), categoriesQuery, tagsQuery, currenciesQuery],
  });

  const mutation = useMutation({
    mutationFn: () => save(toRequest(state)),
    onMutate: () => setErrors(NO_ERRORS),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["transactions"] }),
        queryClient.invalidateQueries({ queryKey: ["balances"] }),
      ]);
      onSaved();
    },
    onError: (error) => {
      if (error instanceof ApiError && error.details.length > 0) {
        setErrors(mapServerErrors(error.details));
      } else {
        setErrors({ rows: {}, form: [errorMessage(error)] });
      }
    },
  });

  const loadError = accounts.error ?? categories.error ?? tags.error ?? currencies.error;
  if (!accounts.data || loadError) {
    return <QueryState isPending={accounts.isPending} error={loadError} />;
  }
  const accountList = accounts.data;
  const currencyCodes = new Set(
    state.splits.map((row) => rowCurrency(accountList, row.source_id, row.destination_id)),
  );
  const [onlyCurrency] = currencyCodes;
  const total =
    currencyCodes.size === 1 && onlyCurrency
      ? sumAmounts(
          state.splits.map((row) => normalizeAmountInput(row.amount)),
          placesOf(currencies.data, onlyCurrency),
        )
      : undefined;

  return (
    <form
      className="flex flex-col gap-4"
      noValidate={false}
      onSubmit={(event) => {
        event.preventDefault();
        mutation.mutate();
      }}
    >
      <Field id="title" label="Title (optional, for groups with several splits)">
        <Input
          id="title"
          value={state.title}
          onChange={(e) => setState((s) => ({ ...s, title: e.target.value }))}
        />
      </Field>
      {state.splits.map((row, index) => (
        <SplitRowFields
          key={row.key}
          index={index}
          row={row}
          errors={errors.rows[index]}
          accounts={accountList}
          categories={categories.data ?? []}
          tags={tags.data ?? []}
          currencies={currencies.data ?? []}
          canRemove={state.splits.length > 1}
          onChange={(changes) => setState((s) => updateSplit(s, index, changes))}
          onRemove={() => {
            setState((s) => removeSplit(s, index));
            setErrors(NO_ERRORS);
          }}
        />
      ))}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Button
          type="button"
          variant="outline"
          onClick={() => setState((s) => addSplit(s, todayIso()))}
        >
          <Plus /> Add split
        </Button>
        {total !== undefined && onlyCurrency ? (
          <p className="text-sm text-muted-foreground" data-testid="editor-total">
            Total: {total} {onlyCurrency}
          </p>
        ) : null}
      </div>
      {errors.form.length > 0 ? (
        <Alert>
          {errors.form.map((message) => (
            <p key={message}>{message}</p>
          ))}
        </Alert>
      ) : null}
      {Object.keys(errors.rows).length > 0 ? (
        <Alert>Some splits need attention; see the messages above.</Alert>
      ) : null}
      <div>
        <Button type="submit" disabled={mutation.isPending}>
          {mutation.isPending ? "Saving…" : submitLabel}
        </Button>
      </div>
    </form>
  );
}
