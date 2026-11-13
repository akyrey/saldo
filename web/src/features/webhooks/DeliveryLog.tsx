import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { errorMessage } from "@/api/errors";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Pager } from "@/features/transactions/Pager";
import { cn } from "@/lib/utils";
import { DELIVERIES_PER_PAGE, deliveriesQuery, redeliver, type Delivery } from "./queries";

const STATUS_CLASS: Record<string, string> = {
  succeeded: "border-green-600/40 text-green-700 dark:text-green-400",
  pending: "border-amber-600/40 text-amber-700 dark:text-amber-400",
  dead: "border-destructive/40 text-destructive",
};

function when(timestamp: string | null | undefined): string {
  return timestamp ? new Date(timestamp).toLocaleString() : "—";
}

function result(delivery: Delivery): string {
  if (delivery.last_error) return delivery.last_error;
  if (delivery.last_status_code) return `HTTP ${delivery.last_status_code}`;
  return "—";
}

/** The newest deliveries of one endpoint, with a redeliver button each. */
export function DeliveryLog({ webhookId }: { webhookId: string }) {
  const [page, setPage] = useState(1);
  const queryClient = useQueryClient();
  const deliveries = useQuery(deliveriesQuery(webhookId, page));
  const again = useMutation({
    mutationFn: redeliver,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["webhooks", "deliveries", webhookId] }),
  });
  const items = deliveries.data?.data ?? [];
  const pages = Math.ceil((deliveries.data?.meta.total ?? 0) / DELIVERIES_PER_PAGE);

  return (
    <div className="mt-3">
      <QueryState isPending={deliveries.isPending} error={deliveries.error} />
      {again.error ? <Alert className="mb-2">{errorMessage(again.error)}</Alert> : null}
      {deliveries.data && items.length === 0 ? (
        <p className="text-sm text-muted-foreground">No deliveries yet.</p>
      ) : null}
      {items.length > 0 ? (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Event</TableHead>
              <TableHead>Status</TableHead>
              <TableHead className="hidden sm:table-cell">Attempts</TableHead>
              <TableHead className="hidden md:table-cell">Last attempt</TableHead>
              <TableHead className="hidden md:table-cell">Result</TableHead>
              <TableHead />
            </TableRow>
          </TableHeader>
          <TableBody>
            {items.map((delivery) => (
              <TableRow key={delivery.id}>
                <TableCell className="font-mono text-xs">{delivery.event_type}</TableCell>
                <TableCell>
                  <Badge className={cn(STATUS_CLASS[delivery.status])}>{delivery.status}</Badge>
                </TableCell>
                <TableCell className="hidden sm:table-cell">{delivery.attempts}</TableCell>
                <TableCell className="hidden md:table-cell">
                  {when(delivery.last_attempt_at)}
                </TableCell>
                <TableCell
                  className="hidden max-w-60 truncate md:table-cell"
                  title={result(delivery)}
                >
                  {result(delivery)}
                </TableCell>
                <TableCell className="text-right">
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={again.isPending}
                    aria-label={`Redeliver ${delivery.event_type} event ${delivery.event_seq}`}
                    onClick={() => again.mutate(delivery.id)}
                  >
                    Redeliver
                  </Button>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      ) : null}
      <Pager page={page} pages={pages} onPage={setPage} />
    </div>
  );
}
