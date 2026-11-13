// Reordering templates, kept free of React so it can be unit-tested.

export interface Ordered {
  id: string;
  sort_order: number;
}

/** One `PATCH` the move needs: give template `id` this sort order. */
export interface OrderChange {
  id: string;
  sort_order: number;
}

/**
 * Moves the item at `index` by `delta` places (-1 up, +1 down) and returns
 * the sort orders to store: every item gets its position, and only those
 * that differ from what is stored are returned.
 */
export function moveChanges(
  items: readonly Ordered[],
  index: number,
  delta: number,
): OrderChange[] {
  const target = index + delta;
  if (target < 0 || target >= items.length || delta === 0) return [];
  const reordered = items.filter((_, i) => i !== index);
  const moved = items[index];
  if (!moved) return [];
  const result = [...reordered.slice(0, target), moved, ...reordered.slice(target)];
  return result
    .map((item, position) => ({ id: item.id, sort_order: position, stored: item.sort_order }))
    .filter((item) => item.sort_order !== item.stored)
    .map(({ id, sort_order }) => ({ id, sort_order }));
}
