import type * as React from "react";
import { cn } from "@/lib/utils";
import { fieldClass } from "./input";

/** A styled native `<select>`: accessible, and the best picker on phones. */
export function NativeSelect({ className, ...props }: React.ComponentProps<"select">) {
  return <select className={cn(fieldClass, "pr-8", className)} {...props} />;
}
