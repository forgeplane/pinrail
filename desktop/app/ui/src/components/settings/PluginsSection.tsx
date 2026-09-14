// The Plugins section: the registered plugins with a Notify toggle each,
// the directories they come from, and the place installation will take.

import { Bell, BellOff, ChevronRight, CircleCheck, FolderOpen, FolderPlus, RefreshCw, TriangleAlert, Wrench } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api, inTauri } from "../../api/client";
import type { Plugin, SettingProperty, SettingsSchema } from "../../api/types";
import { PluginBadge } from "../Badges";
import { PluginIcon } from "../PluginIcon";
import { Select } from "../Select";
import { Tooltip } from "../Tooltip";
import { useLive } from "../../state/live";
import { useSettings } from "../../state/settings";
import { Segmented, Toggle } from "./controls";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";

/** `focus` names a plugin whose settings open at once, from the palette. */
export function PluginsSection({ focus }: { focus: string | null }) {
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
            note={p.error ? <span className="danger">{p.error}</span> : p.settings_error ? <span className="danger">settings dropped: {p.settings_error}</span> : <span className="mono">{p.path}</span>}
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

      {plugins
        .filter((p) => p.usable && p.settings_schema)
        .map((p) => (
          <PluginSettings key={p.name} plugin={p} schema={p.settings_schema!} stored={settings.plugins[p.name] ?? {}} open={focus === p.name} onChange={(values) => update({ plugins: { [p.name]: values } })} />
        ))}

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

/** What a property's value is right now: stored, else its default. */
const valueOf = (property: SettingProperty, stored: Record<string, unknown>, key: string) => (key in stored ? stored[key] : property.default);

/** The choices a string property offers, when it offers any. */
const choicesOf = (property: SettingProperty): { value: string; label: string }[] | null => {
  if (property.oneOf) return property.oneOf.map((c) => ({ value: String(c.const), label: c.title ?? String(c.const) }));
  if (property.enum) return property.enum.map((v) => ({ value: String(v), label: String(v) }));
  return null;
};

/** One plugin's settings: a collapsed group under the registered list, rows from the schema. */
function PluginSettings({ plugin, schema, stored, open: openAtStart, onChange }: { plugin: Plugin; schema: SettingsSchema; stored: Record<string, unknown>; open: boolean; onChange: (values: Record<string, unknown>) => void }) {
  const [open, setOpen] = useState(openAtStart);
  const group = useRef<HTMLDivElement>(null);
  const entries = Object.entries(schema.properties);
  const changed = entries.filter(([key, property]) => key in stored && stored[key] !== property.default);

  useEffect(() => {
    if (openAtStart) {
      setOpen(true);
      group.current?.scrollIntoView({ block: "start" });
    }
  }, [openAtStart]);

  const resetAll = () => onChange(Object.fromEntries(entries.map(([key, property]) => [key, property.default])));

  return (
    <div ref={group} className="settings-group settings-plugin" data-plugin-settings={plugin.name}>
      <div className="settings-group-head">
        <button type="button" className="settings-group-toggle" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          <ChevronRight size={14} className={open ? "is-open" : ""} />
          <PluginIcon icon={plugin.icon} size={13} strokeWidth={1.75} />
          <h3>{plugin.title || plugin.name}</h3>
          <span className="faint">
            {entries.length} setting{entries.length === 1 ? "" : "s"}
            {changed.length ? `, ${changed.length} changed` : ""}
          </span>
        </button>
        {open && changed.length ? (
          <button type="button" className="chrome-button settings-caption-action" onClick={resetAll}>
            Reset all
          </button>
        ) : null}
      </div>
      {open ? (
        <div className="settings-card">
          {entries.map(([key, property]) => (
            <SettingRow key={key} plugin={plugin.name} name={key} property={property} value={valueOf(property, stored, key)} onChange={(v) => onChange({ [key]: v })} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** A row for one property, its control from the property's type. */
function SettingRow({ plugin, name, property, value, onChange }: { plugin: string; name: string; property: SettingProperty; value: unknown; onChange: (value: unknown) => void }) {
  const label = property.title ?? name;
  const id = `plugin-setting-${plugin}-${name}`;
  const isDefault = value === property.default;
  const choices = property.type === "string" ? choicesOf(property) : null;
  const [text, setText] = useState(String(value ?? ""));
  useEffect(() => setText(String(value ?? "")), [value]);

  const commitText = () => {
    if (property.type === "string") {
      if (text !== value) onChange(text);
      return;
    }
    const n = property.type === "integer" ? parseInt(text, 10) : parseFloat(text);
    if (Number.isNaN(n)) {
      setText(String(value ?? ""));
      return;
    }
    const bounded = Math.min(property.maximum ?? Infinity, Math.max(property.minimum ?? -Infinity, n));
    if (bounded !== value) onChange(bounded);
    else setText(String(bounded));
  };

  let control: React.ReactNode;
  if (property.type === "boolean") {
    control = <Toggle label={label} checked={value === true} onChange={onChange} />;
  } else if (choices && choices.length <= 4) {
    control = <Segmented label={label} value={String(value)} options={choices} onChange={onChange} />;
  } else if (choices) {
    control = <Select label={label} value={String(value)} options={choices} onChange={onChange} />;
  } else {
    control = (
      <input
        id={id}
        className="settings-input"
        type={property.type === "string" ? "text" : "number"}
        aria-label={label}
        value={text}
        min={property.minimum}
        max={property.maximum}
        step={property.type === "integer" ? 1 : "any"}
        onChange={(e) => setText(e.target.value)}
        onBlur={commitText}
        onKeyDown={(e) => e.key === "Enter" && commitText()}
      />
    );
  }

  return (
    <SettingsRow label={label} description={property.description} note={isDefault ? undefined : <button type="button" className="settings-reset-link" onClick={() => onChange(property.default)}>{property.default === "" ? "Reset to empty" : `Reset to ${String(property.default)}`}</button>}>
      {control}
    </SettingsRow>
  );
}
