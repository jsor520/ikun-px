export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "ikun-px.theme";
const BACKGROUND: Record<ResolvedTheme, string> = {
  light: "#f4f5fa",
  dark: "#090b14",
};

function prefersDark() {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function readThemePreference(): ThemePreference {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark" || stored === "system") {
      return stored;
    }
  } catch {
    // Private mode or a locked-down webview: fall back to following the OS.
  }
  return "system";
}

export function storeThemePreference(preference: ThemePreference) {
  try {
    window.localStorage.setItem(STORAGE_KEY, preference);
  } catch {
    // Persistence is best-effort; the session still honours the choice.
  }
}

export function resolveTheme(preference: ThemePreference): ResolvedTheme {
  if (preference === "system") return prefersDark() ? "dark" : "light";
  return preference;
}

export function applyTheme(theme: ResolvedTheme) {
  const root = document.documentElement;
  root.classList.toggle("dark", theme === "dark");
  root.style.colorScheme = theme;
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute("content", BACKGROUND[theme]);
}

export function watchSystemTheme(onChange: () => void) {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

/**
 * Paints the stored theme before React mounts so the window never flashes the
 * wrong palette on startup.
 */
export function applyStoredThemeEagerly() {
  applyTheme(resolveTheme(readThemePreference()));
}
