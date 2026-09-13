export type Status = "pending" | "decided" | "withdrawn" | "expired";

export type Origin = {
  repo?: string | null;
  workflow?: string | null;
  run_id?: string | null;
  ref?: string | null;
  url?: string | null;
};

export type Decision = {
  decided_by: string;
  decided_at: string;
  data: unknown;
};

export type Summary = {
  counts?: [string, number][];
  subtitle?: string;
};

export type Review = {
  id: string;
  plugin: string;
  plugin_version: number;
  title: string;
  origin: Origin;
  requested_by: string | null;
  created_at: string;
  expires_at: string | null;
  revises: string | null;
  summary: Summary | null;
  status: Status;
  decision: Decision | null;
  agent_note: string | null;
  withdrawn_at: string | null;
  withdrawn_reason: string | null;
  /** present on single-review responses, absent in listings */
  payload?: unknown;
};

export type Plugin = {
  name: string;
  version: number;
  title: string;
  path: string;
  entry: string;
  min_height: number;
  dev: boolean;
  editorial: boolean;
  usable: boolean;
  error: string | null;
};

export type Violation = { path: string; message: string };

export type ReviewEvent = {
  id: number;
  review_id: string | null;
  kind: string;
  actor: string | null;
  at: string;
  attrs: unknown;
};

export type Notice = {
  event_id: number;
  kind: string;
  review_id: string | null;
  review: Review | null;
};

export type Info = {
  version: string;
  data_dir: string;
  port: number;
  pid: number;
  started_at: string;
  user: string;
};
