import { useState } from "react";
import { Field } from "@/components/Field";
import { Input } from "@/components/ui/input";
import { parseTransactionSearch } from "./search";

interface AmountFilterProps {
  id: string;
  label: string;
  value: string | undefined;
  /** Called on blur or Enter with the parsed amount, or undefined when blank. */
  onChange: (value: string | undefined) => void;
}

/** A decimal amount field that applies its value when left, not on every key. */
export function AmountFilter({ id, label, value, onChange }: AmountFilterProps) {
  const [text, setText] = useState(value ?? "");
  const [shown, setShown] = useState(value);
  // A chip or "clear" changed the URL: show the new value.
  if (shown !== value) {
    setShown(value);
    setText(value ?? "");
  }
  const apply = () => {
    const parsed = parseTransactionSearch({ amount_min: text }).amount_min;
    if (parsed !== value) onChange(parsed);
    if (parsed === undefined) setText("");
  };
  return (
    <Field id={id} label={label}>
      <Input
        id={id}
        inputMode="decimal"
        placeholder="Any amount"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={apply}
        onKeyDown={(e) => {
          if (e.key === "Enter") apply();
        }}
      />
    </Field>
  );
}
