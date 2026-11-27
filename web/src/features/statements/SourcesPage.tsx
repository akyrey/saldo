import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Trash2, Upload } from "lucide-react";
import { useState } from "react";
import { explain } from "@/api/errors";
import { Amount } from "@/components/Amount";
import { Field } from "@/components/Field";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import {
  deleteSource,
  sourceDocumentsQuery,
  sourceRowsQuery,
  uploadSource,
} from "./sources-queries";

const KINDS: Record<string, string> = { amazon_orders: "Amazon orders", paypal: "PayPal activity" };

/** Amazon and PayPal exports whose rows explain bank lines when importing. */
export function SourcesPage() {
  const queryClient = useQueryClient();
  const [kind, setKind] = useState("amazon_orders");
  const [file, setFile] = useState<File | null>(null);
  const [unmatched, setUnmatched] = useState(false);
  const [confirm, setConfirm] = useState<string | null>(null);
  const documents = useQuery(sourceDocumentsQuery);
  const rows = useQuery(sourceRowsQuery(unmatched));
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["sources"] });
  const upload = useMutation({
    mutationFn: (selected: File) => uploadSource(kind, selected),
    onSuccess: refresh,
  });
  const remove = useMutation({
    mutationFn: deleteSource,
    onSuccess: async () => {
      setConfirm(null);
      await refresh();
    },
  });

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Sources" />
      <p className="text-sm text-muted-foreground">
        Upload your Amazon order history and PayPal activity: when you import a bank statement,
        their rows name what each Amazon or PayPal payment was for. Upload a newer export whenever
        you like; only new rows are added.
      </p>
      <div className="flex flex-wrap items-end gap-2 rounded-lg border p-4">
        <Field id="source-kind" label="Export">
          <NativeSelect id="source-kind" value={kind} onChange={(e) => setKind(e.target.value)}>
            {Object.entries(KINDS).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="source-file" label="File" className="flex-1">
          <Input
            id="source-file"
            type="file"
            accept=".csv,.xlsx,text/csv"
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          />
        </Field>
        <Button disabled={!file || upload.isPending} onClick={() => file && upload.mutate(file)}>
          <Upload /> Upload
        </Button>
      </div>
      {upload.data ? (
        <p role="status" className="text-sm">
          {upload.data.rows_added} new rows, {upload.data.rows_updated} updated.
        </p>
      ) : null}
      {upload.error ? <Alert>{explain(upload.error)}</Alert> : null}

      <QueryState isPending={documents.isPending} error={documents.error} />
      {documents.data && documents.data.length > 0 ? (
        <section aria-label="Uploads" className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold">Uploads</h2>
          <ul className="divide-y rounded-md border text-sm">
            {documents.data.map((document) => (
              <li key={document.id} className="flex flex-wrap items-center gap-3 px-3 py-2">
                <span className="min-w-0 flex-1 truncate">
                  {KINDS[document.kind] ?? document.kind} · {document.filename}
                </span>
                <span className="text-muted-foreground">
                  {document.first_date} to {document.last_date} · {document.row_count} rows
                </span>
                <Button
                  variant={confirm === document.id ? "destructive" : "outline"}
                  size="sm"
                  aria-label={confirm === document.id ? undefined : `Delete ${document.filename}`}
                  disabled={remove.isPending}
                  onBlur={() => setConfirm(null)}
                  onClick={() =>
                    confirm === document.id ? remove.mutate(document.id) : setConfirm(document.id)
                  }
                >
                  {confirm === document.id ? "Really delete?" : <Trash2 />}
                </Button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {remove.error ? <Alert>{explain(remove.error)}</Alert> : null}

      <section aria-label="Rows" className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <h2 className="text-sm font-semibold">Rows</h2>
          <label className="flex items-center gap-1 text-sm">
            <input
              type="checkbox"
              checked={unmatched}
              onChange={(e) => setUnmatched(e.target.checked)}
            />
            Only rows not used yet
          </label>
          {rows.data ? (
            <span className="text-sm text-muted-foreground">{rows.data.meta.total} in all</span>
          ) : null}
        </div>
        <QueryState isPending={rows.isPending} error={rows.error} />
        {rows.data && rows.data.data.length > 0 ? (
          <ul className="divide-y rounded-md border text-sm">
            {rows.data.data.map((row) => (
              <li key={row.id} className="flex flex-wrap items-center gap-3 px-3 py-2">
                <span className="w-24 shrink-0 text-muted-foreground">{row.date}</span>
                <span className="min-w-0 flex-1 truncate">
                  {row.text ?? row.counterparty ?? row.external_id}
                </span>
                {row.excluded ? <Badge>never charged</Badge> : null}
                {row.matched ? <Badge>used</Badge> : null}
                <Amount value={row.amount} currency={row.currency} />
              </li>
            ))}
          </ul>
        ) : null}
      </section>
    </div>
  );
}
