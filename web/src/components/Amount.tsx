import type * as React from "react";
import { formatMoney, isNegative, isPositive } from "@/api/money";
import { cn } from "@/lib/utils";

type AmountProps = Omit<React.ComponentProps<"span">, "children"> & {
  value: string;
  currency: string;
  /** Money flow: positive amounts get a "+" and green, so in and out read at a glance. */
  signed?: boolean;
};

/** A decimal-string amount with its currency; negative amounts in red. */
export function Amount({ value, currency, signed = false, className, ...props }: AmountProps) {
  return (
    <span
      className={cn(
        "tabular-nums whitespace-nowrap",
        isNegative(value) && "text-destructive",
        signed && isPositive(value) && "text-kind-deposit",
        className,
      )}
      {...props}
    >
      {formatMoney(value, currency, undefined, { signed })}
    </span>
  );
}
