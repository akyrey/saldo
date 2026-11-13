import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { Pencil } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Combobox } from "@/components/Combobox";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { amountRange, shiftDate, type BillPeriod } from "./bill-form";
import { billQuery, periodsQuery } from "./queries";

type Action = "skip" | "reopen";

/** Splits dated within a month of the due date, to pick the payment from. */
function PayPicker({ period, onDone }: { period: BillPeriod; onDone: () => void }) {
  const queryClient = useQueryClient();
  const [splitId, setSplitId] = useState("");
  const from = shiftDate(period.due_date, -31);
  const to = shiftDate(period.due_date, 31);
  const candidates = useQuery({
    queryKey: ["transactions", "bill-candidates", from, to],
    queryFn: async () =>
      (
        await call(
          api.GET("/api/v1/transactions", { params: { query: { from, to, per_page: 100 } } }),
        )
      ).data,
  });
  const pay = useMutation({
    mutationFn: () =>
      call(
        api.POST("/api/v1/bill-periods/{id}/pay", {
          params: { path: { id: period.id } },
          body: { split_id: splitId },
        }),
      ),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["bills"] });
      onDone();
    },
  });
  const splits = (candidates.data ?? []).flatMap((group) => group.splits);
  return (
    <div className="flex flex-col gap-2 rounded-md border p-3">
      <Label htmlFor={`pay-${period.id}`}>Paying transaction</Label>
      <Combobox
        id={`pay-${period.id}`}
        label="Paying transaction"
        placeholder="Type a description or date…"
        value={splitId}
        onChange={setSplitId}
        options={splits.map((split) => ({
          value: split.id,
          label: `${split.date} · ${split.description}`,
          hint: `${split.amount} ${split.currency}`,
        }))}
      />
      {pay.error ? <Alert>{errorMessage(pay.error)}</Alert> : null}
      <div className="flex gap-2">
        <Button size="sm" disabled={!splitId || pay.isPending} onClick={() => pay.mutate()}>
          Confirm payment
        </Button>
        <Button size="sm" variant="ghost" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

export function BillDetailPage() {
  const { billId } = useParams({ from: "/app/bills/$billId" });
  const queryClient = useQueryClient();
  const bill = useQuery(billQuery(billId));
  const periods = useQuery(periodsQuery(billId));
  const [paying, setPaying] = useState<string | null>(null);
  const change = useMutation({
    mutationFn: ({ id, action }: { id: string; action: Action }) =>
      action === "skip"
        ? call(api.POST("/api/v1/bill-periods/{id}/skip", { params: { path: { id } } }))
        : call(api.POST("/api/v1/bill-periods/{id}/reopen", { params: { path: { id } } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["bills"] }),
  });

  return (
    <div>
      <PageHeader
        title={bill.data?.name ?? "Bill"}
        actions={
          <Button size="sm" variant="outline" asChild>
            <Link to="/bills/$billId/edit" params={{ billId }}>
              <Pencil /> Edit
            </Link>
          </Button>
        }
      />
      <QueryState isPending={bill.isPending} error={bill.error ?? periods.error} />
      {bill.data ? (
        <p className="mb-4 text-sm text-muted-foreground">
          {amountRange(bill.data)} · {bill.data.repeat_freq.replace("_", " ")} from{" "}
          {bill.data.first_due_date} · reminders {bill.data.remind_days_before} days before, then
          every {bill.data.remind_every_days} day(s)
        </p>
      ) : null}
      {change.error ? <Alert className="mb-4">{errorMessage(change.error)}</Alert> : null}
      <Card>
        <CardHeader>
          <CardTitle>Periods</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Due</TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {(periods.data ?? []).map((period) => (
                <TableRow key={period.id}>
                  <TableCell>{period.due_date}</TableCell>
                  <TableCell>
                    <Badge>{period.status}</Badge>
                  </TableCell>
                  <TableCell className="text-right">
                    {paying === period.id ? (
                      <PayPicker period={period} onDone={() => setPaying(null)} />
                    ) : (
                      <div className="flex justify-end gap-1">
                        {period.status === "open" ? (
                          <>
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => setPaying(period.id)}
                            >
                              Mark paid
                            </Button>
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() => change.mutate({ id: period.id, action: "skip" })}
                            >
                              Skip
                            </Button>
                          </>
                        ) : (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => change.mutate({ id: period.id, action: "reopen" })}
                          >
                            Reopen
                          </Button>
                        )}
                      </div>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
