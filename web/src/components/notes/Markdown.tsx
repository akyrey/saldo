// Loaded on first use (React.lazy in NotesView), so the Markdown parser stays
// out of the initial route.
import type * as React from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { platform } from "@/platform";

/** GitHub-flavoured Markdown (tables, task lists, autolinks) and a line break per newline. */
const plugins = [remarkGfm, remarkBreaks];

/** Links leave the app: a new tab in the browser, the system browser on desktop. */
function ExternalLink({ href, children }: React.ComponentProps<"a">) {
  function open(event: React.MouseEvent<HTMLAnchorElement>) {
    if (!href) return;
    event.preventDefault();
    void platform().openExternal(href);
  }
  return (
    <a href={href} onClick={open} rel="noopener noreferrer">
      {children}
    </a>
  );
}

const components: Components = { a: ExternalLink };

/**
 * Renders notes written in Markdown. Raw HTML is shown as text and unsafe link
 * targets (`javascript:`) are dropped, which are react-markdown's defaults.
 */
export default function Markdown({ text }: { text: string }) {
  return (
    <div className="saldo-markdown">
      <ReactMarkdown remarkPlugins={plugins} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
}
