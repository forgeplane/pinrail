// The shell owns the theme: dark by default, the choice kept in localStorage,
// stamped on the root element so the stylesheet and the plugins follow it.

export type Theme = "dark" | "light";

const KEY = "wicket:theme";

export function currentTheme(): Theme {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

export function applySavedTheme() {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem(KEY);
  } catch {
    // no storage: dark it is
  }
  document.documentElement.dataset.theme = saved === "light" ? "light" : "dark";
}

export function setTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // a preference that cannot be saved still applies for this session
  }
  window.dispatchEvent(new Event("wicket:appearance"));
}

export function toggleTheme() {
  setTheme(currentTheme() === "dark" ? "light" : "dark");
}
