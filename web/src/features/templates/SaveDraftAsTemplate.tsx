import { useMutation, useQueryClient } from "@tanstack/react-query";
import { BookmarkPlus } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { ApiError, errorMessage } from "@/api/errors";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { Account } from "@/features/accounts/queries";
import {
  mapServerErrors,
  type EditorErrors,
  type EditorState,
} from "@/features/transactions/editor-model";
import { templateKindOf, toTemplateRequest } from "@/features/transactions/prefill";

interface SaveDraftAsTemplateProps {
  state: EditorState;
  accounts: readonly Account[];
  /** Field errors go onto the editor's rows, like a transaction's. */
  onErrors: (errors: EditorErrors) => void;
}

/**
 * Saves what the editor holds as a template. Anything may be empty: only
 * what is filled in is checked, and the date is never stored.
 */
export function SaveDraftAsTemplate({ state, accounts, onErrors }: SaveDraftAsTemplateProps) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [open, setOpen] = useState(false);
  const save = useMutation({
    mutationFn: () => {
      const body = toTemplateRequest(state, name, templateKindOf(state, accounts));
      return call(api.POST("/api/v1/transaction-templates", { body }));
    },
    onMutate: () => onErrors({ rows: {}, form: [] }),
    onSuccess: async () => {
      setOpen(false);
      setName("");
      await queryClient.invalidateQueries({ queryKey: ["templates"] });
    },
    onError: (error) => {
      const details = error instanceof ApiError ? error.details : [];
      onErrors(
        details.length > 0 ? mapServerErrors(details) : { rows: {}, form: [errorMessage(error)] },
      );
    },
  });

  if (!open) {
    return (
      <div className="flex items-center gap-2">
        <Button type="button" variant="outline" onClick={() => setOpen(true)}>
          <BookmarkPlus /> Save as template
        </Button>
        {save.isSuccess ? (
          <span className="text-sm text-muted-foreground" role="status">
            Template saved
          </span>
        ) : null}
      </div>
    );
  }
  // Not a nested <form>: the editor is already one.
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Input
        aria-label="Template name"
        className="w-48"
        placeholder="Template name"
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key !== "Enter") return;
          e.preventDefault();
          save.mutate();
        }}
      />
      <Button
        type="button"
        disabled={save.isPending || name.trim() === ""}
        onClick={() => save.mutate()}
      >
        Save template
      </Button>
      <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
        Cancel
      </Button>
    </div>
  );
}
