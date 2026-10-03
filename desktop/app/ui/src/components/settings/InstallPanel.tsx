// Installing a plugin, in place in the Plugins section: one field for the
// source, a look at what it is before it is installed, and what installing
// replaces. Install is the consent.

import { FolderOpen, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ApiError, api, inTauri, type InstallRequest } from "../../api/client";
import type { Inspection, Plugin } from "../../api/types";
import { takes } from "../../lib/format";
import { PluginIcon } from "../PluginIcon";
import { Tooltip } from "../Tooltip";
import { Toggle } from "./controls";

type Stage =
  | { at: "source" }
  | { at: "looking" }
  | { at: "seen"; seen: Inspection }
  | { at: "installing"; seen: Inspection }
  | { at: "done"; plugin: Plugin }
  | { at: "failed"; seen: Inspection; error: string };

/** A zip, which is installed as it is and cannot be linked. */
const isZip = (source: string) => /\.zip$/i.test(source.trim());

const failure = (e: unknown) =>
  e instanceof ApiError
    ? (e.violations[0]?.message ?? e.message)
    : e instanceof Error
      ? e.message
      : "Something went wrong";

/** Where the plugin comes from, in one line. */
function Origin({ seen }: { seen: Inspection }) {
  const { origin } = seen;
  if (origin.kind === "folder") {
    return (
      <p>
        {seen.link ? "Linked from the folder " : "From the folder "}
        <span className="mono">{origin.resolved}</span>
      </p>
    );
  }
  return (
    <p>
      From the zip <span className="mono">{origin.resolved}</span>
    </p>
  );
}

/** What runs on this computer, and what happens to what is already installed. */
function Consequences({ seen }: { seen: Inspection }) {
  const installed = seen.installed;
  return (
    <>
      {seen.origin.kind === "archive" ? (
        <p className="install-runs" data-runs="nothing">
          <b>Nothing runs on your computer.</b> The bundle is unpacked, checked and used as it is.
        </p>
      ) : seen.link ? (
        <p className="install-runs" data-runs="nothing">
          <b>Nothing is copied.</b> Pinrail serves the folder directly, so changes appear the next time the view opens.
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
          data-replaces={installed.linked ? "link" : installed.unchanged ? "unchanged" : seen.older ? "older" : "same"}
        >
          <b>
            {seen.name} {installed.version} is already installed
          </b>
          {installed.linked
            ? installed.path === seen.origin.resolved
              ? `, as a link to this folder. Installing makes a copy and removes the link.`
              : `, as a link to ${installed.path}. Installing copies this folder and removes the link.`
            : installed.unchanged
              ? `, from this source, and the source has not changed. Installing again replaces it with the same files.`
              : seen.older
                ? `, and it is newer than this version. Installing replaces it with this older version for new reviews.`
                : `. Installing replaces it for new reviews; existing reviews keep the version they were made with.`}
        </p>
      ) : null}
    </>
  );
}

export function InstallPanel({ initial, onClose }: { initial?: string; onClose: () => void }) {
  const [source, setSource] = useState(initial ?? "");
  const [link, setLink] = useState(false);
  const [stage, setStage] = useState<Stage>({ at: "source" });
  const [error, setError] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const actions = useRef<HTMLDivElement>(null);
  const native = inTauri();

  useEffect(() => {
    field.current?.focus();
    panel.current?.scrollIntoView({ block: "nearest" });
  }, []);

  // what a look or an install adds sits below the field: bring it into view
  useEffect(() => {
    if (stage.at !== "source") actions.current?.scrollIntoView({ block: "nearest" });
  }, [stage.at]);

  const request = (): InstallRequest => ({ source: source.trim(), link: link && !isZip(source) });

  const look = async () => {
    if (!source.trim() || stage.at === "looking") return;
    setError(null);
    setStage({ at: "looking" });
    try {
      setStage({ at: "seen", seen: await api.inspectPlugin(request()) });
    } catch (e) {
      setError(failure(e));
      setStage({ at: "source" });
    }
  };

  // the source given from a row is looked at right away
  useEffect(() => {
    if (initial) look();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, for the source the panel opened with
  }, []);

  const install = async (seen: Inspection) => {
    setError(null);
    setStage({ at: "installing", seen });
    try {
      setStage({ at: "done", plugin: await api.installPlugin({ ...request(), force: seen.older }) });
    } catch (e) {
      setStage({ at: "failed", seen, error: failure(e) });
    }
  };

  const setLinked = (v: boolean) => {
    setLink(v);
    if (stage.at === "seen") setStage({ at: "source" });
  };

  const choose = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false, title: "Choose the plugin folder" });
    if (typeof picked === "string") {
      setSource(picked);
      setStage({ at: "source" });
    }
  };

  const busy = stage.at === "looking" || stage.at === "installing";
  const seen = "seen" in stage ? stage.seen : null;

  return (
    <div ref={panel} className="install-panel" data-install-panel>
      <div className="install-panel-head">
        <div className="settings-label">{stage.at === "done" ? "Installed" : "Install a plugin"}</div>
        <Tooltip label="Close">
          <button type="button" className="bar-button" onClick={onClose} aria-label="Close the install" disabled={busy}>
            <X size={15} />
          </button>
        </Tooltip>
      </div>

      {stage.at === "source" || stage.at === "looking" || stage.at === "seen" ? (
        <>
          <div className="install-source">
            <input
              ref={field}
              className="settings-input install-source-field"
              type="text"
              aria-label="Source"
              placeholder="/path/to/plugin or /path/to/plugin.zip"
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              spellCheck={false}
              value={source}
              disabled={busy}
              onChange={(e) => {
                setSource(e.target.value);
                if (stage.at === "seen") setStage({ at: "source" });
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") look();
                if (e.key === "Escape" && !busy) {
                  e.stopPropagation();
                  onClose();
                }
              }}
            />
            {native ? (
              <Tooltip label="Choose a folder">
                <button
                  type="button"
                  className="bar-button"
                  onClick={choose}
                  aria-label="Choose a folder"
                  disabled={busy}
                >
                  <FolderOpen size={15} />
                </button>
              </Tooltip>
            ) : null}
            <button
              type="button"
              className="chrome-button"
              onClick={look}
              disabled={busy || !source.trim()}
              data-install-look
            >
              {stage.at === "looking" ? "Inspecting…" : "Inspect"}
            </button>
          </div>
          {source.trim() && !isZip(source) ? (
            <div className="install-link">
              <Toggle label="Link instead of copying" checked={link} disabled={busy} onChange={setLinked} />
              <span onClick={() => !busy && setLinked(!link)}>
                Link instead of copying
                <span className="faint"> (changes to the folder appear immediately)</span>
              </span>
            </div>
          ) : null}
          {error ? <p className="notice notice-danger install-error">{error}</p> : null}
        </>
      ) : null}

      {seen && stage.at !== "done" ? (
        <div className="install-seen" data-install-seen>
          <div className="install-seen-head">
            <span className="settings-row-icon">
              <PluginIcon icon={seen.icon} size={16} strokeWidth={1.75} />
            </span>
            <div>
              <div className="install-seen-title">{seen.title}</div>
              <div className="faint mono">
                {seen.name} · {seen.version}
              </div>
            </div>
          </div>
          <Origin seen={seen} />
          <Consequences seen={seen} />
        </div>
      ) : null}

      {stage.at === "failed" ? <p className="notice notice-danger install-error">{stage.error}</p> : null}

      {stage.at === "done" ? (
        <p className="install-done" data-install-done>
          <b>{stage.plugin.title || stage.plugin.name}</b> {stage.plugin.version} is ready. Reviews for this plugin now
          open with this version.
        </p>
      ) : null}

      <div ref={actions} className="dialog-actions install-actions">
        {stage.at === "seen" ? (
          <button
            type="button"
            className="chrome-button button-primary"
            onClick={() => install(stage.seen)}
            data-install-confirm
          >
            {stage.seen.link ? "Link" : stage.seen.installed?.unchanged ? "Install again" : "Install"}
          </button>
        ) : stage.at === "failed" ? (
          <button type="button" className="chrome-button" onClick={() => setStage({ at: "seen", seen: stage.seen })}>
            Back
          </button>
        ) : stage.at === "done" ? (
          <button type="button" className="chrome-button button-primary" onClick={onClose} data-install-close>
            Done
          </button>
        ) : stage.at === "installing" ? (
          <span className="dim install-wait">Installing…</span>
        ) : null}
      </div>
    </div>
  );
}
