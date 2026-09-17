export type Status = "pending" | "decided" | "withdrawn" | "expired" | "discarded";

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
  /** the person's "no, and stop", with who and why */
  discarded_at: string | null;
  discarded_by: string | null;
  discarded_reason: string | null;
  /** present on single-review responses, absent in listings */
  payload?: unknown;
};

/** A numbered page of reviews, with the total and what the filter menus can offer. */
export type ReviewPage = {
  reviews: Review[];
  total: number;
  offset: number;
  limit: number;
  facets: { plugins: string[]; repos: string[]; unassigned: boolean };
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
  /** a lucide icon name, when the manifest sets one */
  icon: string | null;
  usable: boolean;
  error: string | null;
  /** the plugin's own settings, as the manifest declares them; null when it declares none */
  settings_schema: SettingsSchema | null;
  /** why a declared schema was dropped */
  settings_error: string | null;
  /** the values as they stand: defaults under what someone changed */
  settings: Record<string, unknown> | null;
  /** the keys the view answers, as the manifest declares them */
  shortcuts: PluginShortcut[];
  shortcuts_error: string | null;
  /** how it got here; null for a built-in */
  install: PluginInstall | null;
};

/** The record an install left: where the plugin came from and what was placed. */
export type PluginInstall = {
  kind: "path" | "git" | "release";
  source: string;
  version: string;
  /** served live from its folder rather than copied */
  linked: boolean;
  commit: string | null;
  tag: string | null;
  asset_hash: string | null;
  hash: string | null;
  /** the store entry's files no longer match the hash recorded at install */
  modified: boolean;
  installed_at: string;
};

/** What installing a source would do, as the core reports it before anything runs. */
export type Inspection = {
  source: string;
  link: boolean;
  name: string;
  version: string;
  major: number;
  title: string;
  icon: string | null;
  entry: string;
  /** the exact command a build runs; null when nothing runs */
  build: string | null;
  origin: {
    kind: "path" | "git" | "release";
    resolved: string | { url?: string; path?: string | null; ref?: string | null; owner?: string; repo?: string; tag?: string; asset?: string; asset_size?: number; pinned?: boolean };
    commit: string | null;
  };
  /** what is installed under the name already */
  installed: { version: string; major: number; linked: boolean; kind: string; path: string; unchanged: boolean } | null;
  /** the source is older than what is installed on the same line */
  older: boolean;
};

export type InstallJob = {
  id: string;
  source: string;
  status: "fetching" | "inspecting" | "building" | "placing" | "done" | "failed";
  log: string;
  error: string | null;
  plugin: Plugin | null;
};

export type PluginUpdates =
  | { state: "up_to_date"; commit?: string; tag?: string }
  | { state: "available"; version?: string; commit?: string; tag?: string; installed?: string; message?: string }
  | { state: "pinned"; ref?: string; tag?: string }
  | { state: "linked" }
  | { state: "unknown"; message?: string };

export type PluginShortcut = { keys: string; does: string; group?: string };

/** A flat JSON Schema: one row per property, each a scalar with a default. */
export type SettingsSchema = { properties: Record<string, SettingProperty> };
export type SettingProperty = {
  type: "boolean" | "string" | "integer" | "number";
  title?: string;
  description?: string;
  default: unknown;
  enum?: unknown[];
  oneOf?: { const: unknown; title?: string }[];
  minimum?: number;
  maximum?: number;
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
  /** for settings_changed: the settings that changed, as JSON pointers */
  keys?: string[];
};

/** What /api/v1/settings returns; the shell reads the keys it applies. */
export type ServerSettings = {
  appearance: { theme: "system" | "dark" | "light"; text_size: "small" | "default" | "large" };
  sidebar: { open: boolean };
  close_window: "hide" | "quit";
  menu_bar_icon: boolean;
  notifications: { enabled: boolean; paused_until: string | null; sound: boolean; muted_plugins: string[] };
  shortcut: { global: string; global_opens: "oldest" | "inbox" };
  /** each plugin's own settings, only the values someone changed */
  plugins: Record<string, Record<string, unknown>>;
  /** the loopback server's port; applies at the next start */
  port: number;
  /** how long ended reviews are kept, in days; null keeps them forever */
  history: { keep_days: number | null };
  [key: string]: unknown;
};

export type Info = {
  version: string;
  data_dir: string;
  port: number;
  pid: number;
  started_at: string;
  user: string;
};
