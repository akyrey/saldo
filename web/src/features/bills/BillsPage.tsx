import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { todayIso } from "@/lib/dates";
import { amountRange } from "./bill-form";
import { billsQuery } from "./queries";

export function BillsPage() {
  const bills = useQuery(billsQuery);
  const today = todayIso();
  return (
    <div>
      <PageHeader
        title="Bills"
        actions={
          <Button size="sm" asChild>
            <Link to="/bills/new">
              <Plus /> New bill
            </Link>
          </Button>
        }
      />
      <QueryState isPending={bills.isPending} error={bills.error} />
      {bills.data && bills.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">No bills yet.</p>
      ) : null}
      {bills.data && bills.data.length > 0 ? (
        <Card>
          <CardContent>
            <ul className="divide-y">
              {bills.data.map((bill) => {
                const overdue =
                  bill.next_due_date !== null &&
                  bill.next_due_date !== undefined &&
                  bill.next_due_date < today;
                return (
                  <li key={bill.id} className="flex items-center justify-between gap-2 py-2">
                    <div className="min-w-0">
                      <Link
                        to="/bills/$billId"
                        params={{ billId: bill.id }}
                        className="font-medium hover:underline"
                      >
                        {bill.name}
                      </Link>
                      <div className="flex flex-wrap gap-1 text-xs text-muted-foreground">
                        <span>{bill.repeat_freq.replace("_", " ")}</span>
                        {bill.active ? null : <Badge>inactive</Badge>}
                      </div>
                    </div>
                    <div className="text-right text-sm">
                      <div className="tabular-nums">{amountRange(bill)}</div>
                      <div className={overdue ? "text-destructive" : "text-muted-foreground"}>
                        {bill.next_due_date
                          ? `${overdue ? "overdue since" : "due"} ${bill.next_due_date}`
                          : "nothing due"}
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
}
