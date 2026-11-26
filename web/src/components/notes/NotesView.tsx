import { lazy, Suspense } from "react";
import { cn } from "@/lib/utils";

const Markdown = lazy(() => import("./Markdown"));

/** Notes rendered as Markdown; the plain text shows while the renderer loads. */
export function NotesView({ text, className }: { text: string; className?: string }) {
  return (
    <div className={cn("text-sm", className)}>
      <Suspense fallback={<p className="whitespace-pre-wrap">{text}</p>}>
        <Markdown text={text} />
      </Suspense>
    </div>
  );
}
