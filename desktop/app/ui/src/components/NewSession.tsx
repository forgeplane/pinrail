// The New session page: the agent and the folder above a message box, as a
// chat starts. The first message is optional; without one, the agent starts
// with nothing to do. The new session opens in the view a new run opens in.
// An agent that names its session only once it starts, such as Codex, is
// waited for until it does.

import {
  ArrowUp,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  FolderOpen,
  Lightbulb,
  SlidersHorizontal,
} from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { ApiError, inTauri } from "../api/client";
import { runtime, type RecentFolder, type StartableAgent } from "../api/runtime";
import { splitArgs } from "../lib/args";
import { combo, hasMod, MOD } from "../lib/keys";
import { TIPS, tipParts } from "../lib/tips";
import { useWindowKey } from "../lib/useWindowKey";
import { AgentIcon } from "./AgentIcon";
import { useItemPicker } from "./ItemPicker";
import { PopoverMenu, type MenuEntry } from "./PopoverMenu";
import { startRun } from "./session/startRun";
import { sessionPath } from "./Thread";

/** Where the page is. */
export const NEW_SESSION_PATH = "/sessions/new";

/** What the page opens with, such as a run to start again after an edit. */
export type NewSessionInitial = { folder?: string; agent?: string; task?: string; args?: string[] };

// the last answers, shown at once the next time the page opens
let knownAgents: StartableAgent[] = [];
let knownFolders: RecentFolder[] = [];

/** What the person reads when a start is refused. */
function refusal(error: unknown, agent: StartableAgent | undefined): string {
  if (!(error instanceof ApiError)) return "Pinrail could not be reached.";
  const name = agent?.name ?? "The agent";
  switch (error.kind) {
    case "agent_not_found":
      return `${name} is not installed on this computer.`;
    case "folder_missing":
      return "The folder no longer exists.";
    case "host_offline":
      return "The terminal host is not running. Pinrail is starting it again.";
    default:
      return error.violations[0]?.message ?? error.message;
  }
}

/** The arguments as they are written in the field. */
const writeArgs = (args: string[]) => args.map((a) => (/[\s"'\\]/.test(a) || !a ? JSON.stringify(a) : a)).join(" ");

/** Fits a message box to its text, up to its maximum height, past which
 *  it scrolls. */
function grow(box: HTMLTextAreaElement) {
  box.style.height = "auto";
  box.style.height = `${box.scrollHeight}px`;
  box.style.overflowY = box.scrollHeight > box.clientHeight + 1 ? "auto" : "hidden";
}

/** A folder's last part, which names it on its picker. */
const baseName = (folder: string) => folder.replace(/\/+$/, "").split("/").pop() || folder;

export function NewSession() {
  const navigate = useNavigate();
  const initial = ((useLocation().state as { start?: NewSessionInitial } | null)?.start ?? {}) as NewSessionInitial;
  const [agents, setAgents] = useState(knownAgents);
  const [folders, setFolders] = useState(knownFolders);
  const [folder, setFolder] = useState(() => initial.folder ?? knownFolders.find((f) => f.exists)?.folder ?? "");
  const [agent, setAgent] = useState<string | null>(
    () =>
      initial.agent ??
      (knownAgents.find((a) => a.last_used && a.installed) ?? knownAgents.find((a) => a.installed))?.id ??
      null,
  );
  const [task, setTask] = useState(initial.task ?? "");
  const [args, setArgs] = useState(initial.args ? writeArgs(initial.args) : "");
  const [showArgs, setShowArgs] = useState(!!initial.args?.length);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState<string | null>(null);
  const [agentMenu, setAgentMenu] = useState<HTMLElement | null>(null);
  // a tip picked when the page opens, and the next one on request
  const [tip, setTip] = useState(() => Math.floor(Math.random() * TIPS.length));
  const [folderOpen, setFolderOpen] = useState(false);
  const message = useRef<HTMLTextAreaElement>(null);
  const localId = useRef(crypto.randomUUID());
  // the box fits its text, as it is typed, carried over or given a key
  useLayoutEffect(() => {
    if (message.current) grow(message.current);
  }, [task]);

  // the last known lists show at once; fresh ones replace them
  useEffect(() => {
    let current = true;
    runtime.startable().then(
      (found) => {
        knownAgents = found;
        if (!current) return;
        setAgents(found);
        setAgent((chosen) => {
          if (chosen && found.some((a) => a.id === chosen && a.installed)) return chosen;
          return (found.find((a) => a.last_used && a.installed) ?? found.find((a) => a.installed))?.id ?? null;
        });
      },
      (e: unknown) => current && setError(refusal(e, undefined)),
    );
    runtime.recentFolders().then(
      (found) => {
        knownFolders = found;
        if (!current) return;
        setFolders(found);
        setFolder((typed) => typed || found.find((f) => f.exists)?.folder || "");
      },
      () => {},
    );
    return () => {
      current = false;
    };
  }, []);

  // installed agents first, then up to two that are not
  const shown = [...agents.filter((a) => a.installed), ...agents.filter((a) => !a.installed).slice(0, 2)];
  const chosen = agents.find((a) => a.id === agent);
  const known = folders.find((f) => f.folder === folder);
  const folderGone = !!known && !known.exists;
  // # lists the work items, the folder's repository's first
  const picker = useItemPicker({ box: message, value: task, onChange: setTask, repo: known?.repo ?? null });
  // an agent that chooses its own session's id tells Pinrail only through
  // its hooks: without them, its session would never show
  const needsReports = !!chosen?.installed && chosen.names_session === false && chosen.reports === false;
  // one whose session Pinrail names starts without them, and shows its
  // terminal, but its session shows no turns, prompts or replies
  const offersReports = !!chosen?.installed && !needsReports && chosen.reports === false;
  const ready = !!folder.trim() && !!chosen?.installed && !folderGone && !starting && !needsReports;
  const [turningOn, setTurningOn] = useState(false);
  const turnOnReports = async () => {
    if (!chosen) return;
    setTurningOn(true);
    setError(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_agent_hooks", { id: chosen.id, on: true });
      const found = await runtime.startable();
      knownAgents = found;
      setAgents(found);
    } catch (e) {
      setError(typeof e === "string" ? e : `Pinrail could not turn on Report status for ${chosen.name}.`);
    } finally {
      setTurningOn(false);
    }
  };

  const start = async () => {
    if (!ready || !chosen) return;
    setError(null);
    setStarting(`Starting ${chosen.name}…`);
    try {
      const session = await startRun({
        agent: chosen.id,
        folder: folder.trim(),
        task: task.trim() || undefined,
        args: splitArgs(args),
        local_id: localId.current,
      });
      // the next press of Start is a new start
      localId.current = crypto.randomUUID();
      navigate(sessionPath(session), { replace: true, state: { newRun: true } });
    } catch (e) {
      setStarting(null);
      // a new attempt is a new start
      localId.current = crypto.randomUUID();
      setError(refusal(e, chosen));
    }
  };

  useWindowKey((event) => {
    if (hasMod(event) && event.key === "Enter" && !event.shiftKey && !event.altKey) {
      event.preventDefault();
      void start();
    }
  });

  const agentEntries: MenuEntry[] = shown.map((a) => ({
    label: a.name,
    mark: <AgentIcon requestedBy={a.id} size={14} />,
    hint: a.installed ? (a.version ?? undefined) : undefined,
    checked: a.id === agent,
    disabled: a.installed ? undefined : "Not installed",
    onPick: () => {
      setAgent(a.id);
      message.current?.focus();
    },
  }));

  return (
    <div className="new-session">
      <div className="new-session-column">
        <h1 className="new-session-title">
          What should <em>{chosen?.name ?? "the agent"}</em> work on
          {folder ? (
            <>
              {" "}
              in <em>{baseName(folder)}</em>
            </>
          ) : null}
          ?
        </h1>
        <div className="new-session-pickers">
          <button
            type="button"
            className="new-session-picker"
            aria-label={`Agent: ${chosen?.name ?? "none"}`}
            aria-haspopup="menu"
            onClick={(event) => {
              const button = event.currentTarget;
              setAgentMenu((open) => (open ? null : button));
            }}
          >
            {chosen ? <AgentIcon requestedBy={chosen.id} size={14} /> : null}
            <span>{chosen?.name ?? (agents.length ? "Choose an agent" : "Looking for agents…")}</span>
            {chosen?.version ? <span className="faint">{chosen.version}</span> : null}
            <ChevronDown size={13} aria-hidden="true" />
          </button>
          <FolderPicker
            folder={folder}
            folders={folders}
            open={folderOpen}
            onOpen={setFolderOpen}
            onChoose={(picked) => {
              setFolder(picked);
              setFolderOpen(false);
              message.current?.focus();
            }}
          />
        </div>
        {agentMenu ? (
          <PopoverMenu anchor={agentMenu} label="Agents" entries={agentEntries} onClose={() => setAgentMenu(null)} />
        ) : null}

        <div className="new-session-box">
          <textarea
            ref={message}
            // the page is there to start a session
            autoFocus
            aria-label="First message"
            className="new-session-message"
            rows={3}
            placeholder="Describe the task, # for a work item, or leave it empty to start without one"
            value={task}
            onChange={(e) => setTask(e.target.value)}
          />
          {picker}
          {showArgs ? (
            <input
              className="new-session-args"
              aria-label="Extra arguments"
              spellCheck={false}
              autoComplete="off"
              placeholder="Extra arguments, such as --model opus"
              value={args}
              onChange={(e) => setArgs(e.target.value)}
            />
          ) : null}
          <div className="new-session-box-foot">
            <button
              type="button"
              className={`new-session-chip ${showArgs ? "is-on" : ""}`}
              aria-pressed={showArgs}
              onClick={() => setShowArgs((on) => !on)}
            >
              <SlidersHorizontal size={13} aria-hidden="true" />
              Arguments
            </button>
            <button
              type="button"
              className="new-session-start"
              disabled={!ready}
              onClick={() => void start()}
              aria-label="Start"
              title={`Start (${combo(MOD, "Enter")})`}
            >
              <ArrowUp size={16} aria-hidden="true" />
            </button>
          </div>
        </div>

        {folderGone ? <p className="notice notice-danger">This folder no longer exists.</p> : null}
        {needsReports ? (
          <div className="notice new-session-reports" role="status">
            <p>
              {chosen.name} needs Report status to start from Pinrail: it tells Pinrail its session only through its
              status hooks.
              {inTauri() ? null : " Turn it on in Settings › Agents."}
            </p>
            {inTauri() ? (
              <button
                type="button"
                className="chrome-button button-primary"
                disabled={turningOn}
                onClick={() => void turnOnReports()}
              >
                Turn on Report status
              </button>
            ) : null}
          </div>
        ) : null}
        {offersReports ? (
          <div className="notice new-session-reports" data-testid="new-session-reports-offer">
            <p>
              Report status shows {chosen.name}&apos;s turns, prompts and replies in its session.
              {inTauri() ? null : " Turn it on in Settings › Agents."}
            </p>
            {inTauri() ? (
              <button type="button" className="chrome-button" disabled={turningOn} onClick={() => void turnOnReports()}>
                Turn on Report status
              </button>
            ) : null}
          </div>
        ) : null}
        {error ? (
          <p className="notice notice-danger" role="alert">
            {error}
          </p>
        ) : null}
        {starting ? (
          <p className="dim" role="status">
            {starting}
          </p>
        ) : null}
        {error || starting ? null : (
          <aside className="new-session-tip" aria-label="Tip" data-testid="new-session-tip">
            <Lightbulb size={13} className="new-session-tip-icon" aria-hidden="true" />
            <p className="new-session-tip-text">
              <span className="new-session-tip-category">{TIPS[tip].category}</span>
              {tipParts(TIPS[tip].text).map((part, i) =>
                typeof part === "string" ? (
                  part
                ) : (
                  <span key={i} className="new-session-tip-keys">
                    {part.map((key) => (
                      <kbd key={key}>{key}</kbd>
                    ))}
                  </span>
                ),
              )}
            </p>
            <span className="new-session-tip-nav">
              <button
                type="button"
                className="bar-button"
                aria-label="Previous tip"
                title="Previous tip"
                onClick={() => setTip((t) => (t + TIPS.length - 1) % TIPS.length)}
              >
                <ChevronLeft size={14} />
              </button>
              <button
                type="button"
                className="bar-button"
                aria-label="Next tip"
                title="Next tip"
                onClick={() => setTip((t) => (t + 1) % TIPS.length)}
              >
                <ChevronRight size={14} />
              </button>
            </span>
          </aside>
        )}
      </div>
    </div>
  );
}

/** The folder's picker: its name, with the project and branch, and under it
 *  the recent folders, filtered as a path is typed, and the system's folder
 *  dialog in the app. */
function FolderPicker({
  folder,
  folders,
  open,
  onOpen,
  onChoose,
}: {
  folder: string;
  folders: RecentFolder[];
  open: boolean;
  onOpen: (open: boolean) => void;
  onChoose: (folder: string) => void;
}) {
  const [typed, setTyped] = useState("");
  const [active, setActive] = useState(0);
  const known = folders.find((f) => f.folder === folder);
  const query = typed.trim().toLowerCase();
  const listed = folders.filter((f) => !query || f.folder.toLowerCase().includes(query));
  // a typed path that is no recent folder can be chosen as it is
  const asTyped = typed.trim().startsWith("/") && !folders.some((f) => f.folder === typed.trim());
  const choices = [...(asTyped ? [typed.trim()] : []), ...listed.filter((f) => f.exists).map((f) => f.folder)];

  const chooseFolder = async () => {
    const { open: pick } = await import("@tauri-apps/plugin-dialog");
    const picked = await pick({ directory: true, multiple: false, title: "Choose a folder" });
    if (typeof picked === "string") onChoose(picked);
  };

  return (
    <div className="new-session-folder">
      <button
        type="button"
        className="new-session-picker"
        aria-label={`Folder: ${folder || "none"}`}
        aria-expanded={open}
        aria-controls="new-session-folders"
        title={folder}
        onClick={() => {
          // each opening starts from the whole list
          setTyped("");
          setActive(0);
          onOpen(!open);
        }}
      >
        <FolderOpen size={14} aria-hidden="true" />
        <span>{folder ? baseName(folder) : "Choose a folder"}</span>
        {known?.repo || known?.branch ? (
          <span className="faint">{[known.repo, known.branch].filter(Boolean).join(" · ")}</span>
        ) : null}
        <ChevronDown size={13} aria-hidden="true" />
      </button>
      {open ? (
        <div
          className="new-session-folders"
          id="new-session-folders"
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node | null)) onOpen(false);
          }}
        >
          <input
            autoFocus
            role="combobox"
            aria-expanded="true"
            aria-controls="new-session-folder-list"
            aria-label="Folder"
            autoComplete="off"
            spellCheck={false}
            placeholder="Filter, or type a path"
            value={typed}
            onChange={(e) => {
              setTyped(e.target.value);
              setActive(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                onOpen(false);
              } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                const step = e.key === "ArrowDown" ? 1 : -1;
                setActive((i) => Math.max(0, Math.min(choices.length - 1, i + step)));
              } else if (e.key === "Enter" && choices[active]) {
                e.preventDefault();
                onChoose(choices[active]);
              }
            }}
          />
          <ul id="new-session-folder-list" role="listbox" aria-label="Recent folders">
            {asTyped ? (
              <li
                role="option"
                aria-selected={active === 0}
                className={active === 0 ? "is-active" : undefined}
                onMouseDown={(e) => {
                  e.preventDefault();
                  onChoose(typed.trim());
                }}
              >
                <span className="truncate">{typed.trim()}</span>
                <span className="dim">Use this folder</span>
              </li>
            ) : null}
            {listed.map((f) => {
              const at = choices.indexOf(f.folder);
              return (
                <li
                  key={f.folder}
                  role="option"
                  aria-selected={f.folder === folder}
                  aria-disabled={!f.exists}
                  className={
                    [f.exists ? "" : "is-gone", at === active ? "is-active" : ""].join(" ").trim() || undefined
                  }
                  onMouseDown={(e) => {
                    e.preventDefault();
                    if (f.exists) onChoose(f.folder);
                  }}
                >
                  <span className="truncate">{f.folder}</span>
                  <span className="dim">
                    {f.exists ? [f.repo, f.branch].filter(Boolean).join(" · ") : "This folder no longer exists."}
                  </span>
                </li>
              );
            })}
          </ul>
          {inTauri() ? (
            <button type="button" className="new-session-choose" onClick={() => void chooseFolder()}>
              <FolderOpen size={14} aria-hidden="true" /> Choose a folder…
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
