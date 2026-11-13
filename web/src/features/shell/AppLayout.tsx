import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, useNavigate, useRouteContext } from "@tanstack/react-router";
import { Bell, LogOut } from "lucide-react";
import { api, call } from "@/api/client";
import { Button } from "@/components/ui/button";
import { useLiveEvents } from "@/features/live/useLiveEvents";
import { platform } from "@/platform";
import { MoreMenu } from "./MoreMenu";
import { MORE_NAV, OWNER_NAV, PRIMARY_NAV } from "./nav";
import { ScopeSwitch } from "./ScopeSwitch";

export function AppLayout() {
  const { me } = useRouteContext({ from: "/app" });
  const more = me.role === "owner" ? [...MORE_NAV, ...OWNER_NAV] : MORE_NAV;
  const queryClient = useQueryClient();
  useLiveEvents();
  const navigate = useNavigate();
  const logout = useMutation({
    mutationFn: () => call(api.POST("/api/v1/auth/logout")),
    onSettled: async () => {
      // The desktop app forgets its token even if the server was unreachable.
      await platform().clearToken();
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
          <nav aria-label="Main" className="hidden items-center gap-1 md:flex">
            {PRIMARY_NAV.map(({ to, label }) => (
              <Link
                key={to}
                to={to}
                className="rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent"
                activeProps={{ className: "bg-accent text-foreground font-medium" }}
              >
                {label}
              </Link>
            ))}
            <MoreMenu items={more} variant="header" />
          </nav>
          <div className="ml-auto flex items-center gap-2">
            <span className="hidden text-sm text-muted-foreground sm:inline">
              {me.user.display_name} · {me.household.name}
            </span>
            <ScopeSwitch currentUserId={me.user.id} />
            <Button variant="ghost" size="icon" asChild>
              <Link to="/settings/notifications" aria-label="Notifications" title="Notifications">
                <Bell />
              </Link>
            </Button>
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
        className="fixed inset-x-0 bottom-0 z-10 grid border-t bg-background pb-[env(safe-area-inset-bottom)] md:hidden"
        // One equal column per main page, plus one for "More".
        style={{ gridTemplateColumns: `repeat(${PRIMARY_NAV.length + 1}, minmax(0, 1fr))` }}
      >
        {PRIMARY_NAV.map(({ to, label, icon: Icon }) => (
          <Link
            key={to}
            to={to}
            className="flex min-w-0 flex-col items-center gap-0.5 py-2 text-[11px] text-muted-foreground"
            activeProps={{ className: "text-primary" }}
          >
            <Icon className="size-5" aria-hidden />
            <span className="max-w-full truncate">{label}</span>
          </Link>
        ))}
        <MoreMenu items={more} variant="bar" />
      </nav>
    </div>
  );
}
