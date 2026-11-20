// Loaded on first use (React.lazy in CommandCenter), so cmdk stays out of
// the first page's download.
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useNavigate, useRouteContext } from "@tanstack/react-router";
import { Command } from "cmdk";
import { Search } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { savedViewsQuery } from "@/features/saved-views/queries";
import { templatesQuery } from "@/features/templates/queries";
import { useTheme } from "@/features/theme/context";
import { useDebounced } from "@/lib/use-debounced";
import { type PaletteTarget, paletteGroups, searchTarget } from "./palette-model";

/** Account search starts after this many characters. */
const ACCOUNT_SEARCH_MIN = 2;
const ACCOUNT_RESULTS = 8;
const DEBOUNCE_MS = 200;

const ITEM_CLASS =
  "flex cursor-pointer items-center gap-2 rounded px-3 py-2 text-sm data-[selected=true]:bg-accent";

function useAccountMatches(text: string) {
  const q = useDebounced(text.trim(), DEBOUNCE_MS);
  return useQuery({
    queryKey: ["accounts", "palette", q],
    enabled: q.length >= ACCOUNT_SEARCH_MIN,
    placeholderData: keepPreviousData,
    queryFn: async () =>
      (
        await call(
          api.GET("/api/v1/accounts", {
            params: { query: { q, active: true, per_page: ACCOUNT_RESULTS } },
          }),
        )
      ).data,
  });
}

interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CommandPalette({ open, onOpenChange }: CommandPaletteProps) {
  const { me } = useRouteContext({ from: "/app" });
  const navigate = useNavigate();
  const { setChoice } = useTheme();
  const [text, setText] = useState("");
  const templates = useQuery({ ...templatesQuery, enabled: open });
  const views = useQuery({ ...savedViewsQuery, enabled: open });
  const accounts = useAccountMatches(text);
  const searching = text.trim().length >= ACCOUNT_SEARCH_MIN;

  const groups = paletteGroups({
    isOwner: me.role === "owner",
    templates: templates.data ?? [],
    savedViews: views.data ?? [],
    accounts: searching ? (accounts.data ?? []) : [],
  });

  const run = (target: PaletteTarget) => {
    onOpenChange(false);
    setText("");
    if (target.kind === "theme") {
      setChoice(target.choice);
      return;
    }
    void navigate({ to: target.to, search: target.search ?? {} });
  };

  return (
    <Command.Dialog
      open={open}
      onOpenChange={onOpenChange}
      label="Command palette"
      overlayClassName="fixed inset-0 z-40 bg-black/40"
      contentClassName="fixed top-[12vh] left-1/2 z-50 w-[min(36rem,calc(100vw-2rem))] -translate-x-1/2 overflow-hidden rounded-lg border bg-background shadow-lg"
    >
      <div className="flex items-center gap-2 border-b px-3">
        <Search className="size-4 text-muted-foreground" aria-hidden />
        <Command.Input
          value={text}
          onValueChange={setText}
          placeholder="Go to a page, run an action, find an account…"
          className="h-11 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        />
      </div>
      <Command.List className="max-h-[60vh] overflow-y-auto p-1">
        <Command.Empty className="px-3 py-6 text-center text-sm text-muted-foreground">
          Nothing matches.
        </Command.Empty>
        {groups.map((group) => (
          <Command.Group
            key={group.heading}
            heading={group.heading}
            className="[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:text-muted-foreground"
          >
            {group.items.map((item) => (
              <Command.Item
                key={item.id}
                value={item.id}
                keywords={[item.label, ...(item.keywords ?? [])]}
                onSelect={() => run(item.target)}
                className={ITEM_CLASS}
              >
                {item.label}
              </Command.Item>
            ))}
          </Command.Group>
        ))}
        {text.trim() !== "" ? (
          <Command.Item
            forceMount
            value="search-transactions"
            onSelect={() => run(searchTarget(text))}
            className={ITEM_CLASS}
          >
            <Search className="size-4" aria-hidden />
            Search transactions for “{text.trim()}”
          </Command.Item>
        ) : null}
      </Command.List>
    </Command.Dialog>
  );
}
