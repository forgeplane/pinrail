// Settings › Agents: the global `pinrail` skill for each agent found on this
// computer. The skill says how to submit through
// Pinrail; the prompt or skill that sends an agent to it says when.

import { Check, ChevronDown, ChevronRight, ChevronUp, Copy, Plug, TriangleAlert } from "lucide-react";
import { Fragment, useCallback, useEffect, useState, type ReactNode } from "react";
import { ApiError, api, inTauri } from "../../api/client";
import { AgentIcon } from "../AgentIcon";
import { useSettings } from "../../state/settings";
import { Toggle } from "./controls";
import { SettingsRow } from "./layout";
import { useCopy } from "../../lib/useCopy";
import { short } from "../welcome/names";

export type AgentStatus = {
  id: string;
  name: string;
  /** its configuration folder is there */
  found: boolean;
  /** where the skill goes */
  skill: string;
  /** the connection as a whole: the skill, and the hooks, extension or
   *  plugin for an agent whose status Pinrail reports */
  state: "absent" | "connected" | "outdated" | "theirs";
  /** the skill alone, `theirs` when the person's own is kept */
  skill_state?: "absent" | "connected" | "outdated" | "theirs";
  /** the other agents whose skill folders this one also reads */
  also_reads?: string[];
  /** What connecting installs for the agent's status, for an agent Pinrail
   *  can report the status of */
  integration?: Integration | null;
  /** Pinrail can start the agent, so the person's arguments for it apply */
  starts?: boolean;
  /** the flag of the person's arguments that turns the agent's permission
   *  checks off, if one does */
  skips_checks?: string | null;
  /** an example of arguments for the agent, from its `--help` */
  args_example?: string | null;
};

/** What connecting installs for an agent's status: hooks, an extension or a
 *  plugin, the file it goes in, and what the file holds of it. */
export type Integration = { kind: "hooks" | "extension" | "plugin"; file: string; state: HookState };

export type HookState = "absent" | "current" | "outdated" | "modified" | "unreadable";

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

/** The skill's folder, with ~ for the home folder, on Windows too. */
export const where = (skill: string) => short(skill).replace(/[\\/]SKILL\.md$/, "");

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

  const act = useCallback(
    async (command: "connect_agent" | "disconnect_agent", id: string) => {
      setBusy(id);
      setError(null);
      try {
        setAgents(await invoke<AgentStatus[]>(command, { id }));
      } catch (e) {
        setError({ id, message: String(e) });
        // part of it may be done even so, such as hooks that could not be
        // trusted
        check();
      } finally {
        setBusy(null);
      }
    },
    [check],
  );

  return { agents, busy, error, act, check };
}

/** What an agent's hooks, extension or plugin is called in a sentence. */
const NAMES: Record<Integration["kind"], string> = { hooks: "hooks", extension: "extension", plugin: "plugin" };

/** What a connection of the agent includes, each with where it is: the
 *  skill, and what reports the agent's status, when Pinrail can add it. */
export function connectionParts(agent: AgentStatus): { label: string; path: string }[] {
  const parts = [
    { label: agent.skill_state === "theirs" ? "Your own skill in" : "Skill in", path: where(agent.skill) },
  ];
  const integration = agent.integration;
  if (integration && integration.state !== "unreadable") {
    const name = NAMES[integration.kind];
    parts.push({ label: `${name[0].toUpperCase()}${name.slice(1)} in`, path: where(integration.file) });
  }
  return parts;
}

/** What the card says about the agent's connection beyond its parts: what
 *  is older or missing, what is the person's own, or what Connect adds. */
export function connectionNote(agent: AgentStatus): string | null {
  const integration = agent.integration;
  const name = integration ? NAMES[integration.kind] : null;
  if (integration?.state === "unreadable") {
    const file = where(integration.file);
    return integration.kind === "hooks"
      ? `Pinrail cannot read ${file}, so it leaves the file as it is and cannot show ${agent.name}'s status.`
      : `${file} is not Pinrail's ${name}, so Pinrail leaves it as it is and cannot show ${agent.name}'s status.`;
  }
  if (agent.state === "absent") {
    return integration
      ? `Connect adds the pinrail skill and Pinrail's ${name}, so Pinrail can show when ${agent.name} works, waits for you or finishes.`
      : `Connect adds the pinrail skill. Pinrail cannot show ${agent.name}'s status yet.`;
  }
  if (agent.state === "theirs") {
    return integration
      ? `Connect adds only Pinrail's ${name}, and leaves your skill as it is.`
      : "Pinrail leaves it alone. Remove or rename it to let Pinrail add its skill.";
  }
  if (agent.state === "outdated") {
    const parts: string[] = [];
    if (agent.skill_state === "outdated") parts.push("The skill differs from this version of Pinrail's.");
    if (agent.skill_state === "absent") parts.push("The skill is missing.");
    if (integration?.state === "absent") parts.push(`Pinrail's ${name} ${name === "hooks" ? "are" : "is"} missing.`);
    if (integration?.state === "outdated")
      parts.push(`Pinrail's ${name} ${name === "hooks" ? "were" : "was"} installed by another version of Pinrail.`);
    if (integration?.state === "modified") parts.push("One of Pinrail's hooks was changed by hand.");
    return parts.join(" ") || null;
  }
  return null;
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
  onArgs,
}: {
  agent: AgentStatus;
  /** the connected agents whose skill this one reads */
  usesSkillOf: string[];
  busy: boolean;
  error: string | null;
  act: (command: "connect_agent" | "disconnect_agent", id: string) => void;
  /** looks at the agents again, after their arguments changed */
  onArgs: () => void;
}) {
  const [open, setOpen] = useState(false);
  const config = where(agent.skill).replace(/\/skills\/pinrail$/, "");
  const button = (label: string, command: "connect_agent" | "disconnect_agent", primary = false) => (
    <button
      type="button"
      className={`chrome-button ${primary ? "button-primary" : ""}`}
      onClick={() => act(command, agent.id)}
      disabled={busy}
    >
      {primary ? <Plug size={13} /> : null}
      {busy ? (command === "connect_agent" ? "Connecting…" : "Removing…") : label}
    </button>
  );
  const parts = (
    <>
      {connectionParts(agent).map((part, i) => (
        <Fragment key={part.label}>
          {i ? " · " : null}
          {part.label} <span className="mono">{part.path}</span>
        </Fragment>
      ))}
    </>
  );
  const linked = agent.found && (agent.state === "connected" || agent.state === "outdated");
  // a connected agent that Pinrail starts has settings of its own
  const settings = linked && !!agent.starts;

  let where_: ReactNode;
  let side: ReactNode = null;
  let note: ReactNode = error ?? connectionNote(agent);
  let state = agent.found ? agent.state : "missing";
  if (linked) {
    where_ = parts;
    side = (
      <>
        {settings ? (
          <button
            type="button"
            className={`chrome-button ${open ? "is-active" : ""}`}
            aria-expanded={open}
            aria-label={`${agent.name}'s settings`}
            onClick={() => setOpen((o) => !o)}
          >
            More {open ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
          </button>
        ) : null}
        {button("Remove", "disconnect_agent")}
        {agent.state === "outdated" ? button("Update", "connect_agent", true) : null}
      </>
    );
  } else if (agent.state === "theirs") {
    where_ = (
      <>
        <span className="mono">{where(agent.skill)}</span> is a skill of your own
      </>
    );
    if (agent.integration && agent.integration.state !== "unreadable") side = button("Connect", "connect_agent", true);
  } else if (agent.found) {
    where_ = (
      <>
        Found in <span className="mono">{config}</span>
      </>
    );
    side = button("Connect", "connect_agent", true);
  } else {
    where_ = "Not found";
    note = null;
    state = "missing";
  }
  const reads =
    agent.found && agent.state === "absent" && usesSkillOf.length ? (
      <span className="agent-note-ok">
        <Check size={12} strokeWidth={2.5} /> Already uses {usesSkillOf[0]}'s skill
      </span>
    ) : agent.found && agent.also_reads?.length && agent.state !== "theirs" ? (
      `Reads ${possessives(agent.also_reads)} skills too`
    ) : null;

  return (
    <div className={`agent-card is-${state}`} data-testid={`agent-${agent.id}`}>
      <Tile id={agent.id} />
      <div className="agent-text">
        <div className="agent-name">
          {agent.name}
          {linked ? (
            <span className="agent-badge">
              <Check size={11} strokeWidth={2.5} aria-hidden="true" /> Connected
            </span>
          ) : null}
          {settings && agent.skips_checks ? (
            <span className="agent-flag" title={`${agent.skips_checks} turns ${agent.name}'s permission checks off`}>
              <TriangleAlert size={11} aria-hidden="true" /> Permission checks off
            </span>
          ) : null}
        </div>
        <div className="agent-where">{where_}</div>
        {note ? <div className="agent-note">{note}</div> : null}
        {reads ? <div className="agent-note">{reads}</div> : null}
      </div>
      {side ? <div className="agent-side">{side}</div> : null}
      {settings && open ? (
        <div className="agent-settings">
          <AgentArgs agent={agent} onSaved={onArgs} />
        </div>
      ) : null}
    </div>
  );
}

/** The arguments the person gives an agent for every start and resume, a
 *  line saved when they press Enter or leave the field; Esc puts back the
 *  saved line. The core says why it refuses one. */
function AgentArgs({ agent, onSaved }: { agent: AgentStatus; onSaved: () => void }) {
  const { settings } = useSettings();
  const saved = settings.agents.args[agent.id] ?? "";
  const [line, setLine] = useState(saved);
  const [refused, setRefused] = useState<string | null>(null);
  // a line saved elsewhere, or by this field, replaces what it shows
  const [shown, setShown] = useState(saved);
  if (shown !== saved) {
    setShown(saved);
    setLine(saved);
  }
  const save = async () => {
    const next = line.trim();
    if (next === saved) return setRefused(null);
    try {
      await api.patchSettings({ agents: { args: { [agent.id]: next || null } } });
      setRefused(null);
      onSaved();
    } catch (e) {
      setRefused(e instanceof ApiError ? (e.violations[0]?.message ?? e.message) : String(e));
    }
  };
  return (
    <div className="agent-setting">
      <div className="agent-setting-text">
        <div className="agent-setting-label">Arguments</div>
        <div className="agent-setting-desc">
          Pinrail adds these arguments to every start and resume of {agent.name}. Write them as on a shell line; quotes
          work.
        </div>
      </div>
      <div className="agent-setting-control">
        <input
          className={`settings-input mono ${refused ? "is-refused" : ""}`}
          value={line}
          placeholder={agent.args_example ? `Such as ${agent.args_example}` : undefined}
          aria-label={`${agent.name}'s arguments`}
          spellCheck={false}
          onChange={(e) => setLine(e.target.value)}
          onBlur={() => void save()}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") {
              e.stopPropagation();
              setLine(saved);
              setRefused(null);
            }
          }}
        />
        {refused ? (
          <div className="agent-setting-note is-refused" role="alert">
            <TriangleAlert size={12} aria-hidden="true" /> Pinrail did not save this line. It {refused}.
          </div>
        ) : agent.skips_checks && line.trim() === saved ? (
          <div className="agent-setting-note is-warning">
            <TriangleAlert size={12} aria-hidden="true" /> With {agent.skips_checks}, {agent.name} does not ask before
            it runs commands or changes files.
          </div>
        ) : null}
      </div>
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
  const { agents, busy, error, act, check } = useAgents(open);
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
          Connecting an agent adds a global skill named pinrail, which tells the agent how to submit a review and read
          your decision. For the agents that support it, Pinrail also adds hooks, so that it can show when the agent
          works, waits for you or finishes.
        </p>
        {inTauri() ? (
          <div className="settings-card agent-auto-update">
            <SettingsRow
              label="Update agents automatically"
              description="When Pinrail starts, updates the skill and hooks of each connected agent that an update of Pinrail changed"
            >
              <Toggle
                label="Update connected agents automatically"
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
                        onArgs={check}
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
                        onArgs={check}
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
