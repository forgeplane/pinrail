// The sidebar's list of sessions: which state each one is in, and how the
// heading's menu groups, orders and filters them. Pure functions of the
// sessions, the waiting reviews, the choices and the time, so the component
// only draws what they return.

import type { Attention, Review, Session } from "../api/types";
import { age } from "./format";

/** A session without hooks sent a status this recently: it is working. */
export const WORKING_MS = 5 * 60_000;

export type Grouping = "project" | "repository" | "status" | "updated" | "none";
export type Ordering = "status" | "updated";
/** What a session's mark shows: the attention it asks of the person. */
export type State = Attention;
/** The states the Status grouping and filter list; an unknown session is
 *  listed as idle. */
export const LISTED_STATES = ["needs_you", "finished", "working", "idle", "stopped"] as const;
export type ListedState = (typeof LISTED_STATES)[number];
/** The name of each state, as the marks, groups and filters show it. */
export const STATE_LABEL: Record<State, string> = {
  needs_you: "Needs you",
  finished: "Finished",
  working: "Working",
  idle: "Idle",
  stopped: "Stopped",
  unknown: "Status unknown",
};
/** The state a session is listed under. */
export const listedAs = (state: State): ListedState => (state === "unknown" ? "idle" : state);
/** The details a session's second line can show. */
export type Detail = "activity" | "status" | "repo" | "branch" | "agent";
/** The details in the order the line shows them, whatever order they were chosen in. */
export const DETAILS: Detail[] = ["activity", "status", "repo", "branch", "agent"];

export type Choices = {
  grouping: Grouping;
  ordering: Ordering;
  show_details: boolean;
  details: Detail[];
  show_reviews: boolean;
  /** agents to list; none lists every agent */
  agents: string[];
  /** repositories to list, with "-" for none, and projects as
   *  "project:<id>"; none lists every one */
  repos: string[];
  /** states to list; none lists every state */
  states: ListedState[];
  /** list the archived sessions too */
  archived: boolean;
};

export const DEFAULT_CHOICES: Choices = {
  grouping: "repository",
  ordering: "status",
  show_details: false,
  details: ["activity", "status"],
  show_reviews: true,
  agents: [],
  repos: [],
  states: [],
  archived: false,
};

/** How many filters are set, for the menu's Reset. */
/** The choices that clear every filter, and keep the grouping and details. */
export const NO_FILTERS = {
  agents: DEFAULT_CHOICES.agents,
  repos: DEFAULT_CHOICES.repos,
  states: DEFAULT_CHOICES.states,
  archived: DEFAULT_CHOICES.archived,
} satisfies Partial<Choices>;

export const filtersSet = (c: Choices) =>
  (c.agents.length ? 1 : 0) + (c.repos.length ? 1 : 0) + (c.states.length ? 1 : 0) + (c.archived ? 1 : 0);

/**
 * What a session asks of the person: the server's attention when its
 * agent's hooks report on it, and otherwise a guess from its statuses.
 */
export function stateOf(session: Session, now: number): State {
  if (waitingCount(session) > 0) return "needs_you";
  if (session.presence && session.attention) return session.attention;
  const status = session.status;
  if (status?.kind === "done") return "stopped";
  // a note is not the latest status, but it is a report of work
  const at = session.reported_at ?? status?.at;
  if (at && now - Date.parse(at) < WORKING_MS) return "working";
  return "idle";
}

/** Why a session stopped, as its header says it. */
const STOPPED: Record<string, string> = {
  ended: "session ended",
  exited: "process exited",
  cleared: "cleared",
};

/**
 * The chip beside a session's title: its state, and what explains it. A
 * session without hooks shows one only for what its statuses say for sure.
 */
export function chipOf(session: Session, now: number): { state: State; text: string } | null {
  const state = stateOf(session, now);
  const p = session.presence;
  const chip = (text: string) => ({ state, text });
  switch (state) {
    case "needs_you": {
      const waiting = waitingCount(session);
      if (waiting) return chip(`Needs you · ${waiting} waiting`);
      if (p?.wait?.kind === "permission")
        return chip(`Needs you · ${p.wait.tool ? `${p.wait.tool} permission` : "a permission"}`);
      return chip("Needs you · a question");
    }
    case "finished":
      return chip("Finished");
    case "working":
      return chip(p ? `Working · ${age(p.turn_started ?? p.since, now)}` : "Working");
    case "idle":
      return p ? chip("Idle") : null;
    case "stopped": {
      const why = p?.stop_reason ? STOPPED[p.stop_reason] : null;
      return chip(why ? `Stopped · ${why}` : "Stopped");
    }
    case "unknown":
      return p ? chip("Status unknown") : null;
  }
}

/**
 * The order to show while the pointer is over the list: the rows shown
 * keep their places, a row that left goes, and a new row takes the place
 * `next` gives it.
 */
export function steadyOrder(shown: string[], next: string[]): string[] {
  const kept = new Set(next);
  const order = shown.filter((key) => kept.has(key));
  const known = new Set(order);
  next.forEach((key, i) => {
    if (!known.has(key)) order.splice(Math.min(i, order.length), 0, key);
  });
  return order;
}

/** How many questions and reviews wait for the person in a session. */
export const waitingCount = (session: Session) => session.waiting.questions + session.waiting.reviews;

export type Row = {
  session: Session;
  state: State;
  /** what waits in it, from the live list */
  reviews: Review[];
  waiting: number;
};

export type Group = {
  key: string;
  label: string;
  /** a project, a repository, or a status or a time */
  kind: "project" | "repository" | "other";
  /** a project whose sessions and reviews come from more than one repository */
  mixed: boolean;
  /** the project, for a project's group */
  project?: { id: string; name: string; color?: string | null; emoji?: string | null };
  rows: Row[];
  /** waiting reviews that came from no session */
  loose: Review[];
  /** what waits in the group */
  waiting: number;
};

/** The heading, named after the grouping. */
export const HEADINGS: Record<Grouping, string> = {
  project: "Projects",
  repository: "Repositories",
  status: "Status",
  updated: "Updated",
  none: "Sessions",
};

const STATUS_GROUPS = LISTED_STATES.map((key) => ({ key, label: STATE_LABEL[key] }));
const UPDATED_GROUPS = [
  { key: "hour", label: "Last hour" },
  { key: "today", label: "Today" },
  { key: "yesterday", label: "Yesterday" },
  { key: "earlier", label: "Earlier" },
];

const sessionKey = (agent: string, id: string) => `${agent}/${id}`;

function updatedKey(iso: string, now: number): string {
  const at = Date.parse(iso);
  if (now - at < 60 * 60_000) return "hour";
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  if (at >= midnight.getTime()) return "today";
  const yesterday = new Date(midnight);
  yesterday.setDate(yesterday.getDate() - 1);
  return at >= yesterday.getTime() ? "yesterday" : "earlier";
}

/** The Priority order: what needs the person, then finished turns, work, idle and stopped sessions. */
const statusRank = (row: Row) => LISTED_STATES.indexOf(listedAs(row.state));

/**
 * The groups the sidebar lists, in order, each with its sessions in order.
 * A group with nothing in it is left out.
 */
export function arrange(sessions: Session[], pending: Review[], choices: Choices, now: number): Group[] {
  // what waits, by the session that sent it
  const bySession = new Map<string, Review[]>();
  const loose: Review[] = [];
  for (const review of pending) {
    if (review.requested_by && review.session) {
      const key = sessionKey(review.requested_by, review.session);
      bySession.set(key, [...(bySession.get(key) ?? []), review]);
    } else loose.push(review);
  }

  // states saved before they were renamed are ignored
  const states = choices.states.filter((s) => (LISTED_STATES as readonly string[]).includes(s));
  const listed = (repo: string | null | undefined, project?: { id: string } | null) =>
    !choices.repos.length ||
    choices.repos.includes(repo ?? "-") ||
    (!!project && choices.repos.includes(`project:${project.id}`));
  const rows: Row[] = sessions
    .filter((s) => choices.archived || !s.archived_at)
    .filter((s) => !choices.agents.length || choices.agents.includes(s.agent))
    .filter((s) => listed(s.repo, s.project))
    .map((session) => ({
      session,
      state: stateOf(session, now),
      reviews: bySession.get(sessionKey(session.agent, session.id)) ?? [],
      waiting: waitingCount(session),
    }))
    .filter((row) => !states.length || states.includes(listedAs(row.state)));

  const recent = (a: Row, b: Row) => b.session.last_at.localeCompare(a.session.last_at);
  rows.sort(choices.ordering === "status" ? (a, b) => statusRank(a) - statusRank(b) || recent(a, b) : recent);

  // a review from no session follows the agent filter only when it has an agent
  const looseShown = loose
    .filter((r) => listed(r.origin.repo, r.project))
    .filter((r) => !choices.agents.length || (!!r.requested_by && choices.agents.includes(r.requested_by)))
    .filter(() => !states.length || states.includes("needs_you"));

  const groups = new Map<string, Group>();
  /** the repositories seen in each group, to tell a mixed project */
  const seen = new Map<string, Set<string>>();
  const groupFor = (key: string, label: string, repo: string | null | undefined, project?: Group["project"] | null) => {
    let group = groups.get(key);
    if (!group) {
      const kind = key.startsWith("project:")
        ? "project"
        : choices.grouping === "repository" || choices.grouping === "project"
          ? "repository"
          : "other";
      group = { key, label, kind, mixed: false, rows: [], loose: [], waiting: 0 };
      if (kind === "project" && project) group.project = project;
      groups.set(key, group);
      seen.set(key, new Set());
    }
    const repos = seen.get(key)!;
    repos.add(repo ?? "");
    group.mixed = group.kind === "project" && repos.size > 1;
    return group;
  };
  /** a project's group, or the repository's when it is in none */
  const projectKey = (
    project: { id: string; name: string } | null | undefined,
    repo: string | null | undefined,
  ): [string, string] => (project ? [`project:${project.id}`, project.name] : [repo ?? "", repo ?? "No repository"]);
  const keyOf = (row: Row): [string, string] => {
    switch (choices.grouping) {
      case "project":
        return projectKey(row.session.project, row.session.repo);
      case "repository":
        return [row.session.repo ?? "", row.session.repo ?? "No repository"];
      case "status": {
        const key = listedAs(row.state);
        return [key, STATE_LABEL[key]];
      }
      case "updated": {
        const key = updatedKey(row.session.last_at, now);
        return [key, UPDATED_GROUPS.find((g) => g.key === key)!.label];
      }
      case "none":
        return ["", HEADINGS.none];
    }
  };
  const looseKeyOf = (review: Review): [string, string] => {
    switch (choices.grouping) {
      case "project":
        return projectKey(review.project, review.origin.repo);
      case "repository":
        return [review.origin.repo ?? "", review.origin.repo ?? "No repository"];
      case "status":
        return ["needs_you", STATE_LABEL.needs_you];
      case "updated": {
        const key = updatedKey(review.created_at, now);
        return [key, UPDATED_GROUPS.find((g) => g.key === key)!.label];
      }
      case "none":
        return ["", HEADINGS.none];
    }
  };
  // projects and repositories come in the order of their first session,
  // which the ordering put first; status and updated groups in their fixed
  // order
  for (const row of rows) {
    const group = groupFor(...keyOf(row), row.session.repo, row.session.project);
    group.rows.push(row);
    group.waiting += row.waiting;
  }
  for (const review of looseShown) {
    const group = groupFor(...looseKeyOf(review), review.origin.repo, review.project);
    group.loose.push(review);
    group.waiting += 1;
  }
  const fixed = choices.grouping === "status" ? STATUS_GROUPS : choices.grouping === "updated" ? UPDATED_GROUPS : null;
  const all = [...groups.values()];
  if (fixed) all.sort((a, b) => fixed.findIndex((g) => g.key === a.key) - fixed.findIndex((g) => g.key === b.key));
  return all;
}
