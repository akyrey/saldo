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
import { parseMonth } from "@/features/budgets/budget-model";
import { parseReportSearch } from "@/features/reports/report-model";
import { parseScope, type Scope } from "@/features/shared/scope";
import { parseNewTransactionSearch, parseTransactionSearch } from "@/features/transactions/search";
import { safeRedirect } from "@/lib/safe-redirect";
import { platform } from "@/platform";

export interface RouterContext {
  queryClient: QueryClient;
}

const rootRoute = createRootRouteWithContext<RouterContext>()({ component: RootLayout });

/** The desktop app needs a server before anything else can load. */
function requireServer(): void {
  if (platform().kind === "desktop" && platform().serverUrl() === "") {
    throw redirect({ to: "/connect" });
  }
}

const connectRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "connect",
  component: lazyRouteComponent(() => import("@/features/auth/ConnectPage"), "ConnectPage"),
});

const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "setup",
  beforeLoad: requireServer,
  component: lazyRouteComponent(() => import("@/features/auth/SetupPage"), "SetupPage"),
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "login",
  validateSearch: (search: Record<string, unknown>): { redirect?: string; error?: string } => ({
    redirect: safeRedirect(search.redirect),
    error: typeof search.error === "string" ? search.error : undefined,
  }),
  beforeLoad: requireServer,
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
    requireServer();
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
  validateSearch: parseNewTransactionSearch,
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

const billsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "bills",
  component: lazyRouteComponent(() => import("@/features/bills/BillsPage"), "BillsPage"),
});

const newBillRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "bills/new",
  component: lazyRouteComponent(() => import("@/features/bills/BillFormPage"), "NewBillPage"),
});

export const billRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "bills/$billId",
  component: lazyRouteComponent(() => import("@/features/bills/BillDetailPage"), "BillDetailPage"),
});

export const editBillRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "bills/$billId/edit",
  component: lazyRouteComponent(() => import("@/features/bills/BillFormPage"), "EditBillPage"),
});

const rulesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "rules",
  component: lazyRouteComponent(() => import("@/features/rules/RulesPage"), "RulesPage"),
});

const newRuleRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "rules/new",
  validateSearch: (search: Record<string, unknown>): { group?: string } => ({
    group: typeof search.group === "string" ? search.group : undefined,
  }),
  component: lazyRouteComponent(() => import("@/features/rules/RuleEditorPage"), "NewRulePage"),
});

export const ruleRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "rules/$ruleId",
  component: lazyRouteComponent(() => import("@/features/rules/RuleEditorPage"), "EditRulePage"),
});

const exportRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "export",
  component: lazyRouteComponent(() => import("@/features/export/ExportPage"), "ExportPage"),
});

const webhooksRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "webhooks",
  component: lazyRouteComponent(() => import("@/features/webhooks/WebhooksPage"), "WebhooksPage"),
});

export const budgetsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "budgets",
  validateSearch: (search: Record<string, unknown>): { month?: string } => ({
    month: parseMonth(search.month),
  }),
  component: lazyRouteComponent(() => import("@/features/budgets/BudgetsPage"), "BudgetsPage"),
});

export const budgetRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "budgets/$budgetId",
  component: lazyRouteComponent(
    () => import("@/features/budgets/BudgetDetailPage"),
    "BudgetDetailPage",
  ),
});

export const dashboardRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "reports",
  component: lazyRouteComponent(() => import("@/features/reports/DashboardPage"), "DashboardPage"),
});

export const reportRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "reports/$report",
  validateSearch: parseReportSearch,
  component: lazyRouteComponent(() => import("@/features/reports/ReportPage"), "ReportPage"),
});

const notificationsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "settings/notifications",
  component: lazyRouteComponent(
    () => import("@/features/notifications/NotificationsPage"),
    "NotificationsPage",
  ),
});

const piggyBanksRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "piggy-banks",
  component: lazyRouteComponent(
    () => import("@/features/piggy-banks/PiggyBanksPage"),
    "PiggyBanksPage",
  ),
});

const recurrencesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "recurrences",
  component: lazyRouteComponent(
    () => import("@/features/recurrences/RecurrencesPage"),
    "RecurrencesPage",
  ),
});

const newRecurrenceRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "recurrences/new",
  component: lazyRouteComponent(
    () => import("@/features/recurrences/RecurrenceEditorPage"),
    "NewRecurrencePage",
  ),
});

export const editRecurrenceRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "recurrences/$recurrenceId/edit",
  component: lazyRouteComponent(
    () => import("@/features/recurrences/RecurrenceEditorPage"),
    "EditRecurrencePage",
  ),
});

const templatesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "templates",
  component: lazyRouteComponent(
    () => import("@/features/templates/TemplatesPage"),
    "TemplatesPage",
  ),
});

const routeTree = rootRoute.addChildren([
  connectRoute,
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
    billsRoute,
    newBillRoute,
    billRoute,
    editBillRoute,
    rulesRoute,
    newRuleRoute,
    ruleRoute,
    webhooksRoute,
    budgetsRoute,
    budgetRoute,
    dashboardRoute,
    reportRoute,
    notificationsRoute,
    piggyBanksRoute,
    recurrencesRoute,
    newRecurrenceRoute,
    editRecurrenceRoute,
    templatesRoute,
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
