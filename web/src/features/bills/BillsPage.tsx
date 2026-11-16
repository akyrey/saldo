import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { ActiveSwitch } from "@/components/ActiveSwitch";
import { InactiveSection } from "@/components/InactiveSection";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { todayIso } from "@/lib/dates";
import { cn } from "@/lib/utils";
import { partitionByActive } from "@/features/shared/active";
import { amountRange, type Bill } from "./bill-form";
import { billsQuery } from "./queries";

interface ToggleBill {
  id: string;
  active: boolean;
}

function BillRow({
  bill,
  today,
  busy,
  onToggle,
}: {
  bill: Bill;
  today: string;
  busy: boolean;
  onToggle: (change: ToggleBill) => void;
}) {
  const overdue =
    bill.active &&
    bill.next_due_date !== null &&
    bill.next_due_date !== undefined &&
    bill.next_due_date < today;
  return (
    <li className="flex items-center justify-between gap-3 py-2">
      <ActiveSwitch
        name={bill.name}
        checked={bill.active}
        disabled={busy}
        onChange={(active) => onToggle({ id: bill.id, active })}
      />
      {/* Inactive bills are dimmed; the switch keeps full strength. */}
      <div
        className={cn(
          "flex min-w-0 flex-1 items-center justify-between gap-2",
          !bill.active && "opacity-60",
        )}
      >
        <div className="min-w-0">
          <Link
            to="/bills/$billId"
            params={{ billId: bill.id }}
            className="font-medium hover:underline"
          >
            {bill.name}
          </Link>
          <div className="text-xs text-muted-foreground">{bill.repeat_freq.replace("_", " ")}</div>
        </div>
        <div className="text-right text-sm">
          <div className="tabular-nums">{amountRange(bill)}</div>
          <div className={overdue ? "text-destructive" : "text-muted-foreground"}>
            {bill.next_due_date
              ? `${overdue ? "overdue since" : "due"} ${bill.next_due_date}`
              : "nothing due"}
          </div>
        </div>
      </div>
    </li>
  );
}

export function BillsPage() {
  const queryClient = useQueryClient();
  const bills = useQuery(billsQuery);
  const today = todayIso();
  const toggle = useMutation({
    mutationFn: ({ id, active }: ToggleBill) =>
      call(api.PATCH("/api/v1/bills/{id}", { params: { path: { id } }, body: { active } })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["bills"] }),
  });
  const { active, inactive } = partitionByActive(bills.data ?? []);
  const rows = (list: Bill[]) => (
    <Card>
      <CardContent>
        <ul className="divide-y">
          {list.map((bill) => (
            <BillRow
              key={bill.id}
              bill={bill}
              today={today}
              busy={toggle.isPending}
              onToggle={(change) => toggle.mutate(change)}
            />
          ))}
        </ul>
      </CardContent>
    </Card>
  );
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
      {toggle.error ? <Alert className="mb-4">{errorMessage(toggle.error)}</Alert> : null}
      {bills.data && bills.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">No bills yet.</p>
      ) : null}
      {active.length > 0 ? rows(active) : null}
      {bills.data && bills.data.length > 0 && active.length === 0 ? (
        <p className="text-sm text-muted-foreground">No active bills.</p>
      ) : null}
      <InactiveSection count={inactive.length}>{rows(inactive)}</InactiveSection>
    </div>
  );
}
