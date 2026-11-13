import { useMutation } from "@tanstack/react-query";
import { useRouteContext } from "@tanstack/react-router";
import { Download } from "lucide-react";
import { downloadExport, type ExportFormat } from "@/api/download";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { saveFile } from "./save-file";

interface ExportOption {
  format: ExportFormat;
  title: string;
  description: string;
  button: string;
}

const OPTIONS: readonly ExportOption[] = [
  {
    format: "json",
    title: "Complete JSON",
    description:
      "Every account, transaction, category, tag and attachment record in one file, complete enough to rebuild the household.",
    button: "Download JSON",
  },
  {
    format: "csv",
    title: "Spreadsheet (CSV)",
    description:
      "A zip with one CSV per entity. transactions.csv has one row per split with account names and decimal amounts.",
    button: "Download CSV",
  },
];

/** Owner-only page with the two export downloads. */
export function ExportPage() {
  const { me } = useRouteContext({ from: "/app" });
  const download = useMutation({
    mutationFn: downloadExport,
    onSuccess: ({ blob, filename }) => saveFile(blob, filename),
  });

  if (me.role !== "owner") {
    return (
      <>
        <PageHeader title="Export" />
        <Alert>Only the household owner can export the household&apos;s data.</Alert>
      </>
    );
  }

  return (
    <>
      <PageHeader title="Export" />
      {download.error ? <Alert className="mb-4">{errorMessage(download.error)}</Alert> : null}
      <div className="grid gap-4 md:grid-cols-2">
        {OPTIONS.map((option) => (
          <Card key={option.format}>
            <CardHeader>
              <CardTitle>{option.title}</CardTitle>
              <CardDescription>{option.description}</CardDescription>
            </CardHeader>
            <CardContent>
              <Button disabled={download.isPending} onClick={() => download.mutate(option.format)}>
                <Download aria-hidden />
                {download.isPending && download.variables === option.format
                  ? "Preparing…"
                  : option.button}
              </Button>
            </CardContent>
          </Card>
        ))}
      </div>
    </>
  );
}
