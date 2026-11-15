import { Link } from "@tanstack/react-router";
import { Ellipsis, RefreshCw } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import type { NavItem } from "./nav";

interface MoreMenuProps {
  items: readonly NavItem[];
  /** `bar`: a tab in the phone's bottom bar, opening upwards. */
  variant: "header" | "bar";
  /** Desktop app only: adds "Check for updates" below the pages. */
  onCheckForUpdates?: (() => void) | null;
}

/** A "More" button that shows the remaining pages; closes on choice, Escape or outside click. */
export function MoreMenu({ items, variant, onCheckForUpdates }: MoreMenuProps) {
  const [open, setOpen] = useState(false);
  const listId = useId();
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const onPointer = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onPointer);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onPointer);
    };
  }, [open]);

  const bar = variant === "bar";
  return (
    <div ref={root} className={bar ? "flex" : "relative"}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={listId}
        onClick={() => setOpen((o) => !o)}
        className={
          bar
            ? "flex min-w-0 flex-1 flex-col items-center gap-0.5 py-2 text-[11px] text-muted-foreground"
            : "rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent"
        }
      >
        {bar ? <Ellipsis className="size-5" aria-hidden /> : null}
        <span>More</span>
      </button>
      {open ? (
        <ul
          id={listId}
          aria-label="More pages"
          className={`absolute z-20 grid min-w-48 gap-0.5 rounded-md border bg-background p-1 shadow-md ${
            bar ? "inset-x-2 bottom-full mb-2 grid-cols-2" : "top-full left-0 mt-1"
          }`}
        >
          {items.map(({ to, label, icon: Icon }) => (
            <li key={to}>
              <Link
                to={to}
                onClick={() => setOpen(false)}
                className="flex items-center gap-2 rounded px-3 py-2 text-sm hover:bg-accent"
                activeProps={{ className: "bg-accent font-medium" }}
              >
                <Icon className="size-4" aria-hidden />
                {label}
              </Link>
            </li>
          ))}
          {onCheckForUpdates ? (
            <li className={bar ? "col-span-2 border-t pt-0.5" : "border-t pt-0.5"}>
              <button
                type="button"
                onClick={() => {
                  setOpen(false);
                  onCheckForUpdates();
                }}
                className="flex w-full items-center gap-2 rounded px-3 py-2 text-sm hover:bg-accent"
              >
                <RefreshCw className="size-4" aria-hidden />
                Check for updates
              </button>
            </li>
          ) : null}
        </ul>
      ) : null}
    </div>
  );
}
