/*
 * pinrail plugin SDK, protocol 1.
 *
 * Served by the app at /sdk/v1/pinrail-plugin.js. A plugin loads it with one
 * script tag and calls Pinrail.connect(handlers); everything the protocol
 * requires (ready, origin pinning, resize, drafts, submitted, violations,
 * collect and the Cmd/Ctrl+Enter shortcut) is handled here.
 *
 *   const plugin = Pinrail.connect({
 *     resize: "auto",                 // "auto" (content height), "fill" (viewport), or "manual"
 *     onInit({ gate, previous, readonly, draft, settings }) { … },
 *     onViolations(errors) { … },     // [{ path, message }]
 *     onSubmitted(decision) { … },    // the decision was accepted; render read-only
 *     onCollect() { … },              // the shell's hand-over button, or Cmd/Ctrl+Enter
 *     onAppearance(theme) { … },      // optional; "dark" | "light", already applied
 *     onSettings(settings) { … },     // optional; the plugin's own settings changed
 *     onKey(key) { … },               // optional; a declared shortcut pressed while the shell had focus
 *   });
 *
 * Load this with a plain <script src> tag, not a deferred or module one: it
 * reads the theme off the frame's URL and sets data-theme on the document, so
 * the view is in the shell's theme from the frame it first paints.
 *   plugin.submit(data);
 *   plugin.draft(data);               // debounced; { flush: true } posts at once
 *   plugin.status({ label: "…" });    // what the shell's hand-over button should read
 *   plugin.open("https://example.com"); // the shell opens it in the system browser
 *   plugin.settings;                  // the plugin's own settings, as the manifest declares them
 *   plugin.setSetting("diff", "split"); // asks the shell to keep one; it comes back as `settings`
 *
 * Pinrail.icon("check") returns an icon from the set the app serves, as markup
 * that takes the colour of the text around it:
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
 * The same code runs in Node for tests through Pinrail.createPlugin(env, handlers).
 */
(function (root) {
  "use strict";

  const PROTOCOL = 1;
  const VERSION = "1.8.0";
  const THEMES = ["dark", "light"];
  const DRAFT_DEBOUNCE_MS = 150;
  const ICON_BASE = "/sdk/v1/icons/";

  function escape(s) {
    return String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
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
     { "$attachment": "pivot.glb" }. ATTACHMENT_SCHEMA is that object as JSON
     Schema, to paste into a payload schema's $defs; attachmentName reads the
     name back out, or gives null for anything else. */
  const ATTACHMENT_SCHEMA = Object.freeze({
    type: "object",
    additionalProperties: false,
    required: ["$attachment"],
    properties: { $attachment: { type: "string", minLength: 1, maxLength: 120 } },
    description: "A file sent beside the payload, by its name on the review.",
  });
  function attachmentName(ref) {
    return ref && typeof ref === "object" && typeof ref.$attachment === "string" ? ref.$attachment : null;
  }

  /* What the superseded round decided for an item id, for views whose
     decision has `decisions: [{id, action, note}]` and `undecided: [id]`. */
  function previousVerdict(previous, id) {
    const data = previous && previous.decision && previous.decision.data;
    if (!data) return null;
    const d = (data.decisions || []).find((x) => x.id === id);
    if (d) return { action: d.action, note: d.note || "" };
    if ((data.undecided || []).includes(id)) return { action: "undecided", note: "" };
    return null;
  }

  // The skeleton a view built with layout(), so auto sizing can measure the
  // body rather than the document, which no longer scrolls.
  let skeleton = null;

  function createPlugin(env, handlers) {
    handlers = handlers || {};
    const resizeMode = handlers.resize || "auto";
    const state = {
      shellOrigin: null,
      gate: null,
      previous: null,
      readonly: false,
      initialised: false,
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

    function startResize() {
      if (resizeMode === "fill") {
        post({ type: "resize", height: "fill" });
      } else if (resizeMode === "auto" && !stopObserving && env.observeSize) {
        stopObserving = env.observeSize((height) => post({ type: "resize", height }));
      }
    }

    /* Where a link goes is the shell's to open — in the system browser, not
       in the panel. Anything but http, https or mailto is not a link a view
       may send anyone to, and is dropped here rather than posted. */
    function open(url) {
      if (SAFE_HREF.test(url)) post({ type: "open", url: String(url) });
    }

    function collect() {
      if (state.readonly) return;
      if (handlers.onCollect) handlers.onCollect();
    }

    // The plugin's own settings, as the manifest declares them and the
    // person set them; anything but an object means none.
    const settingsOf = (value) => (value && typeof value === "object" && !Array.isArray(value) ? value : {});

    function handle(data, origin) {
      if (!data || data.pinrail !== PROTOCOL || typeof data.type !== "string") return;
      if (state.shellOrigin && origin !== state.shellOrigin) return;
      switch (data.type) {
        case "init":
          if (data.shell_origin) state.shellOrigin = data.shell_origin;
          state.gate = data.gate;
          state.previous = data.previous || null;
          state.readonly = !!data.readonly;
          state.settings = settingsOf(data.settings);
          state.capabilities = Array.isArray(data.capabilities) ? data.capabilities : [];
          state.initialised = true;
          if (handlers.onInit) handlers.onInit({ gate: state.gate, previous: state.previous, readonly: state.readonly, draft: data.draft || null, settings: state.settings });
          startResize();
          break;
        case "settings":
          // A change in Settings, or the answer to setSetting: the values
          // as they stand now, every key the manifest declares.
          state.settings = settingsOf(data.settings);
          if (handlers.onSettings) handlers.onSettings(state.settings);
          break;
        case "violations":
          if (handlers.onViolations) handlers.onViolations(Array.isArray(data.errors) ? data.errors : []);
          break;
        case "submitted":
          state.readonly = true;
          if (state.gate) { state.gate.decision = data.decision || null; state.gate.status = "decided"; }
          if (handlers.onSubmitted) handlers.onSubmitted(data.decision || null);
          break;
        case "appearance":
          // The shell owns the theme; the plugin follows it. `data-theme` on
          // the root is set here, so a view only needs the CSS for it.
          if (THEMES.includes(data.theme)) {
            state.theme = data.theme;
            if (env.applyTheme) env.applyTheme(data.theme);
            if (handlers.onAppearance) handlers.onAppearance(data.theme);
          }
          break;
        case "collect":
          collect();
          break;
        case "attachment": {
          // the shell's answer to attachment(): the bytes, transferred, or why not
          const waiting = asked.get(data.req);
          if (!waiting) break;
          asked.delete(data.req);
          if (data.ok && data.bytes instanceof ArrayBuffer) waiting.resolve(data.bytes);
          else waiting.reject(new Error(typeof data.error === "string" ? data.error : `the shell could not hand over ${waiting.name}`));
          break;
        }
        case "key":
          // One of the manifest's shortcuts, pressed while the shell rather
          // than the frame had focus. It lands as a keydown on the document,
          // so a view that already listens for its keys needs no change.
          if (typeof data.key === "string") {
            const key = {
              key: data.key, code: typeof data.code === "string" ? data.code : "",
              metaKey: !!data.metaKey, ctrlKey: !!data.ctrlKey, altKey: !!data.altKey, shiftKey: !!data.shiftKey,
            };
            if (env.dispatchKey) env.dispatchKey(key);
            if (handlers.onKey) handlers.onKey(key);
          }
          break;
      }
    }

    env.listen(handle);
    if (env.onLink) env.onLink(open);
    if (handlers.shortcut !== false && env.onShortcut) env.onShortcut(collect);
    // the app's own keys on the review screen reach it from inside the view too
    if (env.onAppKey) env.onAppKey((key) => post(Object.assign({ type: "key" }, key)));
    post({ type: "ready" });

    /* The bytes of a file the review carries, from the shell: a view's
       frame can fetch nothing, so it asks, and the shell answers with the
       bytes and nothing else. `round: "previous"` asks for a file of the
       round this one revises. */
    function attachment(name, opts) {
      const round = opts && opts.round === "previous" ? "previous" : "current";
      const gate = round === "previous" ? state.previous : state.gate;
      if (!state.capabilities.includes("attachments")) {
        return Promise.reject(new Error("this version of Pinrail cannot hand files to a view; update the app"));
      }
      const listed = gate && Array.isArray(gate.attachments) ? gate.attachments : [];
      if (!listed.some((a) => a && a.name === name)) {
        return Promise.reject(new Error(`no attachment "${name}" on this ${round === "previous" ? "previous round" : "review"}`));
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
      const gate = opts && opts.round === "previous" ? state.previous : state.gate;
      const listed = (gate.attachments || []).find((a) => a.name === name);
      const type = (opts && opts.type) || (listed && listed.media_type) || "application/octet-stream";
      if (!env.objectUrl) throw new Error("Pinrail: attachmentUrl needs a browser");
      return env.objectUrl(bytes, type);
    }

    return {
      get gate() { return state.gate; },
      /** the files the review carries: { name, size, media_type, sha256 } each */
      get attachments() { return (state.gate && state.gate.attachments) || []; },
      attachment,
      attachmentUrl,
      get previous() { return state.previous; },
      get readonly() { return state.readonly; },
      get shellOrigin() { return state.shellOrigin; },
      get initialised() { return state.initialised; },
      get theme() { return state.theme; },
      get settings() { return state.settings; },
      submit(data) { post({ type: "submit", data }); },
      /* Asks the shell to keep a setting of this plugin's; the shell checks
         it against the manifest and answers with `settings` (or with
         `violations` when it will not have it). */
      setSetting(key, value) { post({ type: "settings_set", patch: { [key]: value } }); },
      draft(data, opts) {
        if (state.readonly) return;
        env.clearTimeout(draftTimer);
        const send = () => post({ type: "draft", data });
        if (opts && opts.flush) send();
        else draftTimer = env.setTimeout(send, DRAFT_DEBOUNCE_MS);
      },
      /** opens a link in the system browser, as a click on one in the view does */
      open,
      resize(height) { post({ type: "resize", height }); },
      status(status) { post({ type: "status", label: (status || {}).label }); },
      collect,
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

  /* An icon from the set the app serves, as markup, so a view that builds
     HTML strings can drop one in. The name is reduced to the characters an
     icon file can have: a view may take it from a gate payload, and a payload
     is not ours to trust. A name with no file behind it renders as nothing,
     with the name left on the element to find it by.

     Decorative by default; pass a label and it becomes an image with a name. */
  function icon(name, options) {
    options = options || {};
    const safe = String(name == null ? "" : name).toLowerCase().replace(/[^a-z0-9-]/g, "");
    const size = options.size == null ? "" : `--wi-size:${typeof options.size === "number" ? options.size + "px" : options.size};`;
    const extra = options.class ? " " + escape(options.class) : "";
    const described = options.label
      ? ` role="img" aria-label="${escape(options.label)}"`
      : ' aria-hidden="true"';
    return `<span class="wi${extra}" data-icon="${safe}" style="--wi:url(${ICON_BASE}${safe}.svg);${size}"${described}></span>`;
  }

  function browserEnv(win) {
    const doc = win.document;
    return {
      initialTheme: () => themeFromUrl(win),
      post: (msg, targetOrigin) => win.parent.postMessage(msg, targetOrigin),
      listen: (fn) => win.addEventListener("message", (e) => fn(e.data, e.origin)),
      setTimeout: (fn, ms) => win.setTimeout(fn, ms),
      clearTimeout: (t) => win.clearTimeout(t),
      observeSize: (cb) => {
        // With a skeleton the document does not scroll, so its height says
        // nothing; measure the header and the body instead.
        const emit = () =>
          cb(
            skeleton
              ? (skeleton.header ? skeleton.header.offsetHeight : 0) + skeleton.content.scrollHeight
              : doc.documentElement.scrollHeight
          );
        const ro = new win.ResizeObserver(emit);
        ro.observe(skeleton ? skeleton.content : doc.body);
        emit();
        return () => ro.disconnect();
      },
      applyTheme: (theme) => { doc.documentElement.dataset.theme = theme; },
      dispatchKey: (key) => {
        const event = new win.KeyboardEvent("keydown", Object.assign({ bubbles: true, cancelable: true }, key));
        // so a handler can tell a forwarded key from one typed in the frame
        Object.defineProperty(event, "pinrailForwarded", { value: true });
        doc.dispatchEvent(event);
      },
      // A view's frame is sandboxed without allow-popups, so a link in it
      // opens nothing on its own and navigating the frame away from the view
      // is not what a click means either. The shell opens it instead.
      onLink: (fn) => doc.addEventListener("click", (e) => {
        if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
        const anchor = e.target && e.target.closest ? e.target.closest("a[href]") : null;
        if (!anchor) return;
        if (SAFE_HREF.test(anchor.href)) {
          e.preventDefault();
          fn(anchor.href);
        } else if (anchor.getAttribute("href") === "#") {
          // a link the renderer emptied, or a control written as one: it
          // goes nowhere, so it should not jump the view to the top either
          e.preventDefault();
        }
        // anything else, a fragment into the view among it, behaves as written
      }),
      // ? for the keys, [ and ] for the rounds: the app's, so a press the
      // view left alone, outside a text field, goes up to it
      onAppKey: (fn) => win.addEventListener("keydown", (e) => {
        if (e.defaultPrevented || e.pinrailForwarded || e.metaKey || e.ctrlKey || e.altKey) return;
        if (!APP_KEYS.includes(e.key)) return;
        const el = e.target;
        if (el && el.closest && el.closest("input, textarea, select, [contenteditable]")) return;
        fn({ key: e.key, code: e.code || "", metaKey: false, ctrlKey: false, altKey: false, shiftKey: !!e.shiftKey });
      }),
      onShortcut: (fn) => win.addEventListener("keydown", (e) => {
        if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); fn(); }
      }),
      objectUrl: (bytes, type) => win.URL.createObjectURL(new win.Blob([bytes], { type })),
    };
  }

  // the keys the app answers on the review screen, which a view passes up
  const APP_KEYS = ["?", "[", "]"];

  const Pinrail = {
    version: VERSION,
    protocol: PROTOCOL,
    connect: (handlers) => createPlugin(browserEnv(root), handlers),
    createPlugin,
    layout,
    icon,
    escape,
    markdown,
    markdownInline,
    previousVerdict,
    attachmentName,
    ATTACHMENT_SCHEMA,
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
