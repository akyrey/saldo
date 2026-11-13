import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { NativeSelect } from "@/components/ui/native-select";
import { templatesQuery } from "./queries";

interface TemplatePickerProps {
  /** The template the editor currently shows, if any. */
  value?: string;
}

/** "New from template": opens the transaction editor filled from a template. */
export function TemplatePicker({ value = "" }: TemplatePickerProps) {
  const navigate = useNavigate();
  const templates = useQuery(templatesQuery);
  if (!templates.data || templates.data.length === 0) return null;
  return (
    <NativeSelect
      aria-label="New from template"
      className="w-auto"
      value={value}
      onChange={(event) => {
        const template = event.target.value || undefined;
        void navigate({ to: "/transactions/new", search: { template } });
      }}
    >
      <option value="">New from template…</option>
      {templates.data.map((t) => (
        <option key={t.id} value={t.id}>
          {t.name}
        </option>
      ))}
    </NativeSelect>
  );
}
