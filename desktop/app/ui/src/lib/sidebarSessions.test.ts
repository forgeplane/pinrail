import { describe, expect, test } from "vitest";
import type { Review, Session, SessionStatus } from "../api/types";
import type { Attention, Presence } from "../api/types";
import { type Choices, DEFAULT_CHOICES, arrange, chipOf, filtersSet, stateOf, steadyOrder } from "./sidebarSessions";

// noon, so the "today" and "yesterday" groups do not depend on the time zone's edges
const NOW = new Date(2026, 9, 31, 12, 0, 0).getTime();
const ago = (minutes: number) => new Date(NOW - minutes * 60_000).toISOString();

function session(
  id: string,
  { agent = "claude-code", repo = "acme/api", last = 60, status = null as SessionStatus | null, waiting = 0 } = {},
  extra: Partial<Session> = {},
): Session {
  return {
    agent,
    id,
    agent_session: null,
    repo,
    ref: "main",
    folder: null,
    status,
    title: `Stage ${id}`,
    first_at: ago(600),
    last_at: ago(last),
    total: { questions: 0, reviews: 0 },
    waiting: { questions: 0, reviews: waiting },
    ...extra,
  };
}
const status = (minutes: number, kind: SessionStatus["kind"] = "step"): SessionStatus => ({
  id: minutes,
  text: "Building",
  kind,
  at: ago(minutes),
});
const review = (id: string, from: { agent?: string; session?: string; repo?: string } = {}) =>
  ({
    id,
    title: id,
    requested_by: from.agent ?? null,
    session: from.session ?? null,
    origin: { repo: from.repo ?? "acme/api" },
    created_at: ago(1),
  }) as Review;

const choose = (c: Partial<Choices>): Choices => ({ ...DEFAULT_CHOICES, ...c });
const ids = (groups: ReturnType<typeof arrange>) => groups.map((g) => [g.label, g.rows.map((r) => r.session.id)]);

/** What the server says of a session its hooks report on. */
const presence = (attention: Attention): Pick<Session, "presence" | "attention"> => ({
  attention,
  presence: { attention, state: "idle", since: ago(1), turn: 1 } as Presence,
});

describe("stateOf", () => {
  test("without hooks or terminal records, a session is idle, whatever its statuses say", () => {
    expect(stateOf(session("a", { status: status(1) }))).toBe("idle");
    expect(stateOf(session("a", { status: status(1, "done") }))).toBe("idle");
    expect(stateOf(session("a", { status: status(30) }))).toBe("idle");
    expect(stateOf(session("a"))).toBe("idle");
  });

  test("a question or review waiting on the person needs them, with hooks or without", () => {
    expect(stateOf(session("a", { status: status(4), waiting: 1 }))).toBe("needs_you");
    expect(stateOf(session("a", { waiting: 1 }, presence("needs_you")))).toBe("needs_you");
  });

  test("with hooks, the server's attention is the state, whatever the statuses say", () => {
    for (const attention of ["finished", "working", "idle", "stopped", "unknown"] as const)
      expect(stateOf(session("a", { status: status(1, "done") }, presence(attention)))).toBe(attention);
  });
});

describe("steadyOrder", () => {
  test("keeps the rows shown in their place, and puts a new one where it now belongs", () => {
    expect(steadyOrder(["a", "b", "c"], ["c", "a", "b"])).toEqual(["a", "b", "c"]);
    expect(steadyOrder(["a", "b", "c"], ["c", "d", "a"])).toEqual(["a", "d", "c"]);
    expect(steadyOrder([], ["b", "a"])).toEqual(["b", "a"]);
  });
});

describe("arrange", () => {
  const sessions = [
    session("quiet-api", { last: 30 }),
    session("done-web", { repo: "acme/web", last: 2, status: status(2, "done") }, presence("stopped")),
    session("working-web", { repo: "acme/web", last: 3, status: status(3) }, presence("working")),
    session("waiting-api", { last: 120, waiting: 2 }),
  ];

  test("groups by repository, each ordered by status, repositories by their first session", () => {
    expect(ids(arrange(sessions, [], DEFAULT_CHOICES, NOW))).toEqual([
      ["acme/api", ["waiting-api", "quiet-api"]],
      ["acme/web", ["working-web", "done-web"]],
    ]);
  });

  test("orders by the latest activity when asked", () => {
    expect(ids(arrange(sessions, [], choose({ ordering: "updated" }), NOW))).toEqual([
      ["acme/web", ["done-web", "working-web"]],
      ["acme/api", ["quiet-api", "waiting-api"]],
    ]);
  });

  test("groups by status in a fixed order, what waits first", () => {
    expect(ids(arrange(sessions, [], choose({ grouping: "status" }), NOW))).toEqual([
      ["Needs you", ["waiting-api"]],
      ["Working", ["working-web"]],
      ["Idle", ["quiet-api"]],
      ["Stopped", ["done-web"]],
    ]);
  });

  test("puts what needs the person first, then finished turns, then work, and lists unknown with idle", () => {
    const reported = [
      session("stopped", { last: 1 }, presence("stopped")),
      session("unknown", { last: 2 }, presence("unknown")),
      session("idle", { last: 3 }, presence("idle")),
      session("working", { last: 4 }, presence("working")),
      session("finished", { last: 5 }, presence("finished")),
      session("needs-you", { last: 6, waiting: 1 }, presence("needs_you")),
    ];
    expect(ids(arrange(reported, [], choose({ grouping: "none" }), NOW))).toEqual([
      ["Sessions", ["needs-you", "finished", "working", "unknown", "idle", "stopped"]],
    ]);
    expect(ids(arrange(reported, [], choose({ grouping: "status" }), NOW))).toEqual([
      ["Needs you", ["needs-you"]],
      ["Finished", ["finished"]],
      ["Working", ["working"]],
      ["Idle", ["unknown", "idle"]],
      ["Stopped", ["stopped"]],
    ]);
  });

  test("groups by when a session was last updated", () => {
    const old = [
      session("hour", { last: 20 }),
      session("today", { last: 3 * 60 }),
      session("yesterday", { last: 20 * 60 }),
      session("earlier", { last: 3 * 24 * 60 }),
    ];
    expect(ids(arrange(old, [], choose({ grouping: "updated", ordering: "updated" }), NOW))).toEqual([
      ["Last hour", ["hour"]],
      ["Today", ["today"]],
      ["Yesterday", ["yesterday"]],
      ["Earlier", ["earlier"]],
    ]);
  });

  test("groups by project, a repository in none being a group of its own", () => {
    const pinrail = { id: "p_1", name: "Pinrail" };
    const grouped = [
      session("app", { repo: "forgeplane/pinrail", last: 5 }, { project: pinrail }),
      session("infra", { repo: "forgeplane/pinrail-infra", last: 10 }, { project: pinrail }),
      session("api", { repo: "acme/api", last: 20 }),
      session("loose", { repo: null as unknown as string, last: 30 }),
    ];
    const pending = [{ ...review("ci", { repo: "forgeplane/pinrail-infra" }), project: pinrail }];
    const groups = arrange(grouped, pending, choose({ grouping: "project", ordering: "updated" }), NOW);
    expect(ids(groups)).toEqual([
      ["Pinrail", ["app", "infra"]],
      ["acme/api", ["api"]],
      ["No repository", ["loose"]],
    ]);
    expect(groups.map((g) => [g.kind, g.mixed])).toEqual([
      ["project", true],
      ["repository", false],
      ["repository", false],
    ]);
    expect(groups[0].loose.map((r) => r.id)).toEqual(["ci"]);
    expect(groups[0].waiting).toBe(1);
    // a project with one repository needs no repository on each row
    const single = arrange(grouped.slice(0, 1), [], choose({ grouping: "project" }), NOW);
    expect(single[0].mixed).toBe(false);
  });

  test("filters by a project as by its repositories", () => {
    const pinrail = { id: "p_1", name: "Pinrail" };
    const listed = [
      session("app", { repo: "forgeplane/pinrail" }, { project: pinrail }),
      session("api", { repo: "acme/api" }),
    ];
    expect(ids(arrange(listed, [], choose({ repos: ["project:p_1"] }), NOW))).toEqual([
      ["forgeplane/pinrail", ["app"]],
    ]);
  });

  test("lists everything in one group without a grouping", () => {
    expect(ids(arrange(sessions, [], choose({ grouping: "none" }), NOW))).toEqual([
      ["Sessions", ["waiting-api", "working-web", "quiet-api", "done-web"]],
    ]);
  });

  test("puts what waits under the session that sent it, and counts it in the group", () => {
    const groups = arrange(
      sessions,
      [
        review("r1", { agent: "claude-code", session: "waiting-api" }),
        review("r2", { agent: "codex", session: "waiting-api" }),
      ],
      DEFAULT_CHOICES,
      NOW,
    );
    expect(groups[0].rows[0].reviews.map((r) => r.id)).toEqual(["r1"]);
    // the session's own count, which the live list may not hold in full
    expect(groups[0].waiting).toBe(2);
  });

  test("lists a waiting review from no session in its repository's group", () => {
    const groups = arrange(sessions, [review("ci", { repo: "acme/infra" })], DEFAULT_CHOICES, NOW);
    expect(groups.map((g) => [g.label, g.loose.map((r) => r.id), g.waiting])).toEqual([
      ["acme/api", [], 2],
      ["acme/web", [], 0],
      ["acme/infra", ["ci"], 1],
    ]);
    // by status, it waits like the rest
    const byStatus = arrange(sessions, [review("ci")], choose({ grouping: "status" }), NOW);
    expect(byStatus[0].loose.map((r) => r.id)).toEqual(["ci"]);
  });

  test("filters by agent, repository and state, and leaves out the archived sessions", () => {
    const more = [
      ...sessions,
      session("codex-api", { agent: "codex" }),
      session("archived", {}, { archived_at: ago(5) }),
      session("no-project", { repo: null as unknown as string }),
    ];
    const listed = (c: Partial<Choices>) =>
      arrange(more, [], choose({ grouping: "none", ordering: "updated", ...c }), NOW)[0]?.rows.map(
        (r) => r.session.id,
      ) ?? [];
    expect(listed({ agents: ["codex"] })).toEqual(["codex-api"]);
    expect(listed({ repos: ["acme/web"] })).toEqual(["done-web", "working-web"]);
    expect(listed({ repos: ["-"] })).toEqual(["no-project"]);
    expect(listed({ states: ["stopped", "working"] })).toEqual(["done-web", "working-web"]);
    expect(listed({})).not.toContain("archived");
    expect(listed({ archived: true })).toContain("archived");
  });

  test("leaves out an empty group", () => {
    expect(arrange([], [], DEFAULT_CHOICES, NOW)).toEqual([]);
  });
});

test("filtersSet counts each filter once", () => {
  expect(filtersSet(DEFAULT_CHOICES)).toBe(0);
  expect(filtersSet(choose({ agents: ["a", "b"], archived: true }))).toBe(2);
});

describe("chipOf", () => {
  const reported = (attention: Attention, more: Partial<Presence> = {}) => {
    const s = session("a", {}, presence(attention));
    return { ...s, presence: { ...s.presence!, ...more } };
  };

  test("says what needs the person: what waits, or the agent's own prompt", () => {
    expect(chipOf(session("a", { waiting: 2 }), NOW)?.text).toBe("Needs you · 2 waiting");
    const permission = reported("needs_you", { wait: { kind: "permission", tool: "Bash", summary: null } });
    expect(chipOf(permission, NOW)?.text).toBe("Needs you · Bash permission");
    const question = reported("needs_you", { wait: { kind: "question", tool: null, summary: null } });
    expect(chipOf(question, NOW)?.text).toBe("Needs you · a question");
  });

  test("says how long a turn has worked, and why a session stopped", () => {
    expect(chipOf(reported("working", { turn_started: ago(4) }), NOW)?.text).toBe("Working · 4m");
    expect(chipOf(reported("finished"), NOW)?.text).toBe("Finished");
    expect(chipOf(reported("idle"), NOW)?.text).toBe("Idle");
    expect(chipOf(reported("stopped", { stop_reason: "exited" }), NOW)?.text).toBe("Stopped · process exited");
    expect(chipOf(reported("stopped", { stop_reason: "ended" }), NOW)?.text).toBe("Stopped · session ended");
    expect(chipOf(reported("unknown"), NOW)?.text).toBe("Status unknown");
  });

  test("without hooks or terminal records, shows none, whatever the statuses say", () => {
    expect(chipOf(session("a", { status: status(1) }), NOW)).toBeNull();
    expect(chipOf(session("a", { status: status(1, "done") }), NOW)).toBeNull();
  });
});
