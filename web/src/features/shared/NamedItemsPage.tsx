import { useMutation, useQuery, useQueryClient, type UseQueryOptions } from "@tanstack/react-query";
import { Check, Merge, Pencil, Trash2, X } from "lucide-react";
import { useState, type ReactNode } from "react";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";

export interface NamedItem {
  id: string;
  name: string;
}

/** How one kind of named item (categories, tags) is loaded and changed. */
export interface NamedItemsConfig<T extends NamedItem> {
  title: string;
  singular: string;
  query: UseQueryOptions<T[], Error, T[], string[]> & { queryKey: string[] };
  create: (name: string) => Promise<unknown>;
  rename: (id: string, name: string) => Promise<unknown>;
  remove: (id: string) => Promise<unknown>;
  /** Shown before each item's name, for example a category's colour. */
  decorate?: (item: T) => ReactNode;
  /** Folds the item `sourceId` into `intoId`; offers "Merge into…" when set. */
  merge?: (sourceId: string, intoId: string) => Promise<unknown>;
  /** Extra links or buttons in the page header. */
  actions?: ReactNode;
}

interface MergeFormProps<T extends NamedItem> {
  item: T;
  others: readonly T[];
  onMerge: (intoId: string) => void;
  onCancel: () => void;
}

/** Picks the item that `item` is folded into. */
function MergeForm<T extends NamedItem>({ item, others, onMerge, onCancel }: MergeFormProps<T>) {
  const [into, setInto] = useState("");
  return (
    <form
      className="flex items-center gap-2 py-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (into) onMerge(into);
      }}
    >
      <NativeSelect
        aria-label={`Merge ${item.name} into`}
        value={into}
        onChange={(e) => setInto(e.target.value)}
        required
      >
        <option value="">Merge {item.name} into…</option>
        {others.map((other) => (
          <option key={other.id} value={other.id}>
            {other.name}
          </option>
        ))}
      </NativeSelect>
      <Button type="submit">Merge</Button>
      <Button type="button" size="icon" variant="ghost" aria-label="Cancel" onClick={onCancel}>
        <X />
      </Button>
    </form>
  );
}

function ItemRow<T extends NamedItem>(props: {
  item: T;
  singular: string;
  onRename: (name: string) => void;
  onRemove: () => void;
  busy: boolean;
  decoration?: ReactNode;
  /** The items this one can be merged into; omitted when merging is off. */
  mergeTargets?: readonly T[];
  onMerge: (intoId: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [merging, setMerging] = useState(false);
  const [name, setName] = useState(props.item.name);
  if (merging && props.mergeTargets) {
    return (
      <MergeForm
        item={props.item}
        others={props.mergeTargets}
        onCancel={() => setMerging(false)}
        onMerge={(intoId) => {
          props.onMerge(intoId);
          setMerging(false);
        }}
      />
    );
  }
  if (editing) {
    return (
      <form
        className="flex items-center gap-2 py-2"
        onSubmit={(event) => {
          event.preventDefault();
          props.onRename(name);
          setEditing(false);
        }}
      >
        <Input
          aria-label={`${props.singular} name`}
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
          onClick={() => setEditing(false)}
        >
          <X />
        </Button>
      </form>
    );
  }
  return (
    <div className="flex items-center justify-between gap-2 py-2">
      <span className="flex items-center gap-2">
        {props.decoration}
        {props.item.name}
      </span>
      <div className="flex gap-1">
        <Button
          size="icon"
          variant="ghost"
          aria-label={`Rename ${props.item.name}`}
          onClick={() => setEditing(true)}
        >
          <Pencil />
        </Button>
        {props.mergeTargets ? (
          <Button
            size="icon"
            variant="ghost"
            aria-label={`Merge ${props.item.name} into…`}
            disabled={props.busy}
            onClick={() => setMerging(true)}
          >
            <Merge />
          </Button>
        ) : null}
        <Button
          size="icon"
          variant="ghost"
          aria-label={`Delete ${props.item.name}`}
          disabled={props.busy}
          onClick={props.onRemove}
        >
          <Trash2 />
        </Button>
      </div>
    </div>
  );
}

/** A list of named items with create, rename and delete. */
export function NamedItemsPage<T extends NamedItem>({ config }: { config: NamedItemsConfig<T> }) {
  const queryClient = useQueryClient();
  const items = useQuery(config.query);
  const [name, setName] = useState("");
  const onSuccess = () => queryClient.invalidateQueries({ queryKey: config.query.queryKey });
  const create = useMutation({
    mutationFn: () => config.create(name.trim()),
    onSuccess: async () => {
      setName("");
      await onSuccess();
    },
  });
  const rename = useMutation({
    mutationFn: (input: { id: string; name: string }) => config.rename(input.id, input.name.trim()),
    onSuccess,
  });
  const remove = useMutation({ mutationFn: config.remove, onSuccess });
  const merge = useMutation({
    mutationFn: (input: { sourceId: string; intoId: string }) =>
      config.merge ? config.merge(input.sourceId, input.intoId) : Promise.resolve(),
    // A merge touches whatever used the item (transactions, rules, reports).
    onSuccess: () => queryClient.invalidateQueries(),
  });
  const error = create.error ?? rename.error ?? remove.error ?? merge.error;

  return (
    <div className="max-w-xl">
      <PageHeader title={config.title} actions={config.actions} />
      <form
        className="mb-4 flex gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          create.mutate();
        }}
      >
        <Input
          aria-label={`New ${config.singular} name`}
          placeholder={`New ${config.singular}`}
          required
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <Button type="submit" disabled={create.isPending}>
          Add
        </Button>
      </form>
      {error ? <Alert className="mb-4">{errorMessage(error)}</Alert> : null}
      <QueryState isPending={items.isPending} error={items.error} />
      {items.data?.length === 0 ? <p className="text-sm text-muted-foreground">None yet.</p> : null}
      <div className="divide-y">
        {items.data?.map((item) => (
          <ItemRow
            key={item.id}
            item={item}
            singular={config.singular}
            busy={remove.isPending || merge.isPending}
            decoration={config.decorate?.(item)}
            mergeTargets={config.merge ? items.data?.filter((i) => i.id !== item.id) : undefined}
            onMerge={(intoId) => merge.mutate({ sourceId: item.id, intoId })}
            onRename={(newName) => rename.mutate({ id: item.id, name: newName })}
            onRemove={() => remove.mutate(item.id)}
          />
        ))}
      </div>
    </div>
  );
}
