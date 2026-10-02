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
  /** what was decided, as the plugin declares it summed up */
  summary?: Summary | null;
};

/** The fixed set of colours a plugin's summary may use. */
export type Tone = "danger" | "warning" | "info" | "success" | "neutral";

/** A review summed up as its plugin declares: counts, and for an outcome
 *  the overall verdict. The core derives it; nothing reads the payload. */
export type Summary = {
  counts: { label: string; count: number; tone: Tone }[];
  verdict?: { label: string; tone: Tone };
};

export type Review = {
  id: string;
  /** the plugin's full name, such as forgeplane/list */
  plugin: string;
  /** the exact version it was submitted to, such as 1.2.0 */
  plugin_version: string;
  /** the bundle it was submitted to, which it renders with */
  plugin_bundle: string | null;
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
  /** the files the review carries; on single-review responses only, as the payload */
  attachments?: Attachment[];
  /** how many files it carries and their bytes: on every review, listings too */
  attachments_total?: { count: number; bytes: number };
};

/** What a plugin takes: kinds as `.ext` or media types, and its limits. */
export type AttachmentRules = {
  accept: string[];
  max_size?: number;
  max_count?: number;
};

/** A file sent beside a review's payload, which names it {"$attachment": name}. */
export type Attachment = {
  name: string;
  size: number;
  media_type: string;
  sha256: string;
};

/** A page of reviews: the total that match, the cursor to the next page, and
 *  with `include=facets` what the filter menus can offer. */
export type ReviewListing = {
  reviews: Review[];
  total: number;
  has_more: boolean;
  next_cursor: string | null;
  facets?: { plugins: string[]; repos: string[]; unassigned: boolean };
};

export type Plugin = {
  /** the full name, `<publisher>/<name>` */
  plugin: string;
  publisher: string | null;
  name: string;
  /** the version new reviews use, such as 1.2.0 */
  version: string;
  title: string;
  path: string;
  min_height: number;
  dev: boolean;
  /** the markup of the plugin's icon.svg, when it has one */
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
  /** the files it takes beside a payload, as the manifest declares them; null for none */
  attachments?: AttachmentRules | null;
  /** the names of the sample reviews it ships, which anyone can send to see it, in order */
  samples?: string[];
  /** why each sample that did not load was dropped */
  sample_errors?: string[];
  /** how it got here; null for a line kept after its plugin was removed */
  install: PluginInstall | null;
};

/** Where an installed plugin came from, and the bundle new reviews use. */
export type PluginInstall = {
  kind: "bundled" | "folder" | "link" | "git" | "release";
  source: string;
  /** served live from its folder rather than copied */
  linked: boolean;
  commit: string | null;
  tag: string | null;
  asset_hash: string | null;
  /** the bundle new reviews render with; null for a link */
  bundle: string | null;
  /** the release the last update replaced, while it can be rolled back to */
  previous: { version: string; bundle: string; until: string } | null;
  /** the bundle's files no longer match its listing */
  modified: boolean;
  installed_at: string;
  updated_at: string;
};

/** What the app's frame loads to show a review: its view's address, and
 *  the plugin of the review's line. */
export type ReviewView = { url: string; plugin: Plugin };

/** What installing a source would do, as the core reports it before anything runs. */
export type Inspection = {
  source: string;
  link: boolean;
  /** the full name it would install as */
  plugin: string;
  publisher: string;
  name: string;
  version: string;
  title: string;
  icon: string | null;
  /** the exact command a build runs; null when nothing runs */
  build: string | null;
  /** sent back with the install, which runs a build only as it was shown */
  expect: InstallExpect;
  /** the files it would take beside a payload; null for none */
  attachments?: AttachmentRules | null;
  origin: {
    kind: "folder" | "git" | "release";
    resolved:
      | string
      | {
          url?: string;
          path?: string | null;
          ref?: string | null;
          owner?: string;
          repo?: string;
          tag?: string;
          asset?: string;
          asset_size?: number;
          pinned?: boolean;
        };
    commit: string | null;
  };
  /** what is installed under the full name already; `path` is a link's folder */
  installed: {
    version: string;
    linked: boolean;
    kind: string;
    path: string | null;
    unchanged: boolean;
  } | null;
  /** the source is older than what is installed on the same line */
  older: boolean;
};

export type InstallExpect = { build: string | null; commit?: string; asset_hash?: string };

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

/** The web origins a plugin may open without asking, while it stays
 *  installed from the source it had when they were allowed. */
export type LinkPermission = { source: string; origins: string[] };

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
  /** each plugin's permission to open links without asking */
  links?: Record<string, LinkPermission>;
  /** the loopback server's port; applies at the next start */
  port: number;
  /** how long ended reviews are kept, in days; null keeps them forever */
  history: { keep_days: number | null };
  /** look for a new version at start and every few hours */
  updates?: { check: boolean };
  /** the welcome screen was closed */
  welcome?: { seen: boolean };
  [key: string]: unknown;
};

export type Info = {
  version: string;
  data_dir: string;
  port: number;
  pid: number;
  started_at: string;
  user: string;
  /** the files stored beside reviews */
  attachments?: { count: number; bytes: number };
};
