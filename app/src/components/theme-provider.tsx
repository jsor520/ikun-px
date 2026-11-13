import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import {
  applyTheme,
  readThemePreference,
  resolveTheme,
  storeThemePreference,
  watchSystemTheme,
  type ResolvedTheme,
  type ThemePreference,
} from "@/lib/theme";

interface ThemeContextValue {
  preference: ThemePreference;
  theme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
  toggle: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(
    readThemePreference,
  );
  const [theme, setTheme] = useState<ResolvedTheme>(() =>
    resolveTheme(readThemePreference()),
  );

  useEffect(() => {
    const resolved = resolveTheme(preference);
    setTheme(resolved);
    applyTheme(resolved);

    if (preference !== "system") return;
    return watchSystemTheme(() => {
      const next = resolveTheme("system");
      setTheme(next);
      applyTheme(next);
    });
  }, [preference]);

  const setPreference = useCallback((next: ThemePreference) => {
    storeThemePreference(next);
    setPreferenceState(next);
  }, []);

  const toggle = useCallback(() => {
    setPreference(theme === "dark" ? "light" : "dark");
  }, [setPreference, theme]);

  const value = useMemo(
    () => ({ preference, theme, setPreference, toggle }),
    [preference, theme, setPreference, toggle],
  );

  return <ThemeContext value={value}>{children}</ThemeContext>;
}

export function useTheme() {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useTheme must be used inside <ThemeProvider>");
  return value;
}
