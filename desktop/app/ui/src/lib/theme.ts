import { useEffect, useState } from "react";

// The shell owns the theme. The preference is dark, light or system (the
// OS's choice, followed live); the resolved theme is stamped on the root
// element so the stylesheet and the plugins follow it.

export type Theme = "dark" | "light";
export type ThemePreference = Theme | "system";

const KEY = "wicket:theme";
const media = typeof window !== "undefined" && window.matchMedia ? window.matchMedia("(prefers-color-scheme: light)") : null;
let following = false;

export function currentTheme(): Theme {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

export function themePreference(): ThemePreference {
  try {
    const saved = localStorage.getItem(KEY);
    return saved === "light" || saved === "dark" || saved === "system" ? saved : "system";
  } catch {
    return "system";
  }
}

const resolve = (pref: ThemePreference): Theme => (pref === "system" ? (media?.matches ? "light" : "dark") : pref);

function apply(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  window.dispatchEvent(new Event("wicket:appearance"));
}

const onMediaChange = () => {
  if (following) apply(resolve("system"));
};

export function applySavedTheme() {
  setThemePreference(themePreference(), false);
}

export function setThemePreference(pref: ThemePreference, save = true) {
  following = pref === "system";
  media?.removeEventListener("change", onMediaChange);
  if (following) media?.addEventListener("change", onMediaChange);
  apply(resolve(pref));
  if (save) {
    try {
      localStorage.setItem(KEY, pref);
    } catch {
      // a preference that cannot be saved still applies for this session
    }
  }
}

/** Kept for the T key: an explicit flip of whatever is showing. */
export function toggleTheme() {
  setThemePreference(currentTheme() === "dark" ? "light" : "dark");
}

/** The resolved theme as it changes, for controls that show it. */
export function useTheme(): Theme {
  const [theme, setThemeState] = useState<Theme>(currentTheme);
  useEffect(() => {
    const update = () => setThemeState(currentTheme());
    window.addEventListener("wicket:appearance", update);
    return () => window.removeEventListener("wicket:appearance", update);
  }, []);
  return theme;
}
