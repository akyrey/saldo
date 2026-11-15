import { useMutation, useQuery, useQueryClient, type UseQueryOptions } from "@tanstack/react-query";
import { Check, Pencil, Trash2, X } from "lucide-react";
import { useState, type ReactNode } from "react";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

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
}

function ItemRow<T extends NamedItem>(props: {
  item: T;
  singular: string;
  onRename: (name: string) => void;
  onRemove: () => void;
  busy: boolean;
  decoration?: ReactNode;
}) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(props.item.name);
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
  const error = create.error ?? rename.error ?? remove.error;

  return (
    <div className="max-w-xl">
      <PageHeader title={config.title} />
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
            busy={remove.isPending}
            decoration={config.decorate?.(item)}
            onRename={(newName) => rename.mutate({ id: item.id, name: newName })}
            onRemove={() => remove.mutate(item.id)}
          />
        ))}
      </div>
    </div>
  );
}
