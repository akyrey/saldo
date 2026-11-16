import { ChevronRight } from "lucide-react";
import type { ReactNode } from "react";

interface InactiveSectionProps {
  count: number;
  children: ReactNode;
}

/** A closed-by-default section below a list that holds its inactive items. */
export function InactiveSection({ count, children }: InactiveSectionProps) {
  if (count === 0) return null;
  return (
    <details className="group mt-4">
      <summary className="flex cursor-pointer list-none items-center gap-1 text-sm text-muted-foreground select-none hover:text-foreground">
        <ChevronRight className="size-4 transition-transform group-open:rotate-90" />
        Inactive ({count})
      </summary>
      <div className="mt-2">{children}</div>
    </details>
  );
}
