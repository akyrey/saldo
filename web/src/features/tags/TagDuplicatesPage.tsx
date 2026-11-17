import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { groupKey, plannedMerges, type DuplicateGroup } from "./duplicates";
import { duplicatesQuery, mergeTags } from "./queries";

const groups = (n: number) => `${n} ${n === 1 ? "group" : "groups"}`;
const uses = (n: number) => `${n} ${n === 1 ? "transaction" : "transactions"}`;

interface GroupProps {
  group: DuplicateGroup;
  kept: string;
  included: boolean;
  onKeep: (tagId: string) => void;
  onInclude: (included: boolean) => void;
}

/** One set of same-named tags: which to keep, and whether to merge at all. */
function GroupFields({ group, kept, included, onKeep, onInclude }: GroupProps) {
  const label = group.tags[0]?.name.toLocaleLowerCase() ?? "";
  return (
    <fieldset className="py-3" aria-label={label}>
      <div className="mb-1 flex items-center justify-between gap-2">
        <span className="font-medium">{label}</span>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={included} onChange={(e) => onInclude(e.target.checked)} />
          Merge these
        </label>
      </div>
      {group.tags.map((tag) => (
        <label key={tag.id} className="flex items-center gap-2 py-1 text-sm">
          <input
            type="radio"
            name={`keep-${groupKey(group)}`}
            checked={kept === tag.id}
            disabled={!included}
            onChange={() => onKeep(tag.id)}
          />
          <span>
            {tag.name} <span className="text-muted-foreground">· {uses(tag.split_count)}</span>
          </span>
        </label>
      ))}
    </fieldset>
  );
}

/**
 * Tags whose names differ only in case. In each group the chosen tag (the
 * most used by default) takes over the others' transactions, recurrences,
 * templates, rules and attachments; the others are deleted.
 */
export function TagDuplicatesPage() {
  const queryClient = useQueryClient();
  const duplicates = useQuery(duplicatesQuery);
  const [keep, setKeep] = useState<Record<string, string>>({});
  const [skipped, setSkipped] = useState<ReadonlySet<string>>(new Set());
  const [done, setDone] = useState<number | null>(null);
  const merges = plannedMerges(duplicates.data ?? [], keep, skipped);
  const merge = useMutation({
    mutationFn: () => mergeTags(merges),
    onSuccess: async (kept) => {
      setDone(kept.length);
      setKeep({});
      setSkipped(new Set());
      // Merges touch transactions, rules, templates and reports too.
      await queryClient.invalidateQueries();
    },
  });

  const include = (key: string, included: boolean) =>
    setSkipped((old) => {
      const next = new Set(old);
      if (included) next.delete(key);
      else next.add(key);
      return next;
    });

  return (
    <div className="max-w-xl">
      <PageHeader
        title="Case duplicates"
        actions={
          <Link to="/tags" className="text-sm hover:underline">
            All tags
          </Link>
        }
      />
      <p className="mb-4 text-sm text-muted-foreground">
        In each group, the tag you keep takes over the others everywhere they are used, and the
        others are deleted. This cannot be undone.
      </p>
      {merge.error ? <Alert className="mb-4">{errorMessage(merge.error)}</Alert> : null}
      {done !== null ? <p className="mb-4 text-sm">Merged {groups(done)}.</p> : null}
      <QueryState isPending={duplicates.isPending} error={duplicates.error} />
      {duplicates.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">No tags differ only in case.</p>
      ) : null}
      <div className="divide-y">
        {duplicates.data?.map((group) => {
          const key = groupKey(group);
          const planned = merges.find((m) => m.sources.includes(key) || m.into === key);
          return (
            <GroupFields
              key={key}
              group={group}
              kept={planned?.into ?? keep[key] ?? key}
              included={!skipped.has(key)}
              onKeep={(tagId) => setKeep((old) => ({ ...old, [key]: tagId }))}
              onInclude={(included) => include(key, included)}
            />
          );
        })}
      </div>
      {duplicates.data && duplicates.data.length > 0 ? (
        <Button
          className="mt-4"
          disabled={merges.length === 0 || merge.isPending}
          onClick={() => merge.mutate()}
        >
          Merge {groups(merges.length)}
        </Button>
      ) : null}
    </div>
  );
}
