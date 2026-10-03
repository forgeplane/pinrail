// The Plugins section: the registered plugins, each with where it came
// from, a Notify toggle and its own settings folded under it; and the way
// in, the install dialog.

import {
  Bell,
  BellOff,
  ChevronRight,
  Send,
  CircleCheck,
  FolderOpen,
  Link2,
  PackagePlus,
  RefreshCw,
  Trash2,
  TriangleAlert,
  Wrench,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api, inTauri } from "../../api/client";
import type { Plugin, SettingProperty } from "../../api/types";
import { takes } from "../../lib/format";
import { REVEAL } from "../../lib/keys";
import { sourceOf } from "../../lib/links";
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
export function PluginsSection({ focus, onOpenReview }: { focus: string | null; onOpenReview: (id: string) => void }) {
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
  }, [load, live.plugins]);

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

  // the origins a plugin may open without asking, while it keeps its source
  const allowedOrigins = (p: Plugin) => {
    const permission = settings.links[p.name];
    return permission && permission.source === sourceOf(p) ? permission.origins : [];
  };
  const forgetLink = (p: Plugin, origin: string) => {
    const rest = allowedOrigins(p).filter((o) => o !== origin);
    update({ links: { [p.name]: rest.length ? { source: sourceOf(p), origins: rest } : null } });
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
          <SettingsRow
            label="Install a plugin"
            description="From a folder on this machine, a repository, or a GitHub release"
          >
            <button type="button" className="chrome-button" onClick={() => setInstalling({})} data-install-open>
              <PackagePlus size={14} /> Install…
            </button>
          </SettingsRow>
        )}
      </SettingsGroup>

      <SettingsGroup
        caption={plugins.length ? `Installed (${plugins.length}${broken ? `, ${broken} broken` : ""})` : "Installed"}
        action={
          <Tooltip label="Reload plugins from disk">
            <button type="button" className="chrome-button settings-caption-action" onClick={reload}>
              <RefreshCw size={13} /> Reload
            </button>
          </Tooltip>
        }
      >
        {plugins.length === 0 ? (
          <SettingsRow
            label="No plugins installed"
            description="Install a plugin to let agents submit reviews of that kind"
          />
        ) : null}
        {plugins.map((p) => (
          <PluginEntry
            key={p.name}
            plugin={p}
            native={native}
            muted={muted.includes(p.name)}
            stored={settings.plugins[p.name] ?? {}}
            open={focus === p.name}
            onReveal={() => reveal(p.install?.link ? p.install.source : p.path)}
            onNotify={(on) => setNotify(p.name, on)}
            links={allowedOrigins(p)}
            onForgetLink={(origin) => forgetLink(p, origin)}
            onChange={(values) => update({ plugins: { [p.name]: values } })}
            onCopy={() => setInstalling({ source: p.install?.source ?? p.path })}
            onMessage={notify}
            onOpenReview={onOpenReview}
          />
        ))}
      </SettingsGroup>
    </SettingsPage>
  );
}

/** What a property's value is right now: stored, else its default. */
const valueOf = (property: SettingProperty, stored: Record<string, unknown>, key: string) =>
  key in stored ? stored[key] : property.default;

/** The choices a string property offers, when it offers any. */
const choicesOf = (property: SettingProperty): { value: string; label: string }[] | null => {
  if (property.oneOf) return property.oneOf.map((c) => ({ value: String(c.const), label: c.title ?? String(c.const) }));
  if (property.enum) return property.enum.map((v) => ({ value: String(v), label: String(v) }));
  return null;
};

/** Where a plugin came from: how, in words, and from where. */
function originOf(p: Plugin): { how: string; where: string | null } {
  const i = p.install;
  if (!i || i.source_kind === "app") return { how: "Built into Pinrail", where: null };
  if (i.link) return { how: "Linked to", where: i.source };
  if (i.source_kind === "archive") return { how: "Installed from the zip", where: i.source };
  return { how: "Copied from", where: i.source };
}

/** One installed plugin: its row, and its settings folded under it when it declares any. */
function PluginEntry({
  plugin: p,
  native,
  muted,
  stored,
  open: openAtStart,
  onReveal,
  onNotify,
  links,
  onForgetLink,
  onChange,
  onCopy,
  onMessage,
  onOpenReview,
}: {
  plugin: Plugin;
  native: boolean;
  muted: boolean;
  stored: Record<string, unknown>;
  open: boolean;
  onReveal: () => void;
  onNotify: (on: boolean) => void;
  links: string[];
  onForgetLink: (origin: string) => void;
  onChange: (values: Record<string, unknown>) => void;
  onCopy: () => void;
  onMessage: (text: string, tone?: "ok" | "danger") => void;
  onOpenReview: (id: string) => void;
}) {
  const schema = p.usable ? p.settings_schema : null;
  const entries = schema ? Object.entries(schema.properties) : [];
  const changed = entries.filter(([key, property]) => key in stored && stored[key] !== property.default);
  const [open, setOpen] = useState(openAtStart);
  const [removing, setRemoving] = useState<"asking" | "busy" | null>(null);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (openAtStart) {
      setOpen(true);
      box.current?.scrollIntoView({ block: "start" });
    }
  }, [openAtStart]);

  const resetAll = () => onChange(Object.fromEntries(entries.map(([key, property]) => [key, property.default])));
  const toggle = () => setOpen((o) => !o);

  // one of its samples, sent as a review and opened
  const [sending, setSending] = useState(false);
  const sendSample = async (sample?: string) => {
    setSending(true);
    try {
      const review = await api.sendSample(p.name, sample ? { sample } : {});
      onOpenReview(review.id);
    } catch (e) {
      setSending(false);
      onMessage(
        e instanceof ApiError
          ? (e.violations[0]?.message ?? e.message)
          : `The ${p.title || p.name} sample could not be sent`,
        "danger",
      );
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
      onMessage(
        e instanceof ApiError
          ? (e.violations[0]?.message ?? e.message)
          : `The ${p.title || p.name} plugin could not be removed`,
        "danger",
      );
    }
  };

  const linked = p.install?.link ?? false;
  // installed by the person, from a source; the plugins Pinrail ships are not
  const ownInstall = p.install && p.install.source_kind !== "app" ? p.install : null;
  const origin = originOf(p);
  // asked before a removal, in place of whatever the line says
  const ask = removing ? (
    <span className="settings-plugin-ask" data-plugin-remove-ask>
      Remove {p.title || p.name}?
      {linked ? " The folder stays where it is." : " Reviews that rendered from it keep doing so."}
      <button
        type="button"
        className="settings-reset-link danger"
        onClick={remove}
        disabled={removing === "busy"}
        data-plugin-remove-confirm
      >
        {removing === "busy" ? "Removing…" : "Remove"}
      </button>
      <button
        type="button"
        className="settings-reset-link"
        onClick={() => setRemoving(null)}
        disabled={removing === "busy"}
      >
        Keep
      </button>
    </span>
  ) : null;
  // Under the badge, only what answers a click and what was dropped; where
  // the plugin came from and what it takes are in its details.
  const note =
    ask ??
    (p.settings_error ? (
      <span className="danger">Settings ignored: {p.settings_error}</span>
    ) : p.sample_errors?.length ? (
      <span className="danger">Sample ignored: {p.sample_errors.join("; ")}</span>
    ) : null);

  return (
    <div
      ref={box}
      className={`settings-plugin ${open ? "is-open" : ""}`}
      data-plugin-settings={p.name}
      data-plugin-row={p.name}
    >
      <SettingsRow
        icon={<PluginIcon icon={p.icon} size={16} strokeWidth={1.75} />}
        label={p.title || p.name}
        description={
          <span className="settings-plugin-line">
            <PluginBadge name={p.name} version={p.version} icon={p.icon} />
            {p.error ? (
              // why it is broken, on hover or keyboard focus
              <Tooltip label={p.error} tone="danger">
                <span className="with-icon danger" tabIndex={0} data-plugin-broken>
                  <TriangleAlert size={12} />
                  broken
                </span>
              </Tooltip>
            ) : (
              <span className="with-icon ok">
                {linked ? <Link2 size={12} /> : p.dev ? <Wrench size={12} /> : <CircleCheck size={12} />}
                {linked ? "linked" : p.dev ? "development" : "ready"}
              </span>
            )}
          </span>
        }
        note={note}
        onClick={toggle}
      >
        {native ? (
          <Tooltip label={REVEAL}>
            <button type="button" className="bar-button" onClick={onReveal} aria-label={`Reveal ${p.name}`}>
              <FolderOpen size={15} />
            </button>
          </Tooltip>
        ) : null}
        {ownInstall && linked ? (
          <Tooltip label="Install a copy of this linked plugin">
            <button type="button" className="bar-button" onClick={onCopy} aria-label={`Install a copy of ${p.name}`}>
              <PackagePlus size={15} />
            </button>
          </Tooltip>
        ) : null}
        {ownInstall ? (
          <Tooltip label="Remove">
            <button
              type="button"
              className="bar-button"
              onClick={() => setRemoving("asking")}
              aria-label={`Remove ${p.name}`}
              disabled={removing !== null}
            >
              <Trash2 size={15} />
            </button>
          </Tooltip>
        ) : null}
        <Tooltip
          label={
            muted
              ? "Notifications are off for this plugin. Click to turn them on"
              : "Notifies you when a review for this plugin arrives. Click to turn notifications off"
          }
        >
          <button
            type="button"
            role="switch"
            aria-checked={!muted}
            aria-label={`Notify for ${p.name}`}
            className={`bar-button settings-notify ${muted ? "is-muted" : ""}`}
            onClick={() => onNotify(muted)}
          >
            {muted ? <BellOff size={15} /> : <Bell size={15} />}
          </button>
        </Tooltip>
        <Tooltip label={open ? "Hide the details" : "Show the details and settings"}>
          <button
            type="button"
            className="bar-button settings-plugin-toggle"
            aria-expanded={open}
            aria-label={`Details of ${p.name}`}
            onClick={toggle}
          >
            <ChevronRight size={15} className={open ? "is-open" : ""} />
          </button>
        </Tooltip>
      </SettingsRow>
      {open ? (
        <div className="settings-subrows" data-plugin-details={p.name}>
          {p.error ? (
            <p className="notice notice-danger settings-plugin-error" data-plugin-error>
              This plugin is broken: {p.error}
            </p>
          ) : null}
          <dl className="settings-plugin-details">
            <dt>Version</dt>
            <dd>{p.version}</dd>
            <dt>Source</dt>
            <dd className="settings-plugin-origin">
              <span>
                {origin.how} {origin.where ? <span className="mono">{origin.where}</span> : null}
              </span>
              {p.install?.modified ? (
                <span className="danger with-icon">
                  <TriangleAlert size={11} /> Modified since installation
                </span>
              ) : null}
            </dd>
            {p.attachments ? (
              <>
                <dt>Files</dt>
                <dd data-plugin-takes>{takes(p.attachments)}</dd>
              </>
            ) : null}
            {links.length ? (
              <>
                <dt>Opens without asking</dt>
                <dd className="settings-plugin-links" data-plugin-links={p.name}>
                  {links.map((origin) => (
                    <span key={origin} className="settings-link-chip mono">
                      {origin}
                      <button
                        type="button"
                        className="settings-link-forget"
                        aria-label={`Ask again before opening ${origin}`}
                        onClick={() => onForgetLink(origin)}
                        data-forget-link={origin}
                      >
                        <X size={12} />
                      </button>
                    </span>
                  ))}
                </dd>
              </>
            ) : null}
          </dl>
          {p.usable && p.samples?.length ? (
            <div className="settings-plugin-actions">
              {p.samples.length === 1 ? (
                <button
                  type="button"
                  className="chrome-button"
                  onClick={() => sendSample()}
                  disabled={sending}
                  data-plugin-sample
                >
                  <Send size={13} /> Send a sample
                </button>
              ) : (
                p.samples.map((sample) => (
                  <button
                    key={sample}
                    type="button"
                    className="chrome-button"
                    onClick={() => sendSample(sample)}
                    disabled={sending}
                    data-plugin-sample={sample}
                  >
                    <Send size={13} /> {sample}
                  </button>
                ))
              )}
              <span className="dim">
                {p.samples.length === 1 ? "A review" : "Reviews"} with made-up content, to see how it looks
              </span>
            </div>
          ) : null}
          {entries.length ? (
            <div className="settings-subrows-head">
              <span className="settings-subrows-title">Settings</span>
              {changed.length ? (
                <button type="button" className="settings-reset-link" onClick={resetAll}>
                  Reset all to defaults
                </button>
              ) : null}
            </div>
          ) : null}
          {entries.map(([key, property]) => (
            <SettingRow
              key={key}
              plugin={p.name}
              name={key}
              property={property}
              value={valueOf(property, stored, key)}
              onChange={(v) => onChange({ [key]: v })}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** A row for one property, its control from the property's type. */
function SettingRow({
  plugin,
  name,
  property,
  value,
  onChange,
}: {
  plugin: string;
  name: string;
  property: SettingProperty;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
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
    <SettingsRow
      label={label}
      description={property.description}
      note={
        isDefault ? undefined : (
          <button type="button" className="settings-reset-link" onClick={() => onChange(property.default)}>
            {property.default === "" ? "Reset to empty" : `Reset to ${String(property.default)}`}
          </button>
        )
      }
    >
      {control}
    </SettingsRow>
  );
}
