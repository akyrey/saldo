import type * as React from "react";
import { cn } from "@/lib/utils";

export const fieldClass =
  "h-9 w-full min-w-0 rounded-md border border-input bg-transparent px-3 py-1 text-base shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 md:text-sm";

/**
 * No autocorrect or spell check: names, payees and amounts are not dictionary
 * words, and phones "fixing" them is more harm than help. A field can still
 * turn either back on by passing the prop.
 */
export const noCorrection = { autoCorrect: "off", spellCheck: false } as const;

export function Input({ className, ...props }: React.ComponentProps<"input">) {
  return <input {...noCorrection} className={cn(fieldClass, className)} {...props} />;
}

export function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      {...noCorrection}
      className={cn(fieldClass, "h-auto min-h-16 py-2", className)}
      {...props}
    />
  );
}
