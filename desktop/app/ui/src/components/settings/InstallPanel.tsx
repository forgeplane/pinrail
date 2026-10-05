// Installing a plugin, in place in the Plugins section: one field for the
// source, looked at as soon as there is one, then what it is and what
// installing replaces. Install is the consent; linking instead of copying
// is chosen from the same button.

import { ChevronDown, FileArchive, FolderOpen } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ApiError, api, inTauri } from "../../api/client";
import type { Inspection, Plugin } from "../../api/types";
import { takes } from "../../lib/format";
import { PluginIcon } from "../PluginIcon";

type Stage =
  | { at: "empty" }
  | { at: "looking" }
  | { at: "seen"; seen: Inspection }
  | { at: "installing"; seen: Inspection }
  | { at: "failed"; seen: Inspection; error: string };

/** How long typing pauses before the source is looked at. */
const PAUSE = 600;

const failure = (e: unknown) =>
  e instanceof ApiError
    ? (e.violations[0]?.message ?? e.message)
    : e instanceof Error
      ? e.message
      : "Something went wrong";

/** Where the plugin comes from, in one line. */
function Origin({ seen }: { seen: Inspection }) {
  return (
    <p>
      {seen.source_kind === "folder" ? "From the folder " : "From the zip "}
      <span className="mono">{seen.source}</span>
    </p>
  );
}

/** What runs on this computer, and what happens to what is already installed. */
function Consequences({ seen }: { seen: Inspection }) {
  const installed = seen.installed;
  return (
    <>
      {seen.source_kind === "archive" ? (
        <p className="install-runs" data-runs="nothing">
          <b>Nothing runs on your computer.</b> The bundle is unpacked, checked and used as it is.
        </p>
      ) : (
        <p className="install-runs" data-runs="nothing">
          <b>Nothing runs on your computer.</b> The folder is copied without source files, tests and hidden files.
        </p>
      )}
      {seen.attachments ? (
        <p className="install-runs" data-takes>
          <b>Accepts files:</b> {takes(seen.attachments)}. Agents can attach these files to a review. The files are
          stored with the review and are available only to this plugin's view.
        </p>
      ) : null}
      {installed ? (
        <p
          className="install-replaces"
          data-replaces={
            installed.source_kind === "index"
              ? "official"
              : installed.link
                ? "link"
                : installed.unchanged
                  ? "unchanged"
                  : seen.older
                    ? "older"
                    : "same"
          }
        >
          <b>
            {seen.name} {installed.version} is already installed
          </b>
          {installed.source_kind === "index"
            ? `, as the official plugin. This plugin takes its place for new reviews; existing reviews keep the version they were made with.`
            : installed.link
              ? installed.source === seen.source
                ? `, as a link to this folder. Installing makes a copy and removes the link.`
                : `, as a link to ${installed.source}. Installing copies this folder and removes the link.`
              : installed.unchanged
                ? `, from this source, and the source has not changed. Installing again replaces it with the same files.`
                : seen.older
                  ? `, and it is newer than this version. Installing replaces it with this older version for new reviews.`
                  : `. Installing replaces it for new reviews; existing reviews keep the version they were made with.`}
          {installed.links_kept ? null : (
            <span data-links-forgotten>
              {" "}
              The sites it was allowed to open without asking do not carry over to this plugin.
            </span>
          )}
        </p>
      ) : null}
    </>
  );
}

/** Install, with linking the folder in a menu beside it. */
function InstallButton({ seen, onInstall }: { seen: Inspection; onInstall: (link: boolean) => void }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const label = seen.installed?.unchanged ? "Install again" : seen.installed ? "Replace" : "Install";

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", onPointer);
    return () => window.removeEventListener("mousedown", onPointer);
  }, [open]);

  const choose = (link: boolean) => {
    setOpen(false);
    onInstall(link);
  };

  // a zip is always unpacked and copied: there is nothing to link
  if (seen.source_kind !== "folder") {
    return (
      <button
        type="button"
        className="chrome-button button-primary"
        onClick={() => onInstall(false)}
        data-install-confirm
      >
        {label}
      </button>
    );
  }
  return (
    <div
      ref={box}
      className="install-split"
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          setOpen(false);
        }
      }}
    >
      <button
        type="button"
        className="chrome-button button-primary"
        onClick={() => onInstall(false)}
        data-install-confirm
      >
        {label}
      </button>
      <button
        type="button"
        className="chrome-button button-primary install-split-more"
        aria-label="More ways to install"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <ChevronDown size={14} />
      </button>
      {open ? (
        <div className="install-menu" role="menu">
          <button type="button" role="menuitem" autoFocus onClick={() => choose(false)}>
            <span>Install a copy</span>
            <span className="faint">Pinrail keeps its own copy of the folder.</span>
          </button>
          <button type="button" role="menuitem" onClick={() => choose(true)} data-install-link>
            <span>Link to the folder</span>
            <span className="faint">Nothing is copied. Changes to the folder appear the next time the view opens.</span>
          </button>
        </div>
      ) : null}
    </div>
  );
}

export function InstallPanel({ initial, onInstalled }: { initial?: string; onInstalled: (plugin: Plugin) => void }) {
  const [source, setSource] = useState(initial ?? "");
  const [stage, setStage] = useState<Stage>({ at: "empty" });
  const [error, setError] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const card = useRef<HTMLDivElement>(null);
  // only the latest look counts: an earlier one may answer after it
  const looks = useRef(0);
  const pause = useRef<number | undefined>(undefined);
  const native = inTauri();

  const look = async (from: string) => {
    window.clearTimeout(pause.current);
    const trimmed = from.trim();
    const n = ++looks.current;
    setError(null);
    if (!trimmed) {
      setStage({ at: "empty" });
      return;
    }
    setStage({ at: "looking" });
    try {
      const seen = await api.inspectPlugin({ source: trimmed, link: false });
      if (n === looks.current) setStage({ at: "seen", seen });
    } catch (e) {
      if (n !== looks.current) return;
      setError(failure(e));
      setStage({ at: "empty" });
    }
  };

  // a source given from a row is looked at right away, and shown
  useEffect(() => {
    if (!initial) return;
    look(initial);
    field.current?.focus();
    panel.current?.scrollIntoView({ block: "nearest" });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, for the source the panel opened with
  }, []);

  useEffect(() => () => window.clearTimeout(pause.current), []);

  // what a look adds sits below the field: bring it into view
  useEffect(() => {
    if (stage.at === "seen") card.current?.scrollIntoView({ block: "nearest" });
  }, [stage.at]);

  const edit = (value: string) => {
    setSource(value);
    looks.current++;
    setError(null);
    setStage({ at: "empty" });
    window.clearTimeout(pause.current);
    if (value.trim()) pause.current = window.setTimeout(() => look(value), PAUSE);
  };

  const clear = () => {
    window.clearTimeout(pause.current);
    looks.current++;
    setSource("");
    setError(null);
    setStage({ at: "empty" });
  };

  const choose = async (zip: boolean) => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = zip
      ? await open({
          directory: false,
          multiple: false,
          title: "Choose the plugin's zip",
          filters: [{ name: "Plugin zip", extensions: ["zip"] }],
        })
      : await open({ directory: true, multiple: false, title: "Choose the plugin folder" });
    if (typeof picked === "string") {
      setSource(picked);
      look(picked);
    }
  };

  const install = async (seen: Inspection, link: boolean) => {
    setError(null);
    setStage({ at: "installing", seen });
    try {
      const plugin = await api.installPlugin({ source: seen.source, link });
      clear();
      onInstalled(plugin);
    } catch (e) {
      setStage({ at: "failed", seen, error: failure(e) });
    }
  };

  const busy = stage.at === "installing";
  const seen = "seen" in stage ? stage.seen : null;

  return (
    <div ref={panel} className="install-panel" data-install-panel>
      <div className="install-source">
        <input
          ref={field}
          className="settings-input install-source-field"
          type="text"
          aria-label="Source"
          placeholder={native ? "Paste a path, or choose a folder or a zip" : "/path/to/plugin or /path/to/plugin.zip"}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          value={source}
          disabled={busy}
          onChange={(e) => edit(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") look(source);
            if (e.key === "Escape" && source) {
              e.stopPropagation();
              clear();
            }
          }}
          data-install-source
        />
        {native ? (
          <>
            <button type="button" className="chrome-button" onClick={() => choose(false)} disabled={busy}>
              <FolderOpen size={14} /> Folder…
            </button>
            <button type="button" className="chrome-button" onClick={() => choose(true)} disabled={busy}>
              <FileArchive size={14} /> Zip…
            </button>
          </>
        ) : null}
      </div>

      {stage.at === "looking" ? (
        <p className="dim install-wait" data-install-looking>
          Inspecting…
        </p>
      ) : null}
      {error ? <p className="notice notice-danger install-error">{error}</p> : null}

      {seen ? (
        <div ref={card} className="install-seen" data-install-seen>
          <div className="install-seen-head">
            <span className="settings-row-icon">
              <PluginIcon icon={seen.icon} size={16} strokeWidth={1.75} />
            </span>
            <div className="install-seen-name">
              <div className="install-seen-title">{seen.title}</div>
              <div className="faint mono">
                {seen.name} · {seen.version}
              </div>
            </div>
            {stage.at === "installing" ? (
              <span className="dim install-wait">Installing…</span>
            ) : (
              <InstallButton seen={seen} onInstall={(link) => install(seen, link)} />
            )}
          </div>
          <Origin seen={seen} />
          <Consequences seen={seen} />
          {stage.at === "failed" ? <p className="notice notice-danger install-error">{stage.error}</p> : null}
        </div>
      ) : null}
    </div>
  );
}
