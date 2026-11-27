import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bookmark, Check, Pencil, RefreshCw, Trash2, X } from "lucide-react";
import { type FormEvent, type ReactNode, useState } from "react";
import { errorMessage } from "@/api/errors";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  NONE,
  parseTransactionSearch,
  sameFilters,
  type TransactionSearch,
  viewFilters,
} from "@/features/transactions/search";
import { cn } from "@/lib/utils";
import {
  createSavedView,
  deleteSavedView,
  type SavedView,
  savedViewsQuery,
  updateSavedView,
} from "./queries";

/** The fixed first chip: withdrawals and deposits still waiting for a category. */
const UNCATEGORISED: TransactionSearch = { category_id: NONE };

interface SavedViewChipsProps {
  filters: TransactionSearch;
  /** Replaces every filter with these (the scope stays); `{}` clears them. */
  onApply: (filters: TransactionSearch) => void;
}

type Editing = { mode: "new" } | { mode: "rename"; view: SavedView } | null;

/** Chips above the transaction list: one click restores a saved set of filters. */
export function SavedViewChips({ filters, onApply }: SavedViewChipsProps) {
  const client = useQueryClient();
  const views = useQuery(savedViewsQuery);
  const [editing, setEditing] = useState<Editing>(null);
  const [name, setName] = useState("");
  const refresh = () => client.invalidateQueries({ queryKey: ["saved-views"] });
  const save = useMutation({
    mutationFn: async (input: { name: string; editing: NonNullable<Editing> }) =>
      input.editing.mode === "new"
        ? createSavedView(input.name, viewFilters(filters))
        : updateSavedView(input.editing.view.id, { name: input.name }),
    onSuccess: async () => {
      setEditing(null);
      await refresh();
    },
  });
  const update = useMutation({
    mutationFn: (view: SavedView) => updateSavedView(view.id, { filters: viewFilters(filters) }),
    onSuccess: refresh,
  });
  const remove = useMutation({ mutationFn: deleteSavedView, onSuccess: refresh });
  const error = save.error ?? update.error ?? remove.error;

  const list = views.data ?? [];
  const active = list.find((v) => sameFilters(parseTransactionSearch(v.filters), filters));
  const hasFilters = Object.keys(viewFilters(filters)).length > 0;
  // Clicking the chip that is already active toggles it off and clears the filters.
  const toggle = (isActive: boolean, next: TransactionSearch) => onApply(isActive ? {} : next);
  const uncategorisedActive = sameFilters(UNCATEGORISED, filters);

  const startEditing = (next: NonNullable<Editing>) => {
    save.reset();
    setName(next.mode === "rename" ? next.view.name : "");
    setEditing(next);
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (editing) save.mutate({ name: name.trim(), editing });
  };

  return (
    <div className="mb-3">
      <nav aria-label="Saved views" className="flex flex-wrap items-center gap-2">
        <Chip
          active={uncategorisedActive}
          onClick={() => toggle(uncategorisedActive, UNCATEGORISED)}
        >
          Uncategorised
        </Chip>
        {list.map((view) => (
          <Chip
            key={view.id}
            active={view.id === active?.id}
            onClick={() => toggle(view.id === active?.id, parseTransactionSearch(view.filters))}
          >
            {view.name}
          </Chip>
        ))}
        {hasFilters && !active && !editing ? (
          <Button variant="ghost" size="sm" onClick={() => startEditing({ mode: "new" })}>
            <Bookmark /> Save view…
          </Button>
        ) : null}
        {active && !editing ? (
          <>
            <Button
              variant="ghost"
              size="sm"
              aria-label={`Rename ${active.name}`}
              onClick={() => startEditing({ mode: "rename", view: active })}
            >
              <Pencil />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              aria-label={`Delete ${active.name}`}
              onClick={() => remove.mutate(active.id)}
            >
              <Trash2 />
            </Button>
          </>
        ) : null}
        {!active && hasFilters && list.length > 0 && !editing ? (
          <UpdateMenu views={list} onPick={(view) => update.mutate(view)} />
        ) : null}
      </nav>
      {editing ? (
        <form onSubmit={submit} className="mt-2 flex max-w-md items-center gap-2">
          <Input
            aria-label="View name"
            placeholder="Name of this view"
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Button type="submit" size="sm" disabled={name.trim() === "" || save.isPending}>
            <Check /> Save
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={() => setEditing(null)}>
            <X /> Cancel
          </Button>
        </form>
      ) : null}
      {error ? <Alert className="mt-2">{errorMessage(error)}</Alert> : null}
    </div>
  );
}

function Chip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "rounded-full border px-3 py-1 text-sm hover:bg-accent",
        active && "border-primary bg-primary/10 font-medium text-primary",
      )}
    >
      {children}
    </button>
  );
}

/** "Update a view to these filters": a select, so a long list stays compact. */
function UpdateMenu({
  views,
  onPick,
}: {
  views: readonly SavedView[];
  onPick: (view: SavedView) => void;
}) {
  return (
    <label className="flex items-center gap-1 text-sm text-muted-foreground">
      <RefreshCw className="size-4" aria-hidden />
      <span className="sr-only">Update a saved view to the current filters</span>
      <select
        className="rounded-md border bg-background px-2 py-1 text-sm"
        value=""
        onChange={(e) => {
          const view = views.find((v) => v.id === e.target.value);
          if (view) onPick(view);
        }}
      >
        <option value="">Update view…</option>
        {views.map((view) => (
          <option key={view.id} value={view.id}>
            {view.name}
          </option>
        ))}
      </select>
    </label>
  );
}
