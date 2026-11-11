import { useCallback, useEffect, useState } from "react";
import {
  applyTheme,
  getStoredTheme,
  setStoredTheme,
  type ThemeMode,
} from "@/lib/theme";

export function useTheme() {
  const [theme, setThemeState] = useState<ThemeMode>(() => getStoredTheme());

  useEffect(() => {
    applyTheme(theme);
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onSystemChange = () => {
      if (getStoredTheme() === "system") {
        applyTheme("system");
      }
    };
    media.addEventListener("change", onSystemChange);
    return () => media.removeEventListener("change", onSystemChange);
  }, [theme]);

  const setTheme = useCallback((mode: ThemeMode) => {
    setStoredTheme(mode);
    setThemeState(mode);
    applyTheme(mode);
  }, []);

  return { theme, setTheme };
}
