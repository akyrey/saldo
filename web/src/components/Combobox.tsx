import { X } from "lucide-react";
import { type KeyboardEvent, useId, useMemo, useState } from "react";
import { cn } from "@/lib/utils";
import { type ComboboxOption, filterOptions } from "./combobox-filter";
import { fieldClass, noCorrection } from "./ui/input";

export type { ComboboxOption } from "./combobox-filter";

/** More options than this are cut off until the person types more. */
const RENDER_LIMIT = 100;

/** An extra last row that creates what was typed, such as a new account. */
export interface ComboboxCreate {
  /** The row's text for the typed name, or `null` to offer nothing (blank names never ask). */
  label: (text: string) => string | null;
  onCreate: (text: string) => void;
}

interface ComboboxProps {
  id?: string;
  /** The field's name, for the clear button ("Clear Category"). */
  label?: string;
  options: readonly ComboboxOption[];
  /** The chosen option's value; `""` means nothing is chosen. */
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  /** Optional fields get a button that empties them. */
  clearable?: boolean;
  disabled?: boolean;
  className?: string;
  create?: ComboboxCreate;
  "aria-label"?: string;
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
}

/**
 * A text field that suggests options as you type (a combobox): faster than
 * scrolling a long `<select>`. Typing never changes the value by itself;
 * only choosing an option does, so leaving the field keeps the old choice.
 */
export function Combobox(props: ComboboxProps) {
  const { options, value, onChange, clearable = false, create } = props;
  const listId = useId();
  const chosen = options.find((o) => o.value === value);
  const [text, setText] = useState<string | null>(null); // null: not editing
  const [active, setActive] = useState(0);
  const open = text !== null;
  const matches = useMemo(
    () => (open ? filterOptions(options, text).slice(0, RENDER_LIMIT) : []),
    [open, options, text],
  );
  const typed = text?.trim() ?? "";
  const createLabel = create && typed !== "" ? create.label(typed) : null;
  const rowCount = matches.length + (createLabel ? 1 : 0);

  const close = () => {
    setText(null);
    setActive(0);
  };
  const choose = (option: ComboboxOption) => {
    onChange(option.value);
    close();
  };
  const createTyped = () => {
    create?.onCreate(typed);
    close();
  };
  /** Row `index`: an option, or the create row after the last one. */
  const pick = (index: number) => {
    const option = matches[index];
    if (option) choose(option);
    else if (createLabel) createTyped();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) setText("");
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((i) => (rowCount === 0 ? 0 : (i + step + rowCount) % rowCount));
    } else if (event.key === "Enter" && open) {
      event.preventDefault(); // choose; do not submit the form
      if (rowCount > 0) pick(Math.min(active, rowCount - 1));
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      close();
    }
  };

  const activeId = open && active < rowCount ? `${listId}-${active}` : undefined;
  return (
    <div className={cn("relative", props.className)}>
      <input
        id={props.id}
        type="text"
        role="combobox"
        aria-label={props["aria-label"]}
        aria-invalid={props["aria-invalid"]}
        aria-describedby={props["aria-describedby"]}
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={activeId}
        autoComplete="off"
        {...noCorrection}
        disabled={props.disabled}
        placeholder={props.placeholder ?? "Type to search…"}
        value={text ?? chosen?.label ?? ""}
        onFocus={(e) => {
          setText("");
          e.currentTarget.select();
        }}
        onChange={(e) => {
          setText(e.target.value);
          setActive(0);
        }}
        onBlur={close}
        onKeyDown={onKeyDown}
        className={cn(fieldClass, clearable && value !== "" && "pr-8")}
      />
      {clearable && value !== "" && !props.disabled ? (
        <button
          type="button"
          aria-label={`Clear ${props.label ?? props["aria-label"] ?? "choice"}`}
          onClick={() => onChange("")}
          className="absolute top-1/2 right-2 -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:bg-accent"
        >
          <X className="size-4" aria-hidden />
        </button>
      ) : null}
      {open ? (
        <ul
          id={listId}
          role="listbox"
          className="absolute z-30 mt-1 max-h-64 w-full overflow-auto rounded-md border bg-background p-1 shadow-md"
        >
          {rowCount === 0 ? (
            <li className="px-2 py-1.5 text-sm text-muted-foreground">No matches</li>
          ) : (
            matches.map((option, index) => (
              <li
                key={option.value}
                id={`${listId}-${index}`}
                role="option"
                aria-selected={index === active}
                // `mousedown` would blur the field (and close the list) first.
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => choose(option)}
                className={cn(
                  "flex cursor-pointer items-center justify-between gap-2 rounded px-2 py-1.5 text-sm",
                  index === active && "bg-accent",
                  option.value === value && "font-medium",
                )}
              >
                <span>{option.label}</span>
                {option.hint ? (
                  <span className="text-xs text-muted-foreground">{option.hint}</span>
                ) : null}
              </li>
            ))
          )}
          {createLabel ? (
            <li
              id={`${listId}-${matches.length}`}
              role="option"
              aria-selected={active === matches.length}
              onMouseDown={(e) => e.preventDefault()}
              onClick={createTyped}
              className={cn(
                "cursor-pointer rounded px-2 py-1.5 text-sm",
                active === matches.length && "bg-accent",
              )}
            >
              <span>{createLabel}</span>
            </li>
          ) : null}
        </ul>
      ) : null}
    </div>
  );
}
