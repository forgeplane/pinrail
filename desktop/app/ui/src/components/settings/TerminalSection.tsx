// Settings › Terminal: the terminal of an agent that runs in Pinrail, and
// the view a session opens in.

import { useSettings } from "../../state/settings";
import { isMac } from "../../lib/keys";
import { failedRenderers, loadedRenderer } from "../terminal/view";
import { Select } from "../Select";
import { Segmented, Toggle } from "./controls";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";

/** The renderers a person can choose, by the setting's value. */
const RENDERERS: Record<string, string> = { ghostty: "libghostty", xterm: "xterm.js" };

/** The choice the setting's list stands for: one renderer, Automatic, or
 *  a list of the settings file's own. */
function rendererChoice(setting: string[]): string {
  if (setting.length === 0 || (setting.length === 1 && setting[0] === "auto")) return "auto";
  if (setting.length === 1 && RENDERERS[setting[0]]) return setting[0];
  return "custom";
}

/** What the renderer row says: which one draws the terminals now, and why
 *  xterm.js does when libghostty could not load. */
function rendererNote(): string {
  const later = "A change applies to terminals opened afterwards.";
  const loaded = loadedRenderer();
  if (!loaded) return `Automatic uses libghostty, and xterm.js where libghostty cannot run. ${later}`;
  const why = failedRenderers().get("ghostty");
  const inUse = `In use: ${RENDERERS[loaded] ?? loaded}.`;
  if (loaded !== "ghostty" && why) return `${inUse} libghostty could not load: ${why}. ${later}`;
  return `${inUse} ${later}`;
}

export function TerminalSection() {
  const { settings, update } = useSettings();
  const terminal = settings.runtime.terminal;
  const renderer = rendererChoice(terminal.renderer);
  const setTerminal = (change: Partial<typeof terminal>) => update({ runtime: { terminal: change } });
  return (
    <SettingsPage title="Terminal">
      <SettingsGroup caption="The terminal of an agent that runs in Pinrail.">
        <SettingsRow label="Renderer" description={rendererNote()}>
          <Select
            label="Renderer"
            value={renderer}
            onChange={(v) => v !== "custom" && setTerminal({ renderer: [v] })}
            options={[
              { value: "auto", label: "Automatic" },
              { value: "ghostty", label: "libghostty" },
              { value: "xterm", label: "xterm.js" },
              ...(renderer === "custom" ? [{ value: "custom", label: terminal.renderer.join(", ") }] : []),
            ]}
          />
        </SettingsRow>
        {isMac ? (
          <SettingsRow
            label="Option as Meta"
            description="The Option keys that act as Meta in a terminal, sending Esc and the key, rather than typing the character Option makes."
          >
            <Select
              label="Option as Meta"
              value={terminal.option_as_meta}
              onChange={(v) => setTerminal({ option_as_meta: v as typeof terminal.option_as_meta })}
              options={[
                { value: "off", label: "Off" },
                { value: "left", label: "Left Option" },
                { value: "right", label: "Right Option" },
                { value: "both", label: "Both" },
              ]}
            />
          </SettingsRow>
        ) : null}
        <SettingsRow
          label="Screen reader"
          description="Makes a terminal's text readable by screen readers such as VoiceOver and Orca, and announces new output."
        >
          <Toggle
            label="Screen reader"
            checked={terminal.screen_reader}
            onChange={(v) => setTerminal({ screen_reader: v })}
          />
        </SettingsRow>
        <SettingsRow
          label="Let programs copy to the clipboard"
          description="Programs in a terminal can copy text to the clipboard, up to 1 MiB at a time. They can never read it."
        >
          <Toggle
            label="Let programs copy to the clipboard"
            checked={terminal.clipboard_write}
            onChange={(v) => setTerminal({ clipboard_write: v })}
          />
        </SettingsRow>
      </SettingsGroup>
      <SettingsGroup caption="Sessions">
        <SettingsRow
          label="Open a session in"
          description="Last used opens a session in the view you last used for it, or else in the summary."
        >
          <Segmented
            label="Open a session in"
            value={settings.runtime.default_view}
            onChange={(v) => update({ runtime: { default_view: v } })}
            options={[
              { value: "last", label: "Last used" },
              { value: "summary", label: "Summary" },
              { value: "terminal", label: "Terminal" },
            ]}
          />
        </SettingsRow>
        <SettingsRow
          label="Open an agent you start in"
          description="The view the session of an agent you just started opens in."
        >
          <Segmented
            label="Open an agent you start in"
            value={settings.runtime.new_run_view}
            onChange={(v) => update({ runtime: { new_run_view: v } })}
            options={[
              { value: "summary", label: "Summary" },
              { value: "terminal", label: "Terminal" },
            ]}
          />
        </SettingsRow>
      </SettingsGroup>
    </SettingsPage>
  );
}
