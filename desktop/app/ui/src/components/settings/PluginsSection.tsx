// The Plugins section: the registered plugins with a Notify toggle each,
// the directories they come from, and the place installation will take.

import { Bell, BellOff, CircleCheck, FolderOpen, FolderPlus, RefreshCw, TriangleAlert, Wrench } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { ApiError, api, inTauri } from "../../api/client";
import type { Plugin } from "../../api/types";
import { PluginBadge } from "../Badges";
import { PluginIcon } from "../PluginIcon";
import { Tooltip } from "../Tooltip";
import { useLive } from "../../state/live";
import { useSettings } from "../../state/settings";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";

export function PluginsSection() {
  const live = useLive();
  const { settings, update } = useSettings();
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [dirs, setDirs] = useState<string[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [newDir, setNewDir] = useState("");
  const native = inTauri();
  const muted = settings.notifications.muted_plugins;

  const load = useCallback(async () => {
    const { plugins, dirs } = await api.plugins();
    setPlugins(plugins);
    setDirs(dirs);
  }, []);

  useEffect(() => {
    load().catch(() => {});
  }, [load, live.tick]);

  const reload = async () => {
    try {
      const { count } = await api.reloadPlugins();
      setMessage(`Reloaded ${count} plugin${count === 1 ? "" : "s"}`);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Reload failed");
    }
    load().catch(() => {});
  };

  const add = async (dir: string) => {
    if (!dir.trim()) return;
    try {
      const { count } = await api.addPluginDir(dir.trim());
      setMessage(`${count} plugin${count === 1 ? "" : "s"} registered`);
      setNewDir("");
    } catch (e) {
      setMessage(e instanceof ApiError ? (e.violations[0]?.message ?? e.message) : "Could not add the directory");
    }
    load().catch(() => {});
  };

  // the app has a folder picker; a browser takes a path
  const choose = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false, title: "Add a plugin directory" });
    if (typeof picked === "string") add(picked);
  };

  const reveal = async (path: string) => {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    revealItemInDir(path).catch(() => {});
  };

  const setNotify = (name: string, on: boolean) => {
    const next = on ? muted.filter((n) => n !== name) : [...muted.filter((n) => n !== name), name];
    update({ notifications: { muted_plugins: next } });
  };

  const broken = plugins.filter((p) => !p.usable).length;

  return (
    <SettingsPage title="Plugins">
      <SettingsGroup
        caption={plugins.length ? `Registered · ${plugins.length}${broken ? `, ${broken} broken` : ""}` : "Registered"}
        action={
          <Tooltip label="Read the plugin directories again">
            <button type="button" className="chrome-button settings-caption-action" onClick={reload}>
              <RefreshCw size={13} /> Reload
            </button>
          </Tooltip>
        }
      >
        {plugins.length === 0 ? <SettingsRow label="No plugins yet" description="Add a directory of plugins below to give your agents a view to ask through" /> : null}
        {plugins.map((p) => (
          <SettingsRow
            key={p.name}
            icon={<PluginIcon icon={p.icon} size={16} strokeWidth={1.75} />}
            label={p.title || p.name}
            description={
              <span className="settings-plugin-line">
                <PluginBadge name={p.name} version={p.version} icon={p.icon} />
                <span className={`with-icon ${p.error ? "danger" : "ok"}`}>
                  {p.error ? <TriangleAlert size={12} /> : p.dev ? <Wrench size={12} /> : <CircleCheck size={12} />}
                  {p.error ? "broken" : p.dev ? "development" : "ready"}
                </span>
              </span>
            }
            note={p.error ? <span className="danger">{p.error}</span> : <span className="mono">{p.path}</span>}
          >
            {native ? (
              <Tooltip label="Show in Finder">
                <button type="button" className="bar-button" onClick={() => reveal(p.path)} aria-label={`Reveal ${p.name}`}>
                  <FolderOpen size={15} />
                </button>
              </Tooltip>
            ) : null}
            <Tooltip label={muted.includes(p.name) ? "Muted: its reviews arrive without a notification. Click to notify again" : "Notifies when one of its reviews arrives. Click to mute"}>
              <button
                type="button"
                role="switch"
                aria-checked={!muted.includes(p.name)}
                aria-label={`Notify for ${p.name}`}
                className={`bar-button settings-notify ${muted.includes(p.name) ? "is-muted" : ""}`}
                onClick={() => setNotify(p.name, muted.includes(p.name))}
              >
                {muted.includes(p.name) ? <BellOff size={15} /> : <Bell size={15} />}
              </button>
            </Tooltip>
          </SettingsRow>
        ))}
      </SettingsGroup>

      <SettingsGroup caption="Directories">
        {dirs.map((d) => (
          <SettingsRow key={d} label={d} description="Scanned at start and on Reload">
            <Tooltip label="Removing a directory is coming; its plugins stay registered">
              <button type="button" className="chrome-button" disabled>
                Remove
              </button>
            </Tooltip>
          </SettingsRow>
        ))}
        <SettingsRow label="Add a directory" description={native ? "Every plugin inside it is registered at once" : "A path on the machine the server runs on"}>
          {native ? (
            <button type="button" className="chrome-button" onClick={choose}>
              <FolderPlus size={14} /> Choose…
            </button>
          ) : (
            <span className="settings-add">
              <input aria-label="Directory to add" placeholder="/path/to/plugins" value={newDir} onChange={(e) => setNewDir(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add(newDir)} />
              <button type="button" className="chrome-button" onClick={() => add(newDir)}>
                <FolderPlus size={14} /> Add
              </button>
            </span>
          )}
        </SettingsRow>
        {message ? <SettingsRow label={message} /> : null}
      </SettingsGroup>

      <SettingsGroup caption="Install">
        <SettingsRow label="Install from a path or a repository" description="Coming: a source, the manifest to check, the build log">
          <button type="button" className="chrome-button" disabled>
            Install…
          </button>
        </SettingsRow>
      </SettingsGroup>
    </SettingsPage>
  );
}
