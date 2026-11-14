// The Start an agent dialog: a folder, an agent, a task and extra
// arguments, then the new session in the view a new run opens in. An
// agent that names its session only once it starts, such as Codex, is
// waited for until it does.

import { FolderOpen } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { ApiError, inTauri } from "../api/client";
import { runtime, type RecentFolder, type StartableAgent } from "../api/runtime";
import { splitArgs } from "../lib/args";
import { combo, hasMod, isTyping, MOD } from "../lib/keys";
import { useWindowKey } from "../lib/useWindowKey";
import { Dialog } from "./Dialog";
import { terminalPool } from "./terminal/TerminalPane";
import { sessionPath, terminalArea } from "./Thread";

// the last answers, shown at once the next time the dialog opens
let knownAgents: StartableAgent[] = [];
let knownFolders: RecentFolder[] = [];

/** How long the dialog waits for an agent to name its session. */
const BIND_WITHIN_MS = 20_000;

/** The grid the new session's terminal will fit, as its renderer
 *  measures it in the space the session page will give it. */
async function sessionGrid(): Promise<{ cols: number; rows: number }> {
  const main = document.querySelector(".app-main")?.getBoundingClientRect();
  const area = terminalArea(main ?? new DOMRect(0, 0, window.innerWidth, window.innerHeight));
  try {
    const { cols, rows } = await terminalPool().gridFor(area.width, area.height);
    return { cols: Math.min(1000, Math.max(20, cols)), rows: Math.min(1000, Math.max(5, rows)) };
  } catch {
    return { cols: 120, rows: 40 };
  }
}

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

export function StartAgentDialog({ onClose, task: initialTask = "" }: { onClose: () => void; task?: string }) {
  const navigate = useNavigate();
  const [agents, setAgents] = useState(knownAgents);
  const [folders, setFolders] = useState(knownFolders);
  const [folder, setFolder] = useState(() => knownFolders.find((f) => f.exists)?.folder ?? "");
  const [listOpen, setListOpen] = useState(false);
  const [agent, setAgent] = useState<string | null>(
    () => (knownAgents.find((a) => a.last_used && a.installed) ?? knownAgents.find((a) => a.installed))?.id ?? null,
  );
  const [task, setTask] = useState(initialTask);
  const [args, setArgs] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState<string | null>(null);
  const folderField = useRef<HTMLInputElement>(null);
  const localId = useRef(crypto.randomUUID());

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
  const typed = folders.find((f) => f.folder === folder);
  const folderGone = !!typed && !typed.exists;
  const ready = !!folder.trim() && !!chosen?.installed && !folderGone && !starting;

  const chooseFolder = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false, title: "Choose a folder" });
    if (typeof picked === "string") setFolder(picked);
    setListOpen(false);
    folderField.current?.focus();
  };

  const start = async () => {
    if (!ready || !chosen) return;
    setError(null);
    setStarting(`Starting ${chosen.name}…`);
    try {
      const { run, session } = await runtime.start({
        agent: chosen.id,
        folder: folder.trim(),
        task: task.trim() || undefined,
        args: splitArgs(args),
        ...(await sessionGrid()),
        local_id: localId.current,
      });
      let named = session;
      // an agent that names its session once it starts is waited for
      const until = Date.now() + BIND_WITHIN_MS;
      while (!named && Date.now() < until) {
        await new Promise((r) => setTimeout(r, 400));
        const now = await runtime.run(run.id);
        named = now.session;
        if (now.state === "ended") break;
      }
      if (!named) {
        setStarting(null);
        setError(
          `${chosen.name} started, but has not reported its session. Turn on Report status for ${chosen.name} in Settings › Agents, and its session appears in the sidebar.`,
        );
        return;
      }
      onClose();
      navigate(sessionPath(named), { state: { newRun: true } });
    } catch (e) {
      setStarting(null);
      // a new attempt is a new start
      localId.current = crypto.randomUUID();
      setError(refusal(e, chosen));
    }
  };

  useWindowKey(
    (event) => {
      if (hasMod(event) && event.key === "Enter" && !event.shiftKey && !event.altKey) {
        event.preventDefault();
        void start();
        return;
      }
      // 1 to 9 pick an agent, outside the text fields
      if (event.metaKey || event.ctrlKey || event.altKey || isTyping(event.target)) return;
      const n = Number(event.key);
      if (!Number.isInteger(n) || n < 1 || n > 9) return;
      const pick = shown[n - 1];
      if (pick?.installed) {
        event.preventDefault();
        setAgent(pick.id);
      }
    },
    { capture: true },
  );

  return (
    <Dialog
      labelledBy="start-agent-title"
      className="start-agent-dialog"
      initialFocus={folderField}
      onClose={onClose}
      // Esc closes the folder list first
      closeOnEscape={(event) => !event.defaultPrevented}
    >
      <div className="dialog-head">
        <h2 id="start-agent-title">Start an agent</h2>
      </div>

      <label className="start-agent-label" htmlFor="start-agent-folder">
        Folder
      </label>
      <div className="start-agent-folder">
        <input
          ref={folderField}
          id="start-agent-folder"
          role="combobox"
          aria-expanded={listOpen}
          aria-controls="start-agent-folders"
          aria-autocomplete="list"
          autoComplete="off"
          spellCheck={false}
          placeholder="/path/to/project"
          value={folder}
          onChange={(e) => {
            setFolder(e.target.value);
            setListOpen(true);
          }}
          onFocus={() => setListOpen(true)}
          onBlur={() => setTimeout(() => setListOpen(false), 150)}
          onKeyDown={(e) => {
            if (e.key === "Escape" && listOpen) {
              e.preventDefault();
              setListOpen(false);
            }
          }}
        />
        {inTauri() ? (
          <button type="button" className="chrome-button" onClick={() => void chooseFolder()}>
            <FolderOpen size={14} aria-hidden /> Choose a folder…
          </button>
        ) : null}
      </div>
      {listOpen && folders.length ? (
        <ul className="start-agent-folders" id="start-agent-folders" role="listbox" aria-label="Recent folders">
          {folders
            .filter((f) => !folder || f.folder.toLowerCase().includes(folder.toLowerCase()) || folder === typed?.folder)
            .map((f) => (
              <li
                key={f.folder}
                role="option"
                aria-selected={f.folder === folder}
                aria-disabled={!f.exists}
                className={f.exists ? undefined : "is-gone"}
                onMouseDown={(e) => {
                  e.preventDefault();
                  if (!f.exists) return;
                  setFolder(f.folder);
                  setListOpen(false);
                }}
              >
                <span className="start-agent-folder-path truncate">{f.folder}</span>
                <span className="start-agent-folder-where dim">
                  {f.exists ? [f.repo, f.branch].filter(Boolean).join(" · ") : "This folder no longer exists."}
                </span>
              </li>
            ))}
        </ul>
      ) : null}
      {folderGone ? <p className="notice notice-danger">This folder no longer exists.</p> : null}

      <span className="start-agent-label" id="start-agent-agents">
        Agent
      </span>
      <div className="start-agent-agents" role="radiogroup" aria-labelledby="start-agent-agents">
        {shown.map((a, i) => (
          <button
            key={a.id}
            type="button"
            role="radio"
            aria-checked={a.id === agent}
            disabled={!a.installed}
            className={`start-agent-agent ${a.id === agent ? "is-chosen" : ""}`}
            onClick={() => setAgent(a.id)}
          >
            <kbd>{i + 1}</kbd>
            <span>{a.name}</span>
            <span className="dim">{a.installed ? (a.version ?? "") : "Not installed"}</span>
          </button>
        ))}
        {agents.length === 0 ? <span className="dim">Looking for agents…</span> : null}
      </div>

      <label className="start-agent-label" htmlFor="start-agent-task">
        Task
      </label>
      <textarea
        id="start-agent-task"
        className="start-agent-task"
        rows={3}
        placeholder="What should the agent do?"
        value={task}
        onChange={(e) => setTask(e.target.value)}
      />

      <details className="start-agent-more">
        <summary>More</summary>
        <label className="start-agent-label" htmlFor="start-agent-args">
          Extra arguments
        </label>
        <input
          id="start-agent-args"
          spellCheck={false}
          autoComplete="off"
          placeholder="--model opus"
          value={args}
          onChange={(e) => setArgs(e.target.value)}
        />
      </details>

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
      <div className="dialog-actions">
        <button type="button" className="chrome-button" onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          className="chrome-button button-primary"
          disabled={!ready}
          onClick={() => void start()}
          title={combo(MOD, "Enter")}
        >
          Start
        </button>
      </div>
    </Dialog>
  );
}
