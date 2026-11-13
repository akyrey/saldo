// Which options to show for what has been typed (pure, so it is easy to test).

export interface ComboboxOption {
  value: string;
  /** What the field shows once chosen, for example `Checking (EUR)`. */
  label: string;
  /** Extra text that is shown and searched but not filled in, such as the account type. */
  hint?: string;
}

/**
 * Options matching `text` (case-insensitive, in label or hint): labels
 * starting with it first, then the rest, each in their original order.
 * Empty text keeps every option, so the list can still be browsed.
 */
export function filterOptions(options: readonly ComboboxOption[], text: string): ComboboxOption[] {
  const query = text.trim().toLocaleLowerCase();
  if (query === "") return [...options];
  const label = (o: ComboboxOption) => o.label.toLocaleLowerCase();
  const matches = options.filter(
    (o) => label(o).includes(query) || (o.hint ?? "").toLocaleLowerCase().includes(query),
  );
  return [
    ...matches.filter((o) => label(o).startsWith(query)),
    ...matches.filter((o) => !label(o).startsWith(query)),
  ];
}
