import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { todayIso } from "@/lib/dates";
import { recurrenceQuery } from "./queries";
import { RecurrenceEditor } from "./RecurrenceEditor";
import { emptyRecurrence, fromRecurrence } from "./recurrence-model";

export function NewRecurrencePage() {
  const navigate = useNavigate();
  const [initial] = useState(() => emptyRecurrence(todayIso()));
  return (
    <div>
      <PageHeader title="New recurring transaction" />
      <RecurrenceEditor
        initial={initial}
        submitLabel="Create recurring transaction"
        save={(body) => call(api.POST("/api/v1/recurrences", { body }))}
        onSaved={() => void navigate({ to: "/recurrences" })}
      />
    </div>
  );
}

export function EditRecurrencePage() {
  const { recurrenceId } = useParams({ from: "/app/recurrences/$recurrenceId/edit" });
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const recurrence = useQuery(recurrenceQuery(recurrenceId));
  const [confirming, setConfirming] = useState(false);
  const path = { params: { path: { id: recurrenceId } } };
  const remove = useMutation({
    mutationFn: () => call(api.DELETE("/api/v1/recurrences/{id}", path)),
    onSuccess: async () => {
      queryClient.removeQueries({ queryKey: ["recurrences", "one", recurrenceId] });
      await queryClient.invalidateQueries({ queryKey: ["recurrences"] });
      await navigate({ to: "/recurrences" });
    },
  });

  return (
    <div>
      <PageHeader
        title="Edit recurring transaction"
        actions={
          <Button
            variant={confirming ? "destructive" : "outline"}
            size="sm"
            disabled={remove.isPending || !recurrence.data}
            onClick={() => (confirming ? remove.mutate() : setConfirming(true))}
          >
            <Trash2 /> {confirming ? "Really delete?" : "Delete"}
          </Button>
        }
      />
      {remove.error ? <Alert className="mb-4">{errorMessage(remove.error)}</Alert> : null}
      <QueryState isPending={recurrence.isPending} error={recurrence.error} />
      {recurrence.data ? (
        <RecurrenceEditor
          key={recurrence.data.updated_at}
          initial={fromRecurrence(recurrence.data)}
          submitLabel="Save changes"
          save={(body) => call(api.PUT("/api/v1/recurrences/{id}", { ...path, body }))}
          onSaved={() => void navigate({ to: "/recurrences" })}
        />
      ) : null}
    </div>
  );
}
