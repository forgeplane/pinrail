// The Appearance page's theme rows: the theme for each colour scheme, and
// the folder that holds a person's own themes.

import { useState } from "react";
import { api } from "../../api/client";
import { currentTheme, type Theme } from "../../lib/theme";
import { chooseTheme, chosenTheme, newThemeId, themesOf } from "../../lib/themes";
import { useSettings } from "../../state/settings";
import { Select } from "../Select";
import { SettingsRow } from "./layout";

const SCHEMES: { scheme: Theme; label: string; setting: "dark_theme" | "light_theme" }[] = [
  { scheme: "dark", label: "Dark theme", setting: "dark_theme" },
  { scheme: "light", label: "Light theme", setting: "light_theme" },
];

async function open(command: string, args?: Record<string, string>) {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke(command, args).catch(() => {});
}

export function ThemeRows() {
  const { settings, themes, update, native } = useSettings();
  const { appearance } = settings;
  const [failure, setFailure] = useState<string | null>(null);
  const invalid = themes.filter((theme) => theme.error);

  const createTheme = async () => {
    const scheme = currentTheme();
    const chosen = appearance[scheme === "dark" ? "dark_theme" : "light_theme"];
    const id = newThemeId(themes);
    setFailure(null);
    try {
      await api.createTheme(id, chosenTheme(themes, chosen, scheme)?.id ?? scheme);
      await update(chooseTheme(id, scheme, appearance.theme));
      if (native) await open("open_theme_file", { id });
    } catch (error) {
      setFailure(error instanceof Error ? error.message : "The theme could not be created.");
    }
  };

  return (
    <>
      {SCHEMES.map(({ scheme, label, setting }) => (
        <SettingsRow key={scheme} label={label}>
          <Select
            label={label}
            value={chosenTheme(themes, appearance[setting], scheme)?.id ?? scheme}
            options={themesOf(themes, scheme).map((theme) => ({ value: theme.id, label: theme.name }))}
            onChange={(id) => void update(chooseTheme(id, scheme, appearance.theme))}
          />
        </SettingsRow>
      ))}
      <SettingsRow
        label="Custom themes"
        description="A theme is a JSON file in the themes folder. Saving the file updates the app."
        note={
          failure || invalid.length ? (
            <>
              {failure ? <div>{failure}</div> : null}
              {invalid.map((theme) => (
                <div key={theme.id}>
                  {theme.id}.json: {theme.error}
                </div>
              ))}
            </>
          ) : undefined
        }
      >
        {native ? (
          <button type="button" className="chrome-button" onClick={() => void open("open_themes_folder")}>
            Open folder
          </button>
        ) : null}
        <button type="button" className="chrome-button" onClick={() => void createTheme()}>
          New theme…
        </button>
      </SettingsRow>
    </>
  );
}
