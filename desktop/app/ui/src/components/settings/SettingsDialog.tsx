// The settings dialog: a rail of sections, each a page of groups. Every
// control applies as it changes; nothing to save. Esc closes.

import {
  Bell,
  Blocks,
  Bot,
  Database,
  FlaskConical,
  FolderOpen,
  Info,
  Keyboard,
  ListChecks,
  Palette,
  Settings,
  Settings2,
  ShieldCheck,
  X,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { Select } from "../Select";
import { api, inTauri } from "../../api/client";
import { size } from "../../lib/format";
import type { Info as ServerInfo } from "../../api/types";
import { GLOBAL_MODIFIERS, MOD, REVEAL, SHIFT, TRAY, combo, composing } from "../../lib/keys";
import { reviewPath } from "../../lib/url";
import { DEFAULT_GLOBAL_SHORTCUT, SHORTCUTS, SHORTCUT_SECTIONS } from "../../lib/shortcuts";
import { useSettings } from "../../state/settings";
import { describeSystem, useNotificationStatus } from "../../state/notifications";
import { Tooltip } from "../Tooltip";
import { Segmented, ShortcutRecorder, Toggle } from "./controls";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";
import { AgentsSection } from "./AgentsSection";
import { ThemeRows } from "./ThemeRows";
import { CliRow } from "./CliRow";
import { UpdatesRows } from "./UpdatesRow";
import { PluginsSection } from "./PluginsSection";
import { PrivacySection } from "./PrivacySection";
import { useCopy } from "../../lib/useCopy";
import { Dialog } from "../Dialog";

export type SettingsSection =
  "general" | "appearance" | "items" | "shortcuts" | "agents" | "plugins" | "data" | "privacy" | "labs" | "about";

const SECTIONS: { key: SettingsSection; label: string; icon: ReactNode }[] = [
  { key: "general", label: "General", icon: <Settings2 size={15} /> },
  { key: "appearance", label: "Appearance", icon: <Palette size={15} /> },
  { key: "items", label: "Work items", icon: <ListChecks size={15} /> },
  { key: "shortcuts", label: "Shortcuts", icon: <Keyboard size={15} /> },
  { key: "agents", label: "Agents", icon: <Bot size={15} /> },
  { key: "plugins", label: "Plugins", icon: <Blocks size={15} /> },
  { key: "data", label: "Data", icon: <Database size={15} /> },
  { key: "privacy", label: "Privacy", icon: <ShieldCheck size={15} /> },
  { key: "labs", label: "Labs", icon: <FlaskConical size={15} /> },
  { key: "about", label: "About", icon: <Info size={15} /> },
];

/** The port, committed on Enter or blur when it is a port and it changed. */
function PortField({ value, onChange }: { value: number; onChange: (port: number) => void }) {
  const [text, setText] = useState(String(value));
  // a port changed elsewhere replaces what the field shows
  const [shown, setShown] = useState(value);
  if (value !== shown) {
    setShown(value);
    setText(String(value));
  }
  const commit = () => {
    const port = parseInt(text, 10);
    if (Number.isNaN(port) || port < 1024 || port > 65535) {
      setText(String(value));
      return;
    }
    if (port !== value) onChange(port);
  };
  return (
    <input
      className="settings-input"
      type="number"
      aria-label="Port"
      min={1024}
      max={65535}
      step={1}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && !composing(e) && commit()}
    />
  );
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
  if (choice === "tomorrow")
    return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 0).toISOString();
  return null;
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
      const [{ invoke }, { listen }] = await Promise.all([
        import("@tauri-apps/api/core"),
        import("@tauri-apps/api/event"),
      ]);
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

export function SettingsDialog({
  open,
  section,
  plugin,
  onSection,
  onClose,
  onWhatsNew,
}: {
  open: boolean;
  section: SettingsSection;
  plugin?: string | null;
  onSection: (s: SettingsSection) => void;
  onClose: () => void;
  /** opens the newest release's notes, when there are some */
  onWhatsNew?: () => void;
}) {
  const navigate = useNavigate();
  const { settings, update, native } = useSettings();
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const { state: copied, copy } = useCopy(1200);
  const paused = pausedUntil(settings.notifications.paused_until);
  const { system, request: requestNotifications, openSystemSettings } = useNotificationStatus(open);
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
    api
      .info()
      .then(setInfo)
      .catch(() => setInfo(null));
  }, [open]);

  if (!open) return null;

  const copyUrl = () => {
    if (info) void copy(`http://127.0.0.1:${info.port}`);
  };

  return (
    <Dialog
      label="Settings"
      plain
      className="settings"
      onClose={onClose}
      // Esc while recording a shortcut is the recorder's, and in the
      // install panel it clears the source or closes the menu first
      closeOnEscape={() =>
        !document.querySelector(".shortcut-recorder.is-recording") &&
        !document.activeElement?.matches("[data-install-source]:not(:placeholder-shown), .install-menu *")
      }
    >
      <nav className="settings-rail" aria-label="Settings sections">
        <div className="settings-rail-title">Settings</div>
        {SECTIONS.map((s) => (
          <button
            key={s.key}
            type="button"
            className={section === s.key ? "is-active" : ""}
            onClick={() => onSection(s.key)}
          >
            {s.icon}
            {s.label}
          </button>
        ))}
      </nav>
      <div className="settings-body" data-testid="settings-body">
        <Tooltip label="Close" keys={["Esc"]}>
          <button type="button" className="bar-button settings-close" onClick={onClose} aria-label="Close settings">
            <X size={16} />
          </button>
        </Tooltip>

        {section === "general" ? (
          <SettingsPage title="General">
            <SettingsGroup caption="Inbox">
              <SettingsRow
                label="Opening a review"
                description={
                  settings.inbox.open_in === "page"
                    ? "A review opens as a page of its own."
                    : "A review opens in a panel beside the inbox or the history, which stays in view."
                }
              >
                <Segmented
                  label="Opening a review"
                  value={settings.inbox.open_in}
                  onChange={(v) => update({ inbox: { open_in: v } })}
                  options={[
                    { value: "panel", label: "Beside the list" },
                    { value: "page", label: "As a page" },
                  ]}
                />
              </SettingsRow>
            </SettingsGroup>
            <SettingsGroup caption="Startup">
              <SettingsRow
                label="Launch at login"
                description={`Open Pinrail when you sign in, in ${TRAY}`}
                note={native ? undefined : "Only in the app"}
              >
                <Toggle
                  label="Launch at login"
                  checked={settings.autostart === true}
                  disabled={!native || settings.autostart === null}
                  onChange={(v) => update({ autostart: v })}
                />
              </SettingsRow>
              <SettingsRow
                label="Closing the window"
                description={
                  settings.close_window === "quit"
                    ? "Quits Pinrail and removes its icon."
                    : `Hides the window. Pinrail stays in ${TRAY} until you quit it.`
                }
              >
                <Segmented
                  label="Closing the window"
                  value={settings.close_window}
                  onChange={(v) => update({ close_window: v })}
                  options={[
                    { value: "hide", label: `Hide to ${TRAY}` },
                    { value: "quit", label: "Quit" },
                  ]}
                />
              </SettingsRow>
              <SettingsRow
                label={`Show in ${TRAY}`}
                description="An icon with the number of waiting reviews and a menu to open them. When it is off, Pinrail still opens with its shortcut and from notifications."
                note={native ? undefined : "Only in the app"}
              >
                <Toggle
                  label={`Show in ${TRAY}`}
                  checked={settings.menu_bar_icon}
                  disabled={!native}
                  onChange={(v) => update({ menu_bar_icon: v })}
                />
              </SettingsRow>
            </SettingsGroup>
            <SettingsGroup caption="Notifications">
              <SettingsRow
                label="System notifications"
                description="A notification when a review arrives; the tray keeps its count either way"
              >
                <Toggle
                  label="System notifications"
                  checked={settings.notifications.enabled}
                  onChange={(v) => update({ notifications: { enabled: v } })}
                />
              </SettingsRow>
              <SettingsRow
                label="Pause"
                description={
                  paused
                    ? `Nothing is announced until ${clock(paused)}; the tray's menu says so too`
                    : "Nothing is announced while paused; the tray's menu offers the same"
                }
              >
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
                <Toggle
                  label="Sound"
                  checked={settings.notifications.sound}
                  onChange={(v) => update({ notifications: { sound: v } })}
                />
              </SettingsRow>
              <SettingsRow label="System" description={describeSystem(system)}>
                {system.status?.authorization === "not_determined" ? (
                  <button type="button" className="chrome-button" onClick={requestNotifications}>
                    Turn on
                  </button>
                ) : system.status && !system.status.shows ? (
                  <button type="button" className="chrome-button" onClick={openSystemSettings}>
                    <Settings size={14} /> Open System Settings
                  </button>
                ) : null}
              </SettingsRow>
            </SettingsGroup>
          </SettingsPage>
        ) : null}

        {section === "items" ? (
          <SettingsPage title="Work items">
            <SettingsGroup>
              <SettingsRow
                label="Close a parent with its last sub-item"
                description="When its last open sub-item is done or cancelled, and at least one is done, a work item is done too, unless someone works on it or something of it waits for you."
              >
                <Toggle
                  label="Close a parent with its last sub-item"
                  checked={settings.items.close_parents}
                  onChange={(v) => update({ items: { close_parents: v } })}
                />
              </SettingsRow>
            </SettingsGroup>
          </SettingsPage>
        ) : null}
        {section === "appearance" ? (
          <SettingsPage title="Appearance">
            <SettingsGroup>
              <SettingsRow
                label="Colour scheme"
                description={`System follows your computer's setting. ${combo(MOD, SHIFT, "L")} switches between dark and light.`}
              >
                <Segmented
                  label="Colour scheme"
                  value={settings.appearance.theme}
                  onChange={(v) => update({ appearance: { theme: v } })}
                  options={[
                    { value: "system", label: "System" },
                    { value: "dark", label: "Dark" },
                    { value: "light", label: "Light" },
                  ]}
                />
              </SettingsRow>
              <ThemeRows />
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
            <SettingsGroup caption="Anywhere on your computer">
              <SettingsRow
                label="Open Pinrail"
                description={`Click the keys and press a new combination; it needs ${GLOBAL_MODIFIERS}`}
                note={describeShortcut(shortcut, settings.shortcut.global)}
              >
                <ShortcutRecorder
                  label="Global shortcut"
                  value={settings.shortcut.global}
                  onChange={(v) => update({ shortcut: { global: v } })}
                />
                {settings.shortcut.global !== DEFAULT_GLOBAL_SHORTCUT ? (
                  <button
                    type="button"
                    className="chrome-button settings-reset"
                    onClick={() => update({ shortcut: { global: DEFAULT_GLOBAL_SHORTCUT } })}
                  >
                    Reset
                  </button>
                ) : null}
              </SettingsRow>
              <SettingsRow
                label="It opens"
                description={
                  settings.shortcut.global_opens === "inbox"
                    ? "The inbox, whatever is pending"
                    : "The oldest pending review, or the inbox when nothing is pending"
                }
              >
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
            {SHORTCUT_SECTIONS.map((section) => (
              <SettingsGroup key={section} caption={`In the app: ${section}`}>
                {SHORTCUTS.filter((s) => s.section === section).map((s) => (
                  <SettingsRow key={s.what} label={s.what}>
                    <Keys keys={s.keys} />
                  </SettingsRow>
                ))}
              </SettingsGroup>
            ))}
          </SettingsPage>
        ) : null}

        {section === "agents" ? (
          <SettingsPage title="Agents">
            <AgentsSection open={open && section === "agents"} />
          </SettingsPage>
        ) : null}

        {section === "plugins" ? (
          <PluginsSection
            focus={plugin ?? null}
            onOpenReview={(id) => {
              onClose();
              navigate(reviewPath(id, settings.inbox.open_in));
            }}
          />
        ) : null}

        {section === "data" ? (
          <SettingsPage title="Data">
            <SettingsGroup caption="Where things are">
              <SettingsRow
                label="Data directory"
                description={<span className="mono">{info?.data_dir ?? "…"}</span>}
                note="The database, the settings file, and under plugins/ the installed copies, build logs and scratch"
              >
                {native ? (
                  <Tooltip label={REVEAL}>
                    <button
                      type="button"
                      className="bar-button"
                      aria-label="Reveal the data directory"
                      disabled={!info}
                      onClick={() =>
                        info &&
                        import("@tauri-apps/plugin-opener").then(({ revealItemInDir }) =>
                          revealItemInDir(info.data_dir).catch(() => {}),
                        )
                      }
                    >
                      <FolderOpen size={15} />
                    </button>
                  </Tooltip>
                ) : null}
              </SettingsRow>
              <SettingsRow
                label="Server"
                description={info ? <span className="mono">http://127.0.0.1:{info.port}</span> : "…"}
              >
                <button type="button" className="chrome-button" onClick={copyUrl} disabled={!info}>
                  {copied === "done" ? "Copied" : "Copy URL"}
                </button>
              </SettingsRow>
              <SettingsRow
                label="Port"
                description="Where the server listens for the CLI and the agents"
                note={
                  info && settings.port !== info.port ? (
                    <span>
                      Takes effect when Pinrail starts next; until then the server stays on {info.port}. The CLI follows
                      either.
                    </span>
                  ) : undefined
                }
              >
                <PortField value={settings.port} onChange={(port) => update({ port })} />
              </SettingsRow>
            </SettingsGroup>
            <SettingsGroup caption="History">
              <SettingsRow
                label="Keep reviews for"
                description="Decided, withdrawn, discarded and expired reviews older than this are removed; pending ones stay"
              >
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
                  info?.attachments ? (
                    <span data-testid="attachment-totals">
                      {info.attachments.count === 0
                        ? "None stored"
                        : `${info.attachments.count} file${info.attachments.count === 1 ? "" : "s"}, ${size(info.attachments.bytes)}. They go with their reviews`}
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

        {section === "privacy" ? <PrivacySection /> : null}

        {section === "labs" ? (
          <SettingsPage title="Labs">
            <SettingsGroup caption="Features still in the works, each off until you turn it on">
              <SettingsRow
                label="Sessions and questions"
                description="Agents ask you questions with pinrail ask and report what they do with pinrail status, in a thread for each session. Adds the Sessions page and lists the sessions in the sidebar. Update the skill in Settings › Agents after changing this."
              >
                <Toggle
                  label="Sessions and questions"
                  checked={settings.labs.sessions}
                  onChange={(v) => update({ labs: { sessions: v } })}
                />
              </SettingsRow>
            </SettingsGroup>
          </SettingsPage>
        ) : null}

        {section === "about" ? (
          <SettingsPage title="About">
            <SettingsGroup>
              <SettingsRow
                label="Pinrail"
                description={
                  info ? `Version ${info.version} · server started ${new Date(info.started_at).toLocaleString()}` : "…"
                }
              >
                {onWhatsNew ? (
                  <button type="button" className="chrome-button" onClick={onWhatsNew}>
                    What's new
                  </button>
                ) : null}
              </SettingsRow>
              <UpdatesRows />
              <SettingsRow label="Plugins" description="To write your own, see pinrail.dev/docs/building/writing" />
              <SettingsRow label="License" description="Apache License 2.0" note={noticesError ?? undefined}>
                {native ? (
                  <button type="button" className="chrome-button" onClick={openNotices}>
                    Third-party notices
                  </button>
                ) : null}
              </SettingsRow>
            </SettingsGroup>
          </SettingsPage>
        ) : null}
      </div>
    </Dialog>
  );
}
