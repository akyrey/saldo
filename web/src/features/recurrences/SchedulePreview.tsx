import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { errorMessage } from "@/api/errors";
import { previewQuery, type PreviewRequest } from "./queries";

/** How long the schedule must stay unchanged before asking the server. */
const DEBOUNCE_MS = 300;

/** The next dates the schedule fires on, computed by the server. */
export function SchedulePreview({ request }: { request: PreviewRequest | null }) {
  // Compare by content: the parent builds a new object on every render.
  const key = JSON.stringify(request);
  const [settled, setSettled] = useState(request);
  useEffect(() => {
    const timer = setTimeout(
      () => setSettled(JSON.parse(key) as PreviewRequest | null),
      DEBOUNCE_MS,
    );
    return () => clearTimeout(timer);
  }, [key]);
  const preview = useQuery({
    ...previewQuery(settled ?? { first_date: "", repetitions: [] }),
    enabled: settled !== null,
  });

  return (
    <section aria-label="Upcoming dates" className="rounded-lg border p-3">
      <h2 className="mb-1 text-sm font-medium">Upcoming dates</h2>
      {preview.error ? (
        <p className="text-sm text-muted-foreground">{errorMessage(preview.error)}</p>
      ) : preview.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">No upcoming dates.</p>
      ) : (
        <ul className="flex flex-wrap gap-2 text-sm tabular-nums">
          {preview.data?.map((date) => (
            <li key={date}>{date}</li>
          ))}
        </ul>
      )}
    </section>
  );
}
