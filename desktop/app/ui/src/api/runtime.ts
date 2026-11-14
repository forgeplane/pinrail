// The core's agent runtime, as the window calls it: the runs Pinrail
// started, and tickets to attach to their terminals.

import { request, serverUrl } from "./client";

const seg = encodeURIComponent;

/** A run, as the core describes it. */
export type Run = {
  id: string;
  host: string;
  agent: string;
  state: "starting" | "running" | "ended";
  /** `<host>:<terminal id>`, once the host has spawned the agent */
  terminal: string | null;
  folder: string;
  started_at: string;
  ended_at: string | null;
  end_reason: string | null;
  exit_code: number | null;
  session: { agent: string; id: string } | null;
};

export const runtime = {
  status: () => request<{ enabled: boolean; state?: string }>("GET", "/api/v1/runtime"),
  run: (id: string) => request<Run>("GET", `/api/v1/runs/${seg(id)}`),
  runs: (filter: { state?: "live" | "ended"; session?: string } = {}) => {
    const q = new URLSearchParams(Object.entries(filter).filter(([, v]) => v) as [string, string][]);
    const s = q.toString();
    return request<{ runs: Run[] }>("GET", `/api/v1/runs${s ? `?${s}` : ""}`);
  },
  stop: (id: string, force = false) => request<void>("POST", `/api/v1/runs/${seg(id)}/stop`, { force }),
  /** A ticket that opens one attach to `terminal` within 30 seconds. */
  ticket: async (terminal: string) =>
    (await request<{ ticket: string }>("POST", `/api/v1/terminals/${seg(terminal)}/tickets`)).ticket,
  /** The WebSocket address of a terminal's attach, without its query. */
  attachUrl: async (terminal: string) =>
    `${(await serverUrl()).replace(/^http/, "ws")}/api/v1/terminals/${seg(terminal)}/attach`,
};
