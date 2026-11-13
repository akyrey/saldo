import type { ComboboxOption } from "@/components/Combobox";

interface CurrencyLike {
  code: string;
  name: string;
}

/** Currencies to pick from: the code, with the name as a searchable hint. */
export function currencyOptions(currencies: readonly CurrencyLike[]): ComboboxOption[] {
  return currencies.map((c) => ({ value: c.code, label: c.code, hint: c.name }));
}
