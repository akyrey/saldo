import type { Tag } from "@/features/tags/queries";
import { cn } from "@/lib/utils";

/** Toggle buttons for tags; a native multi-select is awkward on desktop. */
export function TagPicker({
  id,
  tags,
  value,
  onChange,
}: {
  id: string;
  tags: readonly Tag[];
  value: readonly string[];
  onChange: (next: string[]) => void;
}) {
  if (tags.length === 0) {
    return <p className="text-xs text-muted-foreground">No tags yet.</p>;
  }
  return (
    <div id={id} role="group" aria-label="Tags" className="flex flex-wrap gap-1">
      {tags.map((tag) => {
        const selected = value.includes(tag.id);
        return (
          <button
            key={tag.id}
            type="button"
            aria-pressed={selected}
            onClick={() =>
              onChange(selected ? value.filter((t) => t !== tag.id) : [...value, tag.id])
            }
            className={cn(
              "rounded-full border px-2 py-0.5 text-xs",
              selected ? "border-primary bg-primary text-primary-foreground" : "hover:bg-accent",
            )}
          >
            {tag.name}
          </button>
        );
      })}
    </div>
  );
}
