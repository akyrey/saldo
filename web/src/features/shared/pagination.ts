import type { components } from "@/api/schema";

type ListMeta = components["schemas"]["ListMeta"];

/** The largest page the server returns. */
export const MAX_PER_PAGE = 200;

/**
 * Fetches every page of a list. Used for small reference lists (accounts,
 * categories, tags) that pickers need in full.
 */
export async function fetchAllPages<T>(
  fetchPage: (page: number) => Promise<{ data: T[]; meta: ListMeta }>,
): Promise<T[]> {
  const items: T[] = [];
  for (let page = 1; ; page += 1) {
    const result = await fetchPage(page);
    items.push(...result.data);
    if (result.data.length === 0 || items.length >= result.meta.total) return items;
  }
}

/** Number of pages for a list result. */
export function pageCount(meta: ListMeta): number {
  return Math.max(1, Math.ceil(meta.total / Math.max(1, meta.per_page)));
}
