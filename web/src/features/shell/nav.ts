// The pages in the main navigation. The first few are always visible; the
// rest sit behind "More" so the bar fits on a phone.
import {
  ArrowLeftRight,
  ChartColumn,
  ClipboardCopy,
  Download,
  FileSpreadsheet,
  FolderTree,
  type LucideIcon,
  MonitorSmartphone,
  PiggyBank,
  Receipt,
  Repeat,
  ShoppingBag,
  Tags,
  Target,
  Wallet,
  Wand2,
  Webhook,
} from "lucide-react";

export interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

/** Where the app opens, and where the "Saldo" name in the header leads. */
export const HOME_PATH = "/transactions";

/** Always visible, on phones and desktops. */
export const PRIMARY_NAV: readonly NavItem[] = [
  { to: "/transactions", label: "Transactions", icon: ArrowLeftRight },
  { to: "/accounts", label: "Accounts", icon: Wallet },
  { to: "/budgets", label: "Budgets", icon: Target },
  { to: "/reports", label: "Reports", icon: ChartColumn },
];

/** Behind "More". */
export const MORE_NAV: readonly NavItem[] = [
  { to: "/bills", label: "Bills", icon: Receipt },
  { to: "/recurrences", label: "Recurring", icon: Repeat },
  { to: "/piggy-banks", label: "Piggy banks", icon: PiggyBank },
  { to: "/categories", label: "Categories", icon: FolderTree },
  { to: "/tags", label: "Tags", icon: Tags },
  { to: "/rules", label: "Rules", icon: Wand2 },
  { to: "/templates", label: "Templates", icon: ClipboardCopy },
  { to: "/statement-profiles", label: "Statement profiles", icon: FileSpreadsheet },
  { to: "/sources", label: "Sources", icon: ShoppingBag },
  { to: "/settings/sessions", label: "Sessions", icon: MonitorSmartphone },
];

/** Behind "More", for the owner only (export and webhooks are owner-only). */
export const OWNER_NAV: readonly NavItem[] = [
  { to: "/export", label: "Export", icon: Download },
  { to: "/webhooks", label: "Webhooks", icon: Webhook },
];
