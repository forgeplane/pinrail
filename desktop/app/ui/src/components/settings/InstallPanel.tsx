// Installing a plugin, in place in the Plugins section: one field for the
// source, a look at what it is before anything runs, the words that say
// what will run on this machine, and the log as the install goes. Install
// is the consent.

import { FolderOpen, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ApiError, api, inTauri, type InstallRequest } from "../../api/client";
import type { InstallJob, Inspection } from "../../api/types";
import { takes } from "../../lib/format";
import { followJob } from "../../lib/jobs";
import { PluginIcon } from "../PluginIcon";
import { Tooltip } from "../Tooltip";
import { Toggle } from "./controls";

type Stage =
  | { at: "source" }
  | { at: "looking" }
  | { at: "seen"; seen: Inspection }
  | { at: "installing"; seen: Inspection; job: InstallJob | null }
  | { at: "done"; job: InstallJob }
  | { at: "failed"; seen: Inspection; job: InstallJob };

const STEPS: InstallJob["status"][] = ["fetching", "inspecting", "building", "placing"];

/** A path on this machine, as opposed to a URL or a repository. */
const isLocal = (source: string) => /^(\/|\.|~)/.test(source.trim());
const isRelease = (source: string) => /\/releases(\/|$)/.test(source.trim());

const short = (commit: string | null) => (commit ? commit.slice(0, 7) : null);

const size = (bytes: number | undefined) => {
  if (bytes === undefined) return null;
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
};

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
        <span className="mono">{String(origin.resolved)}</span>
      </p>
    );
  }
  const r = typeof origin.resolved === "object" ? origin.resolved : {};
  if (origin.kind === "git") {
    return (
      <p>
        From the repository <span className="mono">{r.url}</span>
        {r.path ? (
          <>
            , folder <span className="mono">{r.path}</span>
          </>
        ) : null}
        {r.ref ? (
          <>
            , at <span className="mono">{r.ref}</span>
          </>
        ) : null}
        {origin.commit ? (
          <>
            {" "}
            · commit <span className="mono">{short(origin.commit)}</span>
          </>
        ) : null}
      </p>
    );
  }
  return (
    <p>
      Release <span className="mono">{r.tag}</span> of{" "}
      <span className="mono">
        {r.owner}/{r.repo}
      </span>
      , asset <span className="mono">{r.asset}</span>
      {r.asset_size !== undefined ? ` (${size(r.asset_size)})` : ""}
      {r.pinned ? ", pinned to this tag" : ", the latest release"}
    </p>
  );
}

/** What runs on this computer, and what happens to what is already installed. */
function Consequences({ seen }: { seen: Inspection }) {
  const local = seen.origin.kind === "folder";
  const installed = seen.installed;
  return (
    <>
      {seen.origin.kind === "release" ? (
        <p className="install-runs" data-runs="nothing">
          <b>Nothing runs on your computer.</b> The bundle is unpacked, checked and used as it is.
        </p>
      ) : seen.link ? (
        <p className="install-runs" data-runs="nothing">
          <b>Nothing is copied.</b> Pinrail serves the folder directly, so changes appear the next time the view opens.
        </p>
      ) : seen.build ? (
        <div className={`install-runs ${local ? "" : "is-warning"}`} data-runs="build">
          <p>
            <b>Builds with</b> <code className="mono">{seen.build}</code>
          </p>
          <p>
            {local
              ? "The command runs on this computer with your user permissions, through the shell."
              : "The command runs on this computer with your user permissions, outside any sandbox, together with any scripts the dependencies run when they install."}{" "}
            Any tools it needs must be on your PATH. Choose Install to run it.
          </p>
        </div>
      ) : (
        <p className="install-runs" data-runs="nothing">
          <b>No build.</b> The folder is copied without source files, tests and hidden files.
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
            ? installed.path === String(seen.origin.resolved)
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
  const [ref, setRef] = useState("");
  const [path, setPath] = useState("");
  const [link, setLink] = useState(false);
  const [stage, setStage] = useState<Stage>({ at: "source" });
  // a job followed after the panel closed would poll for nothing
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const [error, setError] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const logBox = useRef<HTMLPreElement>(null);
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

  // the log keeps up with the build
  useEffect(() => {
    if (logBox.current) logBox.current.scrollTop = logBox.current.scrollHeight;
  });

  const request = (): InstallRequest => {
    const body: InstallRequest = { source: source.trim(), link };
    if (!isLocal(source) && !isRelease(source)) {
      if (ref.trim()) body.ref = ref.trim();
      if (path.trim()) body.path = path.trim();
    }
    return body;
  };

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
    setStage({ at: "installing", seen, job: null });
    try {
      const { job } = await api.installPlugin({ ...request(), force: seen.older, expect: seen.expect });
      const state = await followJob(
        job,
        (step) => setStage({ at: "installing", seen, job: step }),
        () => !mounted.current,
      );
      if (!state) return;
      if (state.status === "done") setStage({ at: "done", job: state });
      else setStage({ at: "failed", seen, job: state });
    } catch (e) {
      setError(failure(e));
      setStage({ at: "seen", seen });
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
  const job = "job" in stage ? stage.job : null;

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
              placeholder="/path/to/plugin, github.com/owner/repo/folder@ref, or a releases page"
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
          {!isLocal(source) && !isRelease(source) && source.trim() ? (
            <div className="install-beside">
              <input
                className="settings-input"
                type="text"
                aria-label="Ref"
                placeholder="Branch, tag or commit"
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck={false}
                value={ref}
                disabled={busy}
                onChange={(e) => setRef(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && look()}
              />
              <input
                className="settings-input"
                type="text"
                aria-label="Folder"
                placeholder="Folder in the repository"
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck={false}
                value={path}
                disabled={busy}
                onChange={(e) => setPath(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && look()}
              />
            </div>
          ) : null}
          {isLocal(source) ? (
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

      {stage.at === "installing" || stage.at === "failed" ? (
        <div className="install-progress" data-install-progress={job?.status ?? "starting"}>
          <ol className="install-steps">
            {STEPS.map((step) => {
              const current = job?.status ?? "fetching";
              const index = STEPS.indexOf(current as InstallJob["status"]);
              const at = STEPS.indexOf(step);
              const state =
                stage.at === "failed" ? (at <= index ? "failed" : "") : at < index ? "done" : at === index ? "now" : "";
              return (
                <li key={step} className={state}>
                  {step}
                </li>
              );
            })}
          </ol>
          {job?.log ? (
            <pre ref={logBox} className="install-log" data-install-log>
              {job.log}
            </pre>
          ) : null}
          {stage.at === "failed" ? <p className="notice notice-danger install-error">{stage.job.error}</p> : null}
        </div>
      ) : null}

      {stage.at === "done" ? (
        <p className="install-done" data-install-done>
          <b>{stage.job.plugin?.title ?? stage.job.plugin?.name}</b> {stage.job.plugin?.version} is ready. Reviews for
          this plugin now open with this version.
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
          <span className="dim install-wait">{job?.status === "building" ? "Building…" : "Working…"}</span>
        ) : null}
      </div>
    </div>
  );
}
