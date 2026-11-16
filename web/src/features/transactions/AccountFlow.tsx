import {
  ArrowRight,
  Briefcase,
  CircleHelp,
  CreditCard,
  Scale,
  ShoppingBag,
  Wallet,
  type LucideIcon,
} from "lucide-react";
import type { Account } from "@/features/accounts/queries";
import { cn } from "@/lib/utils";

/** One icon per account type, so the kind of each side reads at a glance. */
const TYPE_ICONS: Record<string, LucideIcon> = {
  asset: Wallet,
  liability: CreditCard,
  expense: ShoppingBag,
  revenue: Briefcase,
  equity: Scale,
};

function AccountChip({ account, muted }: { account: Account | undefined; muted: boolean }) {
  const Icon = (account && TYPE_ICONS[account.type]) ?? CircleHelp;
  const name = account?.name ?? "Unknown account";
  return (
    <span
      title={name}
      className={cn(
        "inline-flex max-w-full min-w-0 items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-xs",
        muted ? "text-muted-foreground" : "text-foreground",
      )}
    >
      <Icon className="size-3 shrink-0 text-muted-foreground" aria-hidden />
      <span className="truncate">{name}</span>
    </span>
  );
}

interface AccountFlowProps {
  source: Account | undefined;
  destination: Account | undefined;
  /** On an account's page that account is already known, so it is shown muted. */
  perspectiveAccountId?: string;
  className?: string;
}

/** "Source → Destination" as two chips; on narrow screens the second wraps below. */
export function AccountFlow(props: AccountFlowProps) {
  const { source, destination, perspectiveAccountId } = props;
  return (
    <span className={cn("flex min-w-0 flex-wrap items-center gap-1", props.className)}>
      <AccountChip account={source} muted={source?.id === perspectiveAccountId} />
      <ArrowRight className="size-3 shrink-0 text-muted-foreground" aria-hidden />
      <span className="sr-only">to</span>
      <AccountChip account={destination} muted={destination?.id === perspectiveAccountId} />
    </span>
  );
}
