/**
 * The pinrail plugin protocol, version 1, as TypeScript: the manifest the app
 * reads, the envelope a view is handed, and the messages both ways.
 *
 *   import type { Manifest, Init, ShellMessage, PluginMessage } from "@forgeplane/pinrail-plugin/types";
 *
 * At run time a view has `Pinrail` on the window from /sdk/v1/pinrail-plugin.js;
 * `PinrailSdk` below is its shape.
 */

export type Protocol = 1;

export type Theme = "dark" | "light";

/** A plugin's own settings: every key `settings_schema` declares, with its value. */
export type Settings = Record<string, string | number | boolean>;

// ---------------------------------------------------------------- manifest

/** A JSON Schema 2020-12 document, inline or by a relative `$ref` into the plugin's folder. */
export type SchemaRef = { $ref: string } | Record<string, unknown>;

export type Shortcut = {
  /** modifiers (`cmd`, `ctrl`, `alt`, `shift`, or `cmdorctrl`) joined by `+` in any order, then one key: `j`, `cmd+shift+f`, `escape` */
  keys: string;
  /** the one-line label the app's shortcuts dialog shows */
  does: string;
  /** puts the entry under a caption */
  group?: string;
};

/** A plugin's manifest. The plugin's files have fixed places beside it:
 *  `view/index.html`, `schemas/payload.schema.json`,
 *  `schemas/decision.schema.json`, `icon.svg` and
 *  `templates/decision.md.j2`. */
export type Manifest = {
  /** the manifest schema's address, for an editor that validates the file */
  $schema?: string;
  /** `[a-z][a-z0-9_-]*`, unique across the installed plugins */
  name: string;
  /** a semantic version, such as `"1.2.0"` */
  version: string;
  /** the oldest Pinrail the plugin works with, as a version range: `">=0.1"` */
  pinrail?: string;
  /** what the app calls the plugin; the name when absent */
  title?: string;
  description?: string;
  /** what the app counts to sum up a review: arrays of the payload and of the decision */
  summary?: { request?: SummaryRules; outcome?: SummaryRules };
  min_height?: number;
  /** an object schema of scalars with defaults; each property is a row in Settings › Plugins */
  settings_schema?: Record<string, unknown>;
  /** the keys the view answers, listed by the app and forwarded when the frame has no focus */
  shortcuts?: Shortcut[];
  /** marks a plugin under development in the app's listings */
  dev?: boolean;
  /** when an agent should ask with this plugin, for `pinrail plugins describe` */
  use_when?: string;
  /** the files the plugin takes beside a payload; without it, none */
  attachments?: AttachmentRules;
};

/** The colours a summary may use; the app maps each to the theme. */
export type SummaryTone = "danger" | "warning" | "info" | "success" | "neutral";

/** A label in a summary, with its plural when the count is not one. */
export type SummaryLabel = { label?: string; plural?: string; tone?: SummaryTone };

/** One side of a summary: counts of arrays, and for the outcome a verdict. */
export type SummaryRules = {
  /** each entry counts the array `items` points to (JSON Pointer, `*` for
   *  every element): by the values of the field `by`, or as a whole */
  counts?: (
    | { items: string; by: string; values: Record<string, SummaryLabel>; other?: boolean }
    | ({ items: string; label: string } & SummaryLabel)
  )[];
  /** the field that holds the decision's overall answer, and how each value reads */
  verdict?: { at: string; values: Record<string, SummaryLabel> };
};

/** What a plugin takes: kinds as `.ext` or media types (`image/*` too), and
 *  limits no looser than the app's 100 MB a file and 32 a review. */
export type AttachmentRules = {
  accept: string[];
  max_size?: number;
  max_count?: number;
};

/** A file a review carries, as the review lists it; the payload names it
 *  `{ "$attachment": name }`. */
export type Attachment = {
  name: string;
  size: number;
  media_type: string;
  sha256: string;
};

/** How a payload names a file: `{ "$attachment": "pivot.glb" }`. */
export type AttachmentRef = { $attachment: string };

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
export type Review<Payload = unknown, Data = unknown> = {
  id: string;
  /** the plugin's name, such as `list` */
  plugin: string;
  /** the exact version it was submitted to, such as `1.2.0` */
  plugin_version: string;
  /** the bundle it was submitted to, which it renders with */
  plugin_bundle: string | null;
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
  /** the files the review carries, by the names its payload uses */
  attachments?: Attachment[];
};

// ---------------------------------------------------------------- messages

/** What `onInit` receives. */
export type Init<Payload = unknown, Data = unknown> = {
  review: Review<Payload, Data>;
  /** the review this one revises, decided, or null */
  previous: Review<Payload, Data> | null;
  /** true whenever the review is not pending; `review.status` says why */
  readonly: boolean;
  /** what the view last posted as a draft, or null */
  draft: any;
  settings: Settings;
};

/** What the shell can do beyond protocol 1's first messages, from `init`. */
export type Capability = "attachments";

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
  | ({ pinrail: Protocol; type: "init"; shell_origin: string; capabilities?: Capability[] } & Init)
  | {
      pinrail: Protocol;
      type: "attachment";
      req: number;
      ok: true;
      name: string;
      media_type: string;
      size: number;
      bytes: ArrayBuffer;
    }
  | { pinrail: Protocol; type: "attachment"; req: number; ok: false; name?: string; error: string }
  | { pinrail: Protocol; type: "violations"; errors: Violation[] }
  | { pinrail: Protocol; type: "submitted"; decision: Decision }
  | { pinrail: Protocol; type: "collect" }
  | { pinrail: Protocol; type: "appearance"; theme: Theme }
  | { pinrail: Protocol; type: "settings"; settings: Settings }
  | ({ pinrail: Protocol; type: "key" } & Key);

/** Plugin → shell, over `postMessage`. */
export type PluginMessage =
  | { pinrail: Protocol; type: "ready" }
  | { pinrail: Protocol; type: "resize"; height: number | "fill" }
  | { pinrail: Protocol; type: "draft"; data: any }
  | { pinrail: Protocol; type: "status"; label?: string }
  | { pinrail: Protocol; type: "submit"; data: any }
  | { pinrail: Protocol; type: "settings_set"; patch: Settings }
  | {
      pinrail: Protocol;
      type: "key";
      key: "?" | "[" | "]";
      code: string;
      metaKey: false;
      ctrlKey: false;
      altKey: false;
      shiftKey: boolean;
    }
  /** open this link outside the app: http, https or mailto */
  | { pinrail: Protocol; type: "open"; url: string }
  /** the bytes of a file the review (or the round it revises) carries */
  | { pinrail: Protocol; type: "attachment"; req: number; name: string; round?: "previous" };

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
  readonly review: Review<Payload, Data> | null;
  readonly previous: Review<Payload, Data> | null;
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
  /** asks the app to open a link in the system browser, as a click on one in the view does; the app asks the person first unless they allowed the site */
  open(url: string): void;
  /** asks the shell to keep one setting; it comes back as `settings`, or as `violations` */
  setSetting(key: string, value: string | number | boolean): void;
  collect(): void;
  /** the files the review carries */
  readonly attachments: Attachment[];
  /** a file's bytes, from the shell; `round: "previous"` for the round this one revises */
  attachment(name: string, opts?: { round?: "previous" }): Promise<ArrayBuffer>;
  /** the same as a blob: URL for an <img>, <video> or <audio>; revoke it when done */
  attachmentUrl(name: string, opts?: { round?: "previous"; type?: string }): Promise<string>;
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

/** `window.Pinrail`, from /sdk/v1/pinrail-plugin.js. */
export type PinrailSdk = {
  version: string;
  protocol: Protocol;
  connect<Payload = unknown, Data = unknown>(handlers: Handlers<Payload, Data>): Plugin<Payload, Data>;
  /** the standard skeleton the stylesheet expects: a header that stays put and a body that scrolls */
  layout(options?: LayoutOptions): Layout;
  /** the plugin's own `icons/<name>.svg`, beside the view, as markup that takes the text's colour */
  icon(name: string, opts?: { label?: string; size?: number | string; class?: string }): string;
  escape(text: string): string;
  /** markdown as HTML: raw HTML escaped, unsafe addresses dropped */
  markdown(source: string): string;
  /** the same, for one line: no paragraph around it */
  markdownInline(source: string): string;
  /** what the previous round decided for an item id, for `decisions: [{id, action, note}]` shapes */
  previousVerdict(previous: Review | null, id: string | number): { action: string; note: string } | null;
  /** the name in `{ "$attachment": name }`, or null for anything else */
  attachmentName(ref: unknown): string | null;
  /** `{ "$attachment": name }` as JSON Schema, for a payload schema's $defs */
  readonly ATTACHMENT_SCHEMA: Record<string, unknown>;
};

declare global {
  interface Window {
    Pinrail: PinrailSdk;
  }
  /** the SDK, as a view's script sees it */
  var Pinrail: PinrailSdk;
}
