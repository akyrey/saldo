import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { explain, fieldErrors } from "@/api/errors";
import { Field } from "@/components/Field";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { type Account, accountsQuery } from "@/features/accounts/queries";
import { PreviewPanel } from "./PreviewPanel";
import { type Profile, type ProfileRequest, presetsQuery } from "./queries";
import { partColumns } from "./review-model";

interface Rule {
  pattern: string;
  /** The first is edited here; the others are kept as saved. */
  accountIds: string[];
}

/** The fields this form shows messages under; others go in the alert. */
const SHOWN_FIELDS = ["name", "account_id", "format", "mapping", "part_accounts", "transfer_rules"];

interface ProfileFormProps {
  /** The profile to edit; a new one when absent. */
  profile?: Profile;
  onDone: () => void;
}

/** The mapping text as a JSON object, or `null` when it is not one. */
function parsed(text: string): Record<string, unknown> | null {
  try {
    const value = JSON.parse(text) as unknown;
    return typeof value === "object" && value !== null && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function AccountOptions({ accounts }: { accounts: readonly Account[] }) {
  return (
    <>
      {accounts.map((a) => (
        <option key={a.id} value={a.id}>
          {a.name}
        </option>
      ))}
    </>
  );
}

/** Create or edit a statement profile, starting from a preset if wanted. */
export function ProfileForm({ profile, onDone }: ProfileFormProps) {
  const queryClient = useQueryClient();
  const presets = useQuery(presetsQuery).data ?? [];
  const accounts = useQuery(accountsQuery()).data ?? [];
  const own = accounts.filter((a) => a.type === "asset" || a.type === "liability");
  const assets = accounts.filter((a) => a.type === "asset");
  const [name, setName] = useState(profile?.name ?? "");
  const [accountId, setAccountId] = useState(profile?.account_id ?? "");
  const [format, setFormat] = useState(profile?.format ?? "xlsx");
  const [mappingText, setMappingText] = useState(
    profile ? JSON.stringify(profile.mapping, null, 2) : "{}",
  );
  const [parts, setParts] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      Object.entries((profile?.part_accounts ?? {}) as Record<string, string | null>).map(
        ([column, id]) => [column, id ?? ""],
      ),
    ),
  );
  const [rules, setRules] = useState<Rule[]>(
    (profile?.transfer_rules ?? []).map((r) => ({
      pattern: r.pattern,
      accountIds: [...r.account_ids],
    })),
  );
  const [jsonError, setJsonError] = useState<string | null>(null);
  const columns = partColumns(parsed(mappingText));

  const save = useMutation({
    mutationFn: async (body: ProfileRequest) =>
      profile
        ? await call(
            api.PUT("/api/v1/statement-profiles/{id}", {
              params: { path: { id: profile.id } },
              body,
            }),
          )
        : await call(api.POST("/api/v1/statement-profiles", { body })),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["statement-profiles"] });
      onDone();
    },
  });
  const errors = fieldErrors(save.error);

  const mapping = (): unknown => {
    const value = parsed(mappingText);
    if (value === null) throw new Error("The mapping is not a valid JSON object.");
    return value;
  };
  const submit = () => {
    const value = parsed(mappingText);
    if (value === null) {
      setJsonError("The mapping is not a valid JSON object.");
      return;
    }
    setJsonError(null);
    save.mutate({
      name,
      account_id: accountId || null,
      format,
      // Checked to be a plain object above; the schema types it as an open object.
      mapping: value as ProfileRequest["mapping"],
      part_accounts: Object.fromEntries(columns.map((column) => [column, parts[column] || null])),
      // A rule with no text and no account was never filled in.
      transfer_rules: rules
        .filter((r) => r.pattern.trim() !== "" || r.accountIds.some((id) => id !== ""))
        .map((r) => ({ pattern: r.pattern, account_ids: r.accountIds.filter((id) => id !== "") })),
    });
  };
  const applyPreset = (presetName: string) => {
    const preset = presets.find((p) => p.name === presetName);
    if (!preset) return;
    setMappingText(JSON.stringify(preset.mapping, null, 2));
    setFormat(preset.format);
    if (name.trim() === "") setName(preset.name);
  };

  return (
    <div className="flex flex-col gap-3 rounded-lg border p-4">
      {profile ? null : (
        <Field id="profile-preset" label="Start from">
          <NativeSelect
            id="profile-preset"
            defaultValue=""
            onChange={(e) => applyPreset(e.target.value)}
          >
            <option value="">Choose a bank…</option>
            {presets.map((p) => (
              <option key={p.name} value={p.name}>
                {p.name}
              </option>
            ))}
          </NativeSelect>
        </Field>
      )}
      <div className="grid gap-3 sm:grid-cols-3">
        <Field id="profile-name" label="Name" error={errors.name}>
          <Input id="profile-name" value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field id="profile-account" label="Account" error={errors.account_id}>
          <NativeSelect
            id="profile-account"
            value={accountId}
            onChange={(e) => setAccountId(e.target.value)}
          >
            <option value="">Choose when importing</option>
            <AccountOptions accounts={own} />
          </NativeSelect>
        </Field>
        <Field id="profile-format" label="Format" error={errors.format}>
          <NativeSelect
            id="profile-format"
            value={format}
            onChange={(e) => setFormat(e.target.value)}
          >
            <option value="xlsx">Excel (xlsx)</option>
            <option value="csv">CSV</option>
          </NativeSelect>
        </Field>
      </div>
      {columns.length > 0 ? (
        <fieldset className="grid gap-3 sm:grid-cols-2">
          <legend className="mb-1 text-sm font-medium">Paid from other accounts</legend>
          {columns.map((column, index) => (
            <Field key={column} id={`part-${index}`} label={column} error={errors.part_accounts}>
              <NativeSelect
                id={`part-${index}`}
                value={parts[column] ?? ""}
                onChange={(e) => setParts({ ...parts, [column]: e.target.value })}
              >
                <option value="">A note on the payment</option>
                <AccountOptions accounts={assets} />
              </NativeSelect>
            </Field>
          ))}
        </fieldset>
      ) : null}
      <fieldset className="flex flex-col gap-2">
        <legend className="mb-1 text-sm font-medium">Transfer rules</legend>
        {rules.map((rule, index) => (
          <div key={index} className="flex flex-wrap gap-2">
            <Input
              aria-label={`Rule ${index + 1} text`}
              placeholder="Text in the description, e.g. satispay"
              className="flex-1"
              value={rule.pattern}
              onChange={(e) =>
                setRules(rules.map((r, i) => (i === index ? { ...r, pattern: e.target.value } : r)))
              }
            />
            <NativeSelect
              aria-label={`Rule ${index + 1} account`}
              className="sm:w-48"
              value={rule.accountIds[0] ?? ""}
              onChange={(e) =>
                setRules(
                  rules.map((r, i) =>
                    i === index
                      ? { ...r, accountIds: [e.target.value, ...r.accountIds.slice(1)] }
                      : r,
                  ),
                )
              }
            >
              <option value="">Account…</option>
              <AccountOptions accounts={own} />
            </NativeSelect>
            <Button
              variant="outline"
              size="sm"
              aria-label={`Remove rule ${index + 1}`}
              onClick={() => setRules(rules.filter((_, i) => i !== index))}
            >
              <Trash2 />
            </Button>
          </div>
        ))}
        {errors.transfer_rules ? (
          <p className="text-xs text-destructive">{errors.transfer_rules}</p>
        ) : null}
        <div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setRules([...rules, { pattern: "", accountIds: [""] }])}
          >
            <Plus /> Add a transfer rule
          </Button>
        </div>
      </fieldset>
      <Field id="profile-mapping" label="Mapping (JSON)" error={jsonError ?? errors.mapping}>
        <textarea
          id="profile-mapping"
          className="min-h-40 rounded-md border bg-transparent p-2 font-mono text-xs"
          value={mappingText}
          onChange={(e) => {
            setMappingText(e.target.value);
            setJsonError(null);
          }}
        />
      </Field>
      <PreviewPanel format={format} mapping={mapping} accountId={accountId || null} />
      {save.error &&
      (Object.keys(errors).length === 0 ||
        Object.keys(errors).some((field) => !SHOWN_FIELDS.includes(field))) ? (
        <Alert>{explain(save.error)}</Alert>
      ) : null}
      <div className="flex gap-2">
        <Button disabled={save.isPending} onClick={submit}>
          Save profile
        </Button>
        <Button variant="outline" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
