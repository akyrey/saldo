import { Dialog } from "radix-ui";
import { X } from "lucide-react";
import { EXTRA_KEYS, type Shortcut, SHORTCUTS } from "./shortcuts";

const GROUPS: readonly Shortcut["group"][] = ["Go to", "Actions", "General"];

function Keys({ keys }: { keys: string }) {
  return <kbd className="rounded border bg-muted px-1.5 py-0.5 font-mono text-xs">{keys}</kbd>;
}

/** The "?" overlay, generated from the shortcut table. */
export function ShortcutHelp({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/40" />
        <Dialog.Content className="fixed top-1/2 left-1/2 z-50 w-[min(28rem,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 rounded-lg border bg-background p-4 shadow-lg">
          <div className="mb-3 flex items-center justify-between">
            <Dialog.Title className="font-semibold">Keyboard shortcuts</Dialog.Title>
            <Dialog.Close aria-label="Close" className="rounded p-1 hover:bg-accent">
              <X className="size-4" />
            </Dialog.Close>
          </div>
          <Dialog.Description className="sr-only">
            Keys that work anywhere outside a text field.
          </Dialog.Description>
          {GROUPS.map((group) => (
            <section key={group} className="mb-3">
              <h3 className="mb-1 text-xs font-medium text-muted-foreground uppercase">{group}</h3>
              <ul className="grid gap-1 text-sm">
                {SHORTCUTS.filter((s) => s.group === group).map((s) => (
                  <li key={s.id} className="flex justify-between gap-2">
                    <span>{s.label}</span>
                    <Keys keys={s.keys.join(" then ")} />
                  </li>
                ))}
                {group === "General"
                  ? EXTRA_KEYS.map((k) => (
                      <li key={k.keys} className="flex justify-between gap-2">
                        <span>{k.label}</span>
                        <Keys keys={k.keys} />
                      </li>
                    ))
                  : null}
              </ul>
            </section>
          ))}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
