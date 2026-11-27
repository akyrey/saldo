import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Badge } from "@/components/ui/badge";
import { accountImportsQuery } from "./queries";

/** The account's latest statement imports, each linking to its review. */
export function StatementImportsHistory({ accountId }: { accountId: string }) {
  const imports = useQuery(accountImportsQuery(accountId));
  const items = imports.data?.data ?? [];
  if (items.length === 0) return null;
  return (
    <section aria-label="Statement imports" className="mt-6 flex flex-col gap-2">
      <h2 className="text-sm font-semibold">Statement imports</h2>
      <ul className="divide-y rounded-md border text-sm">
        {items.map((item) => (
          <li key={item.id} className="flex items-center gap-3 px-3 py-2">
            <Link
              to="/statement-imports/$importId"
              params={{ importId: item.id }}
              className="min-w-0 flex-1 truncate underline-offset-2 hover:underline"
            >
              {item.filename} · {item.created_at.slice(0, 10)}
            </Link>
            <Badge>{item.status}</Badge>
          </li>
        ))}
      </ul>
    </section>
  );
}
