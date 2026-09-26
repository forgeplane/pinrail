// The icon of the coding agent a review is requested by, for the agents the
// app knows; a bot for any other requester. The CLI names the agent it
// runs under (claude-code, codex, …); a person may give the short name too.
// The marks are inline so the ones drawn in currentColor follow the theme.

import { Bot } from "lucide-react";
import claude from "../assets/agents/claude.svg?raw";
import codex from "../assets/agents/codex.svg?raw";
import cursor from "../assets/agents/cursor.svg?raw";
import gemini from "../assets/agents/gemini.svg?raw";
import kimi from "../assets/agents/kimi.svg?raw";
import opencode from "../assets/agents/opencode.svg?raw";

const AGENTS: Record<string, { label: string; svg: string }> = {
  "claude-code": { label: "Claude Code", svg: claude },
  claude: { label: "Claude Code", svg: claude },
  codex: { label: "Codex", svg: codex },
  cursor: { label: "Cursor", svg: cursor },
  "gemini-cli": { label: "Gemini CLI", svg: gemini },
  gemini: { label: "Gemini CLI", svg: gemini },
  opencode: { label: "OpenCode", svg: opencode },
  kimi: { label: "Kimi", svg: kimi },
};

/** The agent a requester names, when the app knows it. */
export function agentOf(requestedBy?: string | null) {
  return requestedBy ? AGENTS[requestedBy.trim().toLowerCase()] ?? null : null;
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
