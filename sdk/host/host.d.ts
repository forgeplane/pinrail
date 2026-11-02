// The app's side of the plugin protocol: what every host of a view uses.

import type { Attachment, Capability, Decision, PluginMessage, Review, AppMessage, Theme, Violation } from "../types";

export const PROTOCOL: 1;
export const CAPABILITIES: Capability[];
/** the longest text a view can copy, in characters */
export const CLIPBOARD_TEXT_MAX: number;
/** how long a view has to answer a request for its decision */
export const COLLECT_TIMEOUT_MS: number;

/** What a host does with a decision the view handed over: accepted, with the
 *  decision as stored, or refused, with what is wrong. Nothing to tell the
 *  view is `null`. */
export type HandOverResult = { ok: true; decision: Decision } | { ok: false; violations: Violation[] } | null;

/** A message the host sends, before the envelope is added. */
export type Outgoing = { type: AppMessage["type"] } & Record<string, unknown>;

/** A review as the host reads it: it sends the whole object to the view,
 *  and reads only its id and the files it lists. */
export type HostReview = { id: string; attachments?: Attachment[] | null };

export type HostOptions = {
  /** posts a message into the view's frame */
  post: (message: AppMessage, transfer: Transferable[]) => void;
  /** the host's own origin, given to the view as `app_origin` */
  origin: string;
  /** the review on screen, read whenever it is needed */
  review: () => HostReview | null;
  previous?: () => HostReview | null;
  readonly?: () => boolean;
  settings?: () => Record<string, unknown> | null;
  /** the theme sent before `init`; a host without one sends no `appearance` */
  theme?: () => Theme;
  /** a custom theme's `--pinrail-*` colours, sent with the theme; empty for a built-in one */
  tokens?: () => Record<string, string>;
  loadDraft?: () => unknown;
  saveDraft?: (data: unknown) => void;
  /** a host without one leaves `submit` to whoever drives the view */
  handOver?: (data: unknown) => Promise<HandOverResult>;
  /** the bytes of a file the review lists */
  file?: (review: HostReview, listed: Attachment, round: "current" | "previous") => Promise<ArrayBuffer>;
  open?: (url: string) => void;
  /** a change to the plugin's settings; the violations when it is refused */
  setSettings?: (patch: Record<string, unknown>) => Promise<Violation[]>;
  /** the text the view gives the hand-over button */
  label?: (text: string) => void;
  /** one of the app's own keys, pressed inside the view */
  appKey?: (message: Extract<PluginMessage, { type: "key" }>) => void;
  /** ⌘/Ctrl+Enter pressed inside the view; without it, the host asks for
   *  the decision itself */
  handOverKey?: () => void;
  /** the matches of the find bar's latest search, as the view counts them */
  onFound?: (found: { count: number; current: number }) => void;
  /** the view has nothing to hand over for the open request yet */
  onDefer?: () => void;
  /** how long the view has to answer a request, in milliseconds */
  collectTimeout?: number;
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (timer: unknown) => void;
  onReady?: () => void;
  /** another page took the view's place in the frame */
  onLeft?: () => void;
  /** every message, as it comes in from the view or goes out to it */
  observe?: (direction: "in" | "out", message: AppMessage | PluginMessage) => void;
  capabilities?: Capability[];
  /** runs speech recognition for the view: starts it, sends the listener the
   *  text recognised so far each time it grows, then `done`, with `error` when
   *  it failed, and returns a function that stops it. When set, `init` lists
   *  "dictation". */
  dictation?: (listener: (event: { text: string } | { done: true; error?: string }) => void) => () => void;
  /** whether `dictation` is available now, checked at each `init`; true when omitted */
  canDictate?: () => boolean;
  /** called when a dictation the view requested starts or ends */
  onDictation?: (listening: boolean) => void;
  /** copies for the view: its text, or the bytes of an image the review
   *  carries. Give it only to a plugin whose manifest declares the
   *  "clipboard" capability. When set, `init` lists "clipboard". */
  clipboard?: (content: { text: string } | { image: ArrayBuffer; media_type: string; name: string }) => Promise<void>;
  /** whether `clipboard` is offered now, checked at each `init` and copy;
   *  true when omitted. A host that learns the plugin later decides here. */
  canCopy?: () => boolean;
};

export type Host = {
  /** a message from the view's window */
  receive(message: unknown): void;
  /** sends `init` again, as the review stands */
  init(): void;
  /** asks the view for its decision; false when there is nothing to ask,
   *  such as while a request is open */
  collect(): boolean;
  appearance(theme: Theme, tokens?: Record<string, string>): void;
  settings(values: Record<string, unknown>): void;
  /** searches the view for the find bar; false before the view is ready */
  find(query: string, options?: { backwards?: boolean }): boolean;
  key(fields: Omit<Extract<AppMessage, { type: "key" }>, "pinrail" | "type">): void;
  send(message: Outgoing, transfer?: Transferable[]): void;
  /** the review or its state changed */
  changed(): void;
  /** the frame finished loading a page */
  loaded(): void;
  /** the host loads the view's page again */
  reload(): void;
  dispose(): void;
  readonly ready: boolean;
  readonly left: boolean;
  readonly handingOver: boolean;
  /** a request for the decision is open */
  readonly collecting: boolean;
  /** stops the view's dictation; the final text and the end message still arrive */
  stopDictation(): void;
  /** whether a dictation the view requested is active */
  readonly dictating: boolean;
};

/** A review as a view receives it, built from the API's. */
export function viewReview(review: unknown): Review | null;
/** A decision as a view sees it. */
export function viewDecision(decision: unknown): Decision | null;

export function createHost(options: HostOptions): Host;

export function connectFrame(
  frame: HTMLIFrameElement,
  options: Omit<HostOptions, "post">,
): Host & { disconnect(): void };

/** The fragment of a frame's URL that gives a view its theme before it paints. */
export function themeFragment(theme: Theme, tokens?: Record<string, string>): string;
