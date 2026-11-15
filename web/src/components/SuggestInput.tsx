import { type ComponentProps, type KeyboardEvent, useId, useState } from "react";
import { cn } from "@/lib/utils";
import { Input } from "./ui/input";

type SuggestInputProps = Omit<ComponentProps<"input">, "value" | "onChange"> & {
  value: string;
  onChange: (value: string) => void;
  /** Texts to offer under the field while the person types. */
  suggestions: readonly string[];
};

/**
 * A free-text field with suggestions (unlike `Combobox`, any text is allowed).
 * The list shows while typing; arrows move through it, Enter or a click takes
 * a suggestion, Escape hides it. Enter with nothing highlighted submits as usual.
 */
export function SuggestInput({
  value,
  onChange,
  suggestions,
  className,
  ...props
}: SuggestInputProps) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const shown = open ? suggestions : [];

  const close = () => {
    setOpen(false);
    setActive(-1);
  };
  const choose = (text: string) => {
    onChange(text);
    close();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (shown.length === 0) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((i) => (i + step + shown.length) % shown.length);
    } else if (event.key === "Enter" && shown[active] !== undefined) {
      event.preventDefault(); // take the suggestion; do not submit the form
      choose(shown[active]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  };

  return (
    <div className="relative">
      <Input
        {...props}
        role="combobox"
        aria-expanded={shown.length > 0}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={shown[active] !== undefined ? `${listId}-${active}` : undefined}
        autoComplete="off"
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
          setActive(-1);
        }}
        onBlur={close}
        onKeyDown={onKeyDown}
        className={className}
      />
      {shown.length > 0 ? (
        <ul
          id={listId}
          role="listbox"
          className="absolute z-30 mt-1 max-h-64 w-full overflow-auto rounded-md border bg-background p-1 shadow-md"
        >
          {shown.map((text, index) => (
            <li
              key={text}
              id={`${listId}-${index}`}
              role="option"
              aria-selected={index === active}
              // `mousedown` would blur the field (and close the list) first.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(text)}
              className={cn(
                "cursor-pointer rounded px-2 py-1.5 text-sm",
                index === active && "bg-accent",
              )}
            >
              {text}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
