import { describe, expect, test } from "vitest";
import { connectionNote, connectionParts, where, type AgentStatus, type Integration } from "./AgentsSection";

test("a skill's folder shows with ~ on Windows too", () => {
  expect(where("/Users/you/.claude/skills/pinrail/SKILL.md")).toBe("~/.claude/skills/pinrail");
  expect(where(String.raw`C:\Users\you\.claude\skills\pinrail\SKILL.md`)).toBe(String.raw`~\.claude\skills\pinrail`);
});

const claude = (fields: Partial<AgentStatus> = {}, hooks: Integration["state"] | null = "current"): AgentStatus => ({
  id: "claude-code",
  name: "Claude Code",
  found: true,
  skill: "/Users/you/.claude/skills/pinrail/SKILL.md",
  state: "connected",
  skill_state: "connected",
  integration: hooks ? { kind: "hooks", file: "/Users/you/.claude/settings.json", state: hooks } : null,
  ...fields,
});

describe("an agent's connection", () => {
  test("lists the skill and what reports the agent's status, each with where it is", () => {
    expect(connectionParts(claude())).toEqual([
      { label: "Skill in", path: "~/.claude/skills/pinrail" },
      { label: "Hooks in", path: "~/.claude/settings.json" },
    ]);
    const pi: AgentStatus = {
      ...claude(),
      name: "Pi",
      skill: "/Users/you/.pi/agent/skills/pinrail/SKILL.md",
      integration: { kind: "extension", file: "/Users/you/.pi/agent/extensions/pinrail.ts", state: "current" },
    };
    expect(connectionParts(pi)[1]).toEqual({ label: "Extension in", path: "~/.pi/agent/extensions/pinrail.ts" });
    // an agent whose status Pinrail does not report has its skill alone
    expect(connectionParts(claude({}, null))).toHaveLength(1);
    // the person's own skill is kept beside Pinrail's hooks
    expect(connectionParts(claude({ skill_state: "theirs" }))[0].label).toBe("Your own skill in");
  });

  test("says what Connect adds to an agent not connected yet", () => {
    expect(connectionNote(claude({ state: "absent", skill_state: "absent" }, "absent"))).toBe(
      "Connect adds the pinrail skill and Pinrail's hooks, so Pinrail can show when Claude Code works, waits for you or finishes.",
    );
    expect(connectionNote(claude({ state: "absent", skill_state: "absent" }, null))).toBe(
      "Connect adds the pinrail skill. Pinrail cannot show Claude Code's status yet.",
    );
    expect(connectionNote(claude({ state: "theirs", skill_state: "theirs" }, "absent"))).toBe(
      "Connect adds only Pinrail's hooks, and leaves your skill as it is.",
    );
  });

  test("says which part is older or missing", () => {
    expect(connectionNote(claude({ state: "outdated" }, "absent"))).toBe("Pinrail's hooks are missing.");
    expect(connectionNote(claude({ state: "outdated", skill_state: "outdated" }, "outdated"))).toBe(
      "The skill differs from this version of Pinrail's. Pinrail's hooks were installed by another version of Pinrail.",
    );
    expect(connectionNote(claude({ state: "outdated" }, "modified"))).toBe(
      "One of Pinrail's hooks was changed by hand.",
    );
    expect(connectionNote(claude())).toBeNull();
  });

  test("says when Pinrail cannot add what reports the agent's status", () => {
    expect(connectionNote(claude({}, "unreadable"))).toBe(
      "Pinrail cannot read ~/.claude/settings.json, so it leaves the file as it is and cannot show Claude Code's status.",
    );
    expect(connectionParts(claude({}, "unreadable"))).toHaveLength(1);
  });
});
