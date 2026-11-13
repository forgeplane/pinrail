// The icon of the coding agent a review is requested by, for the agents the
// app knows; a bot for any other requester. The CLI names the agent it
// runs under (claude-code, codex, …); a person may give the short name too.
// The names come from the agent registry, through agents.generated.json.
// The marks are inline so the ones drawn in currentColor follow the theme.

import { Bot } from "lucide-react";
import agents from "../agents.generated.json";
import amp from "../assets/agents/amp.svg?raw";
import antigravity from "../assets/agents/antigravity.svg?raw";
import augment from "../assets/agents/augment.svg?raw";
import claude from "../assets/agents/claude.svg?raw";
import cline from "../assets/agents/cline.svg?raw";
import codex from "../assets/agents/codex.svg?raw";
import copilot from "../assets/agents/copilot.svg?raw";
import cursor from "../assets/agents/cursor.svg?raw";
import goose from "../assets/agents/goose.svg?raw";
import grok from "../assets/agents/grok.svg?raw";
import hermes from "../assets/agents/hermes.svg?raw";
import kimi from "../assets/agents/kimi.svg?raw";
import kiro from "../assets/agents/kiro.svg?raw";
import opencode from "../assets/agents/opencode.svg?raw";
import pi from "../assets/agents/pi.svg?raw";

/** Each agent's mark, by its id in the registry. */
const ICONS: Record<string, string> = {
  "claude-code": claude,
  codex,
  cursor,
  antigravity,
  opencode,
  grok,
  pi,
  goose,
  amp,
  copilot,
  kiro,
  augment,
  cline,
  "hermes-agent": hermes,
  kimi,
};

/** The agent a requester names, by its id or another of its names, when the
 *  app knows it. */
export function agentOf(requestedBy?: string | null) {
  const name = requestedBy?.trim().toLowerCase();
  if (!name) return null;
  const agent = agents.find((a) => a.id === name || a.aliases.includes(name));
  const svg = agent && ICONS[agent.id];
  return agent && svg ? { label: agent.label, svg } : null;
}

export function AgentIcon({ requestedBy, size = 13 }: { requestedBy?: string | null; size?: number }) {
  if (!requestedBy) return null;
  const agent = agentOf(requestedBy);
  if (!agent) return <Bot className="agent-icon" size={size} aria-hidden="true" />;
  return (
    <span
      className="agent-icon"
      role="img"
      aria-label={agent.label}
      title={agent.label}
      style={{ width: size, height: size }}
      dangerouslySetInnerHTML={{ __html: agent.svg }}
    />
  );
}
