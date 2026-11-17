import type { components } from "@/api/schema";

/** Tags whose names differ only in case, most used first. */
export type DuplicateGroup = components["schemas"]["TagDuplicateGroup"];
export type TagMerge = components["schemas"]["TagMergeRequest"];

/** A stable key for a group: its most used tag. */
export const groupKey = (group: DuplicateGroup): string => group.tags[0]?.id ?? "";

/**
 * The merges for the groups not in `skipped`: in each, the tag chosen in
 * `keep` (by group key), or else the most used one, absorbs the others.
 */
export function plannedMerges(
  groups: readonly DuplicateGroup[],
  keep: Readonly<Record<string, string>>,
  skipped: ReadonlySet<string>,
): TagMerge[] {
  return groups
    .filter((group) => !skipped.has(groupKey(group)))
    .map((group) => {
      const chosen = keep[groupKey(group)];
      const into =
        chosen !== undefined && group.tags.some((t) => t.id === chosen) ? chosen : groupKey(group);
      return { into, sources: group.tags.map((t) => t.id).filter((id) => id !== into) };
    });
}
