import { describe, expect, test } from "vitest";
import type { Attention, Presence, Session } from "../api/types";
import { agentRows } from "./inboxAgents";

const NOW = new Date(2026, 9, 31, 12, 0, 0).getTime();
const ago = (minutes: number) => new Date(NOW - minutes * 60_000).toISOString();

function session(id: string, attention: Attention, presence: Partial<Presence> | null, waiting = 0): Session {
  return {
    agent: "claude-code",
    id,
    repo: "acme/api",
    ref: "main",
    folder: null,
    status: null,
    title: id,
    first_at: ago(600),
    last_at: ago(1),
    total: { questions: 0, reviews: waiting },
    waiting: { questions: 0, reviews: waiting },
    attention,
    presence: presence
      ? ({ state: "idle", since: ago(1), turn: 1, attention, wait: null, ...presence } as Presence)
      : null,
  };
}

describe("agentRows", () => {
  test("lists an agent waiting in its terminal and an unseen finished turn, each with its reason", () => {
    const rows = agentRows(
      [
        session("perm", "needs_you", {
          state: "needs_input",
          wait: { kind: "permission", tool: "Bash", summary: null },
        }),
        session("ask", "needs_you", { state: "needs_input", wait: { kind: "question", tool: null, summary: null } }),
        session("done", "finished", { finished_at: ago(4) }),
      ],
      NOW,
    );
    expect(rows.map((r) => [r.session.id, r.reason])).toEqual([
      ["perm", "Waiting for permission: Bash"],
      ["ask", "Waiting for your answer"],
      ["done", "Finished 4m ago"],
    ]);
  });

  test("leaves the questions and reviews an agent sent to their own rows", () => {
    expect(agentRows([session("review", "needs_you", { state: "working" }, 1)], NOW)).toEqual([]);
    expect(agentRows([session("old", "needs_you", null, 1)], NOW)).toEqual([]);
    expect(agentRows([session("busy", "working", { state: "working" })], NOW)).toEqual([]);
  });
});
