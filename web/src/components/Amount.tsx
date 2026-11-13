import type * as React from "react";
import { formatMoney, isNegative } from "@/api/money";
import { cn } from "@/lib/utils";

type AmountProps = Omit<React.ComponentProps<"span">, "children"> & {
  value: string;
  currency: string;
};

/** A decimal-string amount with its currency; negative amounts in red. */
export function Amount({ value, currency, className, ...props }: AmountProps) {
  return (
    <span
      className={cn(
        "tabular-nums whitespace-nowrap",
        isNegative(value) && "text-destructive",
        className,
      )}
      {...props}
    >
      {formatMoney(value, currency)}
    </span>
  );
}
