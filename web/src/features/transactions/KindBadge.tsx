import {
  ArrowDownLeft,
  ArrowLeftRight,
  ArrowUpRight,
  Flag,
  Scale,
  type LucideIcon,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { KIND_LABELS } from "./editor-model";

interface KindStyle {
  icon: LucideIcon;
  /** Tailwind classes using the `--kind-*` colour tokens from index.css. */
  className: string;
}

const KIND_STYLES: Record<string, KindStyle> = {
  withdrawal: {
    icon: ArrowUpRight,
    className: "border-kind-withdrawal/30 bg-kind-withdrawal/10 text-kind-withdrawal",
  },
  deposit: {
    icon: ArrowDownLeft,
    className: "border-kind-deposit/30 bg-kind-deposit/10 text-kind-deposit",
  },
  transfer: {
    icon: ArrowLeftRight,
    className: "border-kind-transfer/30 bg-kind-transfer/10 text-kind-transfer",
  },
  opening_balance: {
    icon: Flag,
    className: "border-kind-opening/30 bg-kind-opening/10 text-kind-opening",
  },
  reconciliation: {
    icon: Scale,
    className: "border-kind-reconciliation/30 bg-kind-reconciliation/10 text-kind-reconciliation",
  },
};

interface KindBadgeProps {
  kind: string;
  /** Icon only (label for screen readers and as a tooltip), for narrow screens. */
  compact?: boolean;
}

/** A split's kind as a coloured badge with an icon; unknown kinds stay neutral. */
export function KindBadge({ kind, compact = false }: KindBadgeProps) {
  const style = KIND_STYLES[kind];
  const label = KIND_LABELS[kind as keyof typeof KIND_LABELS] ?? kind;
  const Icon = style?.icon;
  if (compact && Icon) {
    return (
      <span
        data-kind={kind}
        title={label}
        className={cn("inline-flex rounded-full border p-0.5", style.className)}
      >
        <Icon className="size-3" aria-hidden />
        <span className="sr-only">{label}</span>
      </span>
    );
  }
  return (
    <Badge data-kind={kind} className={cn("gap-1", style?.className)}>
      {Icon ? <Icon className="size-3" aria-hidden /> : null}
      {label}
    </Badge>
  );
}
