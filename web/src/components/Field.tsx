import type * as React from "react";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

interface FieldProps {
  id: string;
  label: string;
  error?: string;
  className?: string;
  children: React.ReactNode;
}

/**
 * A label, a control and its error message. Give the control `id` and
 * `aria-invalid`; the error is linked with `aria-describedby` = `${id}-error`.
 */
export function Field({ id, label, error, className, children }: FieldProps) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? (
        <p id={`${id}-error`} className="text-xs text-destructive" data-testid={`${id}-error`}>
          {error}
        </p>
      ) : null}
    </div>
  );
}
