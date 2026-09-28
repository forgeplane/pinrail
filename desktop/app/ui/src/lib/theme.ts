import { useEffect, useState } from "react";

// The shell owns the theme. The preference is dark, light or system (the
// OS's choice, followed live); the resolved theme is stamped on the root
// element so the stylesheet and the plugins follow it.

export type Theme = "dark" | "light";
export type ThemePreference = Theme | "system";

// the last appearance the core reported, for the first paint before the
// server answers; the server's value wins as soon as it arrives
const CACHE = "pinrail:appearance";
const media =
  typeof window !== "undefined" && window.matchMedia ? window.matchMedia("(prefers-color-scheme: light)") : null;
let following = false;

export function currentTheme(): Theme {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

export type Appearance = { theme: ThemePreference; text_size: "small" | "default" | "large" };

export function cachedAppearance(): Appearance {
  try {
    const saved = JSON.parse(localStorage.getItem(CACHE) ?? "{}") as Partial<Appearance>;
    return {
      theme: saved.theme === "light" || saved.theme === "dark" || saved.theme === "system" ? saved.theme : "system",
      text_size: saved.text_size === "small" || saved.text_size === "large" ? saved.text_size : "default",
    };
  } catch {
    return { theme: "system", text_size: "default" };
  }
}

export function cacheAppearance(appearance: Appearance) {
  try {
    localStorage.setItem(CACHE, JSON.stringify(appearance));
  } catch {
    // no cache: the first paint is the default until the server answers
  }
}

const resolve = (pref: ThemePreference): Theme => (pref === "system" ? (media?.matches ? "light" : "dark") : pref);

function apply(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  window.dispatchEvent(new Event("pinrail:appearance"));
}

const onMediaChange = () => {
  if (following) apply(resolve("system"));
};

/** The cached appearance, before the server answers. */
export function applySavedTheme() {
  setThemePreference(cachedAppearance().theme);
}

export function setThemePreference(pref: ThemePreference) {
  following = pref === "system";
  media?.removeEventListener("change", onMediaChange);
  if (following) media?.addEventListener("change", onMediaChange);
  apply(resolve(pref));
}

/** ⌘⇧L asks for an explicit flip; the settings store records it. */
export function toggleTheme() {
  window.dispatchEvent(
    new CustomEvent("pinrail:theme-toggle", { detail: currentTheme() === "dark" ? "light" : "dark" }),
  );
}

/** The resolved theme as it changes, for controls that show it. */
export function useTheme(): Theme {
  const [theme, setThemeState] = useState<Theme>(currentTheme);
  useEffect(() => {
    const update = () => setThemeState(currentTheme());
    window.addEventListener("pinrail:appearance", update);
    return () => window.removeEventListener("pinrail:appearance", update);
  }, []);
  return theme;
}
