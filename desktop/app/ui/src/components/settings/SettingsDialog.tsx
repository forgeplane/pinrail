// The settings dialog: a rail of sections, each a page of groups. Every
// control applies as it changes; nothing to save. Esc closes.

import { Bell, Blocks, Database, Info, Keyboard, Palette, Settings2, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Link } from "react-router";
import { Select } from "../Select";
import { api } from "../../api/client";
import type { Info as ServerInfo } from "../../api/types";
import { GLOBAL_SHORTCUT, SHORTCUTS } from "../../lib/shortcuts";
import { useLive } from "../../state/live";
import { useSettings } from "../../state/settings";
import { Tooltip } from "../Tooltip";
import { Segmented, Toggle } from "./controls";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";

export type SettingsSection = "general" | "appearance" | "shortcuts" | "plugins" | "data" | "about";

const SECTIONS: { key: SettingsSection; label: string; icon: ReactNode }[] = [
  { key: "general", label: "General", icon: <Settings2 size={15} /> },
  { key: "appearance", label: "Appearance", icon: <Palette size={15} /> },
  { key: "shortcuts", label: "Shortcuts", icon: <Keyboard size={15} /> },
  { key: "plugins", label: "Plugins", icon: <Blocks size={15} /> },
  { key: "data", label: "Data", icon: <Database size={15} /> },
  { key: "about", label: "About", icon: <Info size={15} /> },
];

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

export function SettingsDialog({ open, section, onSection, onClose }: { open: boolean; section: SettingsSection; onSection: (s: SettingsSection) => void; onClose: () => void }) {
  const { settings, update, native } = useSettings();
  const live = useLive();
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [copied, setCopied] = useState(false);
  const paused = pausedUntil(settings.notifications.paused_until);

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
      if (e.key === "Escape") {
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
      await navigator.clipboard.writeText(`http://127.0.0.1:${info.port}`);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      // the clipboard is not available here
    }
  };

  const plugins = [...live.plugins.values()];
  const broken = plugins.filter((p) => !p.usable).length;

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
                <SettingsRow label="Launch at login" description="Open Wicket when you sign in, in the menu bar" note={native ? undefined : "Only in the app"}>
                  <Toggle label="Launch at login" checked={settings.autostart === true} disabled={!native || settings.autostart === null} onChange={(v) => update({ autostart: v })} />
                </SettingsRow>
                <SettingsRow label="Closing the window" description={settings.close_window === "quit" ? "Quits Wicket; the tray goes with it" : "Hides it; Wicket stays in the menu bar until you quit"}>
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
                <SettingsRow label="Quiet hours and muted plugins" description="Coming next: no notifications between two times, and none for chosen plugins" />
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
                <SettingsRow label="Open the oldest pending review" description="Or the inbox when nothing is pending. Changing the keys is coming with settings in the core">
                  <Keys keys={[GLOBAL_SHORTCUT]} />
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

          {section === "plugins" ? (
            <SettingsPage title="Plugins">
              <SettingsGroup>
                <SettingsRow label="Registered plugins" description={`${plugins.length} plugin${plugins.length === 1 ? "" : "s"}${broken ? `, ${broken} broken` : ""}`}>
                  <Link to="/plugins" className="chrome-button" onClick={onClose}>
                    Open plugins
                  </Link>
                </SettingsRow>
                <SettingsRow label="Directories" description="Add, remove and reload plugin directories on the plugins page; installing from a path or a repository is coming" />
              </SettingsGroup>
            </SettingsPage>
          ) : null}

          {section === "data" ? (
            <SettingsPage title="Data">
              <SettingsGroup caption="Where things are">
                <SettingsRow label="Data directory" description={<span className="mono">{info?.data_dir ?? "…"}</span>} />
                <SettingsRow label="Server" description={info ? <span className="mono">http://127.0.0.1:{info.port}</span> : "…"}>
                  <button type="button" className="chrome-button" onClick={copyUrl} disabled={!info}>
                    {copied ? "Copied" : "Copy URL"}
                  </button>
                </SettingsRow>
              </SettingsGroup>
              <SettingsGroup caption="Housekeeping">
                <SettingsRow label="Export and import" description="A file with your reviews, decisions and settings; coming with settings in the core" />
                <SettingsRow label="Install the CLI" description="Put wicket on your PATH; coming with the packaged app" />
              </SettingsGroup>
            </SettingsPage>
          ) : null}

          {section === "about" ? (
            <SettingsPage title="About">
              <SettingsGroup>
                <SettingsRow label="Wicket" description={info ? `Version ${info.version} · server started ${new Date(info.started_at).toLocaleString()}` : "…"} />
                <SettingsRow label="Updates" description="Checking for updates comes with the packaged app" />
                <SettingsRow label="Plugins" description="How to write one: plugins/README.md in the repository" />
              </SettingsGroup>
            </SettingsPage>
          ) : null}
        </div>
      </div>
    </div>
  );
}
