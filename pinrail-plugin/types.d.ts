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

export type Decision<Data = unknown> = {
  data: Data;
  decided_by: string;
  decided_at: string;
};

/** A review as the app hands it to a view: these fields and no others,
 *  for as long as protocol 1 lasts. */
export type Review<Payload = unknown, Data = unknown> = {
  id: string;
  title: string;
  status: Status;
  created_at: string;
  payload: Payload;
  /** the files the review carries, by the names its payload uses */
  attachments: Attachment[];
  decision: Decision<Data> | null;
};

// ---------------------------------------------------------------- messages

/** What `onInit` receives. */
export type Init<Payload = unknown, Data = unknown> = {
  review: Review<Payload, Data>;
  /** the review this one revises, decided, or null. It may come from an
   *  earlier release of the plugin, so its payload and decision are
   *  `unknown`: check their shape before using them. */
  previous: Review<unknown, unknown> | null;
  /** true whenever the review is not pending; `review.status` says why */
  readonly: boolean;
  /** what the view last posted as a draft, or null. A pending review can
   *  move to a newer release of the plugin, so a draft may have been kept
   *  by an earlier one: check its shape before using it. */
  draft: unknown;
  settings: Settings;
};

/** What the app can do for a view beyond protocol 1, from `init`: nothing yet. */
export type Capability = string;

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
  /** asks for the decision; the view answers `submit` or `defer` with this `req` */
  | { pinrail: Protocol; type: "collect"; req: number }
  | { pinrail: Protocol; type: "appearance"; theme: Theme }
  /** the plugin's settings changed, in the app or through the view */
  | { pinrail: Protocol; type: "settings"; settings: Settings }
  /** the answer to the view's `settings_set` with this `req` */
  | { pinrail: Protocol; type: "settings"; req: number; ok: true }
  | { pinrail: Protocol; type: "settings"; req: number; ok: false; errors: Violation[] }
  | ({ pinrail: Protocol; type: "key" } & Key);

/** Plugin → shell, over `postMessage`. */
export type PluginMessage =
  | { pinrail: Protocol; type: "ready" }
  | { pinrail: Protocol; type: "resize"; height: number | "fill" }
  | { pinrail: Protocol; type: "draft"; data: any }
  | { pinrail: Protocol; type: "status"; label?: string }
  /** the decision, in answer to the `collect` with this `req` */
  | { pinrail: Protocol; type: "submit"; req: number; data: any }
  /** nothing to hand over for the `collect` with this `req` yet */
  | { pinrail: Protocol; type: "defer"; req: number }
  | { pinrail: Protocol; type: "settings_set"; req: number; patch: Settings }
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
  /** ⌘/Ctrl+Enter pressed inside the view: the app starts the hand-over */
  | {
      pinrail: Protocol;
      type: "key";
      key: "Enter";
      code: string;
      metaKey: boolean;
      ctrlKey: boolean;
      altKey: false;
      shiftKey: false;
    }
  /** open this link outside the app: http, https or mailto */
  | { pinrail: Protocol; type: "open"; url: string }
  /** the bytes of a file the review (or the round it revises) carries */
  | { pinrail: Protocol; type: "attachment"; req: number; name: string; round?: "previous" };

// ---------------------------------------------------------------- the SDK

export type Handlers<Payload = unknown, Data = unknown> = {
  /** "auto" (content height, the default), "fill" (the viewport) or "manual" */
  resize?: "auto" | "fill" | "manual";
  onInit?(init: Init<Payload, Data>): void;
  onViolations?(errors: Violation[]): void;
  /** the decision was accepted; render read-only */
  onSubmitted?(decision: Decision<Data> | null): void;
  /** The app's hand-over button, or ⌘/Ctrl+Enter: return the decision, or a
   *  promise of it. Return nothing when the view needs more from the person
   *  first, such as a missing answer or a preview to confirm. */
  onCollect?(): Data | undefined | void | Promise<Data | undefined | void>;
  /** a handler threw, or a decision JSON cannot hold; without it, the console */
  onError?(error: Error): void;
  /** the theme changed; `data-theme` on the root element is already set */
  onAppearance?(theme: Theme): void;
  /** the plugin's own settings changed */
  onSettings?(settings: Settings): void;
};

export type Plugin<Payload = unknown, Data = unknown> = {
  readonly review: Review<Payload, Data> | null;
  readonly previous: Review<unknown, unknown> | null;
  readonly readonly: boolean;
  readonly theme: Theme;
  readonly settings: Settings;
  /** keeps work in progress at once; it comes back in `init`. Throws for a value JSON cannot hold. */
  draft(data: unknown): void;
  /** only for `resize: "manual"` */
  resize(height: number | "fill"): void;
  /** what the shell's hand-over button should read */
  status(status: { label?: string }): void;
  /** asks the app to open a link in the system browser, as a click on one in the view does; the app asks the person first unless they allowed the site */
  open(url: string): void;
  /** asks the app to keep one setting: the settings as they now stand, or
   *  a rejection whose error carries the `violations` */
  setSetting(key: string, value: string | number | boolean): Promise<Settings>;
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
  /** the name in `{ "$attachment": name }`, or null for anything else */
  attachmentName(ref: unknown): string | null;
};

declare global {
  interface Window {
    Pinrail: PinrailSdk;
  }
  interface KeyboardEvent {
    /** true on a keydown the SDK dispatched for a shortcut the person
     *  pressed while the app, not the view, had the focus */
    readonly pinrailForwarded?: true;
  }
  /** the SDK, as a view's script sees it */
  var Pinrail: PinrailSdk;
}
