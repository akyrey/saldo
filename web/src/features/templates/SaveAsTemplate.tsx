import { useMutation, useQueryClient } from "@tanstack/react-query";
import { BookmarkPlus, Check, X } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface SaveAsTemplateProps {
  groupId: string;
  /** Suggested name: the group's title or first description. */
  suggestion: string;
}

/** "Save as template": asks for a name, then saves the group without its date. */
export function SaveAsTemplate({ groupId, suggestion }: SaveAsTemplateProps) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState(suggestion);
  const save = useMutation({
    mutationFn: () =>
      call(
        api.POST("/api/v1/transactions/{id}/template", {
          params: { path: { id: groupId } },
          body: { name: name.trim() },
        }),
      ),
    onSuccess: async () => {
      setOpen(false);
      await queryClient.invalidateQueries({ queryKey: ["templates"] });
    },
  });

  if (!open) {
    return (
      <div className="flex items-center gap-2">
        {save.isSuccess ? (
          <span className="text-sm text-muted-foreground" role="status">
            Template saved
          </span>
        ) : null}
        <Button variant="outline" size="sm" onClick={() => setOpen(true)}>
          <BookmarkPlus /> Save as template
        </Button>
      </div>
    );
  }
  return (
    <form
      className="flex flex-wrap items-center gap-2"
      onSubmit={(event) => {
        event.preventDefault();
        save.mutate();
      }}
    >
      <Input
        aria-label="Template name"
        className="w-48"
        required
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <Button
        type="submit"
        size="icon"
        variant="ghost"
        aria-label="Save template"
        disabled={save.isPending}
      >
        <Check />
      </Button>
      <Button
        type="button"
        size="icon"
        variant="ghost"
        aria-label="Cancel"
        onClick={() => setOpen(false)}
      >
        <X />
      </Button>
      {save.error ? <Alert className="w-full">{errorMessage(save.error)}</Alert> : null}
    </form>
  );
}
