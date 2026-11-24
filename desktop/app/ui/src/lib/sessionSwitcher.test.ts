import { describe, expect, test } from "vitest";
import type { Attention, Presence, Session } from "../api/types";
import { sections, sessionShown, waitText } from "./sessionSwitcher";

const NOW = new Date(2026, 10, 24, 12, 0, 0).getTime();
const ago = (minutes: number) => new Date(NOW - minutes * 60_000).toISOString();

function session(id: string, last: number, extra: Partial<Session> = {}): Session {
  return {
    agent: "claude-code",
    id,
    agent_session: null,
    repo: "acme/api",
    ref: "main",
    folder: null,
    status: null,
    title: id,
    first_at: ago(600),
    last_at: ago(last),
    total: { questions: 0, reviews: 0 },
    waiting: { questions: 0, reviews: 0 },
    ...extra,
  };
}
const hooked = (attention: Attention, presence: Partial<Presence> = {}): Partial<Session> => ({
  attention,
  presence: { attention, state: "idle", since: ago(1), turn: 1, ...presence } as Presence,
});

describe("sections", () => {
  test("lists what needs the person, then finished, working and the rest, each in its order", () => {
    const listed = sections([
      session("idle-old", 300),
      session("work-old", 9, hooked("working")),
      session("need-new", 1, hooked("needs_you", { state: "needs_input", since: ago(2) })),
      session("fin", 4, hooked("finished", { finished_at: ago(4) })),
      session("idle-new", 30, hooked("idle")),
      session("work-new", 1, hooked("working")),
      session("need-old", 3, hooked("needs_you", { state: "needs_input", since: ago(20) })),
      session("stopped", 60, hooked("stopped")),
      session("gone", 1, { archived_at: ago(1) }),
    ]);
    const ids = (list: Session[]) => list.map((s) => s.id);
    expect(ids(listed.needs_you)).toEqual(["need-old", "need-new"]);
    expect(ids(listed.finished)).toEqual(["fin"]);
    expect(ids(listed.working)).toEqual(["work-new", "work-old"]);
    expect(ids(listed.earlier)).toEqual(["idle-new", "stopped", "idle-old"]);
  });
});

describe("waitText", () => {
  test("says what a session waits for", () => {
    const waits = (questions: number, reviews: number) => session("a", 1, { waiting: { questions, reviews } });
    expect(waitText(waits(2, 0))).toBe("2 questions");
    expect(waitText(waits(1, 1))).toBe("1 question, 1 review");
    const asks = (wait: Presence["wait"]) => session("a", 1, hooked("needs_you", { state: "needs_input", wait }));
    expect(waitText(asks({ kind: "permission", tool: "Bash", summary: "npm test" }))).toBe("Bash · npm test");
    expect(waitText(asks({ kind: "permission", tool: null, summary: null }))).toBe("A permission");
    expect(waitText(asks({ kind: "question", tool: null, summary: null }))).toBe("A question");
    expect(waitText(asks({ kind: "auth", tool: null, summary: null }))).toBe("A sign-in");
  });

  test("says nothing for a session that needs nothing", () => {
    expect(waitText(session("a", 1, hooked("working")))).toBeNull();
  });
});

test("sessionShown reads the session a page shows from its path", () => {
  expect(sessionShown("/sessions/claude-code/ses_1")).toEqual({ agent: "claude-code", id: "ses_1" });
  expect(sessionShown("/sessions/new")).toBeNull();
  expect(sessionShown("/items")).toBeNull();
});
