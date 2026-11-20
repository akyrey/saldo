// The colour theme: what the person chose (per device) and what it resolves
// to. `index.html` runs the same logic inline before the first paint, so the
// page never flashes the wrong theme; keep the two in step.

export type ThemeChoice = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export const THEME_KEY = "saldo.theme";
export const THEME_CHOICES: readonly ThemeChoice[] = ["system", "light", "dark"];

/** The browser bar colour per theme: the brand teal, or the dark background. */
const THEME_COLOR: Record<ResolvedTheme, string> = { light: "#0f766e", dark: "#0a0a0a" };

export function parseChoice(value: string | null): ThemeChoice {
  return THEME_CHOICES.find((c) => c === value) ?? "system";
}

export function resolveTheme(choice: ThemeChoice, prefersDark: boolean): ResolvedTheme {
  if (choice === "system") return prefersDark ? "dark" : "light";
  return choice;
}

/** The stored choice; `system` when nothing is stored or storage is blocked. */
export function readChoice(): ThemeChoice {
  try {
    return parseChoice(localStorage.getItem(THEME_KEY));
  } catch {
    return "system";
  }
}

/** Remembers the choice; a blocked storage only means it is asked again next time. */
export function writeChoice(choice: ThemeChoice): void {
  try {
    localStorage.setItem(THEME_KEY, choice);
  } catch {
    // Private mode or blocked site data: the theme still applies for this visit.
  }
}

export function prefersDark(): boolean {
  return typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : false;
}

/** Switches the page: the `dark` class the tokens hang on, form controls, the browser bar. */
export function applyTheme(theme: ResolvedTheme): void {
  const root = document.documentElement;
  root.classList.toggle("dark", theme === "dark");
  root.style.colorScheme = theme;
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", THEME_COLOR[theme]);
}
