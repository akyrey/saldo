// Live updates: the server streams a short notice for every change in the
// household (`GET /api/v1/events/stream`, Server-Sent Events). We turn each
// notice into TanStack Query invalidations, so a page showing data that
// another member just changed refetches it without a reload.
import type { QueryClient, QueryKey } from "@tanstack/react-query";

/** What the server sends for each event. */
export interface EventNotice {
  seq: number;
  type: string;
  entity_type: string;
  entity_id: string;
}

/** The parts of `EventSource` we use, so tests can pass a fake. */
export interface EventSourceLike {
  onmessage: ((event: MessageEvent<string>) => void) | null;
  close(): void;
}

export type EventSourceFactory = new (url: string, init?: EventSourceInit) => EventSourceLike;

/** Wait this long after a notice for more, then invalidate once. */
const BATCH_MS = 250;

/** Query keys (prefixes) per entity type. Add new features here. */
const KEYS: Record<string, QueryKey[]> = {
  // Bill histories show account names.
  account: [["accounts"], ["balances"], ["reports"], ["bills"], ["credit-cards"]],
  // A transaction moves balances, budget spending, bill periods and reports.
  transaction: [
    ["transactions"],
    ["balances"],
    ["accounts"],
    ["budgets"],
    ["bills"],
    ["reports"],
    ["subscriptions"],
    ["goals"],
    ["credit-cards"],
  ],
  category: [["categories"], ["transactions"], ["reports"]],
  tag: [["tags"], ["transactions"], ["reports"]],
  household_member: [["members"]],
  household: [["me"]],
  webhook: [["webhooks"]],
  budget: [["budgets"], ["reports"]],
  budget_limit: [["budgets"], ["reports"]],
  // Bills and recurrences feed the forecast report.
  bill: [["bills"], ["reports"], ["subscriptions"]],
  rule: [["rules"]],
  rule_group: [["rules"]],
  piggy_bank: [["piggy-banks"]],
  recurrence: [["recurrences"], ["reports"]],
  notification_channel: [["notification-channels"]],
  template: [["templates"]],
  saved_view: [["saved-views"]],
  net_worth_goal: [["goals"]],
  // Alerts change nothing on screen; known here so they do not refetch everything.
  forecast: [],
  digest: [],
  subscription_dismissal: [["subscriptions"]],
  reconciliation: [
    ["reconciliations"],
    ["transactions"],
    ["balances"],
    ["accounts"],
    ["credit-cards"],
  ],
  statement_profile: [["statement-profiles"]],
  source_document: [["sources"], ["statement-imports"]],
  statement_import: [
    ["statement-imports"],
    ["transactions"],
    ["balances"],
    ["accounts"],
    ["reports"],
  ],
  credit_card_plan: [["credit-cards"]],
  credit_card_move: [["credit-cards"]],
};

/** The keys an entity type affects, or `"all"` when we do not know it. */
export function keysFor(entityType: string): QueryKey[] | "all" {
  return KEYS[entityType] ?? "all";
}

export function parseNotice(data: string): EventNotice | null {
  try {
    const value = JSON.parse(data) as Partial<EventNotice>;
    if (typeof value.seq !== "number" || typeof value.entity_type !== "string") return null;
    return value as EventNotice;
  } catch {
    return null;
  }
}

/**
 * Opens the stream and invalidates queries as notices arrive. The browser's
 * `EventSource` reconnects by itself and sends `Last-Event-ID`, so nothing
 * is missed. Returns a function that closes the stream.
 */
export function startLiveEvents(
  queryClient: QueryClient,
  url: string,
  Source: EventSourceFactory | undefined,
  onNotice: (notice: EventNotice) => void = () => {},
): () => void {
  if (!Source) return () => {};
  const source = new Source(url, { withCredentials: true });
  let pending = new Set<string>();
  let everything = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const flush = () => {
    timer = undefined;
    if (everything) {
      void queryClient.invalidateQueries();
    } else {
      for (const key of pending) {
        void queryClient.invalidateQueries({ queryKey: JSON.parse(key) as QueryKey });
      }
    }
    pending = new Set();
    everything = false;
  };

  source.onmessage = (event) => {
    const notice = parseNotice(event.data);
    if (!notice) return;
    onNotice(notice);
    const keys = keysFor(notice.entity_type);
    if (keys === "all") everything = true;
    else for (const key of keys) pending.add(JSON.stringify(key));
    timer ??= setTimeout(flush, BATCH_MS);
  };

  return () => {
    if (timer) clearTimeout(timer);
    source.close();
  };
}
