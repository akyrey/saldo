import { type ReactNode, useEffect, useMemo, useState } from "react";
import { platform } from "@/platform";
import { ThemeContext, type ThemeState } from "./context";
import {
  applyTheme,
  prefersDark,
  readChoice,
  resolveTheme,
  type ThemeChoice,
  writeChoice,
} from "./theme";

/** Holds the theme choice, applies it, and follows the system while it is `system`. */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [choice, setChoiceState] = useState<ThemeChoice>(readChoice);
  const [systemDark, setSystemDark] = useState(prefersDark);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const resolved = resolveTheme(choice, systemDark);
  useEffect(() => applyTheme(resolved), [resolved]);
  useEffect(() => {
    // The desktop title bar: `null` hands it back to the OS setting.
    void platform()
      .setTheme(choice === "system" ? null : choice)
      .catch(() => {});
  }, [choice]);

  const value = useMemo<ThemeState>(
    () => ({
      choice,
      resolved,
      setChoice: (next) => {
        writeChoice(next);
        setChoiceState(next);
      },
    }),
    [choice, resolved],
  );
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}
