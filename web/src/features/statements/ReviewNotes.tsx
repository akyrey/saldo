import { Amount } from "@/components/Amount";
import { findingLabel, summaryNotes } from "./review-model";

/** What the statement says about existing transactions, and the rows it skipped. */
export function ReviewNotes({ summary, currency }: { summary: unknown; currency: string }) {
  const { findings, unsupported, more } = summaryNotes(summary);
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      {findings.length > 0 ? (
        <section aria-label="Findings" className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold">Existing transactions to check</h2>
          <ul className="divide-y rounded-md border text-sm">
            {findings.map((finding, index) => (
              <li key={index} className="flex flex-wrap items-center gap-2 px-3 py-2">
                <span className="font-medium">{findingLabel(finding.kind)}</span>
                <span className="text-muted-foreground">{finding.date}</span>
                <span className="min-w-0 flex-1 truncate">{finding.description}</span>
                <Amount value={finding.amount} currency={currency} />
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {unsupported.length > 0 ? (
        <section aria-label="Rows not imported" className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold">Rows not imported</h2>
          <ul className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
            {unsupported.map((row) => (
              <li key={row.row}>
                Row {row.row}: {row.reason}
              </li>
            ))}
            {more > 0 ? <li>and {more} more</li> : null}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
