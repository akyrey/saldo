import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowDown, ArrowUp, Plus } from "lucide-react";
import { useState, type ReactNode } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { ActiveSwitch } from "@/components/ActiveSwitch";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { fromRule, reorder, toRequest, type Rule } from "./rule-form";
import { ruleGroupsQuery, rulesQuery } from "./queries";

export function RulesPage() {
  const queryClient = useQueryClient();
  const groups = useQuery(ruleGroupsQuery);
  const rules = useQuery(rulesQuery);
  const [title, setTitle] = useState("");
  const invalidate = () => queryClient.invalidateQueries({ queryKey: ["rules"] });
  const addGroup = useMutation({
    mutationFn: () =>
      call(
        api.POST("/api/v1/rule-groups", { body: { title, sort_order: groups.data?.length ?? 0 } }),
      ),
    onSuccess: async () => {
      setTitle("");
      await invalidate();
    },
  });
  // Moving a rule renumbers its group and saves the rules whose order changed.
  const move = useMutation({
    mutationFn: async ({ inGroup, id, step }: { inGroup: Rule[]; id: string; step: -1 | 1 }) => {
      const reordered = reorder(inGroup, id, step);
      const changed = reordered.filter(
        (rule, index) =>
          rule.sort_order !== inGroup[index]?.sort_order || rule.id !== inGroup[index]?.id,
      );
      for (const rule of changed) {
        await call(
          api.PUT("/api/v1/rules/{id}", {
            params: { path: { id: rule.id } },
            body: toRequest(fromRule(rule)),
          }),
        );
      }
    },
    onSettled: invalidate,
  });
  // Rules keep their place when switched off: the list order is the order they run in.
  const toggleRule = useMutation({
    mutationFn: ({ rule, active }: { rule: Rule; active: boolean }) =>
      call(
        api.PUT("/api/v1/rules/{id}", {
          params: { path: { id: rule.id } },
          body: toRequest({ ...fromRule(rule), active }),
        }),
      ),
    onSettled: invalidate,
  });
  const toggleGroup = useMutation({
    mutationFn: ({ id, active }: { id: string; active: boolean }) =>
      call(api.PATCH("/api/v1/rule-groups/{id}", { params: { path: { id } }, body: { active } })),
    onSettled: invalidate,
  });
  const toggleError = toggleRule.error ?? toggleGroup.error;

  return (
    <div>
      <PageHeader title="Rules" />
      <QueryState
        isPending={groups.isPending || rules.isPending}
        error={groups.error ?? rules.error}
      />
      {move.error ? <Alert className="mb-4">{errorMessage(move.error)}</Alert> : null}
      {toggleError ? <Alert className="mb-4">{errorMessage(toggleError)}</Alert> : null}
      <form
        className="mb-4 flex max-w-md gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (title.trim()) addGroup.mutate();
        }}
      >
        <Input
          aria-label="New group title"
          placeholder="New group"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
        <Button type="submit" variant="outline" disabled={addGroup.isPending}>
          <Plus /> Add group
        </Button>
      </form>
      <div className="grid gap-4">
        {(groups.data ?? []).map((group) => {
          const inGroup = (rules.data ?? []).filter((rule) => rule.rule_group_id === group.id);
          return (
            <section key={group.id} aria-label={group.title}>
              <Card>
                <CardHeader className="flex-row items-center justify-between">
                  <div className="flex min-w-0 items-center gap-3">
                    <ActiveSwitch
                      name={group.title}
                      checked={group.active}
                      disabled={toggleGroup.isPending}
                      onChange={(active) => toggleGroup.mutate({ id: group.id, active })}
                    />
                    <CardTitle className={cn(!group.active && "opacity-60")}>
                      {group.title}
                    </CardTitle>
                  </div>
                  <Button size="sm" asChild>
                    <Link to="/rules/new" search={{ group: group.id }}>
                      <Plus /> New rule
                    </Link>
                  </Button>
                </CardHeader>
                <CardContent>
                  {inGroup.length === 0 ? (
                    <p className="text-sm text-muted-foreground">No rules in this group.</p>
                  ) : null}
                  <FoldWhenInactive active={group.active} count={inGroup.length}>
                    <ul className="divide-y">
                      {inGroup.map((rule, index) => (
                        <li key={rule.id} className="flex items-center gap-3 py-2">
                          <ActiveSwitch
                            name={rule.title}
                            checked={rule.active}
                            disabled={toggleRule.isPending}
                            onChange={(active) => toggleRule.mutate({ rule, active })}
                          />
                          <div className={cn("min-w-0 flex-1", !rule.active && "opacity-60")}>
                            <Link
                              to="/rules/$ruleId"
                              params={{ ruleId: rule.id }}
                              className="font-medium hover:underline"
                            >
                              {rule.title}
                            </Link>
                            <div className="flex flex-wrap gap-1 text-xs text-muted-foreground">
                              <span>on {rule.trigger_on}</span>
                              {rule.stop_processing ? <Badge>stops</Badge> : null}
                            </div>
                          </div>
                          <div className="flex gap-1">
                            <Button
                              size="icon"
                              variant="ghost"
                              aria-label={`Move ${rule.title} up`}
                              disabled={index === 0 || move.isPending}
                              onClick={() => move.mutate({ inGroup, id: rule.id, step: -1 })}
                            >
                              <ArrowUp />
                            </Button>
                            <Button
                              size="icon"
                              variant="ghost"
                              aria-label={`Move ${rule.title} down`}
                              disabled={index === inGroup.length - 1 || move.isPending}
                              onClick={() => move.mutate({ inGroup, id: rule.id, step: 1 })}
                            >
                              <ArrowDown />
                            </Button>
                          </div>
                        </li>
                      ))}
                    </ul>
                  </FoldWhenInactive>
                </CardContent>
              </Card>
            </section>
          );
        })}
      </div>
    </div>
  );
}

/** An inactive group's rules start folded; an active group's are always shown. */
function FoldWhenInactive({
  active,
  count,
  children,
}: {
  active: boolean;
  count: number;
  children: ReactNode;
}) {
  if (active || count === 0) return children;
  return (
    <details>
      <summary className="cursor-pointer text-sm text-muted-foreground select-none">
        Show {count} {count === 1 ? "rule" : "rules"}
      </summary>
      {children}
    </details>
  );
}
