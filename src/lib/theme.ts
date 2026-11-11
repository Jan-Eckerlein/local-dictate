export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "local-dictate-theme";

export function getStoredTheme(): ThemeMode {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw === "light" || raw === "dark" || raw === "system") {
    return raw;
  }
  return "system";
}

export function setStoredTheme(mode: ThemeMode): void {
  localStorage.setItem(STORAGE_KEY, mode);
}

export function resolveDark(mode: ThemeMode): boolean {
  if (mode === "dark") return true;
  if (mode === "light") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function applyTheme(mode: ThemeMode): void {
  document.documentElement.classList.toggle("dark", resolveDark(mode));
}
