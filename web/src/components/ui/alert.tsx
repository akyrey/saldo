import type * as React from "react";
import { cn } from "@/lib/utils";

export function Alert({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      role="alert"
      className={cn(
        "rounded-lg border border-destructive/40 bg-destructive/5 px-4 py-3 text-sm text-destructive",
        className,
      )}
      {...props}
    />
  );
}
