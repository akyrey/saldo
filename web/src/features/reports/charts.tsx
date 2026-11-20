// The only module that imports Recharts. It is reached through lazily loaded
// report and bill pages, so the chart library never weighs on the first screen.
import {
  Bar,
  BarChart,
  CartesianGrid,
  ComposedChart,
  Legend,
  Line,
  LineChart,
  ReferenceArea,
  ReferenceLine,
  ResponsiveContainer,
  Scatter,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { formatMoney } from "@/api/money";
import type {
  DuePoint,
  DueStatus,
  HistoryChart,
  PaymentPoint,
} from "@/features/bills/history-model";
import type { ChartRow } from "./report-model";

/** Series colours: the validated blue/orange pair from `index.css`. */
const FIRST = "var(--chart-1)";
const SECOND = "var(--chart-2)";
const AXIS = { stroke: "var(--muted-foreground)", fontSize: 12 };

interface ChartProps {
  rows: readonly ChartRow[];
  currency: string;
  /** Series names, e.g. ["Income", "Expenses"]. */
  names: readonly [string, string];
  /** Accessible description of the chart. */
  label: string;
}

/** A tooltip showing the exact decimal strings, never the plotted floats. */
function ExactTooltip({
  active,
  payload,
  names,
  currency,
  only,
}: {
  active?: boolean;
  payload?: readonly { payload?: unknown }[];
  names: readonly [string, string];
  currency: string;
  only?: boolean;
}) {
  const row = payload?.[0]?.payload as ChartRow | undefined;
  if (!active || !row) return null;
  return (
    <div className="rounded-md border bg-background px-3 py-2 text-xs shadow-sm">
      <p className="mb-1 font-medium">{row.label}</p>
      <p>
        {names[0]}: {formatMoney(row.firstText, currency)}
      </p>
      {only ? null : (
        <p>
          {names[1]}: {formatMoney(row.secondText, currency)}
        </p>
      )}
    </div>
  );
}

const compact = (value: number) =>
  new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(value);

/** Two series side by side per interval (income/expenses, budgeted/spent). */
export function PairedBars({ rows, currency, names, label }: ChartProps) {
  return (
    <div role="img" aria-label={label} className="h-72 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={[...rows]} barGap={2} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
          <CartesianGrid vertical={false} stroke="var(--border)" />
          <XAxis dataKey="label" tick={AXIS} tickLine={false} axisLine={false} />
          <YAxis tick={AXIS} tickLine={false} axisLine={false} tickFormatter={compact} width={48} />
          <Tooltip
            cursor={{ fill: "var(--muted)" }}
            content={(props) => <ExactTooltip {...props} names={names} currency={currency} />}
          />
          <Legend wrapperStyle={{ fontSize: 12 }} />
          <Bar dataKey="first" name={names[0]} fill={FIRST} radius={[4, 4, 0, 0]} maxBarSize={24} />
          <Bar
            dataKey="second"
            name={names[1]}
            fill={SECOND}
            radius={[4, 4, 0, 0]}
            maxBarSize={24}
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

/** A horizontal line across a chart, such as a goal's target. */
export interface ChartReference {
  value: number;
  label: string;
}

/** One value over time (net worth). The title names the series. */
export function ValueLine({
  rows,
  currency,
  names,
  label,
  references = [],
}: ChartProps & { references?: readonly ChartReference[] }) {
  return (
    <div role="img" aria-label={label} className="h-72 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={[...rows]} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
          <CartesianGrid vertical={false} stroke="var(--border)" />
          <XAxis dataKey="label" tick={AXIS} tickLine={false} axisLine={false} />
          <YAxis tick={AXIS} tickLine={false} axisLine={false} tickFormatter={compact} width={48} />
          <Tooltip
            content={(props) => <ExactTooltip {...props} names={names} currency={currency} only />}
          />
          <Line
            dataKey="first"
            name={names[0]}
            stroke={FIRST}
            strokeWidth={2}
            dot={{ r: 4 }}
            isAnimationActive={false}
          />
          {references.map((r) => (
            <ReferenceLine
              key={r.label}
              y={r.value}
              stroke="var(--muted-foreground)"
              strokeDasharray="4 4"
              label={{ value: r.label, position: "insideTopLeft", fill: "var(--muted-foreground)" }}
              ifOverflow="extendDomain"
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

/** One bar per item, largest first (categories, tags). */
export function RankedBars({ rows, currency, names, label }: ChartProps) {
  const height = Math.max(160, rows.length * 32 + 32);
  return (
    <div role="img" aria-label={label} className="w-full" style={{ height }}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart
          data={[...rows]}
          layout="vertical"
          margin={{ top: 0, right: 8, bottom: 0, left: 0 }}
        >
          <CartesianGrid horizontal={false} stroke="var(--border)" />
          <XAxis
            type="number"
            tick={AXIS}
            tickLine={false}
            axisLine={false}
            tickFormatter={compact}
          />
          <YAxis
            type="category"
            dataKey="label"
            tick={AXIS}
            tickLine={false}
            axisLine={false}
            width={110}
          />
          <Tooltip
            cursor={{ fill: "var(--muted)" }}
            content={(props) => <ExactTooltip {...props} names={names} currency={currency} only />}
          />
          <Bar dataKey="first" name={names[0]} fill={FIRST} radius={[0, 4, 4, 0]} maxBarSize={20} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

/**
 * One line per year over January to December. The newest year is the
 * first series colour, the year before the second; older years are thin
 * grey lines, so the two that matter stand out.
 */
export function YearLines({
  rows,
  years,
  label,
}: {
  rows: readonly Record<string, unknown>[];
  years: readonly number[];
  label: string;
}) {
  const newest = years.length - 1;
  const stroke = (index: number) =>
    index === newest ? FIRST : index === newest - 1 ? SECOND : "var(--muted-foreground)";
  return (
    <div role="img" aria-label={label} className="h-72 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={[...rows]} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
          <CartesianGrid vertical={false} stroke="var(--border)" />
          <XAxis dataKey="label" tick={AXIS} tickLine={false} axisLine={false} />
          <YAxis tick={AXIS} tickLine={false} axisLine={false} tickFormatter={compact} width={48} />
          <Tooltip />
          <Legend />
          {years.map((year, index) => (
            <Line
              key={year}
              dataKey={String(year)}
              name={String(year)}
              stroke={stroke(index)}
              strokeWidth={index >= newest - 1 ? 2 : 1}
              strokeDasharray={index >= newest - 1 ? undefined : "4 4"}
              dot={false}
              connectNulls={false}
              isAnimationActive={false}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

/** Colour and marker per due-date status; the shape tells them apart without colour. */
const DUE_STYLE: Record<
  DueStatus,
  { label: string; fill: string; shape: "circle" | "cross" | "diamond" | "triangle" }
> = {
  paid: { label: "Paid", fill: "var(--kind-deposit)", shape: "circle" },
  skipped: { label: "Skipped", fill: "var(--muted-foreground)", shape: "cross" },
  open: { label: "Open", fill: SECOND, shape: "diamond" },
  overdue: { label: "Overdue", fill: "var(--destructive)", shape: "triangle" },
  scheduled: { label: "Scheduled", fill: "var(--border)", shape: "circle" },
};

const MONTH_TICK = new Intl.DateTimeFormat(undefined, {
  month: "short",
  year: "2-digit",
  timeZone: "UTC",
});
const YEAR_TICK = new Intl.DateTimeFormat(undefined, { year: "numeric", timeZone: "UTC" });

/** A payment's or a due date's details, with the exact decimal strings. */
function HistoryTooltip({
  active,
  payload,
  currency,
}: {
  active?: boolean;
  payload?: readonly { payload?: unknown }[];
  currency: string;
}) {
  const point = payload?.[0]?.payload as PaymentPoint | DuePoint | undefined;
  if (!active || !point) return null;
  return (
    <div className="rounded-md border bg-background px-3 py-2 text-xs shadow-sm">
      <p className="mb-1 font-medium">{point.date}</p>
      {"amount" in point ? (
        <>
          <p>{point.description}</p>
          <p>{formatMoney(point.amount, currency)}</p>
          <p className="text-muted-foreground">
            {point.dueDate ? `Paid the bill due ${point.dueDate}` : "Not matched to a due date"}
          </p>
        </>
      ) : (
        <p>Due date: {DUE_STYLE[point.status].label.toLowerCase()}</p>
      )}
    </div>
  );
}

/** A bill's payments over its whole duration, with the expected range and its due dates. */
export function PaymentHistoryChart({
  chart,
  currency,
  label,
}: {
  chart: HistoryChart;
  currency: string;
  label: string;
}) {
  const tick = (value: number) =>
    (chart.tickUnit === "year" ? YEAR_TICK : MONTH_TICK).format(new Date(value));
  return (
    <div role="img" aria-label={label} className="h-72 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
          <CartesianGrid vertical={false} stroke="var(--border)" />
          <XAxis
            dataKey="x"
            type="number"
            scale="time"
            domain={chart.domain}
            ticks={chart.ticks}
            tickFormatter={tick}
            tick={AXIS}
            tickLine={false}
            axisLine={false}
            allowDataOverflow
          />
          <YAxis
            dataKey="y"
            type="number"
            domain={[0, "auto"]}
            tick={AXIS}
            tickLine={false}
            axisLine={false}
            tickFormatter={compact}
            width={48}
          />
          <Tooltip
            cursor={{ stroke: "var(--muted-foreground)", strokeDasharray: "4 4" }}
            content={(props) => <HistoryTooltip {...props} currency={currency} />}
          />
          <Legend wrapperStyle={{ fontSize: 12 }} />
          {chart.band.min === chart.band.max ? (
            <ReferenceLine
              y={chart.band.max}
              stroke="var(--muted-foreground)"
              strokeOpacity={0.5}
            />
          ) : (
            <ReferenceArea
              y1={chart.band.min}
              y2={chart.band.max}
              fill="var(--muted)"
              fillOpacity={0.8}
              stroke="var(--muted-foreground)"
              strokeOpacity={0.3}
              ifOverflow="extendDomain"
            />
          )}
          {chart.today === null ? null : (
            <ReferenceLine
              x={chart.today}
              stroke="var(--muted-foreground)"
              strokeDasharray="4 4"
              label={{
                value: "Today",
                position: "insideTopRight",
                fill: "var(--muted-foreground)",
                fontSize: 12,
              }}
            />
          )}
          <Line
            data={chart.payments}
            dataKey="y"
            name="Payments"
            stroke={FIRST}
            strokeWidth={2}
            dot={{ r: 4 }}
            isAnimationActive={false}
          />
          {chart.dues.map((series) => (
            <Scatter
              key={series.status}
              data={series.points}
              name={DUE_STYLE[series.status].label}
              fill={DUE_STYLE[series.status].fill}
              shape={DUE_STYLE[series.status].shape}
              isAnimationActive={false}
            />
          ))}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}
