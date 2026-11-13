import type * as React from "react";

export function PageHeader({ title, actions }: { title: string; actions?: React.ReactNode }) {
  return (
    <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
      <h1 className="text-xl font-semibold">{title}</h1>
      {actions ? <div className="flex gap-2">{actions}</div> : null}
    </div>
  );
}
