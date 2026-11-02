// The themes a person can choose: which one a setting names, and the change
// that chooses one.

import type { ThemeEntry } from "../api/types";
import type { Theme, ThemePreference } from "./theme";

/** Valid themes of a scheme, as Settings offers them. */
export const themesOf = (themes: ThemeEntry[], scheme: Theme) =>
  themes.filter((theme) => theme.scheme === scheme && !theme.error);

/** The theme a setting names, or none when it is missing, invalid or of the other scheme. */
export const chosenTheme = (themes: ThemeEntry[], id: string, scheme: Theme) =>
  themesOf(themes, scheme).find((theme) => theme.id === id);

/**
 * The change that chooses a theme. It also switches to the theme's scheme, so
 * the choice shows, unless the scheme follows the operating system.
 */
export const chooseTheme = (id: string, scheme: Theme, current: ThemePreference) => ({
  appearance: {
    [scheme === "dark" ? "dark_theme" : "light_theme"]: id,
    ...(current === "system" ? {} : { theme: scheme }),
  },
});

/** The first free id for a new theme: my-theme, my-theme-2 and so on. */
export function newThemeId(themes: ThemeEntry[]) {
  const taken = new Set(themes.map((theme) => theme.id));
  if (!taken.has("my-theme")) return "my-theme";
  let n = 2;
  while (taken.has(`my-theme-${n}`)) n += 1;
  return `my-theme-${n}`;
}
