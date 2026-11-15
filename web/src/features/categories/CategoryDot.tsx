import { cn } from "@/lib/utils";
import { colorClass } from "./colors";

interface CategoryDotProps {
  color: string | null | undefined;
  className?: string;
}

/** A small round swatch of a category's colour; nothing when it has none. */
export function CategoryDot({ color, className }: CategoryDotProps) {
  const background = colorClass(color);
  if (!background) return null;
  return (
    <span
      aria-hidden
      data-color={color}
      className={cn("inline-block size-2.5 shrink-0 rounded-full", background, className)}
    />
  );
}
