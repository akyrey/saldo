import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { explain } from "@/api/errors";
import { Amount } from "@/components/Amount";
import { Field } from "@/components/Field";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { previewStatement } from "./queries";

interface PreviewPanelProps {
  format: string;
  /** The mapping to try, or an error message when it does not parse. */
  mapping: () => unknown;
  accountId: string | null;
}

/** Tries the mapping on a sample file without saving anything. */
export function PreviewPanel({ format, mapping, accountId }: PreviewPanelProps) {
  const [file, setFile] = useState<File | null>(null);
  const preview = useMutation({
    mutationFn: (sample: File) => previewStatement(format, mapping(), accountId, sample),
  });
  const data = preview.data;
  return (
    <section aria-label="Preview" className="flex flex-col gap-3 rounded-lg border p-4">
      <div className="flex flex-wrap items-end gap-2">
        <Field id="preview-file" label="Sample file" className="flex-1">
          <Input
            id="preview-file"
            type="file"
            accept=".xlsx,.xlsm,.csv,text/csv"
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          />
        </Field>
        <Button
          variant="outline"
          disabled={!file || preview.isPending}
          onClick={() => file && preview.mutate(file)}
        >
          Preview
        </Button>
      </div>
      {preview.error ? <Alert>{explain(preview.error)}</Alert> : null}
      {data ? (
        <>
          <p className="text-sm text-muted-foreground">
            {data.total_lines} lines
            {data.opening_balance ? `, opening ${data.opening_balance}` : ""}
            {data.closing_balance ? `, closing ${data.closing_balance}` : ""}. The first{" "}
            {data.lines.length}:
          </p>
          <ul className="divide-y rounded-md border text-sm">
            {data.lines.map((line) => (
              <li key={line.row} className="flex flex-wrap items-center gap-3 px-3 py-2">
                <span className="w-24 shrink-0 text-muted-foreground">{line.booked_date}</span>
                <span className="min-w-0 flex-1 truncate">{line.description}</span>
                {line.parts.map((part) => (
                  <span key={part} className="text-xs text-muted-foreground">
                    {part}
                  </span>
                ))}
                <Amount value={line.amount} currency={data.currency} />
              </li>
            ))}
          </ul>
          {data.unsupported.length > 0 ? (
            <ul className="text-sm text-muted-foreground">
              {data.unsupported.map((row) => (
                <li key={row.row}>
                  Row {row.row}: {row.reason}
                </li>
              ))}
            </ul>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
