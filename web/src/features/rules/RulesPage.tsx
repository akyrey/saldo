import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowDown, ArrowUp, Plus } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
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

  return (
    <div>
      <PageHeader title="Rules" />
      <QueryState
        isPending={groups.isPending || rules.isPending}
        error={groups.error ?? rules.error}
      />
      {move.error ? <Alert className="mb-4">{errorMessage(move.error)}</Alert> : null}
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
                  <CardTitle>
                    {group.title} {group.active ? null : <Badge>inactive</Badge>}
                  </CardTitle>
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
                  <ul className="divide-y">
                    {inGroup.map((rule, index) => (
                      <li key={rule.id} className="flex items-center justify-between gap-2 py-2">
                        <div className="min-w-0">
                          <Link
                            to="/rules/$ruleId"
                            params={{ ruleId: rule.id }}
                            className="font-medium hover:underline"
                          >
                            {rule.title}
                          </Link>
                          <div className="flex flex-wrap gap-1 text-xs text-muted-foreground">
                            <span>on {rule.trigger_on}</span>
                            {rule.active ? null : <Badge>inactive</Badge>}
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
                </CardContent>
              </Card>
            </section>
          );
        })}
      </div>
    </div>
  );
}
