// The shell side of the plugin protocol. The element is the sandboxed
// <iframe>; it has an opaque origin, so messages to it use "*" and messages
// from it are trusted only when event.source is its window.
//
// Every message is {pinrail: 1, type, ...}.
//   plugin -> shell: ready | resize {height | "fill"} | draft {data} | submit {data} |
//                    status {label} | settings_set {patch} | key {key, code, shiftKey} |
//                    attachment {req, name, round?: "previous"}
//   shell -> plugin: init {gate, previous, readonly, draft, settings, shell_origin,
//                          capabilities} |
//                    violations {errors} | submitted {decision} | collect |
//                    appearance {theme} | settings {settings} |
//                    attachment {req, ok, name, media_type, size, bytes} | {req, ok: false, error}
//
// A view cannot fetch anything, so a file its review carries comes this way:
// the view asks by name, the shell fetches it from the core (only names the
// review lists) and transfers the bytes into the frame.
//
// The theme is also on the frame's URL as #pinrail-theme=…, which is the only
// way it can reach the view before the view paints.

import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { api } from "../api/client";
import type { Decision, Review, Violation } from "../api/types";
import { EXTERNAL, openExternal } from "../lib/native";
import { currentTheme } from "../lib/theme";

/** The app's review-screen keys a view may pass up: help, and the rounds. */
const VIEW_APP_KEYS = ["?", "[", "]"];

const PROTOCOL = 1;
const DRAFT_PREFIX = "pinrail:draft:";
const LOADING_FALLBACK_MS = 2500;
const MAX_HEIGHT = 50000;
/** what this shell can do for a view beyond protocol 1's first messages */
const CAPABILITIES = ["attachments"];

export type SubmitResult = { ok: true; decision: Decision } | { ok: false; violations: Violation[] };

type Options = {
  frame: RefObject<HTMLIFrameElement | null>;
  /** the frame is rebuilt per review, so the bundle loads again for each */
  reviewId: string | null;
  review: Review | null;
  previous: Review | null;
  readonly: boolean;
  minHeight: number;
  /** the bundle URL without the theme fragment */
  src: string | null;
  connected: boolean;
  onSubmit: (data: unknown) => Promise<SubmitResult>;
  /** the plugin's own settings as they stand; null for a plugin that declares none */
  settings: Record<string, unknown> | null;
  /** the view asks to keep one of its settings; violations when the core refuses */
  onSetSetting: (patch: Record<string, unknown>) => Promise<Violation[]>;
};

export type Bridge = {
  loaded: boolean;
  /** the view asked for the whole remaining height */
  fill: boolean;
  submitting: boolean;
  handoverLabel: string;
  /** asks the view to assemble and submit its decision */
  collect: () => void;
};

export function usePluginBridge(options: Options): Bridge {
  const { frame, reviewId, review, previous, readonly, minHeight, src, connected, onSubmit, settings, onSetSetting } = options;
  const [loaded, setLoaded] = useState(false);
  const [fill, setFill] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [handoverLabel, setHandoverLabel] = useState("Hand over");
  const ready = useRef(false);
  const fallback = useRef<number | undefined>(undefined);
  const latest = useRef({ review, previous, readonly, connected, submitting: false, onSubmit, settings, onSetSetting, minHeight });
  latest.current = { review, previous, readonly, connected, submitting, onSubmit, settings, onSetSetting, minHeight };

  const post = useCallback(
    (msg: Record<string, unknown>) => frame.current?.contentWindow?.postMessage({ pinrail: PROTOCOL, ...msg }, "*"),
    [frame],
  );

  // A file fetched once per frame and review, however often the view asks;
  // each answer transfers a copy, since a transferred buffer is gone.
  const files = useRef(new Map<string, Promise<ArrayBuffer>>());
  const answerAttachment = async (req: unknown, name: unknown, round: unknown) => {
    if (typeof req !== "number" || typeof name !== "string") return;
    const { review, previous } = latest.current;
    const from = round === "previous" ? previous : review;
    const listed = from?.attachments?.find((a) => a.name === name);
    const fail = (error: string) => post({ type: "attachment", req, ok: false, name, error });
    if (!from || !listed) return fail(`no attachment "${name}" on this ${round === "previous" ? "previous round" : "review"}`);
    const key = `${from.id}\u0000${name}`;
    let bytes = files.current.get(key);
    if (!bytes) {
      bytes = api.attachmentBytes(from.id, name);
      files.current.set(key, bytes);
      bytes.catch(() => files.current.delete(key));
    }
    try {
      const copy = (await bytes).slice(0);
      frame.current?.contentWindow?.postMessage(
        { pinrail: PROTOCOL, type: "attachment", req, ok: true, name, media_type: listed.media_type, size: listed.size, bytes: copy },
        "*",
        [copy],
      );
    } catch (error) {
      fail(error instanceof Error ? error.message : `could not fetch ${name}`);
    }
  };

  const draftKey = () => DRAFT_PREFIX + (latest.current.review?.id ?? "");
  const loadDraft = () => {
    try {
      const raw = sessionStorage.getItem(draftKey());
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  };
  const saveDraft = (data: unknown) => {
    if (latest.current.readonly) return;
    try {
      sessionStorage.setItem(draftKey(), JSON.stringify(data));
    } catch {
      // storage unavailable: drafts are a convenience
    }
  };
  const clearDraft = () => {
    try {
      sessionStorage.removeItem(draftKey());
      sessionStorage.removeItem(draftKey() + ":note");
    } catch {
      // ignore
    }
  };

  const sendInit = useCallback(() => {
    const { review, previous, readonly, settings } = latest.current;
    if (!ready.current || !review) return;
    post({ type: "appearance", theme: currentTheme() });
    post({
      type: "init",
      gate: review,
      previous,
      readonly,
      draft: readonly ? null : loadDraft(),
      settings: settings ?? {},
      shell_origin: window.location.origin,
      capabilities: CAPABILITIES,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [post]);

  const markLoaded = useCallback(() => {
    window.clearTimeout(fallback.current);
    setLoaded(true);
  }, []);

  const collect = useCallback(() => {
    const { connected, review, readonly, submitting } = latest.current;
    if (connected && review && !readonly && !submitting) post({ type: "collect" });
  }, [post]);

  // Listen first, then load the bundle: the plugin's "ready" can never be
  // posted before the shell can hear it. Runs once per review and bundle:
  // pointing the frame at the URL it already shows would not reload it, and
  // the view would wait for an init that never comes.
  useEffect(() => {
    const el = frame.current;
    if (!el || !src) return;
    ready.current = false;
    files.current = new Map();
    setLoaded(false);
    setFill(false);
    setHandoverLabel("Hand over");

    const onMessage = async (event: MessageEvent) => {
      if (event.source !== el.contentWindow) return;
      const msg = event.data;
      if (!msg || msg.pinrail !== PROTOCOL) return;
      switch (msg.type) {
        case "ready":
          ready.current = true;
          sendInit();
          // A view that never reports a size would otherwise sit behind the
          // loading cover for good.
          fallback.current = window.setTimeout(markLoaded, LOADING_FALLBACK_MS);
          break;
        case "resize":
          if (msg.height === "fill") {
            el.style.height = "";
            setFill(true);
          } else if (typeof msg.height === "number" && Number.isFinite(msg.height)) {
            const height = Math.min(MAX_HEIGHT, Math.max(latest.current.minHeight, Math.ceil(msg.height)));
            el.style.height = `min(${height}px, 100%)`;
          }
          markLoaded();
          break;
        case "draft":
          saveDraft(msg.data);
          break;
        // A view's frame is sandboxed and cannot open anything itself; the
        // shell opens the link it asks for, once it is one we would follow.
        case "open":
          if (typeof msg.url === "string" && EXTERNAL.test(msg.url)) openExternal(msg.url);
          break;
        case "status":
          if (typeof msg.label === "string" && msg.label.trim()) setHandoverLabel(msg.label);
          break;
        case "attachment":
          void answerAttachment(msg.req, msg.name, msg.round);
          break;
        case "key": {
          // one of the app's own keys, pressed inside the view: the app acts
          // on it as if pressed in its window; nothing else is accepted
          if (typeof msg.key !== "string" || !VIEW_APP_KEYS.includes(msg.key)) return;
          window.dispatchEvent(new KeyboardEvent("keydown", { key: msg.key, code: typeof msg.code === "string" ? msg.code : "", shiftKey: !!msg.shiftKey, bubbles: true, cancelable: true }));
          break;
        }
        case "settings_set": {
          // the view may only ever write its own settings: the handler
          // fills in the plugin's name, and the core checks the values
          const patch = msg.patch;
          if (!patch || typeof patch !== "object" || Array.isArray(patch)) return;
          const violations = await latest.current.onSetSetting(patch as Record<string, unknown>);
          if (violations.length) post({ type: "violations", errors: violations });
          break;
        }
        case "submit": {
          const { readonly, submitting, connected, onSubmit } = latest.current;
          if (readonly || submitting) return;
          if (!connected) {
            post({
              type: "violations",
              errors: [{ path: "", message: "Reconnect before submitting. Your selections are preserved." }],
            });
            return;
          }
          setSubmitting(true);
          try {
            const result = await onSubmit(msg.data);
            if (result.ok) {
              clearDraft();
              post({ type: "submitted", decision: result.decision });
            } else {
              post({ type: "violations", errors: result.violations });
            }
          } finally {
            setSubmitting(false);
          }
          break;
        }
      }
    };
    window.addEventListener("message", onMessage);

    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault();
        collect();
      }
    };
    window.addEventListener("keydown", onKey);

    const onAppearance = () => post({ type: "appearance", theme: currentTheme() });
    window.addEventListener("pinrail:appearance", onAppearance);

    el.src = `${src}#pinrail-theme=${currentTheme()}`;

    return () => {
      window.removeEventListener("message", onMessage);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:appearance", onAppearance);
      window.clearTimeout(fallback.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [frame, reviewId, src, sendInit, markLoaded, collect, post]);

  // The plugin's settings changed, in Settings or through the view itself:
  // the view hears the values as they stand now.
  const sentSettings = useRef<string | null>(null);
  useEffect(() => {
    const now = settings ? JSON.stringify(settings) : null;
    if (!ready.current || now === null || now === sentSettings.current) {
      sentSettings.current = now;
      return;
    }
    sentSettings.current = now;
    post({ type: "settings", settings });
  }, [settings, post]);

  // When the review settles from elsewhere, the view flips to read-only.
  const wasReadonly = useRef(readonly);
  useEffect(() => {
    if (readonly && !wasReadonly.current) sendInit();
    wasReadonly.current = readonly;
  }, [readonly, sendInit]);

  return { loaded, fill, submitting, handoverLabel, collect };
}
