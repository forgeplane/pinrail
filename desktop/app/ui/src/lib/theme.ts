import { useEffect, useState } from "react";

// The shell owns the theme. The preference is dark, light or system (the
// OS's choice, followed live); the resolved scheme is stamped on the root
// element so the stylesheet and the plugins follow it. A custom theme for
// each scheme is an overlay of CSS variables on the root element's style.

export type Theme = "dark" | "light";
export type ThemePreference = Theme | "system";

// the last appearance the core reported, for the first paint before the
// server answers; the server's value wins as soon as it arrives
const CACHE = "pinrail:appearance";
const OVERLAYS_CACHE = "pinrail:theme-overlays";
const media =
  typeof window !== "undefined" && window.matchMedia ? window.matchMedia("(prefers-color-scheme: light)") : null;
let following = false;

/** The CSS variables of a custom theme; empty for a built-in one. */
export type Overlay = { shell: Record<string, string>; plugin: Record<string, string> };
export type Overlays = Record<Theme, Overlay>;

const NO_OVERLAY: Overlay = { shell: {}, plugin: {} };
let overlays: Overlays = { dark: NO_OVERLAY, light: NO_OVERLAY };
let appliedVariables: string[] = [];

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
  const root = document.documentElement;
  root.dataset.theme = theme;
  for (const name of appliedVariables) root.style.removeProperty(name);
  const { shell } = overlays[theme];
  for (const [name, value] of Object.entries(shell)) root.style.setProperty(name, value);
  appliedVariables = Object.keys(shell);
  window.dispatchEvent(new Event("pinrail:appearance"));
}

const onMediaChange = () => {
  if (following) apply(resolve("system"));
};

/** The cached appearance, before the server answers. */
export function applySavedTheme() {
  try {
    const saved = JSON.parse(localStorage.getItem(OVERLAYS_CACHE) ?? "null") as Overlays | null;
    if (saved?.dark?.shell && saved.light?.shell) overlays = saved;
  } catch {
    // no cache: the built-in colours until the server answers
  }
  setThemePreference(cachedAppearance().theme);
}

/** The active custom theme's colours for plugin views. */
export const currentTokens = () => overlays[currentTheme()].plugin;

/** The themes chosen for the two schemes, as the server last listed them. */
export function setThemeOverlays(next: Overlays) {
  if (JSON.stringify(next) === JSON.stringify(overlays)) return;
  overlays = next;
  try {
    localStorage.setItem(OVERLAYS_CACHE, JSON.stringify(next));
  } catch {
    // no cache: the first paint is the built-in theme until the server answers
  }
  apply(currentTheme());
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
