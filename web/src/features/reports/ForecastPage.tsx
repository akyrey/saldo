import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { TriangleAlert } from "lucide-react";
import { Amount } from "@/components/Amount";
import { Field } from "@/components/Field";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { NativeSelect } from "@/components/ui/native-select";
import { accountsQuery, currenciesQuery, placesOf } from "@/features/accounts/queries";
import { todayIso } from "@/lib/dates";
import { ValueLine } from "./charts";
import {
  DEFAULT_HORIZON,
  forecastRows,
  forecastSpan,
  groupByDate,
  HORIZONS,
  isBelowZero,
  type ForecastCurrency,
} from "./forecast-model";
import { forecastQuery } from "./queries";

const SOURCE_LABELS: Record<string, string> = {
  transaction: "Entered",
  recurrence: "Recurring",
  bill: "Bill",
};

interface CurrencyForecastProps {
  forecast: ForecastCurrency;
  places: number;
  accountNames: ReadonlyMap<string, string>;
}

/** The chart, the lowest point and the planned items of one currency. */
function CurrencyForecast({ forecast, places, accountNames }: CurrencyForecastProps) {
  const { currency, lowest } = forecast;
  return (
    <section aria-label={`Forecast in ${currency}`} className="flex flex-col gap-3">
      <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1 text-sm">
        <span>
          Today: <Amount value={forecast.start_balance} currency={currency} />
        </span>
        <span data-testid={`forecast-lowest-${currency}`}>
          Lowest: <Amount value={lowest.balance} currency={currency} /> on {lowest.date}
        </span>
      </div>
      {isBelowZero(lowest.balance) ? (
        <Alert className="flex items-center gap-2">
          <TriangleAlert aria-hidden className="size-4" />
          The balance goes below zero on {lowest.date}.
        </Alert>
      ) : null}
      <ValueLine
        rows={forecastRows(forecast, places)}
        currency={currency}
        names={["Balance", ""]}
        label={`Projected ${currency} balance at the end of each interval`}
      />
      {forecast.items.length === 0 ? (
        <p className="text-sm text-muted-foreground">Nothing planned in this period.</p>
      ) : (
        <ol className="divide-y rounded-md border text-sm">
          {groupByDate(forecast.items).map((group) => (
            <li key={group.date} className="flex flex-col gap-1 px-3 py-2">
              <span className="font-medium">{group.date}</span>
              <ul className="flex flex-col gap-1">
                {group.items.map((item, index) => (
                  <li
                    key={`${item.source_id}-${index}`}
                    className="flex flex-wrap justify-between gap-2"
                  >
                    <span>
                      {item.title}{" "}
                      <span className="text-xs text-muted-foreground">
                        {SOURCE_LABELS[item.source] ?? item.source}
                        {item.account_id
                          ? ` · ${accountNames.get(item.account_id) ?? "account"}`
                          : " · account unknown"}
                      </span>
                    </span>
                    <Amount value={item.amount} currency={currency} />
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

/** Balances projected from recurrences, bills and transactions entered ahead. */
export function ForecastPage() {
  const search = useSearch({ from: "/app/forecast" });
  const { scope } = useSearch({ from: "/app" });
  const navigate = useNavigate({ from: "/forecast" });
  const months = search.months ?? DEFAULT_HORIZON;
  const span = forecastSpan(todayIso(), months);
  const currencies = useQuery(currenciesQuery).data;
  const accounts = useQuery(accountsQuery(scope)).data ?? [];
  const covered = accounts.filter((a) => a.type === "asset" || a.type === "liability");
  const accountNames = new Map(accounts.map((a) => [a.id, a.name]));
  const report = useQuery(forecastQuery({ ...span, scope, account_id: search.account }));

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Forecast"
        actions={
          <Button variant="outline" asChild>
            <Link to="/reports">Dashboard</Link>
          </Button>
        }
      />
      <div className="flex flex-wrap items-end gap-3">
        <div role="group" aria-label="Horizon" className="flex gap-1">
          {HORIZONS.map((h) => (
            <Button
              key={h}
              size="sm"
              variant={h === months ? "default" : "outline"}
              aria-pressed={h === months}
              onClick={() => void navigate({ search: (prev) => ({ ...prev, months: h }) })}
            >
              {h < 12 ? `${h} mo` : `${h / 12} yr`}
            </Button>
          ))}
        </div>
        <Field id="forecast-account" label="Account">
          <NativeSelect
            id="forecast-account"
            value={search.account ?? ""}
            onChange={(e) =>
              void navigate({
                search: (prev) => ({ ...prev, account: e.target.value || undefined }),
              })
            }
          >
            <option value="">All in net worth</option>
            {covered.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
      <QueryState isPending={report.isPending} error={report.error} />
      {report.data?.currencies.map((c) => (
        <CurrencyForecast
          key={c.currency}
          forecast={c}
          places={placesOf(currencies, c.currency)}
          accountNames={accountNames}
        />
      ))}
    </div>
  );
}
