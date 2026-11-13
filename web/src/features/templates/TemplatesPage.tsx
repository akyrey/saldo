import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowDown, ArrowUp, Check, Pencil, Trash2, X } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { KIND_LABELS } from "@/features/transactions/editor-model";
import type { SplitKind } from "@/features/transactions/search";
import { templatesQuery, type Template } from "./queries";
import { moveChanges, type OrderChange } from "./template-model";

type Patch = { id: string; name?: string; sort_order?: number };

function patchTemplate({ id, ...body }: Patch) {
  return call(api.PATCH("/api/v1/transaction-templates/{id}", { params: { path: { id } }, body }));
}

/** What the template fills in, in a few words: `Coffee · 2.50 EUR`. */
function summary(template: Template): string {
  const [first] = template.splits;
  if (!first) return "Empty";
  const amount = first.amount ? `${first.amount}${first.currency ? ` ${first.currency}` : ""}` : "";
  const parts = [first.description ?? "", amount].filter((p) => p !== "");
  const more = template.splits.length > 1 ? ` (+${template.splits.length - 1} splits)` : "";
  return (parts.join(" · ") || "No description") + more;
}

interface TemplateRowProps {
  template: Template;
  busy: boolean;
  isFirst: boolean;
  isLast: boolean;
  onRename: (name: string) => void;
  onMove: (delta: number) => void;
  onRemove: () => void;
}

function RenameForm(props: { name: string; onSave: (name: string) => void; onCancel: () => void }) {
  const [name, setName] = useState(props.name);
  return (
    <form
      className="flex flex-1 items-center gap-2"
      onSubmit={(event) => {
        event.preventDefault();
        props.onSave(name.trim());
      }}
    >
      <Input
        aria-label="Template name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        autoFocus
      />
      <Button type="submit" size="icon" variant="ghost" aria-label="Save">
        <Check />
      </Button>
      <Button
        type="button"
        size="icon"
        variant="ghost"
        aria-label="Cancel"
        onClick={props.onCancel}
      >
        <X />
      </Button>
    </form>
  );
}

function TemplateRow({ template, busy, isFirst, isLast, ...on }: TemplateRowProps) {
  const [editing, setEditing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const kind = KIND_LABELS[template.kind as SplitKind] ?? template.kind;
  if (editing) {
    return (
      <li className="py-2">
        <RenameForm
          name={template.name}
          onCancel={() => setEditing(false)}
          onSave={(name) => {
            on.onRename(name);
            setEditing(false);
          }}
        />
      </li>
    );
  }
  return (
    <li
      className="flex flex-wrap items-center justify-between gap-2 py-2"
      aria-label={template.name}
    >
      <div className="min-w-0">
        <Link
          to="/transactions/new"
          search={{ template: template.id }}
          className="font-medium hover:underline"
        >
          {template.name}
        </Link>{" "}
        <Badge>{kind}</Badge>
        <p className="truncate text-sm text-muted-foreground">{summary(template)}</p>
      </div>
      <div className="flex gap-1">
        <Button
          size="icon"
          variant="ghost"
          aria-label={`Move ${template.name} up`}
          disabled={busy || isFirst}
          onClick={() => on.onMove(-1)}
        >
          <ArrowUp />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          aria-label={`Move ${template.name} down`}
          disabled={busy || isLast}
          onClick={() => on.onMove(1)}
        >
          <ArrowDown />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          aria-label={`Rename ${template.name}`}
          onClick={() => setEditing(true)}
        >
          <Pencil />
        </Button>
        <Button
          size="icon"
          variant={confirming ? "destructive" : "ghost"}
          aria-label={confirming ? `Really delete ${template.name}?` : `Delete ${template.name}`}
          disabled={busy}
          onClick={() => (confirming ? on.onRemove() : setConfirming(true))}
        >
          <Trash2 />
        </Button>
      </div>
    </li>
  );
}

/** Templates: use one (opens the editor), rename, reorder, delete. */
export function TemplatesPage() {
  const queryClient = useQueryClient();
  const templates = useQuery(templatesQuery);
  const onSuccess = () => queryClient.invalidateQueries({ queryKey: ["templates"] });
  const rename = useMutation({ mutationFn: patchTemplate, onSuccess });
  const reorder = useMutation({
    // One PATCH per template whose position changes, one after the other.
    mutationFn: async (changes: OrderChange[]) => {
      for (const change of changes) await patchTemplate(change);
    },
    onSettled: onSuccess,
  });
  const remove = useMutation({
    mutationFn: (id: string) =>
      call(api.DELETE("/api/v1/transaction-templates/{id}", { params: { path: { id } } })),
    onSuccess,
  });
  const error = rename.error ?? reorder.error ?? remove.error;
  const list = templates.data ?? [];

  return (
    <div className="max-w-2xl">
      <PageHeader title="Templates" />
      <p className="mb-4 text-sm text-muted-foreground">
        Save a template from a new transaction&apos;s form or from an existing transaction. Pick a
        template to open the editor filled in from it.
      </p>
      {error ? <Alert className="mb-4">{errorMessage(error)}</Alert> : null}
      <QueryState isPending={templates.isPending} error={templates.error} />
      {templates.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">None yet.</p>
      ) : null}
      <ul className="divide-y">
        {list.map((template, index) => (
          <TemplateRow
            key={template.id}
            template={template}
            busy={reorder.isPending || remove.isPending}
            isFirst={index === 0}
            isLast={index === list.length - 1}
            onRename={(name) => rename.mutate({ id: template.id, name })}
            onMove={(delta) => reorder.mutate(moveChanges(list, index, delta))}
            onRemove={() => remove.mutate(template.id)}
          />
        ))}
      </ul>
    </div>
  );
}
