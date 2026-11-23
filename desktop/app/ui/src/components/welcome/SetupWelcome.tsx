// The setup's first screen: the agents found on this computer, Pinrail and
// the person on one rail, and the one button that sets everything up. Each
// agent is a tile that can be left out.

import { ArrowRight, Blocks, Bot, Check, Plus, User } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import { AgentIcon, agentOf } from "../AgentIcon";
import { PopoverMenu, type MenuEntry } from "../PopoverMenu";
import { Tooltip } from "../Tooltip";
import type { AgentStatus } from "../settings/AgentsSection";
import { Mark } from "./Mark";
import { listed } from "./names";

/** An agent the setup can connect: found, and without Pinrail's current skill. */
export const connectable = (a: AgentStatus) => a.found && (a.state === "absent" || a.state === "outdated");

/** How many agents fit as tiles on the left of the mark. With more, the last
 *  place goes to the +N tile, and the others are in its list. */
const ROOM = 6;

/** How many agents the sentence under the title names; more are counted. */
const NAMED = 4;

function AgentTile({ agent, out, onToggle }: { agent: AgentStatus; out: boolean; onToggle: () => void }) {
  const fixed = !connectable(agent);
  const on = agent.state === "connected" || (!fixed && !out);
  const icon = agentOf(agent.id);
  const label =
    agent.state === "connected"
      ? `${agent.name} is connected`
      : agent.state === "theirs"
        ? `${agent.name} has a skill of your own, which Pinrail leaves alone`
        : on
          ? `Leave ${agent.name} out`
          : `Connect ${agent.name}`;
  return (
    <Tooltip label={label} side="top">
      <button
        type="button"
        className="setup-tile"
        aria-label={agent.name}
        aria-pressed={on}
        aria-disabled={fixed || undefined}
        data-testid={`setup-agent-${agent.id}`}
        onClick={fixed ? undefined : onToggle}
      >
        {icon ? (
          <span className="setup-tile-icon" aria-hidden="true" dangerouslySetInnerHTML={{ __html: icon.svg }} />
        ) : (
          <Bot size={22} strokeWidth={1.75} aria-hidden="true" />
        )}
        {on ? (
          <span className="setup-tile-check">
            <Check size={9} strokeWidth={3.4} aria-hidden="true" />
          </span>
        ) : null}
      </button>
    </Tooltip>
  );
}

export function SetupWelcome({
  agents,
  agentsNote,
  leftOut,
  onToggleAgent,
  menu,
  onMenu,
  plugins,
  telemetry,
  primary,
  onSetUp,
  onPlugins,
}: {
  /** the agents found on this computer, or null while they are not known */
  agents: AgentStatus[] | null;
  /** what to say in place of the agents: that they are being looked for, or why they cannot be */
  agentsNote: string | null;
  leftOut: Set<string>;
  onToggleAgent: (id: string) => void;
  /** the button the list of every agent is open under, when it is open */
  menu: HTMLElement | null;
  onMenu: (anchor: HTMLElement | null) => void;
  /** the titles of the plugins the setup installs */
  plugins: string[];
  /** whether usage data or crash reports are sent */
  telemetry: boolean;
  /** the main button, which takes the focus as the setup opens */
  primary: RefObject<HTMLButtonElement | null>;
  onSetUp: () => void;
  onPlugins: () => void;
}) {
  const found = agents ?? [];
  const on = found.filter((a) => a.state === "connected" || (connectable(a) && !leftOut.has(a.id)));
  const out = found.filter((a) => !on.includes(a));
  const connects = on.some(connectable);
  // a few agents are named, and more are counted
  const named = (list: AgentStatus[], strong: boolean, many: string) =>
    list.length > NAMED
      ? `${list.length} ${many}`
      : listed(
          list.map((a) => a.name),
          strong,
        );
  // the agents after the tiles are behind the +N tile, in a list of them all
  const tiles = found.length > ROOM ? ROOM - 1 : found.length;
  const more = found.length - tiles;
  const entries: MenuEntry[] = found.map((a) => ({
    label: a.name,
    mark: <AgentIcon requestedBy={a.id} size={14} />,
    checked: on.includes(a),
    stays: true,
    hint: a.state === "connected" ? "Connected" : a.state === "theirs" ? "Has its own skill" : undefined,
    disabled: connectable(a)
      ? undefined
      : a.state === "connected"
        ? `${a.name} is connected`
        : `${a.name} has a skill of your own, which Pinrail leaves alone`,
    onPick: () => onToggleAgent(a.id),
  }));

  const steps = [
    "installs its command",
    ...(connects ? [on.length === 1 ? "connects this agent" : "connects these agents"] : []),
    ...(plugins.length === 0
      ? []
      : plugins.length > 3
        ? [`adds ${plugins.length} plugins`]
        : [
            <>
              adds the {listed(plugins)} {plugins.length === 1 ? "plugin" : "plugins"}
            </>,
          ]),
  ];

  return (
    <>
      <div className="setup-body">
        <div className="setup-hero">
          <span className="setup-hero-rail" />
          <div className="setup-stop is-agents" role="group" aria-label="Your agents">
            <div className="setup-tiles" data-many={tiles + (more > 0 ? 1 : 0) > 4 || undefined}>
              {found.length ? (
                <>
                  {found.slice(0, tiles).map((a) => (
                    <AgentTile key={a.id} agent={a} out={leftOut.has(a.id)} onToggle={() => onToggleAgent(a.id)} />
                  ))}
                  {more > 0 ? (
                    <Tooltip label={`Show all ${found.length} agents`} side="top">
                      <button
                        type="button"
                        className="setup-tile is-more"
                        aria-label={`${more} more ${more === 1 ? "agent" : "agents"}`}
                        aria-haspopup="menu"
                        onClick={(e) => onMenu(menu ? null : e.currentTarget)}
                      >
                        +{more}
                      </button>
                    </Tooltip>
                  ) : null}
                </>
              ) : (
                <span className="setup-tile is-empty">
                  <Plus size={18} strokeWidth={1.8} aria-hidden="true" />
                </span>
              )}
            </div>
            <small>
              {!found.length
                ? "No agent found"
                : found.some(connectable)
                  ? "Click an agent to leave it out"
                  : "Your agents"}
            </small>
          </div>
          <div className={`setup-stop is-pinrail ${on.length ? "has-pins" : ""}`}>
            <Mark size={84} />
            <small>Pinrail</small>
          </div>
          <div className="setup-stop is-you">
            <span className="setup-tile is-you">
              <User size={22} strokeWidth={1.8} aria-hidden="true" />
            </span>
            <small>You</small>
          </div>
        </div>

        <h1 className="setup-title">Welcome to Pinrail</h1>
        <p className="setup-sub">
          Your agents ask here before they act. You decide, and they carry on with your answer.
        </p>
        <p className="setup-found" data-testid="setup-agents-sentence">
          {agentsNote ? (
            agentsNote
          ) : !found.length ? (
            "Pinrail found no agent on this computer. You can connect one later in Settings."
          ) : !on.length ? (
            "Pinrail connects no agent. You can connect one later in Settings."
          ) : !out.length ? (
            <>Pinrail found {named(on, true, "agents")} on this computer.</>
          ) : (
            <>
              Pinrail connects {named(on, true, "agents")}. It leaves {named(out, false, "agents")} out.
            </>
          )}
        </p>
        {menu ? <PopoverMenu anchor={menu} label="Agents" entries={entries} onClose={() => onMenu(null)} /> : null}
      </div>

      <div className="setup-foot">
        <div className="setup-actions">
          <button ref={primary} type="button" className="chrome-button button-primary setup-button" onClick={onSetUp}>
            Set up Pinrail <ArrowRight size={15} strokeWidth={2.2} />
          </button>
          <button type="button" className="chrome-button setup-button" onClick={onPlugins}>
            <Blocks size={15} /> Add more plugins
          </button>
        </div>
        <p className="setup-fine" data-testid="setup-summary">
          Pinrail {joined(steps)}.
          {telemetry ? (
            <>
              <br />
              It sends anonymous usage data and crash reports. Nothing you review leaves your computer.
            </>
          ) : null}
        </p>
      </div>
    </>
  );
}

/** The parts of a sentence, joined as a list is. */
function joined(parts: ReactNode[]) {
  return parts.map((part, i) => (
    <span key={i}>
      {i === 0 ? "" : i === parts.length - 1 ? " and " : ", "}
      {part}
    </span>
  ));
}
