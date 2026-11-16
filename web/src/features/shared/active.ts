// Bills, rules and recurrences can be switched off. Lists show the active
// ones first and fold the rest away, so this helper splits them once.

export interface Activatable {
  active: boolean;
}

export interface ByActive<T> {
  active: T[];
  inactive: T[];
}

/** Splits `items` into active and inactive, each in its original order. */
export function partitionByActive<T extends Activatable>(items: readonly T[]): ByActive<T> {
  return {
    active: items.filter((item) => item.active),
    inactive: items.filter((item) => !item.active),
  };
}
