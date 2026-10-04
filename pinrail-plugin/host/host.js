// The app's side of the plugin protocol, once, for every host of a view:
// the desktop app, the core's preview page, the development shell and the
// test harness. Each gives it callbacks for what only it can do (keep a
// draft, hand a decision over, fetch a file, open a link); the messages,
// their order and their guards are the same everywhere.
//
// Every message is {pinrail: 1, type, ...}. The view's frame has an opaque
// origin, so messages to it use "*", and a host trusts a message only when
// it comes from the frame's own window (`connectFrame` checks that).
//
// `createHost` holds the logic and touches no DOM, so it runs in Node for
// tests; `connectFrame` wires it to an <iframe>.

export const PROTOCOL = 1;

/** What a host can do for a view beyond protocol 1's first messages. */
export const CAPABILITIES = ["attachments"];

/** How long a view has to answer a request for its decision. */
export const COLLECT_TIMEOUT_MS = 60_000;

const isObject = (v) => v !== null && typeof v === "object" && !Array.isArray(v);

/** A decision as a view sees it: what was decided, by whom, and when. */
export function viewDecision(decision) {
  if (!isObject(decision)) return null;
  return { data: decision.data, decided_by: decision.decided_by ?? null, decided_at: decision.decided_at ?? null };
}

/**
 * A review as a view receives it, built from the API's: the fields protocol
 * 1 promises a view, and no others, so the API's review can change without
 * breaking a view that is stored with a review. A field can be added here
 * later; none can be taken away.
 */
export function viewReview(review) {
  if (!isObject(review)) return null;
  return {
    id: review.id,
    title: review.title,
    status: review.status,
    created_at: review.created_at ?? null,
    payload: review.payload ?? null,
    attachments: (review.attachments || []).map((a) => ({
      name: a.name,
      size: a.size,
      media_type: a.media_type,
      sha256: a.sha256,
    })),
    decision: viewDecision(review.decision),
  };
}

/**
 * The app's side of one view, from its first `ready` until the host lets it
 * go. A host makes a new one, or calls `reload()`, each time it loads the
 * view's page again.
 */
export function createHost(options) {
  const {
    post,
    origin,
    review,
    previous = () => null,
    readonly = () => false,
    settings = () => ({}),
    theme,
    loadDraft = () => null,
    saveDraft,
    handOver,
    file,
    open,
    setSettings,
    label,
    resize,
    appKey,
    handOverKey,
    onDefer,
    onReady,
    onLeft,
    observe,
    capabilities = CAPABILITIES,
    collectTimeout = COLLECT_TIMEOUT_MS,
    setTimer = (fn, ms) => setTimeout(fn, ms),
    clearTimer = (timer) => clearTimeout(timer),
  } = options;

  let ready = false;
  // The frame loads the view's page once. A later load, or a second
  // `ready`, is another page in its place: it gets nothing from here on,
  // since the frame's window is the same and its messages look the same.
  let left = false;
  let loads = 0;
  let disposed = false;
  // The one request for the decision that is open: its number, and the
  // timer that closes it. Only an answer with this number counts, once.
  let request = 0;
  let asking = null;
  let deadline = null;
  // set before the decision goes out, so a second submit posted in the same
  // moment finds it
  let handingOver = false;
  let handedOver = false;
  let wasReadonly = readonly();

  const send = (msg, transfer = []) => {
    if (disposed || left) return;
    const message = { pinrail: PROTOCOL, ...msg };
    if (observe) observe("out", message);
    post(message, transfer);
  };

  /** Closes the open request: an answer to it, from now on, counts for
   *  nothing. */
  function close() {
    asking = null;
    if (deadline !== null) clearTimer(deadline);
    deadline = null;
  }

  /** Asks the view for its decision; false when there is nothing to ask. */
  function collect() {
    if (!ready || readonly() || asking !== null || handingOver) return false;
    request += 1;
    const req = request;
    asking = req;
    deadline = setTimer(() => {
      if (asking === req) close();
    }, collectTimeout);
    send({ type: "collect", req });
    return true;
  }

  function leave() {
    if (left) return;
    left = true;
    ready = false;
    close();
    if (onLeft) onLeft();
  }

  /** The view's review, and what it needs to show it: sent on `ready`, and
   *  again when a host asks, such as when the review becomes read-only. */
  function init() {
    const current = review();
    if (!ready || !current) return;
    if (theme) send({ type: "appearance", theme: theme() });
    const locked = readonly();
    send({
      type: "init",
      review: viewReview(current),
      previous: viewReview(previous()),
      readonly: locked,
      draft: locked ? null : loadDraft(),
      settings: settings() ?? {},
      shell_origin: origin,
      capabilities: typeof capabilities === "function" ? capabilities() : capabilities,
    });
  }

  async function answerAttachment(msg) {
    const { req, name, round } = msg;
    if (typeof req !== "number" || typeof name !== "string") return;
    const earlier = round === "previous";
    const from = earlier ? previous() : review();
    const listed = from && (from.attachments || []).find((a) => a.name === name);
    const fail = (error) => send({ type: "attachment", req, ok: false, name, error });
    if (!listed) return fail(`no attachment "${name}" on this ${earlier ? "previous round" : "review"}`);
    if (!file) return fail(`could not fetch ${name}`);
    try {
      const bytes = await file(from, listed, earlier ? "previous" : "current");
      // the fetch took time: the frame may show another review by now
      const now = earlier ? previous() : review();
      if (!now || now.id !== from.id) return;
      // a copy, since a transferred buffer is gone from the sender
      const copy = bytes.slice(0);
      send({ type: "attachment", req, ok: true, name, media_type: listed.media_type, size: listed.size, bytes: copy }, [
        copy,
      ]);
    } catch (error) {
      fail(error instanceof Error ? error.message : `could not fetch ${name}`);
    }
  }

  async function answerSubmit(msg) {
    // only the answer to the request that is open: one the app did not ask
    // for, or that came too late, decides nothing
    if (asking === null || msg.req !== asking) return;
    close();
    const data = msg.data;
    if (!handOver || readonly() || handingOver) return;
    handingOver = true;
    let result;
    try {
      result = await handOver(data);
    } catch (error) {
      result = {
        ok: false,
        violations: [{ path: "", message: error instanceof Error ? error.message : String(error) }],
      };
    } finally {
      handingOver = false;
    }
    if (!result) return;
    if (result.ok) {
      handedOver = true;
      send({ type: "submitted", decision: viewDecision(result.decision) });
    } else {
      send({ type: "violations", errors: result.violations || [] });
    }
  }

  /* A change to the plugin's own settings, by request number: the answer
     says whether it was kept, and with the errors when it was not. Everyone
     hears the values as they now stand as `settings`, from the host. */
  async function answerSettings(msg) {
    const { req, patch } = msg;
    if (typeof req !== "number") return;
    const refuse = (errors) => send({ type: "settings", req, ok: false, errors });
    if (!isObject(patch)) return refuse([{ path: "", message: "a settings change is an object of settings" }]);
    if (!setSettings) return refuse([{ path: "", message: "this host keeps no settings" }]);
    let violations;
    try {
      violations = await setSettings(patch);
    } catch (error) {
      violations = [{ path: "", message: error instanceof Error ? error.message : String(error) }];
    }
    if (violations && violations.length) refuse(violations);
    else send({ type: "settings", req, ok: true });
  }

  /** A message from the view's window. */
  function receive(msg) {
    if (disposed || left || !isObject(msg) || msg.pinrail !== PROTOCOL) return;
    if (observe) observe("in", msg);
    switch (msg.type) {
      case "ready":
        // a page says ready once: a second one is another page, which
        // speaks before its load event would give it away
        if (ready) return leave();
        ready = true;
        init();
        if (onReady) onReady();
        break;
      case "resize":
        if (!resize) break;
        if (msg.height === "fill") resize("fill");
        else if (typeof msg.height === "number" && Number.isFinite(msg.height)) resize(Math.ceil(msg.height));
        break;
      case "draft":
        if (saveDraft && !readonly()) saveDraft(msg.data);
        break;
      case "status":
        if (label && typeof msg.label === "string" && msg.label.trim()) label(msg.label);
        break;
      case "open":
        // the view's frame cannot open anything itself; the host decides
        // whether a link opens
        if (open && typeof msg.url === "string") open(msg.url);
        break;
      case "attachment":
        void answerAttachment(msg);
        break;
      case "key":
        if (typeof msg.key !== "string") break;
        // ⌘/Ctrl+Enter inside the view: the hand-over, which the app starts
        if (msg.key === "Enter" && (msg.metaKey || msg.ctrlKey)) {
          if (handOverKey) handOverKey();
          else collect();
        } else if (appKey) appKey(msg);
        break;
      case "settings_set":
        void answerSettings(msg);
        break;
      case "submit":
        void answerSubmit(msg);
        break;
      case "defer":
        // the view has nothing to hand over for this request yet
        if (asking !== null && msg.req === asking) {
          close();
          if (onDefer) onDefer();
        }
        break;
    }
  }

  return {
    receive,
    init,
    /** Asks the view for its decision, as the hand-over button does; false
     *  when there is nothing to ask, such as while a request is open. */
    collect,
    appearance(value) {
      if (ready) send({ type: "appearance", theme: value });
    },
    /** The plugin's settings changed elsewhere: the values as they stand. */
    settings(values) {
      if (ready) send({ type: "settings", settings: values });
    },
    /** A declared shortcut pressed while the host had the focus. */
    key(fields) {
      if (ready) send({ type: "key", ...fields });
    },
    /** Any message, for a host that drives the view by hand. */
    send,
    /** The review or its state changed. A review that became read-only from
     *  elsewhere is sent init again; one the view handed over was told so. */
    changed() {
      const now = readonly();
      if (now) close();
      if (now && !wasReadonly && !handedOver) init();
      wasReadonly = now;
    },
    /** The frame finished loading a page: the first is the view's own. */
    loaded() {
      loads += 1;
      if (loads > 1) leave();
    },
    /** The host loads the view's page again: what follows is the view anew. */
    reload() {
      close();
      ready = false;
      left = false;
      loads = 0;
      handedOver = false;
    },
    dispose() {
      close();
      disposed = true;
    },
    get ready() {
      return ready;
    },
    get left() {
      return left;
    },
    get handingOver() {
      return handingOver;
    },
    /** a request for the decision is open */
    get collecting() {
      return asking !== null;
    },
  };
}

/**
 * A host for the view in `frame`: messages from its window reach the host,
 * the host's messages reach it, and a second page load counts as leaving.
 * `disconnect()` removes the listeners.
 */
export function connectFrame(frame, options) {
  const host = createHost({
    ...options,
    post: (message, transfer) => {
      if (frame.contentWindow) frame.contentWindow.postMessage(message, "*", transfer);
    },
  });
  const onMessage = (event) => {
    if (event.source === frame.contentWindow) host.receive(event.data);
  };
  const onLoad = () => host.loaded();
  window.addEventListener("message", onMessage);
  frame.addEventListener("load", onLoad);
  host.disconnect = () => {
    window.removeEventListener("message", onMessage);
    frame.removeEventListener("load", onLoad);
    host.dispose();
  };
  return host;
}
