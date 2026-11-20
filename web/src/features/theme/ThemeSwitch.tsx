import { Monitor, Moon, Sun } from "lucide-react";
import { cn } from "@/lib/utils";
import type { ThemeChoice } from "./theme";
import { useTheme } from "./context";

const OPTIONS: readonly { choice: ThemeChoice; label: string; icon: typeof Sun }[] = [
  { choice: "system", label: "System", icon: Monitor },
  { choice: "light", label: "Light", icon: Sun },
  { choice: "dark", label: "Dark", icon: Moon },
];

/** Three radio buttons: follow the system, or force light or dark (this device only). */
export function ThemeSwitch({ className }: { className?: string }) {
  const { choice, setChoice } = useTheme();
  return (
    <div role="radiogroup" aria-label="Theme" className={cn("flex gap-0.5", className)}>
      {OPTIONS.map(({ choice: option, label, icon: Icon }) => (
        <button
          key={option}
          type="button"
          role="radio"
          aria-checked={choice === option}
          aria-label={label}
          title={label}
          onClick={() => setChoice(option)}
          className={cn(
            "flex flex-1 items-center justify-center rounded px-2 py-1.5 text-muted-foreground hover:bg-accent",
            choice === option && "bg-accent text-foreground",
          )}
        >
          <Icon className="size-4" aria-hidden />
        </button>
      ))}
    </div>
  );
}
