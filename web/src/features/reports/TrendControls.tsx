import { useQuery } from "@tanstack/react-query";
import { Field } from "@/components/Field";
import { NativeSelect } from "@/components/ui/native-select";
import { categoriesQuery } from "@/features/categories/queries";
import { METRICS, type Metric, MONTH_CHOICES, type TrendSearch, YEAR_CHOICES } from "./trend-model";

interface TrendControlsProps {
  kind: "year-over-year" | "category-averages";
  value: Required<Pick<TrendSearch, "years" | "metric" | "months">> & TrendSearch;
  onChange: (changes: Partial<TrendSearch>) => void;
}

/** Years, metric and category for year over year; the number of months for averages. */
export function TrendControls({ kind, value, onChange }: TrendControlsProps) {
  const categories = useQuery(categoriesQuery);
  if (kind === "category-averages") {
    return (
      <div className="grid gap-3 sm:grid-cols-3">
        <Field id="report-months" label="Average over">
          <NativeSelect
            id="report-months"
            value={value.months}
            onChange={(e) => onChange({ months: Number(e.target.value) })}
          >
            {MONTH_CHOICES.map((m) => (
              <option key={m} value={m}>
                {m} months
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
    );
  }
  return (
    <div className="grid gap-3 sm:grid-cols-3">
      <Field id="report-years" label="Years">
        <NativeSelect
          id="report-years"
          value={value.years}
          onChange={(e) => onChange({ years: Number(e.target.value) })}
        >
          {YEAR_CHOICES.map((y) => (
            <option key={y} value={y}>
              {y} years
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="report-metric" label="Show">
        <NativeSelect
          id="report-metric"
          value={value.metric}
          onChange={(e) =>
            onChange({ metric: METRICS.find((m) => m.value === e.target.value)?.value as Metric })
          }
        >
          {METRICS.map((m) => (
            <option key={m.value} value={m.value}>
              {m.label}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="report-category" label="Category">
        <NativeSelect
          id="report-category"
          value={value.category_id ?? ""}
          onChange={(e) => onChange({ category_id: e.target.value || undefined })}
        >
          <option value="">Every category</option>
          <option value="none">No category</option>
          {(categories.data ?? []).map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </NativeSelect>
      </Field>
    </div>
  );
}
