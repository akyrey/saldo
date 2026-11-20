import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { Amount } from "@/components/Amount";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { HistoryPayment } from "./history-model";

/** Rows shown at first and added by each "Show more". */
export const PAGE_SIZE = 25;

interface LinkedTransactionsProps {
  /** Newest first. */
  payments: readonly HistoryPayment[];
}

/** Every split linked to the bill, newest first. */
export function LinkedTransactions({ payments }: LinkedTransactionsProps) {
  const [shown, setShown] = useState(PAGE_SIZE);
  return (
    <Card className="mb-4">
      <CardHeader>
        <CardTitle>Linked transactions</CardTitle>
      </CardHeader>
      <CardContent>
        {payments.length === 0 ? (
          <p className="text-sm text-muted-foreground">No transactions linked to this bill yet.</p>
        ) : (
          <>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Date</TableHead>
                  <TableHead>Description</TableHead>
                  <TableHead>From → to</TableHead>
                  <TableHead>Due date paid</TableHead>
                  <TableHead className="text-right">Amount</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {payments.slice(0, shown).map((payment) => (
                  <TableRow key={payment.split_id}>
                    <TableCell className="whitespace-nowrap">{payment.date}</TableCell>
                    <TableCell>
                      <Link
                        to="/transactions/$groupId/edit"
                        params={{ groupId: payment.group_id }}
                        className="underline-offset-4 hover:underline"
                      >
                        {payment.description}
                      </Link>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {payment.source.name} → {payment.destination.name}
                    </TableCell>
                    <TableCell className="whitespace-nowrap">
                      {payment.due_date ?? (
                        <span className="text-muted-foreground">Not matched to a due date</span>
                      )}
                    </TableCell>
                    <TableCell className="text-right">
                      <Amount value={payment.amount} currency={payment.currency} />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            {payments.length > shown ? (
              <Button
                className="mt-3"
                size="sm"
                variant="outline"
                onClick={() => setShown((n) => n + PAGE_SIZE)}
              >
                Show more ({payments.length - shown} left)
              </Button>
            ) : null}
          </>
        )}
      </CardContent>
    </Card>
  );
}
