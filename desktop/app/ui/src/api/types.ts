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
};

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
