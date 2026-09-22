/**
 * The wicket plugin protocol, version 1, as TypeScript: the manifest the app
 * reads, the envelope a view is handed, and the messages both ways.
 *
 *   import type { Manifest, Init, ShellMessage, PluginMessage } from "wicket-plugin/types";
 *
 * At run time a view has `Wicket` on the window from /sdk/v1/wicket-plugin.js;
 * `WicketSdk` below is its shape.
 */

export type Protocol = 1;

export type Theme = "dark" | "light";

/** A plugin's own settings: every key `settings_schema` declares, with its value. */
export type Settings = Record<string, string | number | boolean>;

// ---------------------------------------------------------------- manifest

/** A JSON Schema 2020-12 document, inline or by a relative `$ref` into the plugin's folder. */
export type SchemaRef = { $ref: string } | Record<string, unknown>;

export type Shortcut = {
  /** modifiers (`cmd`, `ctrl`, `alt`, `shift`) joined by `+` and one key: `j`, `cmd+shift+f`, `escape` */
  keys: string;
  /** the one-line label the app's shortcuts dialog shows */
  does: string;
  /** puts the entry under a caption */
  group?: string;
};

export type Manifest = {
  /** `[a-z][a-z0-9_]*`, unique across the installed plugins */
  name: string;
  /** semantic (`"1.2.0"`); a bare integer reads as `N.0.0` */
  version: string | number;
  title: string;
  /** a Lucide icon name, shown beside the plugin's reviews */
  icon?: string;
  description?: string;
  payload_schema: SchemaRef;
  decision_schema: SchemaRef;
  /** the HTML the app serves as the view, relative to the folder: `view/index.html` */
  entry?: string;
  /** a MiniJinja file that renders a decision as markdown: `templates/decision.md.j2` */
  decision_template?: string;
  min_height?: number;
  /** the command that produces the bundle, run by an install: `npm ci && npm run build` */
  build?: { command: string };
  /** an object schema of scalars with defaults; each property is a row in Settings › Plugins */
  settings_schema?: Record<string, unknown>;
  /** the keys the view answers, listed by the app and forwarded when the frame has no focus */
  shortcuts?: Shortcut[];
  /** marks a plugin under development in the app's listings */
  dev?: boolean;
};

// ---------------------------------------------------------------- envelope

export type Status = "pending" | "decided" | "withdrawn" | "expired" | "discarded";

export type Origin = {
  repo?: string | null;
  workflow?: string | null;
  run_id?: string | null;
  ref?: string | null;
  url?: string | null;
};

export type Decision<Data = unknown> = {
  decided_by: string;
  decided_at: string;
  data: Data;
};

export type Summary = {
  counts?: [string, number][];
  subtitle?: string;
};

/** A review as the app hands it to a view: the envelope with its payload. */
export type Gate<Payload = unknown, Data = unknown> = {
  id: string;
  plugin: string;
  plugin_version: number;
  title: string;
  origin: Origin;
  requested_by: string | null;
  created_at: string;
  expires_at: string | null;
  /** the id of the review this one is a new round of */
  revises: string | null;
  summary: Summary | null;
  status: Status;
  decision: Decision<Data> | null;
  /** the person's note to the agent, beside the decision */
  agent_note: string | null;
  withdrawn_at?: string | null;
  withdrawn_reason?: string | null;
  discarded_at?: string | null;
  discarded_by?: string | null;
  discarded_reason?: string | null;
  payload: Payload;
};

// ---------------------------------------------------------------- messages

/** What `onInit` receives. */
export type Init<Payload = unknown, Data = unknown> = {
  gate: Gate<Payload, Data>;
  /** the review this one revises, decided, or null */
  previous: Gate<Payload, Data> | null;
  /** true whenever the review is not pending; `gate.status` says why */
  readonly: boolean;
  /** what the view last posted as a draft, or null */
  draft: any;
  settings: Settings;
};

export type Violation = { path: string; message: string };

export type Key = {
  key: string;
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
};

/** Shell → plugin, over `postMessage`. */
export type ShellMessage =
  | ({ wicket: Protocol; type: "init"; shell_origin: string } & Init)
  | { wicket: Protocol; type: "violations"; errors: Violation[] }
  | { wicket: Protocol; type: "submitted"; decision: Decision }
  | { wicket: Protocol; type: "collect" }
  | { wicket: Protocol; type: "appearance"; theme: Theme }
  | { wicket: Protocol; type: "settings"; settings: Settings }
  | ({ wicket: Protocol; type: "key" } & Key);

/** Plugin → shell, over `postMessage`. */
export type PluginMessage =
  | { wicket: Protocol; type: "ready" }
  | { wicket: Protocol; type: "resize"; height: number | "fill" }
  | { wicket: Protocol; type: "draft"; data: any }
  | { wicket: Protocol; type: "status"; label?: string }
  | { wicket: Protocol; type: "submit"; data: any }
  | { wicket: Protocol; type: "settings_set"; patch: Settings }
  /** open this link outside the app: http, https or mailto */
  | { wicket: Protocol; type: "open"; url: string };

// ---------------------------------------------------------------- the SDK

export type Handlers<Payload = unknown, Data = unknown> = {
  /** "auto" (content height, the default), "fill" (the viewport) or "manual" */
  resize?: "auto" | "fill" | "manual";
  /** false leaves ⌘/Ctrl+Enter to the view */
  shortcut?: boolean;
  onInit?(init: Init<Payload, Data>): void;
  onViolations?(errors: Violation[]): void;
  /** the decision was accepted; render read-only */
  onSubmitted?(decision: Decision<Data> | null): void;
  /** the shell's hand-over button, or ⌘/Ctrl+Enter */
  onCollect?(): void;
  /** the theme changed; `data-theme` on the root element is already set */
  onAppearance?(theme: Theme): void;
  /** the plugin's own settings changed */
  onSettings?(settings: Settings): void;
  /** a declared shortcut, pressed while the app rather than the frame had focus */
  onKey?(key: Key): void;
};

export type Plugin<Payload = unknown, Data = unknown> = {
  readonly gate: Gate<Payload, Data> | null;
  readonly previous: Gate<Payload, Data> | null;
  readonly readonly: boolean;
  readonly shellOrigin: string | null;
  readonly initialised: boolean;
  readonly theme: Theme;
  readonly settings: Settings;
  submit(data: Data): void;
  /** debounced; `{ flush: true }` posts at once */
  draft(data: any, opts?: { flush?: boolean }): void;
  /** only for `resize: "manual"` */
  resize(height: number | "fill"): void;
  /** what the shell's hand-over button should read */
  status(status: { label?: string }): void;
  /** opens a link in the system browser, as a click on one in the view does */
  open(url: string): void;
  /** asks the shell to keep one setting; it comes back as `settings`, or as `violations` */
  setSetting(key: string, value: string | number | boolean): void;
  collect(): void;
};

export type LayoutOptions = {
  title?: string;
  meta?: string | (HTMLElement | string)[];
  controls?: HTMLElement | (HTMLElement | string)[];
  /** a header with nothing in it yet */
  header?: boolean;
  /** where the skeleton goes; the body by default */
  into?: HTMLElement;
  document?: Document;
};

export type Layout = {
  header: HTMLElement | null;
  /** the element that scrolls */
  scroll: HTMLElement;
  /** render into this */
  content: HTMLElement;
  title(text: string | null): Layout;
  meta(items: string | (HTMLElement | string)[] | null): Layout;
  controls(items: HTMLElement | (HTMLElement | string)[] | null): Layout;
};

/** `window.Wicket`, from /sdk/v1/wicket-plugin.js. */
export type WicketSdk = {
  version: string;
  protocol: Protocol;
  connect<Payload = unknown, Data = unknown>(handlers: Handlers<Payload, Data>): Plugin<Payload, Data>;
  /** the standard skeleton the stylesheet expects: a header that stays put and a body that scrolls */
  layout(options?: LayoutOptions): Layout;
  /** an icon from the set the app serves, as markup that takes the text's colour */
  icon(name: string, opts?: { label?: string; size?: number | string; class?: string }): string;
  escape(text: string): string;
  /** markdown as HTML: raw HTML escaped, unsafe addresses dropped */
  markdown(source: string): string;
  /** the same, for one line: no paragraph around it */
  markdownInline(source: string): string;
  /** what the previous round decided for an item id, for `decisions: [{id, action, note}]` shapes */
  previousVerdict(previous: Gate | null, id: string | number): { action: string; note: string } | null;
};

declare global {
  interface Window {
    Wicket: WicketSdk;
  }
}
