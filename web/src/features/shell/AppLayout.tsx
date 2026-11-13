import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, useNavigate, useRouteContext } from "@tanstack/react-router";
import {
  ArrowLeftRight,
  Download,
  FolderTree,
  LogOut,
  Receipt,
  Tags,
  Wallet,
  Wand2,
} from "lucide-react";
import { api, call } from "@/api/client";
import { Button } from "@/components/ui/button";
import { ScopeSwitch } from "./ScopeSwitch";

const NAV = [
  { to: "/accounts", label: "Accounts", icon: Wallet },
  { to: "/transactions", label: "Transactions", icon: ArrowLeftRight },
  { to: "/categories", label: "Categories", icon: FolderTree },
  { to: "/tags", label: "Tags", icon: Tags },
  { to: "/bills", label: "Bills", icon: Receipt },
  { to: "/rules", label: "Rules", icon: Wand2 },
] as const;

/** Tailwind needs whole class names in the source, so no `grid-cols-${n}`. */
const MOBILE_COLUMNS: Record<number, string> = {
  4: "grid-cols-4",
  5: "grid-cols-5",
  6: "grid-cols-6",
};

/** Only the owner can export, so only the owner sees the link. */
const OWNER_NAV = [{ to: "/export", label: "Export", icon: Download }] as const;

export function AppLayout() {
  const { me } = useRouteContext({ from: "/app" });
  const nav = me.role === "owner" ? [...NAV, ...OWNER_NAV] : NAV;
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const logout = useMutation({
    mutationFn: () => call(api.POST("/api/v1/auth/logout")),
    onSettled: () => {
      queryClient.clear();
      void navigate({ to: "/login", search: { redirect: undefined } });
    },
  });

  return (
    <div className="flex min-h-dvh flex-col">
      <header className="sticky top-0 z-10 border-b bg-background/95 backdrop-blur">
        <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-2 px-4 py-2">
          <Link to="/accounts" className="mr-2 font-semibold text-primary">
            Saldo
          </Link>
          <nav aria-label="Main" className="hidden gap-1 md:flex">
            {nav.map(({ to, label }) => (
              <Link
                key={to}
                to={to}
                className="rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent"
                activeProps={{ className: "bg-accent text-foreground font-medium" }}
              >
                {label}
              </Link>
            ))}
          </nav>
          <div className="ml-auto flex items-center gap-2">
            <span className="hidden text-sm text-muted-foreground sm:inline">
              {me.user.display_name} · {me.household.name}
            </span>
            <ScopeSwitch currentUserId={me.user.id} />
            <Button
              variant="ghost"
              size="icon"
              aria-label="Log out"
              title="Log out"
              onClick={() => logout.mutate()}
            >
              <LogOut />
            </Button>
          </div>
        </div>
      </header>
      <main className="mx-auto w-full max-w-6xl flex-1 px-4 py-4 pb-24 md:pb-8">
        <Outlet />
      </main>
      {/* Bottom tab bar on phones. */}
      <nav
        aria-label="Main mobile"
        className={`fixed inset-x-0 bottom-0 z-10 grid border-t bg-background pb-[env(safe-area-inset-bottom)] md:hidden ${
          MOBILE_COLUMNS[nav.length] ?? "grid-cols-6"
        }`}
      >
        {nav.map(({ to, label, icon: Icon }) => (
          <Link
            key={to}
            to={to}
            className="flex flex-col items-center gap-0.5 py-2 text-xs text-muted-foreground"
            activeProps={{ className: "text-primary" }}
          >
            <Icon className="size-5" aria-hidden />
            {label}
          </Link>
        ))}
      </nav>
    </div>
  );
}
