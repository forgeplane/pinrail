// Settings › Agents: the global `pinrail` skill for each agent found on this
// computer. The skill says how to submit through
// Pinrail; the prompt or skill that sends an agent to it says when.

import { Check, Copy, Plug } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { inTauri } from "../../api/client";
import { AgentIcon } from "../AgentIcon";
import { copyText } from "../../lib/clipboard";
import { SettingsRow } from "./layout";

export type AgentStatus = {
  id: string;
  name: string;
  /** its configuration folder is there */
  found: boolean;
  /** where the skill goes */
  skill: string;
  state: "absent" | "connected" | "outdated" | "theirs" | "covered";
  /** for `covered`: the agent whose skill this one reads */
  covered_by: string | null;
};

const invoke = <T,>(command: string, args?: Record<string, unknown>) =>
  import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command, args));

/** The skill's folder, with ~ for the home folder. */
const where = (skill: string) => skill.replace(/^\/(?:Users|home)\/[^/]+/, "~").replace(/\/SKILL\.md$/, "");

/** The agents while `open`, looked at again when the window comes back, and
 *  connecting or disconnecting one. */
function useAgents(open: boolean) {
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
function Tile({ id }: { id: string }) {
  return (
    <span className="agent-tile" aria-hidden="true">
      <AgentIcon requestedBy={id} size={18} />
    </span>
  );
}

function AgentCard({
  agent,
  busy,
  error,
  act,
}: {
  agent: AgentStatus;
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
      data-agent-action={command === "connect_agent" ? "connect" : "disconnect"}
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
        The skill in <span className="mono">{skill}</span> is from another version of Pinrail
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
  } else if (agent.state === "covered") {
    where_ = <>Reads {agent.covered_by}'s skills too</>;
    side = (
      <span className="agent-ok">
        <Check size={14} strokeWidth={2.5} /> Uses {agent.covered_by}'s skill
      </span>
    );
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

  return (
    <div className={`agent-card is-${state}`} data-agent={agent.id} data-agent-state={state}>
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
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const t = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(t);
  }, [copied]);
  const copy = () =>
    invoke<string>("agent_skill")
      .then(copyText)
      .then(() => setCopied(true))
      .catch(() => {});
  return (
    <div className="agent-card is-other" data-agent-other>
      <span className="agent-other-icon">
        <Copy size={15} />
      </span>
      <div className="agent-text">
        <div className="agent-name">Another agent?</div>
        <div className="agent-where">Copy the skill, and add it to the agent's skills or instructions yourself.</div>
      </div>
      <div className="agent-side">
        <button type="button" className="chrome-button" onClick={copy}>
          {copied ? <Check size={13} /> : <Copy size={13} />} {copied ? "Copied" : "Copy"}
        </button>
      </div>
    </div>
  );
}

export function AgentsSection({ open }: { open: boolean }) {
  const { agents, busy, error, act } = useAgents(open);
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
        {!inTauri() ? (
          <div className="settings-card">
            <SettingsRow label="Agents" description="The desktop app finds your agents and connects them" />
          </div>
        ) : !agents ? (
          <div className="settings-card">
            <SettingsRow label="Agents" description={error?.message ?? "Looking for agents…"} />
          </div>
        ) : (
          <div className="agent-list">
            {agents.map((agent) => (
              <AgentCard
                key={agent.id}
                agent={agent}
                busy={busy === agent.id}
                error={error && error.id === agent.id ? error.message : null}
                act={act}
              />
            ))}
            <AnotherAgent />
          </div>
        )}
      </div>
    </>
  );
}
