import { describe, expect, test } from "vitest";
import type { Info, Plugin, ServerSettings } from "../api/types";
import { describeDiagnostics } from "./diagnostics";

const info: Info = {
  version: "0.1.1",
  data_dir: "/Users/maya/.local/share/pinrail",
  port: 4747,
  pid: 4242,
  started_at: "2026-10-01T09:00:00Z",
  user: "maya",
  attachments: { count: 27, bytes: 1354093 },
};

const plugin = (name: string, more: Partial<Plugin> = {}) =>
  ({
    name,
    version: "1.2.0",
    line: "1",
    path: `/Users/maya/plugins/${name}`,
    dev: false,
    usable: true,
    ...more,
  }) as Plugin;

const settings = {
  appearance: { theme: "system", text_size: "default" },
  sidebar: { open: true },
  close_window: "hide",
  menu_bar_icon: true,
  notifications: { enabled: true, paused_until: null, sound: false, muted_plugins: [] },
  shortcut: { global: "", global_opens: "oldest" },
  plugins: {},
  port: 4747,
  history: { keep_days: 30 },
  updates: { check: true },
} as ServerSettings;

describe("describeDiagnostics", () => {
  test("says how Pinrail is installed, its plugins and its settings", () => {
    const text = describeDiagnostics({
      info,
      plugins: [plugin("model", { usable: false }), plugin("artifact", { dev: true, version: "0.3.0" })],
      settings,
      cli: { mode: "copy", installed: true, outdated: false, dir_on_path: false },
    });
    expect(text).toBe(
      [
        "Pinrail: 0.1.1",
        "Installed as: AppImage",
        "Command-line tool: installed, not on the PATH",
        "Running since: 2026-10-01T09:00:00Z",
        "Stored attachments: 27, 1.3 MB",
        "",
        "Plugins (2):",
        "  artifact 0.3.0 (linked for development)",
        "  model 1.2.0 (not usable)",
        "",
        "Settings:",
        "  Theme: system, text size default",
        "  Notifications: on, sound off",
        "  Closing the window: hides the window",
        "  Menu bar icon: on",
        "  History kept: 30 days",
        "  Automatic update checks: on",
        "  Port: 4747",
      ].join("\n"),
    );
  });

  test("names a pause of the notifications only while it lasts", () => {
    const now = Date.parse("2026-10-01T12:00:00Z");
    const paused = (until: string) =>
      describeDiagnostics({
        info: null,
        plugins: null,
        settings: { ...settings, notifications: { ...settings.notifications, paused_until: until } },
        cli: null,
        now,
      });
    expect(paused("2026-09-27T17:33:05Z")).toContain("  Notifications: on, sound off\n");
    expect(paused("2026-10-01T13:00:00Z")).toContain(
      "  Notifications: on, sound off, paused until 2026-10-01T13:00:00Z\n",
    );
  });

  test("leaves out paths and the user name", () => {
    const text = describeDiagnostics({ info, plugins: [plugin("model")], settings, cli: null });
    expect(text).not.toContain("/Users/maya");
    expect(text).not.toContain("maya");
  });

  test("says what it could not find out", () => {
    expect(describeDiagnostics({ info: null, plugins: null, settings: null, cli: null })).toBe(
      "Pinrail: unknown\nInstalled as: not known (not the app)",
    );
  });
});
