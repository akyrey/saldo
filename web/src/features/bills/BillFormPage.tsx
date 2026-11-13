import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams, useRouteContext } from "@tanstack/react-router";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input, Textarea } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { currenciesQuery } from "@/features/accounts/queries";
import { todayIso } from "@/lib/dates";
import {
  emptyBillForm,
  FREQUENCIES,
  fromBill,
  toCreateRequest,
  toPatch,
  type Bill,
  type BillForm,
} from "./bill-form";
import { billQuery } from "./queries";

function BillFormFields(props: {
  initial: BillForm;
  isNew: boolean;
  save: (form: BillForm) => Promise<{ data: Bill }>;
}) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [form, setForm] = useState(props.initial);
  const currencies = useQuery(currenciesQuery);
  const mutation = useMutation({
    mutationFn: () => props.save(form),
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: ["bills"] });
      await navigate({ to: "/bills/$billId", params: { billId: result.data.id } });
    },
  });
  const errors = fieldErrors(mutation.error);
  const set = <K extends keyof BillForm>(key: K, value: BillForm[K]) =>
    setForm((prev) => ({ ...prev, [key]: value }));
  const text = (key: keyof BillForm, label: string, type = "text", className?: string) => (
    <Field id={key} label={label} error={errors[key]} className={className}>
      <Input
        id={key}
        type={type}
        value={String(form[key])}
        onChange={(e) => set(key, e.target.value as never)}
        {...errorProps(key, errors[key])}
      />
    </Field>
  );

  return (
    <form
      className="grid max-w-2xl gap-4 sm:grid-cols-2"
      onSubmit={(event) => {
        event.preventDefault();
        mutation.mutate();
      }}
    >
      {text("name", "Name", "text", "sm:col-span-2")}
      {text("amount_min", "Minimum amount")}
      {text("amount_max", "Maximum amount")}
      <Field id="currency" label="Currency" error={errors.currency}>
        <NativeSelect
          id="currency"
          disabled={!props.isNew}
          value={form.currency}
          onChange={(e) => set("currency", e.target.value)}
        >
          {(currencies.data ?? [{ code: form.currency, name: form.currency }]).map((c) => (
            <option key={c.code} value={c.code}>
              {c.code} · {c.name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="repeat_freq" label="Repeats" error={errors.repeat_freq}>
        <NativeSelect
          id="repeat_freq"
          value={form.repeat_freq}
          onChange={(e) => set("repeat_freq", e.target.value)}
        >
          {FREQUENCIES.map((f) => (
            <option key={f} value={f}>
              {f.replace("_", " ")}
            </option>
          ))}
        </NativeSelect>
      </Field>
      {text("first_due_date", "First due date", "date")}
      {text("end_date", "End date (optional)", "date")}
      {text("skip", "Skip periods between due dates", "number")}
      {text("remind_days_before", "Remind days before", "number")}
      {text("remind_every_days", "Then remind every N days", "number")}
      <Field id="active" label="Active">
        <input
          id="active"
          type="checkbox"
          className="size-4"
          checked={form.active}
          onChange={(e) => set("active", e.target.checked)}
        />
      </Field>
      <Field id="notes" label="Notes" error={errors.notes} className="sm:col-span-2">
        <Textarea id="notes" value={form.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
      {mutation.error && Object.keys(errors).length === 0 ? (
        <Alert className="sm:col-span-2">{errorMessage(mutation.error)}</Alert>
      ) : null}
      <div className="sm:col-span-2">
        <Button type="submit" disabled={mutation.isPending}>
          {props.isNew ? "Create bill" : "Save"}
        </Button>
      </div>
    </form>
  );
}

export function NewBillPage() {
  const { me } = useRouteContext({ from: "/app" });
  return (
    <div>
      <PageHeader title="New bill" />
      <BillFormFields
        isNew
        initial={emptyBillForm(me.household.primary_currency, todayIso())}
        save={(form) => call(api.POST("/api/v1/bills", { body: toCreateRequest(form) }))}
      />
    </div>
  );
}

export function EditBillPage() {
  const { billId } = useParams({ from: "/app/bills/$billId/edit" });
  const bill = useQuery(billQuery(billId));
  return (
    <div>
      <PageHeader title="Edit bill" />
      <QueryState isPending={bill.isPending} error={bill.error} />
      {bill.data ? (
        <BillFormFields
          isNew={false}
          initial={fromBill(bill.data)}
          save={(form) =>
            call(
              api.PATCH("/api/v1/bills/{id}", {
                params: { path: { id: billId } },
                body: toPatch(bill.data, form),
              }),
            )
          }
        />
      ) : null}
    </div>
  );
}
