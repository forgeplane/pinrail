// The core's agent runtime, as the window calls it: the runs Pinrail
// started, and tickets to attach to their terminals.

import { request, serverUrl } from "./client";

const seg = encodeURIComponent;

/** A run, as the core describes it. */
export type Run = {
  id: string;
  host: string;
  agent: string;
  /** a new session, or one resumed */
  kind?: "start" | "resume";
  state: "starting" | "running" | "ended";
  /** `<host>:<terminal id>`, once the host has spawned the agent */
  terminal: string | null;
  folder: string;
  started_at: string;
  ended_at: string | null;
  end_reason: string | null;
  exit_code: number | null;
  exit_signal: string | null;
  session: { agent: string; id: string } | null;
  /** what the person started it with; only a single run has them */
  task?: string | null;
  args?: string[];
};

/** An agent Pinrail can start, as the host finds it. */
export type StartableAgent = {
  id: string;
  name: string;
  program: string;
  installed: boolean;
  version: string | null;
  last_used: boolean;
};

/** A folder agents worked in lately. */
export type RecentFolder = {
  folder: string;
  repo: string | null;
  branch: string | null;
  exists: boolean;
  used_at: string;
};

export type StartRequest = {
  agent: string;
  folder: string;
  task?: string;
  args?: string[];
  cols: number;
  rows: number;
  local_id: string;
};

export const runtime = {
  startable: async () => (await request<{ agents: StartableAgent[] }>("GET", "/api/v1/agents/startable")).agents,
  recentFolders: async () => (await request<{ folders: RecentFolder[] }>("GET", "/api/v1/folders/recent")).folders,
  start: (body: StartRequest) =>
    request<{ run: Run; session: { agent: string; id: string } | null }>("POST", "/api/v1/runs", {
      host: "local",
      ...body,
    }),
  status: () => request<{ enabled: boolean; state?: string }>("GET", "/api/v1/runtime"),
  run: (id: string) => request<Run>("GET", `/api/v1/runs/${seg(id)}`),
  runs: (filter: { state?: "live" | "ended"; session?: string } = {}) => {
    const q = new URLSearchParams(Object.entries(filter).filter(([, v]) => v) as [string, string][]);
    const s = q.toString();
    return request<{ runs: Run[] }>("GET", `/api/v1/runs${s ? `?${s}` : ""}`);
  },
  /** The last lines a run's terminal showed. */
  output: async (id: string, lines = 20) =>
    (
      await request<{ text: string }>("GET", `/api/v1/runs/${seg(id)}/output?lines=${lines}`, undefined, {
        asPerson: true,
      })
    ).text,
  /** A run's screen as a paint any emulator reads, with its size: for an
   *  ended run, the screen it ended with. */
  screen: (id: string) =>
    request<{ paint: string; cols: number; rows: number }>("GET", `/api/v1/runs/${seg(id)}/screen`, undefined, {
      asPerson: true,
    }),
  /** Resumes a stopped session in a new terminal of `cols` by `rows`. */
  /** With `anyway`, also a session still running in another terminal, for
   *  an agent that allows it. */
  resume: (
    session: { agent: string; id: string },
    body: { cols: number; rows: number; local_id: string; anyway?: boolean },
  ) =>
    request<{ run: Run; session: { agent: string; id: string } | null }>(
      "POST",
      `/api/v1/sessions/${seg(session.agent)}/${seg(session.id)}/resume`,
      { host: "local", ...body },
    ),
  /** The runs a restart of the computer stopped, with their sessions'
   *  titles, until they are resumed or dismissed. */
  restarted: async () =>
    (await request<{ runs: (Run & { title: string | null })[] }>("GET", "/api/v1/runtime/restarted")).runs,
  dismissRestart: () => request<void>("DELETE", "/api/v1/runtime/restarted"),
  stop: (id: string, force = false) => request<void>("POST", `/api/v1/runs/${seg(id)}/stop`, { force }),
  /** A ticket that opens one attach to `terminal` within 30 seconds. */
  ticket: async (terminal: string) =>
    (await request<{ ticket: string }>("POST", `/api/v1/terminals/${seg(terminal)}/tickets`)).ticket,
  /** The WebSocket address of a terminal's attach, without its query. */
  attachUrl: async (terminal: string) =>
    `${(await serverUrl()).replace(/^http/, "ws")}/api/v1/terminals/${seg(terminal)}/attach`,
};
