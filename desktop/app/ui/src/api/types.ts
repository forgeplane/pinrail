export type Status = "pending" | "decided" | "withdrawn" | "expired" | "discarded";

export type Origin = {
  repo?: string | null;
  workflow?: string | null;
  run_id?: string | null;
  ref?: string | null;
  url?: string | null;
  /** the folder the agent worked in, as the CLI found it */
  folder?: string | null;
};

/** Where a decision or a discard was made: in the app, with the pinrail
 *  CLI, or by another client of the API. Null when it was recorded before
 *  the app kept this. */
export type Via = "app" | "cli" | "api";

export type Decision = {
  /** the person, for a decision made in the app; otherwise whoever the client named, or nobody */
  decided_by: string | null;
  decided_at: string;
  data: unknown;
  /** what was decided, as the plugin declares it summed up */
  summary?: Summary | null;
  via?: Via | null;
  /** the agent's session that decided with the CLI, if any */
  session?: string | null;
};

/** The fixed set of colours a plugin's summary may use. */
export type Tone = "danger" | "warning" | "info" | "success" | "neutral";

/** A review summed up as its plugin declares: counts, and for an outcome
 *  the overall verdict. The core derives it; nothing reads the payload. */
export type Summary = {
  counts: { label: string; count: number; tone: Tone }[];
  verdict?: { label: string; tone: Tone };
};

/** A plugin's review, or questions an agent asked with `pinrail ask`. */
export type Kind = "review" | "question";

export type Review = {
  id: string;
  kind: Kind;
  /** the plugin's name, such as list; empty for questions */
  plugin: string;
  /** the exact version it was submitted to, such as 1.2.0 */
  plugin_version: string;
  /** the bundle it was submitted to, which it renders with */
  plugin_bundle: string | null;
  title: string;
  origin: Origin;
  requested_by: string | null;
  /** the agent's session the review was submitted from, if the CLI knew it */
  session?: string | null;
  created_at: string;
  expires_at: string | null;
  revises: string | null;
  summary: Summary | null;
  status: Status;
  decision: Decision | null;
  agent_note: string | null;
  withdrawn_at: string | null;
  withdrawn_reason: string | null;
  /** who withdrew it, the client used, and the agent's session */
  withdrawn_by?: string | null;
  withdrawn_via?: Via | null;
  withdrawn_session?: string | null;
  /** the person's "no, and stop", with who and why */
  discarded_at: string | null;
  discarded_by: string | null;
  discarded_reason: string | null;
  discarded_via?: Via | null;
  discarded_session?: string | null;
  /** present on single-review responses, absent in listings */
  payload?: unknown;
  /** the files the review carries; on single-review responses only, as the payload */
  attachments?: Attachment[];
  /** how many files it carries and their bytes: on every review, listings too */
  attachments_total?: { count: number; bytes: number };
  /** when the person starred this round, if they did */
  starred_at?: string | null;
  /** whether any round of the review is starred */
  starred?: boolean;
  /** the project its repository is in */
  project?: ProjectRef | null;
};

/** A project as a review or a session names it. */
export type ProjectRef = { id: string; name: string };

/** A repository the app knows, with what it holds. */
export type ProjectRepo = {
  repo: string;
  /** its sessions that are not archived */
  sessions: number;
  /** its questions and reviews that wait for the person */
  waiting: number;
  /** its latest session activity or review */
  last_at: string | null;
};

/** A named set of repositories the inbox and the sidebar can group by. */
export type Project = {
  id: string;
  name: string;
  /** a line on what the project is, if the person wrote one */
  description: string | null;
  created_at: string;
  repos: ProjectRepo[];
  sessions: number;
  waiting: number;
  last_at: string | null;
};

/** The projects by name, and the repositories in none. */
export type ProjectsListing = { projects: Project[]; loose: ProjectRepo[] };

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
/** One question of those an agent asked, as its payload holds it. */
export type Question = {
  id: string;
  question: string;
  answer: "text" | "choice" | "choices" | "yes-no";
  choices?: string[];
  recommended?: string;
  context?: string;
};

/** The answer to one question, as the decision holds it: one of text,
 *  choice, choices, yes or unanswered, and a note. */
export type Answer = {
  id: string;
  text?: string;
  choice?: string;
  choices?: string[];
  yes?: boolean;
  unanswered?: true;
  note?: string;
};

/** A stage an agent started, as the stage search finds it. */
export type Stage = { id: number; agent: string; session: string; text: string; at: string; repo: string | null };

/** An agent's session, as the reviews and questions it sent make it. */
export type Session = {
  agent: string;
  id: string;
  repo: string | null;
  ref: string | null;
  /** the folder the agent works in, as the CLI found it */
  folder: string | null;
  /** what the agent last said it is doing */
  status: SessionStatus | null;
  /** the stage the agent last said it started, which names the session */
  title: string | null;
  /** when the person archived the session, if they did */
  archived_at?: string | null;
  /** when the agent last reported anything, a note included */
  reported_at?: string | null;
  /** the project its repository is in */
  project?: ProjectRef | null;
  first_at: string;
  last_at: string;
  total: { questions: number; reviews: number };
  waiting: { questions: number; reviews: number };
};

export type ReviewListing = {
  reviews: Review[];
  total: number;
  has_more: boolean;
  next_cursor: string | null;
  facets?: { plugins: string[]; repos: string[]; unassigned: boolean };
};

export type Plugin = {
  /** the name agents, settings and reviews know it by */
  name: string;
  /** the version new reviews use, such as 1.2.0 */
  version: string;
  title: string;
  path: string;
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
  /** what the view may ask the app to do beyond the protocol, as far as this
   *  Pinrail grants it, such as "clipboard"; the app offers each only to a
   *  plugin that declares it */
  capabilities?: string[];
  capabilities_error?: string | null;
  /** the files it takes beside a payload, as the manifest declares them; null for none */
  attachments?: AttachmentRules | null;
  /** the names of the sample reviews it ships, which anyone can send to see it, in order */
  samples?: string[];
  /** why each sample that did not load was dropped */
  sample_errors?: string[];
  /** how the payload schema's file fields disagree with the manifest's attachments */
  attachment_errors?: string[];
  /** what it tells the agents that ask with it, from its `skill/` folder */
  skill?: { name?: string | null; description: string; files: string[] } | null;
  /** why a skill it has could not be used */
  skill_error?: string | null;
  /** how it got here; null for a bundle loaded for a review */
  install: PluginInstall | null;
  /** the newer version an official plugin can be updated to, if any */
  update?: string | null;
};

/** An official plugin the app carries, as the registry's index lists one. */
export type CatalogEntry = {
  /** `forgeplane/<name>` */
  id: string;
  name: string;
  version: string;
  title: string;
  description: string | null;
  use_when: string | null;
  icon: string | null;
  official: boolean;
  /** selected by the setup */
  recommended: boolean;
  /** the version installed from here, if any */
  installed: string | null;
  /** the Pinrail version it needs, when this one is older */
  needs: string | null;
};

/** Where an installed plugin came from, and the bundle new reviews use. */
export type PluginInstall = {
  /** `index` for an official plugin, installed by its id; else a folder or a zip on disk */
  source_kind: "index" | "folder" | "archive";
  /** the official plugin's id, such as `forgeplane/list`, or the folder or the zip as a full path */
  source: string;
  /** served live from its folder rather than copied */
  link: boolean;
  /** the bundle new reviews render with; null for a link */
  bundle: string | null;
  /** the bundle's files no longer match its listing */
  modified: boolean;
  /** a linked folder changed since it was stored; the next use stores it */
  folder_changed: boolean;
  /** the person allowed its skill to reach the agents */
  skill_allowed?: boolean;
  installed_at: string;
  updated_at: string;
};

/** A review opened for the app's frame: the review, moved to the version
 *  of its plugin installed now when that version takes it; the bundle it
 *  renders with and its view's address; and `refused` when the installed
 *  version does not take it, saying why. */
export type ReviewView = {
  url: string;
  bundle: string;
  plugin: Plugin;
  review: Review;
  refused: string | null;
  /** the bundle the plugin's installation holds now */
  installed: string | null;
};

/** What installing a source would do, as the core reports it before anything runs. */
export type Inspection = {
  /** the name it would install under */
  name: string;
  version: string;
  title: string;
  icon: string | null;
  source_kind: "folder" | "archive";
  /** the folder or the zip, as a full path */
  source: string;
  link: boolean;
  /** the files it would take beside a payload; null for none */
  attachments?: AttachmentRules | null;
  /** what is installed under the name already, which installing replaces */
  installed: {
    version: string;
    source_kind: "index" | "folder" | "archive";
    source: string;
    link: boolean;
    unchanged: boolean;
    /** whether the sites the plugin may open stay allowed */
    links_kept: boolean;
  } | null;
  /** the source is older than what is installed */
  older: boolean;
};

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
  /** for session_status: the session and the status the agent reported */
  session?: { agent: string; id: string; status: SessionStatus };
};

/** What an agent said it is doing, with `pinrail status`. */
/** A file a note carries. */
export type NoteAttachment = { name: string; size: number; media_type: string };

export type SessionStatus = {
  id: number;
  text: string;
  /** a step of the agent's work, the start of a task, which names the
   *  session, the end of its work, or a note, whose text is Markdown */
  kind: "step" | "start" | "done" | "note";
  at: string;
  /** in a session's thread, the review it came after, or null for one
   *  before them all */
  after?: string | null;
  /** the files a note carries, by name */
  attachments?: NoteAttachment[];
};

/** A message the person sent the agent from its session's thread: queued
 *  until the agent reports a status, which delivers it. */
export type SessionMessage = {
  id: number;
  text: string;
  sent_by: string;
  sent_at: string;
  /** when a status handed it to the agent; null while it is queued */
  delivered_at: string | null;
  /** the session's newest status when it was sent, which places it among
   *  the statuses: their times, in whole seconds, do not always tell */
  after_status?: number | null;
};

/** The web origins a plugin may open without asking, while it stays
 *  installed from the source it had when they were allowed. */
export type LinkPermission = { source: string; origins: string[] };

/** What /api/v1/settings returns; the shell reads the keys it applies. */
/** A theme as the core lists it. A file with an `error` has no scheme and cannot be chosen. */
export type ThemeEntry = {
  id: string;
  name: string;
  scheme: "dark" | "light" | null;
  source: "bundled" | "user";
  shell: Record<string, string>;
  plugin: Record<string, string>;
  warnings: string[];
  error: string | null;
};

export type ServerSettings = {
  appearance: {
    theme: "system" | "dark" | "light";
    dark_theme: string;
    light_theme: string;
    text_size: "small" | "default" | "large";
  };
  sidebar: { open: boolean; sessions?: Partial<import("../lib/sidebarSessions").Choices & { collapsed: string[] }> };
  close_window: "hide" | "quit";
  menu_bar_icon: boolean;
  notifications: { enabled: boolean; paused_until: string | null; sound: boolean; muted_plugins: string[] };
  shortcut: { global: string; global_opens: "oldest" | "inbox" };
  inbox?: { open_in: "panel" | "page"; layout?: "project" | "repository" | "list" };
  whats_new?: { seen: string | null };
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
  /** update the connected agents' skills when the app starts */
  agents?: { auto_update: boolean };
  /** features to try, each off until turned on in Settings › Labs */
  labs?: { sessions: boolean };
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
