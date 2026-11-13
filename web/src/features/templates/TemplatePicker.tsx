import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Combobox } from "@/components/Combobox";
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
    <Combobox
      aria-label="New from template"
      className="w-56"
      placeholder="New from template…"
      value={value}
      onChange={(id) => {
        void navigate({ to: "/transactions/new", search: { template: id || undefined } });
      }}
      options={templates.data.map((t) => ({ value: t.id, label: t.name }))}
    />
  );
}
