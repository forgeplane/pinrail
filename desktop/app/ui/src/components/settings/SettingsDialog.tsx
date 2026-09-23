// The settings dialog: a rail of sections, each a page of groups. Every
// control applies as it changes; nothing to save. Esc closes.

import { Bell, Blocks, Database, FolderOpen, Info, Keyboard, Palette, Settings, Settings2, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Select } from "../Select";
import { api, inTauri } from "../../api/client";
import { copyText } from "../../lib/clipboard";
import { size } from "../../lib/format";
import type { Info as ServerInfo } from "../../api/types";
import { DEFAULT_GLOBAL_SHORTCUT, SHORTCUTS } from "../../lib/shortcuts";
import { useLive } from "../../state/live";
import { useSettings } from "../../state/settings";
import { Tooltip } from "../Tooltip";
import { Segmented, ShortcutRecorder, Toggle } from "./controls";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";
import { CliRow } from "./CliRow";
import { PluginsSection } from "./PluginsSection";

export type SettingsSection = "general" | "appearance" | "shortcuts" | "plugins" | "data" | "about";

const SECTIONS: { key: SettingsSection; label: string; icon: ReactNode }[] = [
  { key: "general", label: "General", icon: <Settings2 size={15} /> },
  { key: "appearance", label: "Appearance", icon: <Palette size={15} /> },
  { key: "shortcuts", label: "Shortcuts", icon: <Keyboard size={15} /> },
  { key: "plugins", label: "Plugins", icon: <Blocks size={15} /> },
  { key: "data", label: "Data", icon: <Database size={15} /> },
  { key: "about", label: "About", icon: <Info size={15} /> },
];

/** The port, committed on Enter or blur when it is a port and it changed. */
function PortField({ value, onChange }: { value: number; onChange: (port: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const port = parseInt(text, 10);
    if (Number.isNaN(port) || port < 1024 || port > 65535) {
      setText(String(value));
      return;
    }
    if (port !== value) onChange(port);
  };
  return <input className="settings-input" type="number" aria-label="Port" min={1024} max={65535} step={1} value={text} onChange={(e) => setText(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} data-setting-port />;
}

/** The pause as a moment still to come, or null. */
const pausedUntil = (iso: string | null) => {
  if (!iso) return null;
  const t = new Date(iso);
  return t.getTime() > Date.now() ? t : null;
};

const clock = (t: Date) => t.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

/** What a choice in the pause menu means, as a timestamp or null. */
const pauseUntil = (choice: string): string | null => {
  const now = new Date();
  if (choice === "15") return new Date(now.getTime() + 15 * 60_000).toISOString();
  if (choice === "60") return new Date(now.getTime() + 60 * 60_000).toISOString();
  if (choice === "tomorrow") return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 0).toISOString();
  return null;
};

/** What macOS reports for the app's notifications; null outside the app bundle. */
type NotificationStatus = { authorization: "authorized" | "denied" | "not_determined" | "provisional"; alert_style: "none" | "banner" | "alert"; alerts: boolean; sound: boolean; badge: boolean; shows: boolean };
type SystemState = { known: boolean; status: NotificationStatus | null };

/** Asked while the dialog is open, and again each time the window comes back (from System Settings, say). */
function useNotificationStatus(open: boolean): SystemState {
  const [state, setState] = useState<SystemState>({ known: false, status: null });
  useEffect(() => {
    if (!open || !inTauri()) return;
    let cancelled = false;
    const ask = () =>
      import("@tauri-apps/api/core").then(({ invoke }) =>
        invoke<NotificationStatus | null>("notification_status")
          .then((s) => !cancelled && setState({ known: true, status: s }))
          .catch(() => !cancelled && setState({ known: true, status: null })),
      );
    ask();
    window.addEventListener("focus", ask);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", ask);
    };
  }, [open]);
  return state;
}

const describeSystem = ({ known, status }: SystemState) => {
  if (!inTauri()) return "What macOS allows shows here in the app";
  if (!known) return "…";
  if (!status) return "Through the notification plugin in this development build; macOS reports nothing for it";
  if (status.authorization === "denied") return "Not allowed in System Settings";
  if (status.authorization === "not_determined") return "Not yet allowed; macOS asks the first time";
  if (status.alert_style === "none") return "Allowed, but the alert style is None in System Settings, so nothing appears";
  if (!status.alerts) return "Allowed, but alerts are off in System Settings";
  const parts = [status.alert_style === "alert" ? "Alerts" : "Banners", status.sound ? "sound on" : "sound off in System Settings", status.badge ? "badge" : "no badge"];
  return `Allowed: ${parts.join(", ")}`;
};

/** The shortcut as the app registered it; null in a browser. */
type ShortcutState = { shortcut: string; error: string | null };

function useShortcutState(open: boolean): ShortcutState | null {
  const [state, setState] = useState<ShortcutState | null>(null);
  useEffect(() => {
    if (!open || !inTauri()) return;
    let cancelled = false;
    let stop: (() => void) | undefined;
    (async () => {
      const [{ invoke }, { listen }] = await Promise.all([import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]);
      invoke<ShortcutState>("shortcut_state")
        .then((s) => !cancelled && setState(s))
        .catch(() => {});
      const unlisten = await listen<ShortcutState>("pinrail:shortcut", (e) => setState(e.payload));
      if (cancelled) unlisten();
      else stop = unlisten;
    })();
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [open]);
  return state;
}

const describeShortcut = (state: ShortcutState | null, wanted: string) => {
  if (!inTauri()) return "Registered by the app";
  if (!state || state.shortcut !== wanted) return undefined;
  return state.error ? `Not registered: ${state.error}` : undefined;
};

const openNotificationSettings = () => import("@tauri-apps/api/core").then(({ invoke }) => invoke("open_notification_settings")).catch(() => {});

const Keys = ({ keys }: { keys: string[][] }) => (
  <span className="settings-keys">
    {keys.map((combo, i) => (
      <span key={i} className="settings-combo">
        {combo.map((k, j) => (
          <kbd key={j}>{k}</kbd>
        ))}
      </span>
    ))}
  </span>
);

export function SettingsDialog({ open, section, plugin, onSection, onClose }: { open: boolean; section: SettingsSection; plugin?: string | null; onSection: (s: SettingsSection) => void; onClose: () => void }) {
  const { settings, update, native } = useSettings();
  const live = useLive();
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [copied, setCopied] = useState(false);
  const paused = pausedUntil(settings.notifications.paused_until);
  const system = useNotificationStatus(open);
  const [noticesError, setNoticesError] = useState<string | null>(null);
  const openNotices = () => {
    setNoticesError(null);
    import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke("open_notices"))
      .catch((e) => setNoticesError(String(e)));
  };
  const shortcut = useShortcutState(open);

  // a pause ends on its own: the row says so within the minute
  const [, tick] = useState(0);
  useEffect(() => {
    if (!open || !paused) return;
    const timer = window.setInterval(() => tick((n) => n + 1), 30_000);
    return () => window.clearInterval(timer);
  }, [open, paused]);

  useEffect(() => {
    if (!open) return;
    api.info().then(setInfo).catch(() => setInfo(null));
    const onKey = (e: KeyboardEvent) => {
      // Esc while recording a shortcut is the recorder's, and in the
      // install panel's field it closes the panel, not the dialog
      if (e.key === "Escape" && !document.querySelector(".shortcut-recorder.is-recording") && !document.activeElement?.closest("[data-install-panel]")) {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, onClose]);

  if (!open) return null;

  const copyUrl = async () => {
    if (!info) return;
    try {
      await copyText(`http://127.0.0.1:${info.port}`);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      // the clipboard is not available here
    }
  };

  return (
    <div className="app-dialog-backdrop" onMouseDown={onClose}>
      <div className="settings" role="dialog" aria-label="Settings" onMouseDown={(e) => e.stopPropagation()} data-settings>
        <nav className="settings-rail" aria-label="Settings sections">
          <div className="settings-rail-title">Settings</div>
          {SECTIONS.map((s) => (
            <button key={s.key} type="button" className={section === s.key ? "is-active" : ""} onClick={() => onSection(s.key)} data-section={s.key}>
              {s.icon}
              {s.label}
            </button>
          ))}
        </nav>
        <div className="settings-body">
          <Tooltip label="Close" keys={["Esc"]}>
            <button type="button" className="bar-button settings-close" onClick={onClose} aria-label="Close settings">
              <X size={16} />
            </button>
          </Tooltip>

          {section === "general" ? (
            <SettingsPage title="General">
              <SettingsGroup caption="Startup">
                <SettingsRow label="Launch at login" description="Open Pinrail when you sign in, in the menu bar" note={native ? undefined : "Only in the app"}>
                  <Toggle label="Launch at login" checked={settings.autostart === true} disabled={!native || settings.autostart === null} onChange={(v) => update({ autostart: v })} />
                </SettingsRow>
                <SettingsRow label="Closing the window" description={settings.close_window === "quit" ? "Quits Pinrail; the tray goes with it" : "Hides it; Pinrail stays in the menu bar until you quit"}>
                  <Segmented
                    label="Closing the window"
                    value={settings.close_window}
                    onChange={(v) => update({ close_window: v })}
                    options={[
                      { value: "hide", label: "Hide to tray" },
                      { value: "quit", label: "Quit" },
                    ]}
                  />
                </SettingsRow>
                <SettingsRow label="Show in the menu bar" description="The tray icon with the pending count and its menu; off leaves the Dock icon, the shortcut and notifications" note={native ? undefined : "Only in the app"}>
                  <Toggle label="Show in the menu bar" checked={settings.menu_bar_icon} disabled={!native} onChange={(v) => update({ menu_bar_icon: v })} />
                </SettingsRow>
              </SettingsGroup>
              <SettingsGroup caption="Notifications">
                <SettingsRow label="System notifications" description="A notification when a review arrives; the tray keeps its count either way">
                  <Toggle label="System notifications" checked={settings.notifications.enabled} onChange={(v) => update({ notifications: { enabled: v } })} />
                </SettingsRow>
                <SettingsRow label="Pause" description={paused ? `Nothing is announced until ${clock(paused)}; the tray's menu says so too` : "Nothing is announced while paused; the tray's menu offers the same"}>
                  <Select
                    label="Pause notifications"
                    icon={<Bell size={14} />}
                    value={paused ? "paused" : ""}
                    onChange={(v) => update({ notifications: { paused_until: pauseUntil(v) } })}
                    options={
                      paused
                        ? [
                            { value: "paused", label: `Paused until ${clock(paused)}` },
                            { value: "", label: "Resume" },
                          ]
                        : [
                            { value: "", label: "Not paused" },
                            { value: "15", label: "For 15 minutes" },
                            { value: "60", label: "For 1 hour" },
                            { value: "tomorrow", label: "Until tomorrow" },
                          ]
                    }
                  />
                </SettingsRow>
                <SettingsRow label="Sound" description="The system's notification sound with each one">
                  <Toggle label="Sound" checked={settings.notifications.sound} onChange={(v) => update({ notifications: { sound: v } })} />
                </SettingsRow>
                <SettingsRow label="System" description={describeSystem(system)}>
                  {system.status && !system.status.shows ? (
                    <button type="button" className="chrome-button" onClick={openNotificationSettings}>
                      <Settings size={14} /> Open System Settings
                    </button>
                  ) : null}
                </SettingsRow>
              </SettingsGroup>
            </SettingsPage>
          ) : null}

          {section === "appearance" ? (
            <SettingsPage title="Appearance">
              <SettingsGroup>
                <SettingsRow label="Theme" description="System follows macOS; T switches between dark and light">
                  <Segmented
                    label="Theme"
                    value={settings.appearance.theme}
                    onChange={(v) => update({ appearance: { theme: v } })}
                    options={[
                      { value: "system", label: "System" },
                      { value: "dark", label: "Dark" },
                      { value: "light", label: "Light" },
                    ]}
                  />
                </SettingsRow>
                <SettingsRow label="Text size" description="The size of the interface; plugin views keep their own">
                  <Segmented
                    label="Text size"
                    value={settings.appearance.text_size}
                    onChange={(v) => update({ appearance: { text_size: v } })}
                    options={[
                      { value: "small", label: "Small" },
                      { value: "default", label: "Default" },
                      { value: "large", label: "Large" },
                    ]}
                  />
                </SettingsRow>
              </SettingsGroup>
            </SettingsPage>
          ) : null}

          {section === "shortcuts" ? (
            <SettingsPage title="Shortcuts">
              <SettingsGroup caption="Anywhere on the Mac">
                <SettingsRow label="Open Pinrail" description="Click the keys and press a new combination; it needs ⌘, ⌃ or ⌥" note={describeShortcut(shortcut, settings.shortcut.global)}>
                  <ShortcutRecorder label="Global shortcut" value={settings.shortcut.global} onChange={(v) => update({ shortcut: { global: v } })} />
                  {settings.shortcut.global !== DEFAULT_GLOBAL_SHORTCUT ? (
                    <button type="button" className="chrome-button settings-reset" onClick={() => update({ shortcut: { global: DEFAULT_GLOBAL_SHORTCUT } })}>
                      Reset
                    </button>
                  ) : null}
                </SettingsRow>
                <SettingsRow label="It opens" description={settings.shortcut.global_opens === "inbox" ? "The inbox, whatever is pending" : "The oldest pending review, or the inbox when nothing is pending"}>
                  <Segmented
                    label="The shortcut opens"
                    value={settings.shortcut.global_opens}
                    onChange={(v) => update({ shortcut: { global_opens: v } })}
                    options={[
                      { value: "oldest", label: "Oldest review" },
                      { value: "inbox", label: "Inbox" },
                    ]}
                  />
                </SettingsRow>
              </SettingsGroup>
              <SettingsGroup caption="In the app">
                {SHORTCUTS.map((s) => (
                  <SettingsRow key={s.what} label={s.what}>
                    <Keys keys={s.keys} />
                  </SettingsRow>
                ))}
              </SettingsGroup>
            </SettingsPage>
          ) : null}

          {section === "plugins" ? <PluginsSection focus={plugin ?? null} /> : null}

          {section === "data" ? (
            <SettingsPage title="Data">
              <SettingsGroup caption="Where things are">
                <SettingsRow label="Data directory" description={<span className="mono">{info?.data_dir ?? "…"}</span>} note="The database, the settings file, and under plugins/ the installed copies, build logs and scratch">
                  {native ? (
                    <Tooltip label="Show in Finder">
                      <button type="button" className="bar-button" aria-label="Reveal the data directory" disabled={!info} onClick={() => info && import("@tauri-apps/plugin-opener").then(({ revealItemInDir }) => revealItemInDir(info.data_dir).catch(() => {}))}>
                        <FolderOpen size={15} />
                      </button>
                    </Tooltip>
                  ) : null}
                </SettingsRow>
                <SettingsRow label="Server" description={info ? <span className="mono">http://127.0.0.1:{info.port}</span> : "…"}>
                  <button type="button" className="chrome-button" onClick={copyUrl} disabled={!info}>
                    {copied ? "Copied" : "Copy URL"}
                  </button>
                </SettingsRow>
                <SettingsRow label="Port" description="Where the server listens for the CLI and the agents" note={info && settings.port !== info.port ? <span>Takes effect when Pinrail starts next; until then the server stays on {info.port}. The CLI follows either.</span> : undefined}>
                  <PortField value={settings.port} onChange={(port) => update({ port })} />
                </SettingsRow>
              </SettingsGroup>
              <SettingsGroup caption="History">
                <SettingsRow label="Keep reviews for" description="Decided, withdrawn, discarded and expired reviews older than this are removed; pending ones stay">
                  <Select
                    label="Keep reviews for"
                    value={settings.history.keep_days === null ? "forever" : String(settings.history.keep_days)}
                    options={[
                      { value: "forever", label: "Forever" },
                      { value: "30", label: "30 days" },
                      { value: "90", label: "90 days" },
                      { value: "365", label: "A year" },
                    ]}
                    onChange={(v) => update({ history: { keep_days: v === "forever" ? null : Number(v) } })}
                  />
                </SettingsRow>
                <SettingsRow
                  label="Files sent with reviews"
                  description={
                    info?.artifacts ? (
                      <span data-artifact-totals>
                        {info.artifacts.count === 0
                          ? "None stored"
                          : `${info.artifacts.count} file${info.artifacts.count === 1 ? "" : "s"}, ${size(info.artifacts.bytes)}. They go with their reviews`}
                      </span>
                    ) : (
                      "…"
                    )
                  }
                />
              </SettingsGroup>
              <SettingsGroup caption="Command line">
                <CliRow open={open && section === "data"} />
              </SettingsGroup>
            </SettingsPage>
          ) : null}

          {section === "about" ? (
            <SettingsPage title="About">
              <SettingsGroup>
                <SettingsRow label="Pinrail" description={info ? `Version ${info.version} · server started ${new Date(info.started_at).toLocaleString()}` : "…"} />
                <SettingsRow label="Updates" description="Checking for updates comes with the packaged app" />
                <SettingsRow label="Plugins" description="How to write one: plugins/README.md in the repository" />
                <SettingsRow label="License" description="Apache License 2.0" note={noticesError ?? undefined}>
                  {native ? (
                    <button type="button" className="chrome-button" onClick={openNotices} data-open-notices>
                      Third-party notices
                    </button>
                  ) : null}
                </SettingsRow>
              </SettingsGroup>
            </SettingsPage>
          ) : null}
        </div>
      </div>
    </div>
  );
}
