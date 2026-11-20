// What the command palette offers, as plain data, so it can be tested
// without rendering. The component turns each entry into a cmdk item.
import { REPORTS } from "@/features/reports/report-model";
import { MORE_NAV, OWNER_NAV, PRIMARY_NAV } from "@/features/shell/nav";
import type { ThemeChoice } from "@/features/theme/theme";
import { NONE } from "@/features/transactions/search";

export type PaletteTarget =
  | { kind: "navigate"; to: string; search?: Record<string, unknown> }
  | { kind: "theme"; choice: ThemeChoice };

export interface PaletteItem {
  id: string;
  label: string;
  /** Extra words cmdk matches on, besides the label. */
  keywords?: string[];
  target: PaletteTarget;
}

export interface PaletteGroup {
  heading: string;
  items: PaletteItem[];
}

interface Named {
  id: string;
  name: string;
}

export interface PaletteSources {
  isOwner: boolean;
  templates: readonly Named[];
  savedViews: readonly (Named & { filters: Record<string, unknown> })[];
  accounts: readonly Named[];
}

const page = (to: string, label: string, keywords?: string[]): PaletteItem => ({
  id: `page:${to}`,
  label,
  keywords,
  target: { kind: "navigate", to },
});

function pages(isOwner: boolean): PaletteItem[] {
  const nav = [...PRIMARY_NAV, ...MORE_NAV, ...(isOwner ? OWNER_NAV : [])];
  return [
    // The reports page is the dashboard too.
    ...nav.map((item) =>
      page(item.to, item.label, item.to === "/reports" ? ["dashboard", "overview"] : undefined),
    ),
    page("/forecast", "Forecast", ["cash flow", "future"]),
    ...REPORTS.map((r) => page(`/reports/${r.kind}`, `Report: ${r.title}`, [r.description])),
    page("/settings/notifications", "Notification settings", ["alerts", "push"]),
  ];
}

const THEMES: readonly { choice: ThemeChoice; label: string }[] = [
  { choice: "system", label: "Theme: follow the system" },
  { choice: "light", label: "Theme: light" },
  { choice: "dark", label: "Theme: dark" },
];

/** Every group with at least one entry, in display order. */
export function paletteGroups(sources: PaletteSources): PaletteGroup[] {
  const groups: PaletteGroup[] = [
    {
      heading: "Actions",
      items: [
        {
          id: "action:new-transaction",
          label: "New transaction",
          keywords: ["add", "create", "expense"],
          target: { kind: "navigate", to: "/transactions/new" },
        },
        ...THEMES.map((t) => ({
          id: `theme:${t.choice}`,
          label: t.label,
          keywords: ["dark mode", "appearance"],
          target: { kind: "theme" as const, choice: t.choice },
        })),
      ],
    },
    { heading: "Pages", items: pages(sources.isOwner) },
    {
      heading: "Templates",
      items: sources.templates.map((t) => ({
        id: `template:${t.id}`,
        label: `New from template: ${t.name}`,
        target: { kind: "navigate", to: "/transactions/new", search: { template: t.id } },
      })),
    },
    {
      heading: "Saved views",
      items: [
        {
          id: "view:uncategorised",
          label: "Uncategorised transactions",
          keywords: ["inbox", "no category"],
          target: { kind: "navigate", to: "/transactions", search: { category_id: NONE } },
        },
        ...sources.savedViews.map((v) => ({
          id: `view:${v.id}`,
          label: `View: ${v.name}`,
          target: { kind: "navigate" as const, to: "/transactions", search: v.filters },
        })),
      ],
    },
    {
      heading: "Accounts",
      items: sources.accounts.map((a) => ({
        id: `account:${a.id}`,
        label: a.name,
        keywords: ["account"],
        target: { kind: "navigate", to: `/accounts/${a.id}` },
      })),
    },
  ];
  return groups.filter((g) => g.items.length > 0);
}

/** The last entry, always shown: a full-text search of the transactions. */
export function searchTarget(text: string): PaletteTarget {
  return { kind: "navigate", to: "/transactions", search: { q: text.trim() } };
}
