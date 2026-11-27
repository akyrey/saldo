import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Amount } from "@/components/Amount";
import { QueryState } from "@/components/QueryState";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { progressWidth } from "./progress";
import { type Goal, goalsQuery } from "./queries";

/** Active goals with their progress, on the dashboard. */
export function GoalsCard() {
  const goals = useQuery(goalsQuery);
  const active = (goals.data ?? []).filter((g) => g.active);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Net worth goals</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <QueryState isPending={goals.isPending} error={goals.error} />
        {goals.data && active.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No goals yet.{" "}
            <Link
              to="/reports/$report"
              params={{ report: "net-worth" }}
              search={{}}
              className="text-primary underline-offset-4 hover:underline"
            >
              Set one on the net worth report
            </Link>
          </p>
        ) : null}
        {active.map((goal) => (
          <GoalProgress key={goal.id} goal={goal} />
        ))}
      </CardContent>
    </Card>
  );
}

export function GoalProgress({ goal }: { goal: Goal }) {
  const p = goal.progress;
  return (
    <div className="flex flex-col gap-1 text-sm" data-testid="goal">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <span className="font-medium">{goal.name}</span>
        <span>
          <Amount value={p.current} currency={goal.currency} /> of{" "}
          <Amount value={goal.target_amount} currency={goal.currency} />
        </span>
      </div>
      <div
        className="h-2 overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-label={`${goal.name} progress`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.trunc(p.progress_bp / 100)}
      >
        <div className="h-full bg-primary" style={{ width: progressWidth(p.progress_bp) }} />
      </div>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
        {p.reached ? (
          <Badge>Reached</Badge>
        ) : (
          <Badge className={p.on_track ? undefined : "bg-muted text-muted-foreground"}>
            {p.on_track ? "On track" : "Behind"}
          </Badge>
        )}
        {p.required_monthly && !p.reached ? (
          <span>
            Needs <Amount value={p.required_monthly} currency={goal.currency} /> a month until{" "}
            {goal.target_date}
          </span>
        ) : null}
        {p.projected_date && !p.reached ? <span>At this pace: {p.projected_date}</span> : null}
      </div>
    </div>
  );
}
