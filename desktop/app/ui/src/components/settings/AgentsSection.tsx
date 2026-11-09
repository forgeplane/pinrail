// Settings › Agents: the global `pinrail` skill for each agent found on this
// computer. The skill says how to submit through
// Pinrail; the prompt or skill that sends an agent to it says when.

import { Check, ChevronRight, Copy, Plug } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { inTauri } from "../../api/client";
import { AgentIcon } from "../AgentIcon";
import { useSettings } from "../../state/settings";
import { Toggle } from "./controls";
import { SettingsRow } from "./layout";
import { useCopy } from "../../lib/useCopy";

export type AgentStatus = {
  id: string;
  name: string;
  /** its configuration folder is there */
  found: boolean;
  /** where the skill goes */
  skill: string;
  state: "absent" | "connected" | "outdated" | "theirs";
  /** the other agents whose skill folders this one also reads */
  also_reads?: string[];
};

const invoke = <T,>(command: string, args?: Record<string, unknown>) =>
  import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command, args));

/** The agents in alphabetical order of their names. */
export const byName = (agents: AgentStatus[]) => [...agents].sort((a, b) => a.name.localeCompare(b.name));

/** Whether an agent has Pinrail's skill, current or not. */
const connected = (a: AgentStatus) => a.state === "connected" || a.state === "outdated";

/** The connected agents among those whose skill folders an agent also reads. */
export const readsSkillOf = (agent: AgentStatus, agents: AgentStatus[]) =>
  (agent.also_reads ?? []).filter((name) => agents.some((a) => a.name === name && connected(a)));

/** "Claude Code's", or "Claude Code's and Cursor's". */
const possessives = (names: string[]) => names.map((n) => `${n}'s`).join(" and ");

/** The skill's folder, with ~ for the home folder. */
export const where = (skill: string) => skill.replace(/^\/(?:Users|home)\/[^/]+/, "~").replace(/\/SKILL\.md$/, "");

/** The agents while `open`, looked at again when the window comes back, and
 *  connecting or disconnecting one. */
export function useAgents(open: boolean) {
  const [agents, setAgents] = useState<AgentStatus[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<{ id: string | null; message: string } | null>(null);

  const check = useCallback(() => {
    invoke<AgentStatus[]>("agents_status")
      .then(setAgents)
      .catch((e) => setError({ id: null, message: String(e) }));
  }, []);

  useEffect(() => {
    if (!open || !inTauri()) return;
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [open, check]);

  const act = useCallback(async (command: "connect_agent" | "disconnect_agent", id: string) => {
    setBusy(id);
    setError(null);
    try {
      setAgents(await invoke<AgentStatus[]>(command, { id }));
    } catch (e) {
      setError({ id, message: String(e) });
    } finally {
      setBusy(null);
    }
  }, []);

  return { agents, busy, error, act };
}

/** The agent's mark, as the inbox shows it beside a review. */
export function Tile({ id }: { id: string }) {
  return (
    <span className="agent-tile" aria-hidden="true">
      <AgentIcon requestedBy={id} size={18} />
    </span>
  );
}

function AgentCard({
  agent,
  usesSkillOf,
  busy,
  error,
  act,
}: {
  agent: AgentStatus;
  /** the connected agents whose skill this one reads */
  usesSkillOf: string[];
  busy: boolean;
  error: string | null;
  act: (command: "connect_agent" | "disconnect_agent", id: string) => void;
}) {
  const skill = where(agent.skill);
  const config = skill.replace(/\/skills\/pinrail$/, "");
  const button = (
    label: string,
    command: "connect_agent" | "disconnect_agent",
    kind: "primary" | "plain" | "quiet",
  ) => (
    <button
      type="button"
      className={kind === "quiet" ? "agent-remove" : `chrome-button ${kind === "primary" ? "button-primary" : ""}`}
      onClick={() => act(command, agent.id)}
      disabled={busy}
    >
      {kind === "primary" ? <Plug size={13} /> : null}
      {busy ? (command === "connect_agent" ? "Connecting…" : "Removing…") : label}
    </button>
  );

  let where_: ReactNode;
  let side: ReactNode = null;
  let note: ReactNode = error;
  let state = agent.found ? agent.state : "missing";
  if (agent.state === "connected") {
    where_ = (
      <>
        Skill added to <span className="mono">{skill}</span>
      </>
    );
    side = (
      <>
        <span className="agent-ok">
          <Check size={14} strokeWidth={2.5} /> Connected
        </span>
        {button("Remove", "disconnect_agent", "quiet")}
      </>
    );
  } else if (agent.state === "outdated") {
    where_ = (
      <>
        The skill in <span className="mono">{skill}</span> differs from this version of Pinrail's
      </>
    );
    side = (
      <>
        {button("Remove", "disconnect_agent", "quiet")}
        {button("Update", "connect_agent", "primary")}
      </>
    );
  } else if (agent.state === "theirs") {
    where_ = (
      <>
        <span className="mono">{skill}</span> is a skill of your own
      </>
    );
    note ??= "Pinrail leaves it alone. Remove or rename it to let Pinrail add its skill.";
  } else if (agent.found) {
    where_ = (
      <>
        Found in <span className="mono">{config}</span>
      </>
    );
    side = button("Connect", "connect_agent", "primary");
  } else {
    where_ = "Not found";
    state = "missing";
  }
  if (agent.found && agent.state === "absent" && usesSkillOf.length)
    note ??= (
      <span className="agent-note-ok">
        <Check size={12} strokeWidth={2.5} /> Already uses {usesSkillOf[0]}'s skill
      </span>
    );
  else if (agent.found && agent.also_reads?.length && agent.state !== "theirs")
    note ??= `Reads ${possessives(agent.also_reads)} skills too`;

  return (
    <div className={`agent-card is-${state}`} data-testid={`agent-${agent.id}`}>
      <Tile id={agent.id} />
      <div className="agent-text">
        <div className="agent-name">{agent.name}</div>
        <div className="agent-where">{where_}</div>
        {note ? <div className="agent-note">{note}</div> : null}
      </div>
      {side ? <div className="agent-side">{side}</div> : null}
    </div>
  );
}

/** For an agent Pinrail does not know: the skill, to give it by hand. */
function AnotherAgent() {
  const { state: copied, copy: copyNow } = useCopy(1600);
  const copy = () => void copyNow(() => invoke<string>("agent_skill"));
  return (
    <div className="agent-card is-other">
      <span className="agent-other-icon">
        <Copy size={15} />
      </span>
      <div className="agent-text">
        <div className="agent-name">Another agent?</div>
        <div className="agent-where">Copy the skill, and add it to the agent's skills or instructions yourself.</div>
      </div>
      <div className="agent-side">
        <button type="button" className="chrome-button" onClick={copy}>
          {copied === "done" ? <Check size={13} /> : <Copy size={13} />} {copied === "done" ? "Copied" : "Copy"}
        </button>
      </div>
    </div>
  );
}

export function AgentsSection({ open }: { open: boolean }) {
  const { agents, busy, error, act } = useAgents(open);
  const { settings, update } = useSettings();
  const [updating, setUpdating] = useState(false);
  const [showMissing, setShowMissing] = useState(false);
  const outdated = (agents ?? []).filter((a) => a.found && a.state === "outdated");
  const updateAll = async () => {
    setUpdating(true);
    for (const agent of outdated) await act("connect_agent", agent.id);
    setUpdating(false);
  };
  // turned on, it also updates the skills that are outdated now
  const setAutoUpdate = (on: boolean) => {
    update({ agents: { auto_update: on } });
    if (on && outdated.length && !updating) updateAll();
  };
  return (
    <>
      <div className="settings-group">
        <div className="settings-group-head">
          <h3>Connect your agents</h3>
        </div>
        <p className="settings-intro">
          Connecting an agent adds a global skill named pinrail. It tells the agent how to submit a review and read your
          decision, whenever a prompt or another skill asks it to use Pinrail.
        </p>
        {inTauri() ? (
          <div className="settings-card agent-auto-update">
            <SettingsRow
              label="Update skills automatically"
              description="When Pinrail starts, updates the skill of each connected agent that an update of Pinrail changed"
            >
              <Toggle
                label="Update agent skills automatically"
                checked={settings.agents.auto_update}
                onChange={setAutoUpdate}
              />
            </SettingsRow>
          </div>
        ) : null}
        {!inTauri() ? (
          <div className="settings-card">
            <SettingsRow label="Agents" description="The desktop app finds your agents and connects them" />
          </div>
        ) : !agents ? (
          <div className="settings-card">
            <SettingsRow label="Agents" description={error?.message ?? "Looking for agents…"} />
          </div>
        ) : (
          <>
            {[
              { title: "Connected", list: agents.filter((a) => a.found && connected(a)) },
              { title: "Not connected", list: agents.filter((a) => a.found && !connected(a)) },
            ].map(({ title, list }) =>
              list.length ? (
                <div key={title} className="agent-group" data-testid={`agent-group-${title}`}>
                  <div className="agent-group-head">
                    <h4 className="agent-group-title">{title}</h4>
                    {/* one agent to update has its own button */}
                    {title === "Connected" && (outdated.length > 1 || updating) ? (
                      <button
                        type="button"
                        className="chrome-button settings-caption-action"
                        onClick={updateAll}
                        disabled={updating || busy !== null}
                      >
                        {updating ? "Updating…" : `Update all ${outdated.length}`}
                      </button>
                    ) : null}
                  </div>
                  <div className="agent-list">
                    {byName(list).map((agent) => (
                      <AgentCard
                        key={agent.id}
                        agent={agent}
                        usesSkillOf={readsSkillOf(agent, agents)}
                        busy={busy === agent.id}
                        error={error && error.id === agent.id ? error.message : null}
                        act={act}
                      />
                    ))}
                  </div>
                </div>
              ) : null,
            )}
            {agents.some((a) => !a.found) ? (
              <div className="agent-group" data-testid="agent-group-Not found">
                <button
                  type="button"
                  className="agent-group-title agent-group-toggle"
                  aria-expanded={showMissing}
                  onClick={() => setShowMissing((v) => !v)}
                >
                  <ChevronRight size={13} className={showMissing ? "is-open" : ""} />
                  Not found on this computer ({agents.filter((a) => !a.found).length})
                </button>
                {showMissing ? (
                  <div className="agent-list">
                    {byName(agents.filter((a) => !a.found)).map((agent) => (
                      <AgentCard
                        key={agent.id}
                        agent={agent}
                        usesSkillOf={readsSkillOf(agent, agents)}
                        busy={busy === agent.id}
                        error={error && error.id === agent.id ? error.message : null}
                        act={act}
                      />
                    ))}
                  </div>
                ) : null}
              </div>
            ) : null}
            <div className="agent-list">
              <AnotherAgent />
            </div>
          </>
        )}
      </div>
    </>
  );
}
