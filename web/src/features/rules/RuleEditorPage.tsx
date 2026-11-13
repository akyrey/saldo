import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { Field } from "@/components/Field";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Combobox } from "@/components/Combobox";
import { NativeSelect } from "@/components/ui/native-select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  ACTION_KINDS,
  emptyAction,
  emptyRuleForm,
  emptyTrigger,
  fromRule,
  toRequest,
  TRIGGER_KINDS,
  type ActionRow,
  type Rule,
  type RuleForm,
  type TriggerRow,
} from "./rule-form";
import { ruleGroupsQuery, ruleQuery } from "./queries";
import { ValueInput } from "./ValueInput";

/** Keyword options, plus the stored keyword if Saldo does not know it (imported). */
function kindOptions(known: readonly { kind: string; label: string }[], current: string) {
  const options = known.map((k) => ({ value: k.kind, label: k.label }));
  return options.some((o) => o.value === current)
    ? options
    : [...options, { value: current, label: `${current} (unsupported)` }];
}

function Check(props: {
  id: string;
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label htmlFor={props.id} className="flex items-center gap-2 text-sm">
      <input
        id={props.id}
        type="checkbox"
        className="size-4"
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
      />
      {props.label}
    </label>
  );
}

function RuleFormFields(props: {
  initial: RuleForm;
  isNew: boolean;
  save: (form: RuleForm) => Promise<{ data: Rule }>;
}) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const groups = useQuery(ruleGroupsQuery);
  const [form, setForm] = useState(props.initial);
  const mutation = useMutation({
    mutationFn: () => props.save(form),
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: ["rules"] });
      await navigate({ to: "/rules/$ruleId", params: { ruleId: result.data.id } });
    },
  });
  const errors = fieldErrors(mutation.error);
  const set = <K extends keyof RuleForm>(key: K, value: RuleForm[K]) =>
    setForm((prev) => ({ ...prev, [key]: value }));
  const setTrigger = (i: number, patch: Partial<TriggerRow>) =>
    set(
      "triggers",
      form.triggers.map((t, j) => (j === i ? { ...t, ...patch } : t)),
    );
  const setAction = (i: number, patch: Partial<ActionRow>) =>
    set(
      "actions",
      form.actions.map((a, j) => (j === i ? { ...a, ...patch } : a)),
    );

  return (
    <form
      className="grid max-w-3xl gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        mutation.mutate();
      }}
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <Field id="title" label="Title" error={errors.title}>
          <Input
            id="title"
            required
            value={form.title}
            onChange={(e) => set("title", e.target.value)}
          />
        </Field>
        <Field id="group" label="Group" error={errors.rule_group_id}>
          <Combobox
            id="group"
            label="Group"
            value={form.rule_group_id}
            onChange={(id) => set("rule_group_id", id)}
            options={(groups.data ?? []).map((g) => ({ value: g.id, label: g.title }))}
          />
        </Field>
        <Field id="trigger_on" label="Runs when a transaction is" error={errors.trigger_on}>
          <NativeSelect
            id="trigger_on"
            value={form.trigger_on}
            onChange={(e) => set("trigger_on", e.target.value)}
          >
            <option value="create">created</option>
            <option value="update">updated</option>
            <option value="manual">never (apply by hand)</option>
          </NativeSelect>
        </Field>
        <div className="flex flex-col justify-end gap-2">
          <Check
            id="strict"
            label="All triggers must match"
            checked={form.strict}
            onChange={(v) => set("strict", v)}
          />
          <Check
            id="stop"
            label="Stop the group's later rules"
            checked={form.stop_processing}
            onChange={(v) => set("stop_processing", v)}
          />
          <Check
            id="active"
            label="Active"
            checked={form.active}
            onChange={(v) => set("active", v)}
          />
        </div>
      </div>

      <fieldset className="grid gap-2">
        <legend className="mb-1 font-medium">Triggers</legend>
        {errors.triggers ? <p className="text-xs text-destructive">{errors.triggers}</p> : null}
        {form.triggers.map((t, i) => (
          <div key={i} className="grid gap-2 sm:grid-cols-[1fr_1fr_auto_auto] sm:items-center">
            <Combobox
              aria-label={`Trigger ${i + 1} kind`}
              value={t.kind}
              onChange={(kind) => setTrigger(i, { kind, value: "" })}
              options={kindOptions(TRIGGER_KINDS, t.kind)}
            />
            <ValueInput
              id={`trigger-${i}`}
              label={`Trigger ${i + 1} value`}
              kind={t.kind}
              value={t.value}
              onChange={(value) => setTrigger(i, { value })}
            />
            <Check
              id={`negate-${i}`}
              label="not"
              checked={t.negate}
              onChange={(negate) => setTrigger(i, { negate })}
            />
            <Button
              type="button"
              size="icon"
              variant="ghost"
              aria-label={`Remove trigger ${i + 1}`}
              onClick={() =>
                set(
                  "triggers",
                  form.triggers.filter((_, j) => j !== i),
                )
              }
            >
              <Trash2 />
            </Button>
            {errors[`triggers[${i}]`] ? (
              <p className="text-xs text-destructive sm:col-span-4">{errors[`triggers[${i}]`]}</p>
            ) : null}
          </div>
        ))}
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="justify-self-start"
          onClick={() => set("triggers", [...form.triggers, emptyTrigger()])}
        >
          <Plus /> Add trigger
        </Button>
      </fieldset>

      <fieldset className="grid gap-2">
        <legend className="mb-1 font-medium">Actions</legend>
        {form.actions.map((a, i) => (
          <div key={i} className="grid gap-2 sm:grid-cols-[1fr_1fr_auto_auto] sm:items-center">
            <Combobox
              aria-label={`Action ${i + 1} kind`}
              value={a.kind}
              onChange={(kind) => setAction(i, { kind, value: "" })}
              options={kindOptions(ACTION_KINDS, a.kind)}
            />
            <ValueInput
              id={`action-${i}`}
              label={`Action ${i + 1} value`}
              kind={a.kind}
              value={a.value}
              onChange={(value) => setAction(i, { value })}
            />
            <Check
              id={`stop-${i}`}
              label="then stop"
              checked={a.stop_processing}
              onChange={(stop_processing) => setAction(i, { stop_processing })}
            />
            <Button
              type="button"
              size="icon"
              variant="ghost"
              aria-label={`Remove action ${i + 1}`}
              onClick={() =>
                set(
                  "actions",
                  form.actions.filter((_, j) => j !== i),
                )
              }
            >
              <Trash2 />
            </Button>
            {errors[`actions[${i}]`] ? (
              <p className="text-xs text-destructive sm:col-span-4">{errors[`actions[${i}]`]}</p>
            ) : null}
          </div>
        ))}
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="justify-self-start"
          onClick={() => set("actions", [...form.actions, emptyAction()])}
        >
          <Plus /> Add action
        </Button>
      </fieldset>

      {mutation.error && Object.keys(errors).length === 0 ? (
        <Alert>{errorMessage(mutation.error)}</Alert>
      ) : null}
      <div>
        <Button type="submit" disabled={mutation.isPending}>
          {props.isNew ? "Create rule" : "Save"}
        </Button>
      </div>
    </form>
  );
}

/** "Test rule": the transactions the saved rule would change, nothing saved. */
function TestPreview({ ruleId }: { ruleId: string }) {
  const test = useMutation({
    mutationFn: async () =>
      (await call(api.POST("/api/v1/rules/{id}/test", { params: { path: { id: ruleId } } }))).data,
  });
  return (
    <section className="mt-8 grid gap-2" aria-label="Test">
      <div>
        <Button variant="outline" onClick={() => test.mutate()} disabled={test.isPending}>
          Test rule
        </Button>
      </div>
      {test.error ? <Alert>{errorMessage(test.error)}</Alert> : null}
      {test.data ? (
        <>
          <p className="text-sm text-muted-foreground">
            {test.data.total} matching split{test.data.total === 1 ? "" : "s"}
            {test.data.total > test.data.matches.length
              ? ` (showing ${test.data.matches.length})`
              : ""}
          </p>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Date</TableHead>
                <TableHead>Description</TableHead>
                <TableHead className="text-right">Amount</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {test.data.matches.map((m) => (
                <TableRow key={m.split_id}>
                  <TableCell>{m.date}</TableCell>
                  <TableCell>{m.description}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {m.amount} {m.currency}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </>
      ) : null}
    </section>
  );
}

export function NewRulePage() {
  const { group } = useSearch({ from: "/app/rules/new" });
  const groups = useQuery(ruleGroupsQuery);
  const groupId = group ?? groups.data?.[0]?.id;
  return (
    <div>
      <PageHeader title="New rule" />
      <QueryState isPending={!groupId && groups.isPending} error={groups.error} />
      {groupId ? (
        <RuleFormFields
          isNew
          initial={emptyRuleForm(groupId)}
          save={(form) => call(api.POST("/api/v1/rules", { body: toRequest(form) }))}
        />
      ) : groups.data ? (
        <Alert>Create a rule group first.</Alert>
      ) : null}
    </div>
  );
}

export function EditRulePage() {
  const { ruleId } = useParams({ from: "/app/rules/$ruleId" });
  const rule = useQuery(ruleQuery(ruleId));
  return (
    <div>
      <PageHeader title={rule.data?.title ?? "Rule"} />
      <QueryState isPending={rule.isPending} error={rule.error} />
      {rule.data ? (
        <RuleFormFields
          key={rule.data.updated_at}
          isNew={false}
          initial={fromRule(rule.data)}
          save={(form) =>
            call(
              api.PUT("/api/v1/rules/{id}", {
                params: { path: { id: ruleId } },
                body: toRequest(form),
              }),
            )
          }
        />
      ) : null}
      <TestPreview ruleId={ruleId} />
    </div>
  );
}
