// The app's side of the plugin protocol, for the review screen: the SDK's
// host module speaks the protocol (as the preview page, the development
// shell and the test harness do), and this hook gives it what only the app
// has. The element is the sandboxed <iframe>; it has an opaque origin, so
// messages to it use "*" and messages from it are trusted only when
// event.source is its window.
//
// Every message is {pinrail: 1, type, ...}.
//   plugin -> shell: ready | draft {data} | submit {req, data} |
//                    defer {req} | status {label} | settings_set {patch} |
//                    key {key, code, metaKey, ctrlKey, shiftKey} |
//                    attachment {req, name, round?: "previous"} |
//                    found {req, count, current} |
//                    clipboard_write {req, text} | {req, attachment}
//   shell -> plugin: init {review, previous, readonly, draft, settings, app_origin,
//                          capabilities} |
//                    violations {errors} | submitted {decision} | collect {req} |
//                    appearance {theme} | settings {settings} |
//                    attachment {req, ok, name, media_type, size, bytes} | {req, ok: false, error} |
//                    find {req, query, backwards} |
//                    clipboard {req, ok} | {req, ok: false, error}
//
// A view cannot fetch anything, so a file its review carries comes this way:
// the view asks by name, the shell fetches it from the core (only names the
// review lists) and transfers the bytes into the frame.
//
// The theme is also on the frame's URL as #pinrail-theme=…, with a custom
// theme's colours as &pinrail-tokens=…, which is the only
// way it can reach the view before the view paints.

import type { Found } from "../lib/findText";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { connectFrame, themeFragment, type Host } from "pinrail-sdk/host";
import { api } from "../api/client";
import type { Decision, Review, Violation } from "../api/types";
import { currentTheme, currentTokens } from "../lib/theme";
import { modalOpen } from "../lib/keys";
import { copyImage, copyText } from "../lib/clipboard";
import { canDictate, listen } from "../lib/dictation";
import { useLatest } from "../lib/useLatest";

/** The review-screen shortcuts a view may forward to the app. Help, the
 *  rounds, and J and K for the inbox come without a modifier. Discard and
 *  find come with ⌘ or Ctrl. */
const VIEW_APP_KEYS = ["?", "[", "]", "j", "k", "Escape"];
const VIEW_APP_MOD_KEYS = ["d", "f"];

const DRAFT_PREFIX = "pinrail:draft:";

export type { Found };

export type SubmitResult = { ok: true; decision: Decision } | { ok: false; violations: Violation[] };

type Options = {
  frame: RefObject<HTMLIFrameElement | null>;
  /** the frame is rebuilt per review, so the bundle loads again for each */
  reviewId: string | null;
  review: Review | null;
  previous: Review | null;
  readonly: boolean;
  /** the bundle URL without the theme fragment */
  src: string | null;
  connected: boolean;
  onSubmit: (data: unknown) => Promise<SubmitResult>;
  /** the plugin's own settings as they stand; null for a plugin that declares none */
  settings: Record<string, unknown> | null;
  /** the view asks to keep one of its settings; violations when the core refuses */
  onSetSetting: (patch: Record<string, unknown>) => Promise<Violation[]>;
  /** the view asks to open a link; the screen decides whether it opens */
  onOpen: (url: string) => void;
  /** shows the message of a failed dictation the view requested */
  onDictationError?: (message: string) => void;
  /** the plugin's manifest declares the "clipboard" capability, so the app
   *  copies for its view; the frame cannot reach the clipboard itself */
  canCopy?: boolean;
  /** tells the person what the view copied: the text, or the image by name */
  onCopied?: (what: { text: string } | { image: string }) => void;
};

export type Bridge = {
  loaded: boolean;
  submitting: boolean;
  handoverLabel: string;
  /** asks the view to assemble and submit its decision */
  collect: () => void;
  /** the frame went to another page, which the bridge no longer answers */
  left: boolean;
  /** loads the view's own page again */
  reload: () => void;
  /** how many times the view was reloaded: part of the frame's key, so a
   *  reload gets a new frame. Setting a frame's src to the address it already
   *  has does not always load it again, in WebKit after a blocked navigation. */
  reloads: number;
  /** searches the view's text: the next match when the query is the one
   *  before, the previous with `backwards`; an empty query clears it */
  find: (query: string, backwards?: boolean) => void;
  /** what the view found for the latest search; null before it answers */
  found: Found | null;
  /** whether the view is dictating into one of its fields */
  dictating: boolean;
  /** stops the view's dictation */
  stopDictation: () => void;
};

export function usePluginBridge(options: Options): Bridge {
  const {
    frame,
    reviewId,
    review,
    previous,
    readonly,
    src,
    connected,
    onSubmit,
    settings,
    onSetSetting,
    onOpen,
    onDictationError,
    canCopy = false,
    onCopied,
  } = options;
  const [loaded, setLoaded] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [handoverLabel, setHandoverLabel] = useState("Hand over");
  // the frame went to another page, which gets nothing from here on
  const [left, setLeft] = useState(false);
  const [reloads, setReloads] = useState(0);
  const [found, setFound] = useState<Found | null>(null);
  const [dictating, setDictating] = useState(false);
  const reload = useCallback(() => setReloads((n) => n + 1), []);
  const host = useRef<(Host & { disconnect(): void }) | null>(null);
  const latest = useLatest({
    review,
    previous,
    readonly,
    connected,
    onSubmit,
    settings,
    onSetSetting,
    onOpen,
    onDictationError,
    canCopy,
    onCopied,
  });

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
    try {
      sessionStorage.setItem(draftKey(), JSON.stringify(data));
    } catch {
      // storage unavailable: drafts are a convenience
    }
  };
  /** Forgets the draft of the review open now, or of the one `key` names. */
  const clearDraft = (key = draftKey()) => {
    try {
      sessionStorage.removeItem(key);
      sessionStorage.removeItem(key + ":note");
    } catch {
      // ignore
    }
  };

  const collect = useCallback(() => {
    // not while a dialog is open: ⌘Enter in a plugin's setting field is no
    // hand-over
    if (latest.current.connected && !modalOpen()) host.current?.collect();
  }, [latest]);

  const find = useCallback((query: string, backwards = false) => {
    if (!query) setFound(null);
    host.current?.find(query, { backwards });
  }, []);

  // Listen first, then load the bundle: the plugin's "ready" can never be
  // posted before the app can hear it. Runs once per review and bundle:
  // pointing the frame at the URL it already shows would not reload it, and
  // the view would wait for an init that never comes.
  useEffect(() => {
    const el = frame.current;
    if (!el || !src) return;
    setLeft(false);
    setLoaded(false);
    setHandoverLabel("Hand over");
    setFound(null);

    // A file fetched once per frame and review, however often the view asks;
    // the host transfers a copy each time.
    const files = new Map<string, Promise<ArrayBuffer>>();
    const fetchFile = (id: string, name: string) => {
      const key = `${id}\u0000${name}`;
      let bytes = files.get(key);
      if (!bytes) {
        bytes = api.attachmentBytes(id, name);
        files.set(key, bytes);
        bytes.catch(() => files.delete(key));
      }
      return bytes;
    };

    const connection = connectFrame(el, {
      origin: window.location.origin,
      review: () => latest.current.review,
      previous: () => latest.current.previous,
      readonly: () => latest.current.readonly,
      settings: () => latest.current.settings ?? {},
      theme: currentTheme,
      tokens: currentTokens,
      loadDraft,
      saveDraft,
      handOver: async (data) => {
        const { connected, onSubmit } = latest.current;
        if (!connected) {
          return {
            ok: false,
            violations: [{ path: "", message: "Reconnect before submitting. Your selections are preserved." }],
          };
        }
        setSubmitting(true);
        // the review this decision is for, whatever is open when it lands
        const drafted = draftKey();
        try {
          const result = await onSubmit(data);
          if (result.ok) clearDraft(drafted);
          return result.ok ? { ok: true, decision: result.decision } : { ok: false, violations: result.violations };
        } finally {
          setSubmitting(false);
        }
      },
      file: (from, listed) => fetchFile(from.id, listed.name),
      // A view's frame is sandboxed and cannot open anything itself; the
      // screen opens the link it asks for, once it is one we would follow
      open: (url) => latest.current.onOpen(url),
      // the view may only ever write its own settings: the handler fills in
      // the plugin's name, and the core checks the values
      setSettings: (patch) => latest.current.onSetSetting(patch),
      // a sandboxed frame cannot use speech recognition, so the app runs it
      // on the view's behalf and shows that it is listening
      dictation: canDictate
        ? (listener) =>
            listen((heard) => {
              if ("done" in heard && heard.error) latest.current.onDictationError?.(heard.error);
              listener(heard);
            })
        : undefined,
      onDictation: setDictating,
      // offered only to a plugin that declares the capability; every copy is
      // shown to the person, since the frame cannot prove a click asked for it
      clipboard: latest.current.canCopy
        ? async (content) => {
            if ("text" in content) {
              await copyText(content.text);
              latest.current.onCopied?.({ text: content.text });
            } else {
              await copyImage(content.image, content.media_type);
              latest.current.onCopied?.({ image: content.name });
            }
          }
        : undefined,
      label: setHandoverLabel,
      // one of the app's own keys, pressed inside the view: the app acts on
      // it as if pressed in its window; nothing else is accepted
      appKey: (message) => {
        const mod = !!message.metaKey || !!message.ctrlKey;
        if (!(mod ? VIEW_APP_MOD_KEYS : VIEW_APP_KEYS).includes(message.key)) return;
        window.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: message.key,
            code: typeof message.code === "string" ? message.code : "",
            metaKey: !!message.metaKey,
            ctrlKey: !!message.ctrlKey,
            shiftKey: !mod && !!message.shiftKey,
            bubbles: true,
            cancelable: true,
          }),
        );
      },
      // ⌘/Ctrl+Enter pressed inside the view: the hand-over, as from the app
      handOverKey: collect,
      onFound: setFound,
      // the view is listening and has its review: the loading cover goes
      onReady: () => setLoaded(true),
      onLeft: () => setLeft(true),
    });
    host.current = connection;

    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault();
        collect();
      }
    };
    window.addEventListener("keydown", onKey);

    const onAppearance = () => connection.appearance(currentTheme(), currentTokens());
    window.addEventListener("pinrail:appearance", onAppearance);

    el.src = `${src}${themeFragment(currentTheme(), currentTokens())}`;

    return () => {
      connection.disconnect();
      if (host.current === connection) host.current = null;
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:appearance", onAppearance);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the draft helpers read the review from latest
  }, [frame, reviewId, src, reloads, collect]);

  // The plugin's settings changed, in Settings or through the view itself:
  // the view hears the values as they stand now.
  const sentSettings = useRef<string | null>(null);
  useEffect(() => {
    const now = settings ? JSON.stringify(settings) : null;
    const changed = now !== null && now !== sentSettings.current;
    sentSettings.current = now;
    if (changed && settings && host.current?.ready) host.current.settings(settings);
  }, [settings]);

  // When the review settles from elsewhere, a withdrawal or an expiry, the
  // view is sent init again, read-only; the host knows whether the view
  // handed it over itself, and was told so.
  useEffect(() => {
    host.current?.changed();
  }, [readonly]);

  const stopDictation = useCallback(() => host.current?.stopDictation(), []);

  return { loaded, submitting, handoverLabel, collect, left, reload, reloads, find, found, dictating, stopDictation };
}
