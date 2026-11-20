import { useNavigate, useRouterState } from "@tanstack/react-router";
import { Search } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import { Button } from "@/components/ui/button";
import type { ShortcutAction } from "./shortcuts";
import { ShortcutHelp } from "./ShortcutHelp";
import { useShortcuts } from "./useShortcuts";

const CommandPalette = lazy(() =>
  import("./CommandPalette").then((m) => ({ default: m.CommandPalette })),
);

/** The transaction list's search field (TransactionFilters). */
const SEARCH_FIELD_ID = "filter-q";

function focusSearchField(): void {
  // After a navigation the list renders a moment later; try for a short while.
  let tries = 20;
  const attempt = () => {
    const field = document.getElementById(SEARCH_FIELD_ID);
    if (field) {
      field.focus();
      return;
    }
    if (--tries > 0) requestAnimationFrame(attempt);
  };
  attempt();
}

/** Keyboard shortcuts, the ⌘K palette, the "?" help, and a search button for touch. */
export function CommandCenter() {
  const navigate = useNavigate();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const [paletteOpen, setPaletteOpen] = useState(false);
  // Mounted on first open and kept, so reopening is instant.
  const [paletteLoaded, setPaletteLoaded] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);

  const openPalette = () => {
    setPaletteLoaded(true);
    setPaletteOpen(true);
  };
  const onAction = (action: ShortcutAction) => {
    switch (action.kind) {
      case "navigate":
        void navigate({ to: action.to, search: {} });
        return;
      case "new-transaction":
        void navigate({ to: "/transactions/new", search: {} });
        return;
      case "focus-search":
        if (path !== "/transactions") void navigate({ to: "/transactions", search: {} });
        focusSearchField();
        return;
      case "help":
        setHelpOpen(true);
        return;
    }
  };
  useShortcuts({ onAction, onPalette: openPalette });

  return (
    <>
      <Button
        variant="ghost"
        size="icon"
        aria-label="Search and commands"
        title="Search and commands (⌘K)"
        onClick={openPalette}
      >
        <Search />
      </Button>
      {paletteLoaded ? (
        <Suspense fallback={null}>
          <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} />
        </Suspense>
      ) : null}
      <ShortcutHelp open={helpOpen} onOpenChange={setHelpOpen} />
    </>
  );
}
