import { describe, expect, test } from "vitest";
import type { Review, Session, SessionStatus } from "../api/types";
import { type Choices, DEFAULT_CHOICES, arrange, filtersSet, stateOf } from "./sidebarSessions";

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

describe("stateOf", () => {
  test("a status in the last five minutes is working, and done wins over it", () => {
    expect(stateOf(session("a", { status: status(4) }), NOW)).toBe("working");
    expect(stateOf(session("a", { status: status(6) }), NOW)).toBe("quiet");
    expect(stateOf(session("a", { status: status(1, "done") }), NOW)).toBe("done");
    expect(stateOf(session("a"), NOW)).toBe("quiet");
  });

  test("a recent note is working too, but a note after the end leaves it done", () => {
    expect(stateOf(session("a", { status: status(30) }, { reported_at: ago(2) }), NOW)).toBe("working");
    expect(stateOf(session("a", { status: status(30, "done") }, { reported_at: ago(2) }), NOW)).toBe("done");
  });
});

describe("arrange", () => {
  const sessions = [
    session("quiet-api", { last: 30 }),
    session("done-web", { repo: "acme/web", last: 2, status: status(2, "done") }),
    session("working-web", { repo: "acme/web", last: 3, status: status(3) }),
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
      ["Waiting for you", ["waiting-api"]],
      ["Working", ["working-web"]],
      ["Quiet", ["quiet-api"]],
      ["Done", ["done-web"]],
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
    expect(listed({ states: ["done", "working"] })).toEqual(["done-web", "working-web"]);
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
