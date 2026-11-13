// Every route of the app. Pages are loaded lazily (`lazyRouteComponent`), so
// the first screen downloads only the shell and the page it shows.
import type { QueryClient } from "@tanstack/react-query";
import {
  type RouterHistory,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
  retainSearchParams,
} from "@tanstack/react-router";
import { ApiError } from "@/api/errors";
import { meQuery, setupStatusQuery } from "@/features/auth/queries";
import { AppLayout } from "@/features/shell/AppLayout";
import { RootLayout } from "@/features/shell/RootLayout";
import { parseScope, type Scope } from "@/features/shared/scope";
import { parseTransactionSearch } from "@/features/transactions/search";
import { safeRedirect } from "@/lib/safe-redirect";

export interface RouterContext {
  queryClient: QueryClient;
}

const rootRoute = createRootRouteWithContext<RouterContext>()({ component: RootLayout });

const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "setup",
  component: lazyRouteComponent(() => import("@/features/auth/SetupPage"), "SetupPage"),
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "login",
  validateSearch: (search: Record<string, unknown>): { redirect?: string } => ({
    redirect: safeRedirect(search.redirect),
  }),
  component: lazyRouteComponent(() => import("@/features/auth/LoginPage"), "LoginPage"),
});

/** Everything behind login. Loads the current user first. */
const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "app",
  validateSearch: (search: Record<string, unknown>): { scope?: Scope } => ({
    scope: parseScope(search.scope),
  }),
  // Links inside the app keep the scope switch's value.
  search: { middlewares: [retainSearchParams(["scope"])] },
  beforeLoad: async ({ context, location }) => {
    try {
      return { me: await context.queryClient.ensureQueryData(meQuery) };
    } catch (error) {
      if (!(error instanceof ApiError) || error.status !== 401) throw error;
      const needed = await context.queryClient.fetchQuery(setupStatusQuery);
      if (needed) throw redirect({ to: "/setup" });
      throw redirect({ to: "/login", search: { redirect: location.href } });
    }
  },
  component: AppLayout,
});

const indexRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/accounts" });
  },
});

const accountsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "accounts",
  component: lazyRouteComponent(() => import("@/features/accounts/AccountsPage"), "AccountsPage"),
});

const newAccountRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "accounts/new",
  component: lazyRouteComponent(
    () => import("@/features/accounts/AccountFormPage"),
    "NewAccountPage",
  ),
});

export const accountRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "accounts/$accountId",
  validateSearch: (search: Record<string, unknown>): { page?: number } => {
    const page = Number(search.page);
    return { page: Number.isInteger(page) && page > 1 ? page : undefined };
  },
  component: lazyRouteComponent(
    () => import("@/features/accounts/AccountDetailPage"),
    "AccountDetailPage",
  ),
});

export const editAccountRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "accounts/$accountId/edit",
  component: lazyRouteComponent(
    () => import("@/features/accounts/AccountFormPage"),
    "EditAccountPage",
  ),
});

export const transactionsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "transactions",
  validateSearch: parseTransactionSearch,
  component: lazyRouteComponent(
    () => import("@/features/transactions/TransactionsPage"),
    "TransactionsPage",
  ),
});

const newTransactionRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "transactions/new",
  component: lazyRouteComponent(
    () => import("@/features/transactions/TransactionEditorPage"),
    "NewTransactionPage",
  ),
});

export const editTransactionRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "transactions/$groupId/edit",
  component: lazyRouteComponent(
    () => import("@/features/transactions/TransactionEditorPage"),
    "EditTransactionPage",
  ),
});

const categoriesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "categories",
  component: lazyRouteComponent(
    () => import("@/features/categories/CategoriesPage"),
    "CategoriesPage",
  ),
});

const tagsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "tags",
  component: lazyRouteComponent(() => import("@/features/tags/TagsPage"), "TagsPage"),
});

const exportRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "export",
  component: lazyRouteComponent(() => import("@/features/export/ExportPage"), "ExportPage"),
});

const piggyBanksRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "piggy-banks",
  component: lazyRouteComponent(
    () => import("@/features/piggy-banks/PiggyBanksPage"),
    "PiggyBanksPage",
  ),
});

const routeTree = rootRoute.addChildren([
  setupRoute,
  loginRoute,
  appRoute.addChildren([
    indexRoute,
    accountsRoute,
    newAccountRoute,
    accountRoute,
    editAccountRoute,
    transactionsRoute,
    newTransactionRoute,
    editTransactionRoute,
    categoriesRoute,
    tagsRoute,
    exportRoute,
    piggyBanksRoute,
  ]),
]);

/** `history` is for tests (a memory history); the browser's is the default. */
export function createAppRouter(queryClient: QueryClient, history?: RouterHistory) {
  return createRouter({
    routeTree,
    history,
    context: { queryClient },
    defaultPreload: "intent",
    // Preloading through the router would bypass TanStack Query's cache.
    defaultPreloadStaleTime: 0,
    scrollRestoration: true,
  });
}

// Tells TypeScript about our routes, so `<Link to="/acounts">` is a compile error.
declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
