import { createContext, useContext } from "react";
import type { ResolvedTheme, ThemeChoice } from "./theme";

export interface ThemeState {
  choice: ThemeChoice;
  resolved: ResolvedTheme;
  setChoice: (choice: ThemeChoice) => void;
}

// Without a provider (isolated component tests) the theme is fixed to light.
export const ThemeContext = createContext<ThemeState>({
  choice: "system",
  resolved: "light",
  setChoice: () => {},
});

export function useTheme(): ThemeState {
  return useContext(ThemeContext);
}
