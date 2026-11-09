import { describe, expect, test } from "vitest";
import type { ThemeEntry } from "../api/types";
import { chooseTheme, chosenTheme, newThemeId } from "./themes";

const theme = (id: string, scheme: "dark" | "light", error: string | null = null): ThemeEntry => ({
  id,
  name: id,
  scheme,
  source: "user",
  shell: {},
  plugin: {},
  warnings: [],
  error,
});

describe("chosenTheme", () => {
  const themes = [theme("nord", "dark"), theme("broken", "dark", "bad colour"), theme("paper", "light")];
  test("is the valid theme of the scheme that the setting names", () => {
    expect(chosenTheme(themes, "nord", "dark")?.id).toBe("nord");
  });
  test("is none for a theme with an error, or of the other scheme", () => {
    expect(chosenTheme(themes, "broken", "dark")).toBeUndefined();
    expect(chosenTheme(themes, "paper", "dark")).toBeUndefined();
  });
});

describe("chooseTheme", () => {
  test("sets the scheme's theme and switches to the scheme", () => {
    expect(chooseTheme("nord", "dark", "light")).toEqual({ appearance: { dark_theme: "nord", theme: "dark" } });
    expect(chooseTheme("paper", "light", "dark")).toEqual({ appearance: { light_theme: "paper", theme: "light" } });
  });
  test("leaves the scheme to the system when it follows it", () => {
    expect(chooseTheme("nord", "dark", "system")).toEqual({ appearance: { dark_theme: "nord" } });
  });
});

test("newThemeId is the first free my-theme id", () => {
  expect(newThemeId([])).toBe("my-theme");
  expect(newThemeId([theme("my-theme", "dark")])).toBe("my-theme-2");
  expect(newThemeId([theme("my-theme", "dark"), theme("my-theme-2", "dark"), theme("my-theme-4", "dark")])).toBe(
    "my-theme-3",
  );
});
