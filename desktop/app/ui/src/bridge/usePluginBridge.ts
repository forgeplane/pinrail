// The shell side of the plugin protocol. The element is the sandboxed
// <iframe>; it has an opaque origin, so messages to it use "*" and messages
// from it are trusted only when event.source is its window.
//
// Every message is {wicket: 1, type, ...}.
//   plugin -> shell: ready | resize {height | "fill"} | draft {data} | submit {data} |
//                    status {label}
//   shell -> plugin: init {gate, previous, readonly, draft, shell_origin} |
//                    violations {errors} | submitted {decision} | collect |
//                    appearance {theme}
//
// The theme is also on the frame's URL as #wicket-theme=…, which is the only
// way it can reach the view before the view paints.

import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import type { Decision, Review, Violation } from "../api/types";
import { currentTheme } from "../lib/theme";

const PROTOCOL = 1;
const DRAFT_PREFIX = "wicket:draft:";
const LOADING_FALLBACK_MS = 2500;
const MAX_HEIGHT = 50000;

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
  const { frame, reviewId, review, previous, readonly, minHeight, src, connected, onSubmit } = options;
  const [loaded, setLoaded] = useState(false);
  const [fill, setFill] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [handoverLabel, setHandoverLabel] = useState("Hand over");
  const ready = useRef(false);
  const fallback = useRef<number | undefined>(undefined);
  const latest = useRef({ review, previous, readonly, connected, submitting: false, onSubmit });
  latest.current = { review, previous, readonly, connected, submitting, onSubmit };

  const post = useCallback(
    (msg: Record<string, unknown>) => frame.current?.contentWindow?.postMessage({ wicket: PROTOCOL, ...msg }, "*"),
    [frame],
  );

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
    const { review, previous, readonly } = latest.current;
    if (!ready.current || !review) return;
    post({ type: "appearance", theme: currentTheme() });
    post({
      type: "init",
      gate: review,
      previous,
      readonly,
      draft: readonly ? null : loadDraft(),
      shell_origin: window.location.origin,
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
  // posted before the shell can hear it.
  useEffect(() => {
    const el = frame.current;
    if (!el || !src) return;
    ready.current = false;
    setLoaded(false);
    setFill(false);
    setHandoverLabel("Hand over");

    const onMessage = async (event: MessageEvent) => {
      if (event.source !== el.contentWindow) return;
      const msg = event.data;
      if (!msg || msg.wicket !== PROTOCOL) return;
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
            const height = Math.min(MAX_HEIGHT, Math.max(minHeight, Math.ceil(msg.height)));
            el.style.height = `min(${height}px, 100%)`;
          }
          markLoaded();
          break;
        case "draft":
          saveDraft(msg.data);
          break;
        case "status":
          if (typeof msg.label === "string" && msg.label.trim()) setHandoverLabel(msg.label);
          break;
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
    window.addEventListener("wicket:appearance", onAppearance);

    el.src = `${src}#wicket-theme=${currentTheme()}`;

    return () => {
      window.removeEventListener("message", onMessage);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("wicket:appearance", onAppearance);
      window.clearTimeout(fallback.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [frame, reviewId, src, minHeight, sendInit, markLoaded, collect, post]);

  // When the review settles from elsewhere, the view flips to read-only.
  const wasReadonly = useRef(readonly);
  useEffect(() => {
    if (readonly && !wasReadonly.current) sendInit();
    wasReadonly.current = readonly;
  }, [readonly, sendInit]);

  return { loaded, fill, submitting, handoverLabel, collect };
}
