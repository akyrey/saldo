import { Amount } from "@/components/Amount";
import { Badge } from "@/components/ui/badge";
import { NativeSelect } from "@/components/ui/native-select";
import type { Account } from "@/features/accounts/queries";
import type { Line, LinePatch } from "./queries";
import { newCounterpartyName, proposalOf, statusLabel } from "./review-model";

interface ReviewLinesProps {
  lines: readonly Line[];
  currency: string;
  accountId: string;
  accounts: readonly Account[];
  /** Whether lines can still be changed (a draft). */
  editable: boolean;
  onChange: (position: number, patch: LinePatch) => void;
}

const ACTIONS = [
  { value: "create", label: "Create" },
  { value: "link", label: "Link" },
  { value: "skip", label: "Skip" },
];

interface ProposalControlsProps {
  line: Line;
  editable: boolean;
  onChange: (position: number, patch: LinePatch) => void;
}

/** What a matched Amazon or PayPal row proposes, and whether to book it. */
function ProposalControls({ line, editable, onChange }: ProposalControlsProps) {
  const proposal = proposalOf(line.proposal);
  if (!proposal) return null;
  const source = proposal.source === "paypal" ? "PayPal" : "Amazon";
  const label = `${line.booked_date} ${line.description}`;
  return (
    <span className="mt-1 flex flex-col gap-1 rounded-md bg-muted/50 px-2 py-1 text-xs">
      <span className="font-medium">{proposal.description}</span>
      {proposal.notes.map((note) => (
        <span key={note} className="text-muted-foreground">
          {note}
        </span>
      ))}
      <span className="flex flex-wrap gap-3">
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={line.use_proposal}
            disabled={!editable}
            aria-label={`Use ${source} details for ${label}`}
            onChange={(e) => onChange(line.position, { use_proposal: e.target.checked })}
          />
          Use {source} details
        </label>
        {proposal.items.length > 1 ? (
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              checked={line.split_by_item}
              disabled={!editable || !line.use_proposal}
              aria-label={`Split by item for ${label}`}
              onChange={(e) => onChange(line.position, { split_by_item: e.target.checked })}
            />
            Split by item ({proposal.items.length})
          </label>
        ) : null}
      </span>
    </span>
  );
}

/** The counterparty commit looks up or creates: the proposal's, when kept. */
function counterpartyName(line: Line): string {
  const proposal = proposalOf(line.proposal);
  return proposal && line.use_proposal ? proposal.counterparty : newCounterpartyName(line);
}

function accountName(accounts: readonly Account[], id: string | null | undefined): string {
  return accounts.find((a) => a.id === id)?.name ?? "";
}

/** Every line of the statement with its status, action and counterparty. */
export function ReviewLines({
  lines,
  currency,
  accountId,
  accounts,
  editable,
  onChange,
}: ReviewLinesProps) {
  const counterparties = accounts.filter((a) => a.id !== accountId && a.type !== "equity");
  return (
    <ul className="divide-y rounded-md border text-sm">
      {lines.map((line) => (
        <li
          key={line.position}
          className="flex flex-col gap-2 px-3 py-2 sm:flex-row sm:items-center"
        >
          <span className="w-24 shrink-0 text-muted-foreground">{line.booked_date}</span>
          <span className="min-w-0 flex-1">
            <span className="block truncate">{line.description}</span>
            {line.reason ? (
              <span className="block text-xs text-muted-foreground">{line.reason}</span>
            ) : null}
            <ProposalControls line={line} editable={editable} onChange={onChange} />
            {line.parts.length > 0 ? (
              <span className="block text-xs text-muted-foreground">
                {line.parts.map((p) => `${p.column}: ${p.amount}`).join(", ")}
              </span>
            ) : null}
          </span>
          <Amount value={line.amount} currency={currency} />
          <Badge>{statusLabel(line.status)}</Badge>
          {editable ? (
            <NativeSelect
              aria-label={`Action for ${line.booked_date} ${line.description}`}
              className="sm:w-28"
              value={line.action}
              onChange={(e) => onChange(line.position, { action: e.target.value })}
            >
              {ACTIONS.filter((a) => a.value !== "link" || line.split_id).map((a) => (
                <option key={a.value} value={a.value}>
                  {a.label}
                </option>
              ))}
            </NativeSelect>
          ) : (
            <span className="sm:w-28">{line.action}</span>
          )}
          {editable && line.action === "create" ? (
            <NativeSelect
              aria-label={`Counterparty for ${line.booked_date} ${line.description}`}
              className="sm:w-48"
              value={line.counterparty_account_id ?? ""}
              onChange={(e) =>
                onChange(line.position, { counterparty_account_id: e.target.value || null })
              }
            >
              <option value="">New account: {counterpartyName(line)}</option>
              {/* A chosen account the list leaves out still shows as chosen. */}
              {line.counterparty_account_id &&
              !counterparties.some((a) => a.id === line.counterparty_account_id) ? (
                <option value={line.counterparty_account_id}>
                  {accountName(accounts, line.counterparty_account_id) || "Chosen account"}
                </option>
              ) : null}
              {counterparties.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name} ({a.type})
                </option>
              ))}
            </NativeSelect>
          ) : (
            <span className="truncate text-muted-foreground sm:w-48">
              {line.action === "create" ? accountName(accounts, line.counterparty_account_id) : ""}
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}
