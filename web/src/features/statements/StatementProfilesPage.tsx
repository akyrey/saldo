import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Trash2 } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { accountsQuery } from "@/features/accounts/queries";
import { ProfileForm } from "./ProfileForm";
import { type Profile, profilesQuery } from "./queries";

/** Saved ways of reading each bank's export. */
export function StatementProfilesPage() {
  const queryClient = useQueryClient();
  const profiles = useQuery(profilesQuery);
  const accounts = useQuery(accountsQuery()).data ?? [];
  const [editing, setEditing] = useState<Profile | null>(null);
  // Bumped after each save or cancel, so a new-profile form starts over.
  const [round, setRound] = useState(0);
  const done = () => {
    setEditing(null);
    setRound((n) => n + 1);
  };
  const [confirm, setConfirm] = useState<string | null>(null);
  const remove = useMutation({
    mutationFn: (id: string) =>
      call(api.DELETE("/api/v1/statement-profiles/{id}", { params: { path: { id } } })),
    onSuccess: async (_, id) => {
      setConfirm(null);
      if (editing?.id === id) done();
      await queryClient.invalidateQueries({ queryKey: ["statement-profiles"] });
    },
  });
  const accountName = (id: string | null | undefined) =>
    accounts.find((a) => a.id === id)?.name ?? "any account";

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Statement profiles" />
      <p className="text-sm text-muted-foreground">
        A profile tells Saldo how to read one bank&apos;s export. Start from a preset, try it on a
        file with Preview, then import statements from an account page.
      </p>
      <QueryState isPending={profiles.isPending} error={profiles.error} />
      {profiles.data && profiles.data.length > 0 ? (
        <ul className="divide-y rounded-md border text-sm">
          {profiles.data.map((profile) => (
            <li key={profile.id} className="flex flex-wrap items-center gap-3 px-3 py-2">
              <span className="min-w-0 flex-1 font-medium">{profile.name}</span>
              <span className="text-muted-foreground">
                {profile.format} · {accountName(profile.account_id)}
              </span>
              <Button
                variant="outline"
                size="sm"
                aria-label={`Edit ${profile.name}`}
                onClick={() => setEditing(profile)}
              >
                <Pencil />
              </Button>
              <Button
                variant={confirm === profile.id ? "destructive" : "outline"}
                size="sm"
                aria-label={confirm === profile.id ? undefined : `Delete ${profile.name}`}
                disabled={remove.isPending}
                onBlur={() => setConfirm(null)}
                onClick={() =>
                  confirm === profile.id ? remove.mutate(profile.id) : setConfirm(profile.id)
                }
              >
                {confirm === profile.id ? "Really delete?" : <Trash2 />}
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
      {remove.error ? <Alert>{errorMessage(remove.error)}</Alert> : null}
      <h2 className="text-sm font-semibold">{editing ? `Edit ${editing.name}` : "New profile"}</h2>
      {/* A new key remounts the form, so its fields start from the chosen profile. */}
      <ProfileForm
        key={editing?.id ?? `new-${round}`}
        profile={editing ?? undefined}
        onDone={done}
      />
    </div>
  );
}
