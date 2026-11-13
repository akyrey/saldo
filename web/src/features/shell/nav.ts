// The pages in the main navigation. The first few are always visible; the
// rest sit behind "More" so the bar fits on a phone.
import {
  ArrowLeftRight,
  ChartColumn,
  Download,
  FolderTree,
  type LucideIcon,
  PiggyBank,
  Receipt,
  Repeat,
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

/** Always visible, on phones and desktops. */
export const PRIMARY_NAV: readonly NavItem[] = [
  { to: "/accounts", label: "Accounts", icon: Wallet },
  { to: "/transactions", label: "Transactions", icon: ArrowLeftRight },
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
];

/** Behind "More", for the owner only (export and webhooks are owner-only). */
export const OWNER_NAV: readonly NavItem[] = [
  { to: "/export", label: "Export", icon: Download },
  { to: "/webhooks", label: "Webhooks", icon: Webhook },
];
