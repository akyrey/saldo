import { useMutation, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { type KeyboardEvent, useId, useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Input } from "@/components/ui/input";
import { tagsQuery, type Tag } from "@/features/tags/queries";
import { cn } from "@/lib/utils";
import { tagSuggestions } from "./tag-suggestions";

/** At most this many suggestions while typing. */
const SUGGESTION_LIMIT = 8;

interface TagPickerProps {
  id: string;
  tags: readonly Tag[];
  value: readonly string[];
  onChange: (next: string[]) => void;
}

/** One suggestion row: an existing tag, or "create this tag". */
type Choice = { kind: "tag"; tag: Tag } | { kind: "create"; name: string };

/**
 * Chosen tags as chips plus a search field (a combobox): typing suggests
 * matching tags, and a name that does not exist yet can be created on the spot.
 */
export function TagPicker({ id, tags, value, onChange }: TagPickerProps) {
  const queryClient = useQueryClient();
  const listId = useId();
  const [text, setText] = useState("");
  const [active, setActive] = useState(0);
  const [created, setCreated] = useState<Tag[]>([]);
  const known = [...tags, ...created.filter((c) => !tags.some((t) => t.id === c.id))];

  const create = useMutation({
    mutationFn: async (name: string) =>
      (await call(api.POST("/api/v1/tags", { body: { name } }))).data,
    onSuccess: (tag) => {
      setCreated((list) => [...list, tag]);
      queryClient.setQueryData(tagsQuery.queryKey, (old: Tag[] | undefined) => [
        ...(old ?? []),
        tag,
      ]);
      add(tag.id);
    },
  });

  const add = (tagId: string) => {
    if (!value.includes(tagId)) onChange([...value, tagId]);
    setText("");
    setActive(0);
  };
  const remove = (tagId: string) => onChange(value.filter((t) => t !== tagId));

  const { matches, create: newName } = tagSuggestions(known, value, text, SUGGESTION_LIMIT);
  const choices: Choice[] = [
    ...matches.map((tag): Choice => ({ kind: "tag", tag })),
    ...(newName ? [{ kind: "create", name: newName } as Choice] : []),
  ];
  const choose = (choice: Choice) =>
    choice.kind === "tag" ? add(choice.tag.id) : create.mutate(choice.name);

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((i) => (choices.length === 0 ? 0 : (i + step + choices.length) % choices.length));
    } else if (event.key === "Enter" && choices.length > 0) {
      event.preventDefault(); // pick, do not submit the transaction
      const choice = choices[Math.min(active, choices.length - 1)];
      if (choice) choose(choice);
    } else if (event.key === "Escape") {
      setText("");
    } else if (event.key === "Backspace" && text === "" && value.length > 0) {
      remove(value[value.length - 1] as string);
    }
  };

  const nameOf = (tagId: string) => known.find((t) => t.id === tagId)?.name ?? "…";
  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center gap-1 rounded-md border p-1">
        {value.map((tagId) => (
          <span
            key={tagId}
            className="flex items-center gap-1 rounded-full bg-primary px-2 py-0.5 text-xs text-primary-foreground"
          >
            {nameOf(tagId)}
            <button
              type="button"
              aria-label={`Remove tag ${nameOf(tagId)}`}
              onClick={() => remove(tagId)}
              className="rounded-full hover:bg-primary-foreground/20"
            >
              <X className="size-3" aria-hidden />
            </button>
          </span>
        ))}
        <Input
          id={id}
          role="combobox"
          aria-label="Add tag"
          aria-expanded={choices.length > 0}
          aria-controls={listId}
          aria-autocomplete="list"
          autoComplete="off"
          placeholder={value.length === 0 ? "Type to search or create a tag" : ""}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
          className="h-7 min-w-32 flex-1 border-0 px-1 shadow-none focus-visible:ring-0"
        />
      </div>
      {choices.length > 0 ? (
        <ul
          id={listId}
          role="listbox"
          aria-label="Tag suggestions"
          className="rounded-md border p-1"
        >
          {choices.map((choice, index) => (
            <li
              key={choice.kind === "tag" ? choice.tag.id : `create:${choice.name}`}
              role="option"
              aria-selected={index === active}
              // `mousedown` keeps the focus in the field while choosing.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(choice)}
              className={cn(
                "cursor-pointer rounded px-2 py-1 text-sm",
                index === active && "bg-accent",
              )}
            >
              {choice.kind === "tag" ? choice.tag.name : `Create tag “${choice.name}”`}
            </li>
          ))}
        </ul>
      ) : null}
      {create.error ? (
        <p className="text-xs text-destructive">{errorMessage(create.error)}</p>
      ) : null}
    </div>
  );
}
