import type { Tag } from "@/features/tags/queries";

export interface TagSuggestions {
  /** Tags whose name contains the text, best first. */
  matches: Tag[];
  /** The trimmed text, when no tag has exactly that name and it can be created. */
  create: string | null;
}

/**
 * Tags to suggest for `text`, leaving out `selected`: names starting with the
 * text come first, then names containing it; case is ignored.
 */
export function tagSuggestions(
  tags: readonly Tag[],
  selected: readonly string[],
  text: string,
  limit: number,
): TagSuggestions {
  const query = text.trim().toLocaleLowerCase();
  if (query === "") return { matches: [], create: null };
  const named = (tag: Tag) => tag.name.toLocaleLowerCase();
  const available = tags.filter((t) => !selected.includes(t.id) && named(t).includes(query));
  const starts = available.filter((t) => named(t).startsWith(query));
  const contains = available.filter((t) => !named(t).startsWith(query));
  const exists = tags.some((t) => named(t) === query);
  return {
    matches: [...starts, ...contains].slice(0, limit),
    create: exists ? null : text.trim(),
  };
}
