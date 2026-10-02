// The diagnostics a feedback report can include: how Pinrail is installed,
// its plugins and its settings. Nothing from a review, no paths and no user
// name; the person sees the text before sending it.

import type { Info, Plugin, ServerSettings } from "../api/types";
import { size } from "./format";

/** What the shell says of the command-line tool, as `cli_status` answers. */
export type CliState = {
  mode: "link" | "package" | "copy";
  installed: boolean;
  outdated: boolean;
  dir_on_path: boolean | null;
};

const INSTALLS: Record<CliState["mode"], string> = {
  link: "macOS app",
  package: ".deb or .rpm package",
  copy: "AppImage",
};

function cliLine(cli: CliState): string {
  if (!cli.installed) return "not installed";
  if (cli.outdated) return "installed, from another version";
  if (cli.dir_on_path === false) return "installed, not on the PATH";
  return "installed";
}

const onOff = (on: boolean) => (on ? "on" : "off");

export function describeDiagnostics({
  info,
  plugins,
  settings,
  cli,
  now = Date.now(),
}: {
  info: Info | null;
  plugins: Plugin[] | null;
  settings: ServerSettings | null;
  /** null outside the app, in a browser */
  cli: CliState | null;
  now?: number;
}): string {
  const lines: string[] = [];
  lines.push(`Pinrail: ${info?.version ?? "unknown"}`);
  lines.push(`Installed as: ${cli ? INSTALLS[cli.mode] : "not known (not the app)"}`);
  if (cli) lines.push(`Command-line tool: ${cliLine(cli)}`);
  if (info) {
    lines.push(`Running since: ${info.started_at}`);
    if (info.attachments) {
      lines.push(`Stored attachments: ${info.attachments.count}, ${size(info.attachments.bytes)}`);
    }
  }

  if (plugins) {
    lines.push("", `Plugins (${plugins.length}):`);
    for (const plugin of [...plugins].sort((a, b) => a.plugin.localeCompare(b.plugin))) {
      const notes = [plugin.dev ? "linked for development" : null, plugin.usable ? null : "not usable"].filter(Boolean);
      lines.push(`  ${plugin.plugin} ${plugin.version}${notes.length ? ` (${notes.join(", ")})` : ""}`);
    }
  }

  if (settings) {
    const n = settings.notifications;
    // a pause that has ended stays in the settings until the next one
    const pausedUntil = n.paused_until && Date.parse(n.paused_until) > now ? n.paused_until : null;
    lines.push(
      "",
      "Settings:",
      `  Theme: ${settings.appearance.theme}, text size ${settings.appearance.text_size}`,
      `  Notifications: ${onOff(n.enabled)}, sound ${onOff(n.sound)}${pausedUntil ? `, paused until ${pausedUntil}` : ""}`,
      `  Closing the window: ${settings.close_window === "quit" ? "quits" : "hides the window"}`,
      `  Menu bar icon: ${onOff(settings.menu_bar_icon)}`,
      `  History kept: ${settings.history.keep_days === null ? "forever" : `${settings.history.keep_days} days`}`,
      `  Automatic update checks: ${onOff(settings.updates?.check ?? true)}`,
      `  Port: ${settings.port}`,
    );
  }
  return lines.join("\n");
}
