// The app's side of the plugin protocol: what every host of a view uses.

import type { Attachment, Capability, Decision, PluginMessage, ShellMessage, Theme, Violation } from "../types";

export const PROTOCOL: 1;
export const CAPABILITIES: Capability[];
/** how long a view has to answer a request for its decision */
export const COLLECT_TIMEOUT_MS: number;

/** What a host does with a decision the view handed over: accepted, with the
 *  decision as stored, or refused, with what is wrong. Nothing to tell the
 *  view is `null`. */
export type HandOverResult = { ok: true; decision: Decision } | { ok: false; violations: Violation[] } | null;

/** A message the host sends, before the envelope is added. */
export type Outgoing = { type: ShellMessage["type"] } & Record<string, unknown>;

/** A review as the host reads it: it sends the whole object to the view,
 *  and reads only its id and the files it lists. */
export type HostReview = { id: string; attachments?: Attachment[] | null };

export type HostOptions = {
  /** posts a message into the view's frame */
  post: (message: ShellMessage, transfer: Transferable[]) => void;
  /** the host's own origin, given to the view as `shell_origin` */
  origin: string;
  /** the review on screen, read whenever it is needed */
  review: () => HostReview | null;
  previous?: () => HostReview | null;
  readonly?: () => boolean;
  settings?: () => Record<string, unknown> | null;
  /** the theme sent before `init`; a host without one sends no `appearance` */
  theme?: () => Theme;
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
  resize?: (height: number | "fill") => void;
  /** one of the app's own keys, pressed inside the view */
  appKey?: (message: Extract<PluginMessage, { type: "key" }>) => void;
  /** ⌘/Ctrl+Enter pressed inside the view; without it, the host asks for
   *  the decision itself */
  handOverKey?: () => void;
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
  observe?: (direction: "in" | "out", message: ShellMessage | PluginMessage) => void;
  capabilities?: Capability[] | (() => Capability[]);
};

export type Host = {
  /** a message from the view's window */
  receive(message: unknown): void;
  /** sends `init` again, as the review stands */
  init(): void;
  /** asks the view for its decision; false when there is nothing to ask,
   *  such as while a request is open */
  collect(): boolean;
  appearance(theme: Theme): void;
  settings(values: Record<string, unknown>): void;
  key(fields: Omit<Extract<ShellMessage, { type: "key" }>, "pinrail" | "type">): void;
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
};

export function createHost(options: HostOptions): Host;

export function connectFrame(
  frame: HTMLIFrameElement,
  options: Omit<HostOptions, "post">,
): Host & { disconnect(): void };
