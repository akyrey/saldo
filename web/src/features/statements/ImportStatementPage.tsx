import { useMutation, useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { Upload } from "lucide-react";
import { useState } from "react";
import { explain, fieldErrors } from "@/api/errors";
import { Field } from "@/components/Field";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { accountQuery } from "@/features/accounts/queries";
import { profilesQuery, uploadStatement } from "./queries";

/** Upload a bank or Satispay statement for one account; the review follows. */
export function ImportStatementPage() {
  const { accountId } = useParams({ from: "/app/accounts/$accountId/import" });
  const navigate = useNavigate();
  const account = useQuery(accountQuery(accountId));
  const profiles = useQuery(profilesQuery);
  const [chosen, setChosen] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  // The account's own profile first; any profile can read a file.
  const ordered = [...(profiles.data ?? [])].sort(
    (a, b) => Number(b.account_id === accountId) - Number(a.account_id === accountId),
  );
  const profileId = chosen ?? ordered[0]?.id ?? "";
  const upload = useMutation({
    mutationFn: (selected: File) => uploadStatement(profileId, accountId, selected),
    onSuccess: async (detail) => {
      await navigate({
        to: "/statement-imports/$importId",
        params: { importId: detail.import.id },
      });
    },
  });
  const errors = fieldErrors(upload.error);

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <PageHeader
        title={account.data ? `Import a statement into ${account.data.name}` : "Import a statement"}
        actions={
          <Button variant="outline" size="sm" asChild>
            <Link to="/accounts/$accountId" params={{ accountId }}>
              Back to account
            </Link>
          </Button>
        }
      />
      <QueryState isPending={profiles.isPending} error={profiles.error} />
      {profiles.data && profiles.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          A profile tells Saldo how to read your bank&apos;s export.{" "}
          <Link to="/statement-profiles" className="underline">
            Create one first
          </Link>
          .
        </p>
      ) : null}
      {ordered.length > 0 ? (
        <>
          <Field id="statement-profile" label="Profile" error={errors.profile_id}>
            <NativeSelect
              id="statement-profile"
              value={profileId}
              onChange={(e) => setChosen(e.target.value)}
            >
              {ordered.map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profile.name}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field
            id="statement-file"
            label="Statement file"
            error={errors.file ?? errors.account_id}
          >
            <Input
              id="statement-file"
              type="file"
              accept=".xlsx,.xlsm,.csv,text/csv"
              onChange={(e) => setFile(e.target.files?.[0] ?? null)}
            />
          </Field>
          {/* The file, profile and account fields show their own messages. */}
          {upload.error &&
          (Object.keys(errors).length === 0 ||
            Object.keys(errors).some((f) => !["file", "profile_id", "account_id"].includes(f))) ? (
            <Alert>{explain(upload.error)}</Alert>
          ) : null}
          <div>
            <Button
              disabled={!file || !profileId || upload.isPending}
              onClick={() => file && upload.mutate(file)}
            >
              <Upload /> Upload
            </Button>
          </div>
          <p className="text-sm text-muted-foreground">
            Nothing is booked yet: you review every line before committing.{" "}
            <Link to="/statement-profiles" className="underline">
              Manage profiles
            </Link>
          </p>
        </>
      ) : null}
    </div>
  );
}
