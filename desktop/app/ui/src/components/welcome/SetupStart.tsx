// The setup's last screen: a first session. It offers a request for each
// plugin installed, to start one of the agents just connected on, in a
// folder, and takes a request of the person's own. When Pinrail can start
// none of the connected agents, the requests are there to copy.

import { ArrowRight, ArrowUp, Check, ChevronDown, Copy, Folder, FolderOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { inTauri } from "../../api/client";
import { runtime, type RunSession, type StartableAgent } from "../../api/runtime";
import type { Plugin } from "../../api/types";
import { useCopy } from "../../lib/useCopy";
import { AgentIcon } from "../AgentIcon";
import { baseName, refusal } from "../NewSession";
import { PluginIcon } from "../PluginIcon";
import { PopoverMenu, type MenuEntry } from "../PopoverMenu";
import { startRun } from "../session/startRun";
import type { AgentStatus } from "../settings/AgentsSection";
import { chooseFolder } from "../useAgentStart";
import { startFolders, type StartFolder } from "./startFolders";

/** A request to give an agent for each official plugin, in the order the
 *  screen offers them. The skill that connecting an agent adds tells it
 *  how to submit. */
const REQUESTS: [plugin: string, request: string][] = [
  ["list", "Ask me through Pinrail which TODOs in this repository to tackle first."],
  ["feedback", "Ask me through Pinrail what you need to know before the next task."],
  ["code-review", "Review my last commit, and send me your comments through Pinrail."],
  ["markdown", "Plan the next change, and ask me through Pinrail to review the plan."],
  ["image", "Make three versions of an icon for this project, and ask me through Pinrail which to keep."],
];
/** How many requests the screen has room for. */
const ROOM = 3;

/** A folder's path with ~ for the home folder. */
const short = (folder: string) => folder.replace(/^\/(?:Users|home)\/[^/]+/, "~");

function CopyRequest({ text }: { text: string }) {
  const { state, copy } = useCopy(1600);
  return (
    <button type="button" className="setup-request-go" onClick={() => void copy(text)}>
      {state === "done" ? <Check size={13} /> : <Copy size={13} />} {state === "done" ? "Copied" : "Copy"}
    </button>
  );
}

export function SetupStart({
  connected,
  plugins,
  onMenu,
  onStarted,
  onClose,
}: {
  /** the agents that have Pinrail's skill, in the welcome's order */
  connected: AgentStatus[];
  /** the plugins installed */
  plugins: Plugin[];
  /** says whether one of the screen's menus is open, which takes Esc */
  onMenu: (open: boolean) => void;
  /** the session a request started, and the agent it started */
  onStarted: (session: RunSession, agent: StartableAgent) => void;
  onClose: () => void;
}) {
  // the agents Pinrail can start, or none when its runtime cannot be asked
  const [startable, setStartable] = useState<StartableAgent[] | null>(null);
  const [folders, setFolders] = useState<StartFolder[]>([]);
  useEffect(() => {
    let current = true;
    runtime.startable().then(
      (found) => current && setStartable(found),
      () => current && setStartable([]),
    );
    void startFolders().then((found) => current && setFolders(found));
    return () => {
      current = false;
    };
  }, []);

  // the connected agents Pinrail can start, in the welcome's order; one that
  // names its own session needs Report status to show up at all
  const agents = connected
    .map((a) => startable?.find((s) => s.id === a.id))
    .filter((s): s is StartableAgent => !!s && s.installed && !(s.names_session === false && s.reports === false));
  const [picked, setPicked] = useState<string | null>(null);
  const agent = agents.find((a) => a.id === picked) ?? agents[0];
  const [chosen, setChosen] = useState<string | null>(null);
  const folder = chosen ?? folders[0]?.folder ?? null;

  const [menu, setMenu] = useState<{ kind: "agent" | "folder"; anchor: HTMLElement } | null>(null);
  const open = (kind: "agent" | "folder", anchor: HTMLElement | null) => {
    setMenu(anchor ? { kind, anchor } : null);
    onMenu(!!anchor);
  };
  const [own, setOwn] = useState("");
  const [starting, setStarting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const requests = REQUESTS.filter(([name]) => plugins.some((p) => p.name === name && p.usable)).slice(0, ROOM);
  const iconOf = (name: string) => plugins.find((p) => p.name === name)?.icon;

  /** Asks for a folder in the system's dialog, and keeps the answer. */
  const askFolder = async () => {
    const answer = await chooseFolder();
    if (answer) setChosen(answer);
    return answer;
  };

  const start = async (task: string) => {
    if (!agent || starting) return;
    setError(null);
    // with no folder yet, the request asks for one first
    const where = folder ?? (inTauri() ? await askFolder() : null);
    if (!where) return;
    setStarting(task);
    try {
      const session = await startRun({ agent: agent.id, folder: where, task, local_id: crypto.randomUUID() });
      onStarted(session, agent);
    } catch (e) {
      setError(refusal(e, agent));
      setStarting(null);
    }
  };

  const agentEntries: MenuEntry[] = agents.map((a) => ({
    label: a.name,
    mark: <AgentIcon requestedBy={a.id} size={14} />,
    hint: a.version ?? undefined,
    checked: a.id === agent?.id,
    onPick: () => setPicked(a.id),
  }));
  const folderEntries: MenuEntry[] = [
    ...folders.map((f): MenuEntry => ({
      label: baseName(f.folder),
      icon: Folder,
      hint: short(f.folder),
      checked: f.folder === folder,
      onPick: () => setChosen(f.folder),
    })),
    // a folder chosen outside the list stays offered
    ...(folder && !folders.some((f) => f.folder === folder)
      ? [{ label: baseName(folder), icon: Folder, hint: short(folder), checked: true, onPick: () => {} } as MenuEntry]
      : []),
    ...(inTauri()
      ? [
          { kind: "separator" } as MenuEntry,
          { label: "Choose another folder…", icon: FolderOpen, onPick: () => void askFolder() } as MenuEntry,
        ]
      : []),
  ];

  const copying = startable !== null && !agent;
  return (
    <>
      {copying || startable === null ? null : (
        <button type="button" className="setup-corner is-skip" onClick={onClose}>
          Skip
        </button>
      )}
      <div className="setup-body">
        <h1 className="setup-title is-start">
          {copying ? "Pinrail is ready" : "Pinrail is ready. Start your first session."}
        </h1>
        {startable === null ? (
          <p className="setup-sub">Looking for the agents Pinrail can start…</p>
        ) : copying ? (
          <p className="setup-sub is-wide">
            {connected.length
              ? "Paste one of these requests into your agent to try it."
              : "Connect an agent in Settings › Agents, then paste one of these requests into it."}
          </p>
        ) : (
          <p className="setup-start-line">
            Start
            <button
              type="button"
              className="setup-pick"
              aria-label={`Agent: ${agent.name}`}
              aria-haspopup="menu"
              disabled={agents.length < 2}
              onClick={(e) => open("agent", menu ? null : e.currentTarget)}
            >
              <AgentIcon requestedBy={agent.id} size={16} />
              {agent.name}
              {agents.length > 1 ? <ChevronDown size={13} className="setup-pick-more" /> : null}
            </button>
            in
            <button
              type="button"
              className={`setup-pick ${folder ? "" : "is-empty"}`}
              aria-label={`Folder: ${folder ?? "none"}`}
              aria-haspopup={folderEntries.length > 2 ? "menu" : undefined}
              title={folder ?? undefined}
              disabled={!folder && !inTauri()}
              onClick={(e) =>
                // with nothing to list, the button asks for a folder at once
                folder || folders.length ? open("folder", menu ? null : e.currentTarget) : void askFolder()
              }
            >
              <Folder size={16} className="setup-pick-folder" />
              {folder ? baseName(folder) : "Choose a folder"}
              {folder || folders.length ? <ChevronDown size={13} className="setup-pick-more" /> : null}
            </button>
            with a request
          </p>
        )}

        {startable === null ? null : (
          <div className="setup-requests">
            {requests.map(([name, text]) => (
              <div key={name} className="setup-request" data-testid={`setup-request-${name}`}>
                <span className="setup-request-icon">
                  <PluginIcon icon={iconOf(name)} size={17} />
                </span>
                <span className="setup-request-text">{text}</span>
                {copying ? (
                  <CopyRequest text={text} />
                ) : (
                  <button
                    type="button"
                    className="setup-request-go is-start"
                    aria-label={`Start with: ${text}`}
                    disabled={!!starting}
                    onClick={() => void start(text)}
                  >
                    {starting === text ? "Starting…" : "Start"} <ArrowRight size={13} strokeWidth={2.2} />
                  </button>
                )}
              </div>
            ))}
            {copying ? null : (
              <form
                className="setup-request is-own"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (own.trim()) void start(own.trim());
                }}
              >
                <input
                  type="text"
                  aria-label="Your own request"
                  placeholder="Or write your own request"
                  value={own}
                  onChange={(e) => setOwn(e.target.value)}
                  disabled={!!starting}
                />
                <button type="submit" aria-label="Start with your request" disabled={!own.trim() || !!starting}>
                  <ArrowUp size={15} strokeWidth={2.2} />
                </button>
              </form>
            )}
            {error ? (
              <p className="setup-start-error" role="alert">
                {error}
              </p>
            ) : null}
          </div>
        )}
        {menu ? (
          <PopoverMenu
            anchor={menu.anchor}
            label={menu.kind === "agent" ? "Agents" : "Folders"}
            entries={menu.kind === "agent" ? agentEntries : folderEntries}
            onClose={() => open(menu.kind, null)}
          />
        ) : null}
      </div>
      {copying ? (
        <div className="setup-foot">
          <button type="button" className="chrome-button button-primary setup-button" onClick={onClose} autoFocus>
            Done
          </button>
        </div>
      ) : null}
    </>
  );
}
