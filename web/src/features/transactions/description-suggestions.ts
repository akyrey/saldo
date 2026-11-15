import type { TransactionGroup } from "./queries";

interface Candidate {
  /** The spelling to suggest: the newest one seen (the list is newest first). */
  text: string;
  uses: number;
  /** Position of its newest use; lower is more recent. */
  firstSeen: number;
}

/** Every split description and group title, newest first, as the list returns them. */
function textsOf(groups: readonly TransactionGroup[]): string[] {
  return groups.flatMap((g) => [
    ...(g.title ? [g.title] : []),
    ...g.splits.map((s) => s.description),
  ]);
}

/**
 * Past descriptions containing `text` (case ignored), to offer while typing one:
 * those starting with the text first, then the most used, then the most recent.
 * The exact text already typed is left out.
 */
export function descriptionSuggestions(
  groups: readonly TransactionGroup[],
  text: string,
  limit: number,
): string[] {
  const query = text.trim().toLocaleLowerCase();
  if (query === "") return [];
  const candidates = new Map<string, Candidate>();
  textsOf(groups).forEach((raw, index) => {
    const value = raw.trim();
    const key = value.toLocaleLowerCase();
    if (key === query || !key.includes(query)) return;
    const seen = candidates.get(key);
    candidates.set(
      key,
      seen ? { ...seen, uses: seen.uses + 1 } : { text: value, uses: 1, firstSeen: index },
    );
  });
  const starts = (c: Candidate) => (c.text.toLocaleLowerCase().startsWith(query) ? 0 : 1);
  return [...candidates.values()]
    .sort((a, b) => starts(a) - starts(b) || b.uses - a.uses || a.firstSeen - b.firstSeen)
    .slice(0, limit)
    .map((c) => c.text);
}
