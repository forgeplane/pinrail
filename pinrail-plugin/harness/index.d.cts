import type { FrameLocator, Page } from "@playwright/test";
import type { Gate, PluginMessage, Settings } from "../types.js";

export type { Gate };

export type MountOptions = {
  /** a partial gate, usually a fixture's `{ title, payload }`; defaults fill the rest */
  gate: Partial<Gate> & Record<string, any>;
  previous?: Gate | null;
  readonly?: boolean;
  draft?: any;
  /** the theme the fake shell is in; the frame URL carries it, as in the app */
  theme?: "dark" | "light";
  /** the plugin's own settings, every key the manifest declares */
  settings?: Settings;
  /** files the review carries, by name: a path (relative to the plugin
   *  folder) or `{ path, media_type }`. A fixture's own `artifacts` are
   *  served without this. */
  artifacts?: Record<string, string | { path: string; media_type?: string }>;
  /** what the fake shell says it can do; `[]` plays an app too old for files */
  capabilities?: string[];
};

export type Message = PluginMessage;

export type MountedPlugin = {
  frame: FrameLocator;
  messages(): Promise<Message[]>;
  /** waits for the next `submit` after `after` messages had been seen */
  nextSubmit(after?: number): Promise<any>;
  lastDraft(): Promise<any>;
  /** the label the view last asked the shell's hand-over button to show */
  lastStatus(): Promise<string | null>;
  /** the last link the view asked the shell to open */
  lastOpen(): Promise<string | null>;
  /** the last setting the view asked the shell to keep, as a patch */
  lastSettingsSet(): Promise<Settings | null>;
  /** the plugin's settings changed in the app: sends them as they stand */
  settings(values: Settings): Promise<void>;
  /** a declared shortcut pressed while the shell had focus: "j", "cmd+shift+m" */
  sendKey(combo: string): Promise<void>;
  send(msg: Record<string, any>): Promise<void>;
  sendViolations(errors: { path: string; message: string }[]): Promise<void>;
  sendSubmitted(decision: Gate["decision"]): Promise<void>;
  /** asks the view to hand over, as the shell's button does */
  collect(): Promise<void>;
  /** holds the frame at a height, so a view taller than that has to scroll */
  setFrameHeight(px: number): Promise<void>;
  /** re-sends init with the last draft, as the shell does after a reload */
  reinit(overrides?: Partial<MountOptions>): Promise<void>;
  reload(): Promise<void>;
};

/** A gate envelope with defaults, from a fixture's partial gate. */
export function gateFrom(partial: Partial<Gate> & Record<string, any>): Gate;

/** A fixture file (`{ title, payload }`, or with a `decision`) as a gate. */
export function fixture(file: string): Gate;

/**
 * Mounts the plugin in `pluginDir` under the fake shell on `page`, sends
 * `init` from the options and hands back the frame and the shell's side of
 * the protocol.
 */
export function mountPlugin(page: Page, pluginDir: string, opts: MountOptions): Promise<MountedPlugin>;
