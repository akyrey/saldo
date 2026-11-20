import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { errorMessage } from "@/api/errors";
import { DatePicker } from "@/components/DatePicker";
import { Field } from "@/components/Field";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { GoalProgress } from "./GoalsCard";
import { createGoal, deleteGoal, goalsQuery, updateGoal } from "./queries";

/** Create, archive and delete net worth goals (on the net worth report). */
export function GoalManager() {
  const client = useQueryClient();
  const goals = useQuery(goalsQuery);
  const [name, setName] = useState("");
  const [target, setTarget] = useState("");
  const [date, setDate] = useState("");
  const refresh = () => client.invalidateQueries({ queryKey: ["goals"] });
  const create = useMutation({
    mutationFn: () =>
      createGoal({
        name: name.trim(),
        target_amount: target.trim().replace(",", "."),
        target_date: date || null,
      }),
    onSuccess: async () => {
      setName("");
      setTarget("");
      setDate("");
      await refresh();
    },
  });
  const archive = useMutation({
    mutationFn: (input: { id: string; active: boolean }) =>
      updateGoal(input.id, { active: input.active }),
    onSuccess: refresh,
  });
  const remove = useMutation({ mutationFn: deleteGoal, onSuccess: refresh });
  const error = create.error ?? archive.error ?? remove.error;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    create.mutate();
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Goals</CardTitle>
        <CardDescription>
          A goal starts from today&apos;s net worth. Aim lower than today to track paying a debt
          off.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {(goals.data ?? []).map((goal) => (
          <div key={goal.id} className="flex flex-col gap-2 border-b pb-3 last:border-b-0">
            <GoalProgress goal={goal} />
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                onClick={() => archive.mutate({ id: goal.id, active: !goal.active })}
              >
                {goal.active ? "Archive" : "Restore"}
              </Button>
              <Button size="sm" variant="destructive" onClick={() => remove.mutate(goal.id)}>
                Delete
              </Button>
            </div>
          </div>
        ))}
        <form onSubmit={submit} className="grid gap-3 sm:grid-cols-4 sm:items-end">
          <Field id="goal-name" label="Name">
            <Input id="goal-name" value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
          <Field id="goal-target" label="Target">
            <Input
              id="goal-target"
              inputMode="decimal"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
            />
          </Field>
          <Field id="goal-date" label="By (optional)">
            <DatePicker id="goal-date" value={date} onChange={setDate} />
          </Field>
          <Button
            type="submit"
            disabled={name.trim() === "" || target.trim() === "" || create.isPending}
          >
            Add goal
          </Button>
        </form>
        {error ? <Alert>{errorMessage(error)}</Alert> : null}
      </CardContent>
    </Card>
  );
}
