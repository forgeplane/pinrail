// The Plugins section: the registered plugins, each with where it came
// from, a Notify toggle and its own settings folded under it; and the way
// in, the install dialog.

import { Bell, BellOff, ChevronRight, CircleCheck, CloudDownload, FolderOpen, Link2, PackagePlus, RefreshCw, Trash2, TriangleAlert, Wrench } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api, inTauri } from "../../api/client";
import type { Plugin, PluginUpdates, SettingProperty } from "../../api/types";
import { takes } from "../../lib/format";
import { PluginBadge } from "../Badges";
import { PluginIcon } from "../PluginIcon";
import { Select } from "../Select";
import { Tooltip } from "../Tooltip";
import { useLive } from "../../state/live";
import { useToast } from "../../state/toasts";
import { useSettings } from "../../state/settings";
import { Segmented, Toggle } from "./controls";
import { InstallPanel } from "./InstallPanel";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";

/** `focus` names a plugin whose settings open at once, from the palette. */
export function PluginsSection({ focus }: { focus: string | null }) {
  const live = useLive();
  const { settings, update } = useSettings();
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  /** the install panel, with the source it opens on */
  const [installing, setInstalling] = useState<{ source?: string } | null>(null);
  const notify = useToast();
  const native = inTauri();
  const muted = settings.notifications.muted_plugins;

  const load = useCallback(async () => {
    const { plugins } = await api.plugins();
    setPlugins(plugins);
  }, []);

  useEffect(() => {
    load().catch(() => {});
  }, [load, live.tick]);

  const reload = async () => {
    try {
      const { count } = await api.reloadPlugins();
      notify(`Reloaded ${count} plugin${count === 1 ? "" : "s"}`);
    } catch (e) {
      notify(e instanceof Error ? e.message : "Reload failed", "danger");
    }
    load().catch(() => {});
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
      <SettingsGroup caption="Install">
        {installing ? (
          <InstallPanel key={installing.source ?? ""} initial={installing.source} onClose={() => setInstalling(null)} />
        ) : (
          <SettingsRow label="Install a plugin" description="From a folder on this machine, a repository, or a GitHub release">
            <button type="button" className="chrome-button" onClick={() => setInstalling({})} data-install-open>
              <PackagePlus size={14} /> Install…
            </button>
          </SettingsRow>
        )}
      </SettingsGroup>

      <SettingsGroup
        caption={plugins.length ? `Installed · ${plugins.length}${broken ? `, ${broken} broken` : ""}` : "Installed"}
        action={
          <Tooltip label="Read the store and the links again">
            <button type="button" className="chrome-button settings-caption-action" onClick={reload}>
              <RefreshCw size={13} /> Reload
            </button>
          </Tooltip>
        }
      >
        {plugins.length === 0 ? <SettingsRow label="No plugins yet" description="Install one above to give your agents a view to ask through" /> : null}
        {plugins.map((p) => (
          <PluginEntry
            key={p.name}
            plugin={p}
            native={native}
            muted={muted.includes(p.name)}
            stored={settings.plugins[p.name] ?? {}}
            open={focus === p.name}
            onReveal={() => reveal(p.path)}
            onNotify={(on) => setNotify(p.name, on)}
            onChange={(values) => update({ plugins: { [p.name]: values } })}
            onCopy={() => setInstalling({ source: p.path })}
            onMessage={notify}
          />
        ))}
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

/** Where an installed plugin came from, in a few words. */
function originOf(p: Plugin): string | null {
  const i = p.install;
  if (!i) return null;
  if (i.linked) return `linked · ${p.path}`;
  if (i.kind === "git") return `${i.source}${i.commit ? ` · ${i.commit.slice(0, 7)}` : ""}`;
  if (i.kind === "release") return `${i.source}${i.tag ? ` · ${i.tag}` : ""}`;
  return `copied from ${i.source}`;
}

type Line = { text: string; tone: "ok" | "dim" | "danger"; updatable?: boolean };

/** What "check for updates" found, in a few words. */
function updatesLine(u: PluginUpdates): Line {
  switch (u.state) {
    case "up_to_date":
      return { text: "Up to date", tone: "ok" };
    case "available":
      if (u.version) return { text: `${u.version} is available`, tone: "ok", updatable: true };
      if (u.commit) return { text: `A newer commit is available: ${u.commit.slice(0, 7)}`, tone: "ok", updatable: true };
      return { text: "The folder changed since it was copied", tone: "ok", updatable: true };
    case "pinned":
      return { text: `Pinned to ${u.tag ?? u.ref ?? "this version"}`, tone: "dim" };
    case "linked":
      return { text: "A link is always what the folder holds", tone: "dim" };
    default:
      return { text: `Could not check: ${u.message ?? "unknown"}`, tone: "danger" };
  }
}

/** One installed plugin: its row, and its settings folded under it when it declares any. */
function PluginEntry({ plugin: p, native, muted, stored, open: openAtStart, onReveal, onNotify, onChange, onCopy, onMessage }: { plugin: Plugin; native: boolean; muted: boolean; stored: Record<string, unknown>; open: boolean; onReveal: () => void; onNotify: (on: boolean) => void; onChange: (values: Record<string, unknown>) => void; onCopy: () => void; onMessage: (text: string, tone?: "ok" | "danger") => void }) {
  const schema = p.usable ? p.settings_schema : null;
  const entries = schema ? Object.entries(schema.properties) : [];
  const changed = entries.filter(([key, property]) => key in stored && stored[key] !== property.default);
  const [open, setOpen] = useState(openAtStart && entries.length > 0);
  const [updates, setUpdates] = useState<Line | "checking" | null>(null);
  /** an update under way: the job's step */
  const [updating, setUpdating] = useState<string | null>(null);
  const [removing, setRemoving] = useState<"asking" | "busy" | null>(null);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (openAtStart && entries.length) {
      setOpen(true);
      box.current?.scrollIntoView({ block: "start" });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openAtStart]);

  const resetAll = () => onChange(Object.fromEntries(entries.map(([key, property]) => [key, property.default])));
  const toggle = () => entries.length && setOpen((o) => !o);

  const check = async () => {
    setUpdates("checking");
    try {
      setUpdates(updatesLine(await api.pluginUpdates(p.name)));
    } catch (e) {
      setUpdates({ text: `Could not check: ${e instanceof Error ? e.message : "unknown"}`, tone: "danger" });
    }
  };

  // installs again from where it came; the row follows the job's steps
  const updateNow = async () => {
    setUpdating("starting");
    setUpdates(null);
    try {
      const started = await api.updatePlugin(p.name);
      if (!started.job) {
        setUpdating(null);
        setUpdates({ text: "Up to date", tone: "ok" });
        return;
      }
      const follow = async () => {
        const job = await api.pluginJob(started.job!);
        if (job.status === "done") {
          setUpdating(null);
          const version = job.plugin?.install?.version ?? "";
          setUpdates({ text: `Updated to ${version}`.trim(), tone: "ok" });
          onMessage(`${p.title || p.name} plugin was updated to ${version}`.trim());
        } else if (job.status === "failed") {
          setUpdating(null);
          setUpdates({ text: `Update failed: ${job.error ?? "unknown"}`, tone: "danger" });
        } else {
          setUpdating(job.status);
          window.setTimeout(follow, 300);
        }
      };
      follow();
    } catch (e) {
      setUpdating(null);
      setUpdates({ text: e instanceof ApiError ? (e.violations[0]?.message ?? e.message) : "Update failed", tone: "danger" });
    }
  };

  const remove = async () => {
    setRemoving("busy");
    try {
      await api.removePlugin(p.name);
      // the row goes with the plugins_reloaded notice
      onMessage(`${p.title || p.name} plugin was removed`);
    } catch (e) {
      setRemoving(null);
      onMessage(e instanceof ApiError ? (e.violations[0]?.message ?? e.message) : `The ${p.title || p.name} plugin could not be removed`, "danger");
    }
  };

  const linked = p.install?.linked ?? false;
  const origin = originOf(p);
  const note = p.error ? (
    <span className="danger">{p.error}</span>
  ) : p.settings_error ? (
    <span className="danger">settings dropped: {p.settings_error}</span>
  ) : (
    <span className="settings-plugin-origin">
      <span className="mono">{origin ?? p.path}</span>
      {p.install?.modified ? (
        <span className="danger with-icon">
          <TriangleAlert size={11} /> modified since install
        </span>
      ) : null}
      {removing ? (
        <span className="settings-plugin-ask" data-plugin-remove-ask>
          Remove {p.title || p.name}?{linked ? " The folder stays where it is." : " Reviews that rendered from it keep doing so."}
          <button type="button" className="settings-reset-link danger" onClick={remove} disabled={removing === "busy"} data-plugin-remove-confirm>
            {removing === "busy" ? "Removing…" : "Remove"}
          </button>
          <button type="button" className="settings-reset-link" onClick={() => setRemoving(null)} disabled={removing === "busy"}>
            Keep
          </button>
        </span>
      ) : updating ? (
        <span className="faint" data-plugin-updating>
          Updating: {updating}…
        </span>
      ) : updates === "checking" ? (
        <span className="faint">Checking…</span>
      ) : updates ? (
        <span className="settings-plugin-ask">
          <span className={updates.tone} data-plugin-updates>
            {updates.text}
          </span>
          {updates.updatable ? (
            <button type="button" className="settings-reset-link" onClick={updateNow} data-plugin-update>
              Update
            </button>
          ) : null}
        </span>
      ) : null}
    </span>
  );

  return (
    <div ref={box} className={`settings-plugin ${entries.length ? "has-settings" : ""} ${open ? "is-open" : ""}`} data-plugin-settings={p.name} data-plugin-row={p.name}>
      <SettingsRow
        icon={<PluginIcon icon={p.icon} size={16} strokeWidth={1.75} />}
        label={p.title || p.name}
        description={
          <span className="settings-plugin-line">
            <PluginBadge name={p.name} version={p.version} icon={p.icon} />
            <span className={`with-icon ${p.error ? "danger" : "ok"}`}>
              {p.error ? <TriangleAlert size={12} /> : linked ? <Link2 size={12} /> : p.dev ? <Wrench size={12} /> : <CircleCheck size={12} />}
              {p.error ? "broken" : linked ? "linked" : p.dev ? "development" : "ready"}
            </span>
            {p.install && !linked ? <span className="faint">{p.install.version}</span> : null}
            {p.attachments ? (
              <span className="faint" data-plugin-takes>
                {takes(p.attachments).replace("Takes files", "takes files")}
              </span>
            ) : null}
            {entries.length ? (
              <span className="faint">
                {entries.length} setting{entries.length === 1 ? "" : "s"}
                {changed.length ? `, ${changed.length} changed` : ""}
              </span>
            ) : null}
          </span>
        }
        note={note}
        onClick={entries.length ? toggle : undefined}
      >
        {native ? (
          <Tooltip label="Show in Finder">
            <button type="button" className="bar-button" onClick={onReveal} aria-label={`Reveal ${p.name}`}>
              <FolderOpen size={15} />
            </button>
          </Tooltip>
        ) : null}
        {p.install && linked ? (
          <Tooltip label="Install a copy: done iterating, put it in the store">
            <button type="button" className="bar-button" onClick={onCopy} aria-label={`Install a copy of ${p.name}`}>
              <PackagePlus size={15} />
            </button>
          </Tooltip>
        ) : p.install ? (
          <Tooltip label="Check for updates">
            <button type="button" className="bar-button" onClick={check} aria-label={`Check for updates of ${p.name}`} disabled={updates === "checking"}>
              <CloudDownload size={15} />
            </button>
          </Tooltip>
        ) : null}
        {p.install ? (
          <Tooltip label="Remove">
            <button type="button" className="bar-button" onClick={() => setRemoving("asking")} aria-label={`Remove ${p.name}`} disabled={removing !== null || updating !== null}>
              <Trash2 size={15} />
            </button>
          </Tooltip>
        ) : null}
        <Tooltip label={muted ? "Muted: its reviews arrive without a notification. Click to notify again" : "Notifies when one of its reviews arrives. Click to mute"}>
          <button type="button" role="switch" aria-checked={!muted} aria-label={`Notify for ${p.name}`} className={`bar-button settings-notify ${muted ? "is-muted" : ""}`} onClick={() => onNotify(muted)}>
            {muted ? <BellOff size={15} /> : <Bell size={15} />}
          </button>
        </Tooltip>
        {entries.length ? (
          <Tooltip label={open ? "Hide its settings" : "Show its settings"}>
            <button type="button" className="bar-button settings-plugin-toggle" aria-expanded={open} aria-label={`Settings of ${p.name}`} onClick={toggle}>
              <ChevronRight size={15} className={open ? "is-open" : ""} />
            </button>
          </Tooltip>
        ) : null}
      </SettingsRow>
      {open ? (
        <div className="settings-subrows">
          {changed.length ? (
            <div className="settings-subrows-head">
              <button type="button" className="settings-reset-link" onClick={resetAll}>
                Reset all to defaults
              </button>
            </div>
          ) : null}
          {entries.map(([key, property]) => (
            <SettingRow key={key} plugin={p.name} name={key} property={property} value={valueOf(property, stored, key)} onChange={(v) => onChange({ [key]: v })} />
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
