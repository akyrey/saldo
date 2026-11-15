import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check } from "lucide-react";
import { Popover } from "radix-ui";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { cn } from "@/lib/utils";
import { CategoryDot } from "./CategoryDot";
import { CATEGORY_COLORS, colorClass } from "./colors";
import { categoriesQuery, type Category } from "./queries";

/** A swatch beside a category's name; clicking it offers the palette. */
export function CategoryColorPicker({ category }: { category: Category }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const save = useMutation({
    mutationFn: (color: string | null) =>
      call(
        api.PATCH("/api/v1/categories/{id}", {
          params: { path: { id: category.id } },
          body: { color },
        }),
      ),
    onSuccess: async () => {
      setOpen(false);
      await queryClient.invalidateQueries({ queryKey: categoriesQuery.queryKey });
    },
  });
  const current = category.color ?? null;

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          aria-label={`Colour of ${category.name}`}
          className="inline-flex size-6 items-center justify-center rounded-full hover:bg-accent"
        >
          {colorClass(current) ? (
            <CategoryDot color={current} className="size-3.5" />
          ) : (
            <span className="size-3.5 rounded-full border border-dashed border-muted-foreground" />
          )}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={4}
          className="z-50 w-56 rounded-md border bg-background p-2 shadow-md"
        >
          <div className="grid grid-cols-6 gap-1.5" role="group" aria-label="Colours">
            {CATEGORY_COLORS.map((color) => (
              <button
                key={color}
                type="button"
                aria-label={color}
                aria-pressed={color === current}
                disabled={save.isPending}
                onClick={() => save.mutate(color)}
                className={cn(
                  "flex size-7 items-center justify-center rounded-full ring-offset-2 ring-offset-background hover:ring-2 hover:ring-ring/50",
                  colorClass(color),
                )}
              >
                {color === current ? <Check className="size-4 text-white" aria-hidden /> : null}
              </button>
            ))}
          </div>
          <button
            type="button"
            disabled={save.isPending || current === null}
            onClick={() => save.mutate(null)}
            className="mt-2 w-full rounded px-2 py-1 text-left text-sm text-muted-foreground hover:bg-accent disabled:opacity-50"
          >
            No colour
          </button>
          {save.error ? (
            <p role="alert" className="mt-1 text-xs text-destructive">
              {errorMessage(save.error)}
            </p>
          ) : null}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
