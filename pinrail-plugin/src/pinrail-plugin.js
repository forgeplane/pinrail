/*
 * pinrail plugin SDK, protocol 1.
 *
 * Served by the app at /sdk/v1/pinrail-plugin.js. A plugin loads it with one
 * script tag and calls Pinrail.connect(handlers); everything the protocol
 * requires (ready, origin pinning, resize, drafts, the hand-over, submitted,
 * violations and the Cmd/Ctrl+Enter shortcut) is handled here.
 *
 *   const plugin = Pinrail.connect({
 *     resize: "auto",                 // "auto" (content height), "fill" (viewport), or "manual"
 *     onInit({ review, previous, readonly, draft, settings }) { … },
 *     onCollect() { return decision }, // the app's hand-over button, or Cmd/Ctrl+Enter:
 *                                      // the decision, a promise of it, or nothing to hand over yet
 *     onViolations(errors) { … },     // [{ path, message }]
 *     onSubmitted(decision) { … },    // the decision was accepted; render read-only
 *     onAppearance(theme) { … },      // optional; "dark" | "light", already applied
 *     onSettings(settings) { … },     // optional; the plugin's own settings changed
 *     onError(error) { … },           // optional; a handler threw, or a decision is not JSON
 *   });
 *
 * Load this with a plain <script src> tag, not a deferred or module one: it
 * reads the theme off the frame's URL and sets data-theme on the document, so
 * the view is in the shell's theme from the frame it first paints.
 *   plugin.draft(data);               // debounced; { flush: true } posts at once
 *   plugin.status({ label: "…" });    // what the shell's hand-over button should read
 *   plugin.open("https://example.com"); // the app asks the person, then opens it in the browser
 *   plugin.settings;                  // the plugin's own settings, as the manifest declares them
 *   plugin.setSetting("diff", "split"); // asks the shell to keep one; it comes back as `settings`
 *
 * Pinrail.icon("check") returns the plugin's own icons/check.svg, beside the
 * view, as markup that takes the colour of the text around it:
 *
 *   `<button class="btn">${Pinrail.icon("check")} Accept</button>`
 *
 * Pinrail.layout() builds the standard skeleton that goes with the SDK's
 * stylesheet: a header that stays put and a body that scrolls.
 *
 *   const view = Pinrail.layout({ title: "5 items", controls: [button] });
 *   view.content.innerHTML = …          // render into this
 *   view.title("4 items").meta(["acme-api", "7 days"]);
 *
 * The same code runs in Node for the package's tests, with an environment of
 * their own.
 */
(function (root) {
  "use strict";

  const PROTOCOL = 1;
  /** The key of a client's teardown, for tests. */
  const TEARDOWN = Symbol.for("pinrail.teardown");
  const THEMES = ["dark", "light"];
  const DRAFT_DEBOUNCE_MS = 150;
  /** What `asJson` answers for data that cannot be sent. */
  const NOT_JSON = {};

  function escape(s) {
    return String(s ?? "").replace(
      /[&<>"']/g,
      (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
    );
  }

  /* Markdown is rendered by markdown-it, which the app prepends to this file:
     the script a view loads carries its parser, so Pinrail.markdown and
     Pinrail.markdownInline render CommonMark — headings, tables, blockquotes,
     nested lists and the rest — from the view's first line onwards.

     What the renderer promises is that the HTML is its own: raw HTML in the
     source is escaped rather than passed through, which matters because a
     view's frame runs inline scripts, so markup that reached the DOM would
     run. Addresses are checked too: a link to anything but http, https or
     mailto keeps its text and loses its address. Styling stays the view's:
     the renderer writes plain elements and no classes. */
  const SAFE_HREF = /^(https?:|mailto:)/i;

  function configure(markdownit) {
    const parser = markdownit({
      // the default, and the reason no sanitiser is needed: raw HTML is escaped
      html: false,
      linkify: true,
      breaks: false,
      typographer: false,
    });
    const link = parser.renderer.rules.link_open || ((t, i, o, e, self) => self.renderToken(t, i, o));
    parser.renderer.rules.link_open = (tokens, i, options, env, self) => {
      if (!SAFE_HREF.test(tokens[i].attrGet("href") || "")) tokens[i].attrSet("href", "#");
      tokens[i].attrSet("rel", "noreferrer");
      tokens[i].attrSet("target", "_blank");
      return link(tokens, i, options, env, self);
    };
    return parser;
  }

  // The parser is a global by the time this runs, because it is prepended to
  // this file; the source half of it on its own renders nothing.
  const md = root.markdownit ? configure(root.markdownit) : null;

  function render(method, src) {
    if (!md) throw new Error("Pinrail.markdown needs the parser the app serves with the SDK");
    return src == null ? "" : md[method](String(src));
  }

  const markdown = (src) => render("render", src);
  const markdownInline = (src) => render("renderInline", src);

  /* A file a review carries is named in its payload as
     { "$attachment": "pivot.glb" }. attachmentName reads the name back out,
     or gives null for anything else. The package's
     schemas/attachment.schema.json describes the object for a payload
     schema. */
  function attachmentName(ref) {
    return ref && typeof ref === "object" && typeof ref.$attachment === "string" ? ref.$attachment : null;
  }

  // The skeleton a view built with layout(), so auto sizing can measure the
  // body rather than the document, which no longer scrolls.
  let skeleton = null;

  function createPlugin(env, handlers) {
    handlers = handlers || {};
    const resizeMode = handlers.resize || "auto";
    const state = {
      shellOrigin: null,
      review: null,
      previous: null,
      readonly: false,
      theme: (env.initialTheme && env.initialTheme()) || "dark",
      settings: {},
      capabilities: [],
    };
    // file requests waiting on the shell, by request number
    const asked = new Map();
    let nextAsk = 1;
    let draftTimer = null;
    let stopObserving = null;

    const post = (msg) => env.post(Object.assign({ pinrail: PROTOCOL }, msg), state.shellOrigin || "*");
    /* An error of the view's own code: its handler, or none, the console. */
    const report = (error) => {
      if (handlers.onError) handlers.onError(error);
      else if (env.logError) env.logError(error);
    };
    /* Runs one of the view's handlers. One that throws is reported, and the
       client carries on with its own work. */
    const call = (name, ...args) => {
      if (!handlers[name]) return;
      try {
        handlers[name](...args);
      } catch (error) {
        report(error);
      }
    };
    // what the client set up, undone by the teardown tests use
    const undo = [];
    const keep = (stop) => {
      if (typeof stop === "function") undo.push(stop);
    };
    /* Decisions and drafts reach the app as JSON, so they are sent as the
       plain data they hold, whatever a framework keeps them in: a Vue
       reactive object or a Svelte $state proxy cannot be posted as it is.
       What JSON cannot hold, such as a cycle, is refused as the app would
       refuse it, through onViolations. */
    const asJson = (data, what) => {
      if (data === undefined) return undefined;
      try {
        return JSON.parse(JSON.stringify(data));
      } catch (error) {
        if (handlers.onViolations)
          handlers.onViolations([{ path: "", message: `the ${what} is not JSON: ${error.message}` }]);
        return NOT_JSON;
      }
    };

    function startResize() {
      if (resizeMode === "fill") {
        post({ type: "resize", height: "fill" });
      } else if (resizeMode === "auto" && !stopObserving && env.observeSize) {
        stopObserving = env.observeSize((height) => post({ type: "resize", height }));
        keep(stopObserving);
      }
    }

    /* Where a link goes is the shell's to open — in the system browser, not
       in the panel. Anything but http, https or mailto is not a link a view
       may send anyone to, and is dropped here rather than posted. */
    function open(url) {
      if (SAFE_HREF.test(url)) post({ type: "open", url: String(url) });
    }

    /* The app asks for the decision, by a request number. The view's
       onCollect returns it, or a promise of it, and the answer goes back as
       `submit` with that number; nothing returned is `defer`, the view's
       "not yet": a missing answer, or a preview to confirm first. A handler
       that throws, or a decision JSON cannot hold, is reported through
       onError and hands nothing over. */
    async function collect(req) {
      const defer = () => post({ type: "defer", req });
      if (state.readonly || !handlers.onCollect) return defer();
      let value;
      try {
        value = await handlers.onCollect();
      } catch (error) {
        report(error);
        return defer();
      }
      if (value === undefined || value === null) return defer();
      let data;
      try {
        data = JSON.parse(JSON.stringify(value));
      } catch (error) {
        report(new Error(`the decision is not JSON: ${error.message}`));
        return defer();
      }
      // a draft still waiting is for a decision that is now being made
      env.clearTimeout(draftTimer);
      post({ type: "submit", req, data });
    }

    // The plugin's own settings, as the manifest declares them and the
    // person set them; anything but an object means none.
    const settingsOf = (value) => (value && typeof value === "object" && !Array.isArray(value) ? value : {});

    function handle(data, origin) {
      if (!data || data.pinrail !== PROTOCOL || typeof data.type !== "string") return;
      if (state.shellOrigin && origin !== state.shellOrigin) return;
      switch (data.type) {
        case "init":
          // the origin the browser vouches for; the one the message names
          // must agree with it
          if (data.shell_origin && origin && data.shell_origin !== origin) return;
          if (!state.shellOrigin) state.shellOrigin = origin || data.shell_origin || null;
          state.review = data.review;
          state.previous = data.previous || null;
          state.readonly = !!data.readonly;
          state.settings = settingsOf(data.settings);
          state.capabilities = Array.isArray(data.capabilities) ? data.capabilities : [];
          // every JSON value a view kept comes back as it was, false and 0 too
          call("onInit", {
            review: state.review,
            previous: state.previous,
            readonly: state.readonly,
            draft: data.draft === undefined ? null : data.draft,
            settings: state.settings,
          });
          startResize();
          break;
        case "settings":
          // A change in Settings, or the answer to setSetting: the values
          // as they stand now, every key the manifest declares.
          state.settings = settingsOf(data.settings);
          call("onSettings", state.settings);
          break;
        case "violations":
          call("onViolations", Array.isArray(data.errors) ? data.errors : []);
          break;
        case "submitted":
          state.readonly = true;
          // a new object, so the one the view holds is never changed under it
          if (state.review)
            state.review = Object.assign({}, state.review, { decision: data.decision || null, status: "decided" });
          call("onSubmitted", data.decision || null);
          break;
        case "appearance":
          // The shell owns the theme; the plugin follows it. `data-theme` on
          // the root is set here, so a view only needs the CSS for it.
          if (THEMES.includes(data.theme)) {
            state.theme = data.theme;
            if (env.applyTheme) env.applyTheme(data.theme);
            call("onAppearance", data.theme);
          }
          break;
        case "collect":
          if (typeof data.req === "number") void collect(data.req);
          break;
        case "attachment": {
          // the shell's answer to attachment(): the bytes, transferred, or why not
          const waiting = asked.get(data.req);
          if (!waiting) break;
          asked.delete(data.req);
          if (data.ok && data.bytes instanceof ArrayBuffer) waiting.resolve(data.bytes);
          else
            waiting.reject(
              new Error(typeof data.error === "string" ? data.error : `the shell could not hand over ${waiting.name}`),
            );
          break;
        }
        case "key":
          // One of the manifest's shortcuts, pressed while the app rather
          // than the frame had focus. It lands as a keydown on the document,
          // marked pinrailForwarded, so the listener a view already has for
          // its keys handles both.
          if (typeof data.key === "string" && env.dispatchKey)
            env.dispatchKey({
              key: data.key,
              code: typeof data.code === "string" ? data.code : "",
              metaKey: !!data.metaKey,
              ctrlKey: !!data.ctrlKey,
              altKey: !!data.altKey,
              shiftKey: !!data.shiftKey,
            });
          break;
      }
    }

    keep(env.listen(handle));
    if (env.onLink) keep(env.onLink(open));
    // the app's own keys on the review screen reach it from inside the view
    // too, ⌘/Ctrl+Enter among them: the app starts the hand-over
    if (env.onAppKey) keep(env.onAppKey((key) => post(Object.assign({ type: "key" }, key))));
    keep(() => env.clearTimeout(draftTimer));
    post({ type: "ready" });

    /* The bytes of a file the review carries, from the shell: a view's
       frame can fetch nothing, so it asks, and the shell answers with the
       bytes and nothing else. `round: "previous"` asks for a file of the
       round this one revises. */
    function attachment(name, opts) {
      const round = opts && opts.round === "previous" ? "previous" : "current";
      const review = round === "previous" ? state.previous : state.review;
      if (!state.capabilities.includes("attachments")) {
        return Promise.reject(new Error("this version of Pinrail cannot hand files to a view; update the app"));
      }
      const listed = review && Array.isArray(review.attachments) ? review.attachments : [];
      if (!listed.some((a) => a && a.name === name)) {
        return Promise.reject(
          new Error(`no attachment "${name}" on this ${round === "previous" ? "previous round" : "review"}`),
        );
      }
      return new Promise((resolve, reject) => {
        const req = nextAsk++;
        asked.set(req, { resolve, reject, name });
        post(Object.assign({ type: "attachment", req, name }, round === "previous" ? { round } : {}));
      });
    }

    /* The same file as a blob: URL, for an <img>, <video> or <audio>, which
       the frame's policy lets load blob: and nothing remote. The type is the
       one the review lists unless given; revoke the URL when done. */
    async function attachmentUrl(name, opts) {
      const bytes = await attachment(name, opts);
      const review = opts && opts.round === "previous" ? state.previous : state.review;
      const listed = (review.attachments || []).find((a) => a.name === name);
      const type = (opts && opts.type) || (listed && listed.media_type) || "application/octet-stream";
      if (!env.objectUrl) throw new Error("Pinrail: attachmentUrl needs a browser");
      return env.objectUrl(bytes, type);
    }

    return {
      get review() {
        return state.review;
      },
      /** the files the review carries: { name, size, media_type, sha256 } each */
      get attachments() {
        return (state.review && state.review.attachments) || [];
      },
      attachment,
      attachmentUrl,
      get previous() {
        return state.previous;
      },
      get readonly() {
        return state.readonly;
      },
      get theme() {
        return state.theme;
      },
      get settings() {
        return state.settings;
      },
      /* Asks the shell to keep a setting of this plugin's; the shell checks
         it against the manifest and answers with `settings` (or with
         `violations` when it will not have it). */
      setSetting(key, value) {
        post({ type: "settings_set", patch: { [key]: value } });
      },
      draft(data, opts) {
        if (state.readonly) return;
        env.clearTimeout(draftTimer);
        const value = asJson(data, "draft");
        if (value === NOT_JSON) return;
        const send = () => {
          if (!state.readonly) post({ type: "draft", data: value });
        };
        if (opts && opts.flush) send();
        else draftTimer = env.setTimeout(send, DRAFT_DEBOUNCE_MS);
      },
      /** asks the app to open a link in the system browser, as a click on one in the view does; the app asks the person first unless they allowed the site */
      open,
      resize(height) {
        post({ type: "resize", height });
      },
      status(status) {
        post({ type: "status", label: (status || {}).label });
      },
      /* Undoes what the client set up: its listeners, its timer and the
         observer. For tests, which make a client per case; a view keeps its
         connection for as long as its page lives. */
      [TEARDOWN]() {
        for (const stop of undo.splice(0)) stop();
      },
    };
  }

  /*
   * The skeleton the stylesheet expects. Returns the elements rather than
   * markup, so a view can rewrite its body on every render while the header
   * and its controls stay put, with their listeners attached.
   *
   * Ask for a header by passing any of title, meta or controls (or
   * `header: true`); without them you get a content element and nothing else.
   */
  function layout(options) {
    options = options || {};
    const doc = options.document || (typeof document === "undefined" ? null : document);
    if (!doc) throw new Error("Pinrail.layout needs a document");

    const make = (tag, className) => {
      const node = doc.createElement(tag);
      node.className = className;
      return node;
    };

    const fill = (node, value) => {
      const items = value == null ? [] : [].concat(value);
      node.replaceChildren();
      for (const item of items) {
        if (typeof item === "string") {
          const span = doc.createElement("span");
          span.textContent = item;
          node.append(span);
        } else if (item) {
          node.append(item);
        }
      }
    };

    const into = options.into || doc.body;
    into.className = into.className ? into.className + " plugin-layout" : "plugin-layout";

    const wantsHeader =
      options.header === true || options.title != null || options.meta != null || options.controls != null;

    let header = null;
    let titleNode = null;
    let metaNode = null;
    let controlsNode = null;

    if (wantsHeader) {
      header = make("header", "plugin-header");
      titleNode = make("span", "plugin-title");
      metaNode = make("span", "plugin-meta");
      controlsNode = make("span", "plugin-controls");
      header.append(titleNode, metaNode, controlsNode);
      into.append(header);
    }

    // The body scrolls, not the document, so a heading inside it can pin to
    // the top of the scroll without having to know the header's height.
    const scroll = make("div", "plugin-scroll");
    const content = make("div", "plugin-content");
    scroll.append(content);
    into.append(scroll);

    const view = {
      header,
      scroll,
      content,
      title(value) {
        if (titleNode) titleNode.textContent = value == null ? "" : String(value);
        return view;
      },
      meta(value) {
        if (metaNode) fill(metaNode, value);
        return view;
      },
      controls(value) {
        if (controlsNode) fill(controlsNode, value);
        return view;
      },
    };

    if (options.title != null) view.title(options.title);
    if (options.meta != null) view.meta(options.meta);
    if (options.controls != null) view.controls(options.controls);

    skeleton = view;
    return view;
  }

  /* The shell puts the theme it is in on the frame's URL, so a view is in that
     theme from the frame it first paints. The `appearance` message cannot
     arrive that early: it crosses documents, and the view has painted by the
     time it lands. `appearance` still governs every later change. */
  function themeFromUrl(win) {
    const found = /(?:^|[#&])pinrail-theme=([a-z]+)/.exec((win.location && win.location.hash) || "");
    return found && THEMES.includes(found[1]) ? found[1] : null;
  }

  /* One of the plugin's own icons, icons/<name>.svg beside the view, as
     markup, so a view that builds HTML strings can drop one in. The address is
     made whole here: a relative one inside a custom property may be read
     against the SDK's stylesheet instead. The name is reduced to the
     characters an icon file can have: a view may take it from a review payload,
     and a payload is not ours to trust. A name with no file behind it renders
     as nothing, with the name left on the element to find it by.

     Decorative by default; pass a label and it becomes an image with a name. */
  function icon(name, options) {
    options = options || {};
    const safe = String(name == null ? "" : name)
      .toLowerCase()
      .replace(/[^a-z0-9-]/g, "");
    // a number of pixels, or a length such as "1.25em"; anything else is ignored
    const length =
      typeof options.size === "number" && Number.isFinite(options.size)
        ? options.size + "px"
        : typeof options.size === "string" && /^[0-9.]+(px|em|rem|%)$/.test(options.size)
          ? options.size
          : null;
    const size = length ? `--wi-size:${length};` : "";
    const extra = options.class ? " " + escape(options.class) : "";
    const described = options.label ? ` role="img" aria-label="${escape(options.label)}"` : ' aria-hidden="true"';
    const base = typeof document === "undefined" ? "http://plugin.invalid/view/" : document.baseURI;
    const url = new URL(`icons/${safe}.svg`, base).href;
    return `<span class="wi${extra}" data-icon="${safe}" style="--wi:url(&quot;${escape(url)}&quot;);${size}"${described}></span>`;
  }

  function browserEnv(win) {
    const doc = win.document;
    return {
      initialTheme: () => themeFromUrl(win),
      post: (msg, targetOrigin) => win.parent.postMessage(msg, targetOrigin),
      // only the frame's parent is the shell: another frame on the page
      // may post to this one too
      listen: (fn) => {
        const listener = (e) => {
          if (e.source === win.parent) fn(e.data, e.origin);
        };
        win.addEventListener("message", listener);
        return () => win.removeEventListener("message", listener);
      },
      setTimeout: (fn, ms) => win.setTimeout(fn, ms),
      clearTimeout: (t) => win.clearTimeout(t),
      observeSize: (cb) => {
        // With a skeleton the document does not scroll, so its height says
        // nothing; measure the header and the body instead.
        const emit = () =>
          cb(
            skeleton
              ? (skeleton.header ? skeleton.header.offsetHeight : 0) + skeleton.content.scrollHeight
              : doc.documentElement.scrollHeight,
          );
        const ro = new win.ResizeObserver(emit);
        ro.observe(skeleton ? skeleton.content : doc.body);
        emit();
        return () => ro.disconnect();
      },
      applyTheme: (theme) => {
        doc.documentElement.dataset.theme = theme;
      },
      dispatchKey: (key) => {
        const event = new win.KeyboardEvent("keydown", Object.assign({ bubbles: true, cancelable: true }, key));
        // so a handler can tell a forwarded key from one typed in the frame
        Object.defineProperty(event, "pinrailForwarded", { value: true });
        doc.dispatchEvent(event);
      },
      // A view's frame is sandboxed without allow-popups, so a link in it
      // opens nothing on its own and navigating the frame away from the view
      // is not what a click means either. The shell opens it instead.
      onLink: (fn) => {
        const listener = (e) => {
          if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
          const anchor = e.target && e.target.closest ? e.target.closest("a[href]") : null;
          if (!anchor) return;
          // the address as written: the resolved one of "#" or a relative
          // link is the view's own, which is not a page to open
          const href = anchor.getAttribute("href");
          if (SAFE_HREF.test(href)) {
            e.preventDefault();
            fn(anchor.href);
          } else if (href === "#") {
            // a link the renderer emptied, or a control written as one: it
            // goes nowhere, so it should not jump the view to the top either
            e.preventDefault();
          }
          // anything else, a fragment into the view among it, behaves as written
        };
        doc.addEventListener("click", listener);
        return () => doc.removeEventListener("click", listener);
      },
      // ? for the keys, [ and ] for the rounds: the app's, so a press the
      // view left alone, outside a text field, goes up to it. ⌘/Ctrl+Enter
      // goes up from anywhere, a text field too: it is the hand-over.
      onAppKey: (fn) => {
        const listener = (e) => {
          if (e.defaultPrevented || e.pinrailForwarded) return;
          if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key === "Enter") {
            e.preventDefault();
            fn({
              key: "Enter",
              code: e.code || "Enter",
              metaKey: e.metaKey,
              ctrlKey: e.ctrlKey,
              altKey: false,
              shiftKey: false,
            });
            return;
          }
          if (e.metaKey || e.ctrlKey || e.altKey) return;
          if (!APP_KEYS.includes(e.key)) return;
          const el = e.target;
          if (el && el.closest && el.closest("input, textarea, select, [contenteditable]")) return;
          fn({ key: e.key, code: e.code || "", metaKey: false, ctrlKey: false, altKey: false, shiftKey: !!e.shiftKey });
        };
        win.addEventListener("keydown", listener);
        return () => win.removeEventListener("keydown", listener);
      },
      logError: (error) => win.console.error(error),
      objectUrl: (bytes, type) => win.URL.createObjectURL(new win.Blob([bytes], { type })),
    };
  }

  // the keys the app answers on the review screen, which a view passes up
  const APP_KEYS = ["?", "[", "]"];

  /* A document connects once. A second connect() is a mistake to catch
     where it happens: the app would take its second `ready` for another
     page in the view's place, and stop answering. Connect where the page
     starts, not in a component that can mount more than once. */
  let connected = false;
  function connect(handlers) {
    if (connected) {
      throw new Error(
        "Pinrail.connect was called twice in this document: connect once, where the page starts, and keep the plugin it returns",
      );
    }
    connected = true;
    return createPlugin(browserEnv(root), handlers);
  }

  const Pinrail = {
    protocol: PROTOCOL,
    connect,
    layout,
    icon,
    escape,
    markdown,
    markdownInline,
    attachmentName,
    // the client with an environment of the caller's, for the package's
    // own tests in Node; no view needs it
    [Symbol.for("pinrail.createPlugin")]: createPlugin,
  };

  // Before anything else this file does, and before the view's own script
  // runs: a plugin that loads the SDK with a plain <script> tag never paints
  // in the wrong theme.
  if (root.document && root.document.documentElement) {
    const initial = themeFromUrl(root);
    if (initial) root.document.documentElement.dataset.theme = initial;
  }

  root.Pinrail = Pinrail;
  if (typeof module !== "undefined" && module.exports) module.exports = Pinrail;
})(typeof window !== "undefined" ? window : globalThis);
