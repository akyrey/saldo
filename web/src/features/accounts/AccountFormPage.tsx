import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams, useRouteContext } from "@tanstack/react-router";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { NotesField } from "@/components/notes/NotesField";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Combobox } from "@/components/Combobox";
import { NativeSelect } from "@/components/ui/native-select";
import { currencyOptions } from "@/features/shared/options";
import { membersQuery } from "@/features/auth/queries";
import {
  ASSET_ROLES,
  CREATABLE_TYPES,
  emptyAccountForm,
  fromAccount,
  LIABILITY_DIRECTIONS,
  LIABILITY_TYPES,
  toCreateRequest,
  toPatch,
  type AccountForm,
} from "./account-form";
import { TYPE_LABELS } from "./grouping";
import { accountQuery, currenciesQuery } from "./queries";

const label = (value: string) => value.replaceAll("_", " ");

function AccountFormFields(props: {
  initial: AccountForm;
  isNew: boolean;
  submitLabel: string;
  save: (form: AccountForm) => Promise<{ data: { id: string } }>;
}) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [form, setForm] = useState(props.initial);
  const currencies = useQuery(currenciesQuery);
  const members = useQuery(membersQuery);
  const mutation = useMutation({
    mutationFn: () => props.save(form),
    onSuccess: async (result) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["accounts"] }),
        queryClient.invalidateQueries({ queryKey: ["balances"] }),
      ]);
      await navigate({ to: "/accounts/$accountId", params: { accountId: result.data.id } });
    },
  });
  const errors = fieldErrors(mutation.error);
  const set = <K extends keyof AccountForm>(key: K, value: AccountForm[K]) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  return (
    <form
      className="grid max-w-2xl gap-4 sm:grid-cols-2"
      onSubmit={(event) => {
        event.preventDefault();
        mutation.mutate();
      }}
    >
      <Field id="name" label="Name" error={errors.name} className="sm:col-span-2">
        <Input
          id="name"
          required
          value={form.name}
          onChange={(e) => set("name", e.target.value)}
          {...errorProps("name", errors.name)}
        />
      </Field>
      <Field id="type" label="Type" error={errors.type}>
        <NativeSelect
          id="type"
          disabled={!props.isNew}
          value={form.type}
          onChange={(e) => set("type", e.target.value)}
        >
          {(props.isNew ? CREATABLE_TYPES : [form.type]).map((type) => (
            <option key={type} value={type}>
              {TYPE_LABELS[type] ?? type}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="currency" label="Currency" error={errors.currency}>
        <Combobox
          id="currency"
          label="Currency"
          disabled={!props.isNew}
          value={form.currency}
          onChange={(code) => set("currency", code)}
          options={currencyOptions(
            currencies.data ?? [{ code: form.currency, name: form.currency }],
          )}
        />
      </Field>
      <Field id="owner" label="Owner" error={errors.owner_user_id}>
        <NativeSelect
          id="owner"
          value={form.owner_user_id}
          onChange={(e) => set("owner_user_id", e.target.value)}
        >
          <option value="">Joint</option>
          {(members.data ?? []).map((m) => (
            <option key={m.user_id} value={m.user_id}>
              {m.display_name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      {form.type === "asset" ? (
        <Field id="asset_role" label="Role" error={errors.asset_role}>
          <NativeSelect
            id="asset_role"
            value={form.asset_role}
            onChange={(e) => set("asset_role", e.target.value)}
          >
            <option value="">None</option>
            {ASSET_ROLES.map((r) => (
              <option key={r} value={r}>
                {label(r)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      ) : null}
      {form.type === "liability" ? (
        <>
          <Field id="liability_type" label="Liability type" error={errors.liability_type}>
            <NativeSelect
              id="liability_type"
              value={form.liability_type}
              onChange={(e) => set("liability_type", e.target.value)}
            >
              <option value="">None</option>
              {LIABILITY_TYPES.map((t) => (
                <option key={t} value={t}>
                  {label(t)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field id="liability_direction" label="Direction" error={errors.liability_direction}>
            <NativeSelect
              id="liability_direction"
              value={form.liability_direction}
              onChange={(e) => set("liability_direction", e.target.value)}
            >
              <option value="">None</option>
              {LIABILITY_DIRECTIONS.map((d) => (
                <option key={d} value={d}>
                  {label(d)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        </>
      ) : null}
      <Field id="iban" label="IBAN" error={errors.iban}>
        <Input id="iban" value={form.iban} onChange={(e) => set("iban", e.target.value)} />
      </Field>
      <Field id="account_number" label="Account number" error={errors.account_number}>
        <Input
          id="account_number"
          value={form.account_number}
          onChange={(e) => set("account_number", e.target.value)}
        />
      </Field>
      <Field id="notes" label="Notes" error={errors.notes} className="sm:col-span-2">
        <NotesField id="notes" value={form.notes} onChange={(notes) => set("notes", notes)} />
      </Field>
      <label className="flex items-center gap-2 text-sm sm:col-span-2">
        <input
          type="checkbox"
          checked={form.include_net_worth}
          onChange={(e) => set("include_net_worth", e.target.checked)}
        />
        Include in net worth
      </label>
      {mutation.error && Object.keys(errors).length === 0 ? (
        <Alert className="sm:col-span-2">{errorMessage(mutation.error)}</Alert>
      ) : null}
      <div className="sm:col-span-2">
        <Button type="submit" disabled={mutation.isPending}>
          {mutation.isPending ? "Saving…" : props.submitLabel}
        </Button>
      </div>
    </form>
  );
}

export function NewAccountPage() {
  const { me } = useRouteContext({ from: "/app" });
  const [initial] = useState(() => emptyAccountForm(me.household.primary_currency));
  return (
    <div>
      <PageHeader title="New account" />
      <AccountFormFields
        initial={initial}
        isNew
        submitLabel="Create account"
        save={(form) => call(api.POST("/api/v1/accounts", { body: toCreateRequest(form) }))}
      />
    </div>
  );
}

export function EditAccountPage() {
  const { accountId } = useParams({ from: "/app/accounts/$accountId/edit" });
  const account = useQuery(accountQuery(accountId));
  return (
    <div>
      <PageHeader title="Edit account" />
      <QueryState isPending={account.isPending} error={account.error} />
      {account.data ? (
        <AccountFormFields
          key={account.data.updated_at}
          initial={fromAccount(account.data)}
          isNew={false}
          submitLabel="Save changes"
          save={(form) =>
            call(
              api.PATCH("/api/v1/accounts/{id}", {
                params: { path: { id: accountId } },
                body: toPatch(form),
              }),
            )
          }
        />
      ) : null}
    </div>
  );
}
