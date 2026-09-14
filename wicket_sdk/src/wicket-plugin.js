/*
 * wicket plugin SDK, protocol 1.
 *
 * Served by the app at /sdk/v1/wicket-plugin.js. A plugin loads it with one
 * script tag and calls Wicket.connect(handlers); everything the protocol
 * requires (ready, origin pinning, resize, drafts, submitted, violations,
 * collect and the Cmd/Ctrl+Enter shortcut) is handled here.
 *
 *   const plugin = Wicket.connect({
 *     resize: "auto",                 // "auto" (content height), "fill" (viewport), or "manual"
 *     onInit({ gate, previous, readonly, draft, settings }) { … },
 *     onViolations(errors) { … },     // [{ path, message }]
 *     onSubmitted(decision) { … },    // the decision was accepted; render read-only
 *     onCollect() { … },              // the shell's hand-over button, or Cmd/Ctrl+Enter
 *     onAppearance(theme) { … },      // optional; "dark" | "light", already applied
 *     onSettings(settings) { … },     // optional; the plugin's own settings changed
 *   });
 *
 * Load this with a plain <script src> tag, not a deferred or module one: it
 * reads the theme off the frame's URL and sets data-theme on the document, so
 * the view is in the shell's theme from the frame it first paints.
 *   plugin.submit(data);
 *   plugin.draft(data);               // debounced; { flush: true } posts at once
 *   plugin.status({ label: "…" });    // what the shell's hand-over button should read
 *   plugin.settings;                  // the plugin's own settings, as the manifest declares them
 *   plugin.setSetting("diff", "split"); // asks the shell to keep one; it comes back as `settings`
 *
 * Wicket.icon("check") returns an icon from the set the app serves, as markup
 * that takes the colour of the text around it:
 *
 *   `<button class="btn">${Wicket.icon("check")} Accept</button>`
 *
 * Wicket.layout() builds the standard skeleton that goes with the SDK's
 * stylesheet: a header that stays put and a body that scrolls.
 *
 *   const view = Wicket.layout({ title: "5 items", controls: [button] });
 *   view.content.innerHTML = …          // render into this
 *   view.title("4 items").meta(["acme-api", "7 days"]);
 *
 * The same code runs in Node for tests through Wicket.createPlugin(env, handlers).
 */
(function (root) {
  "use strict";

  const PROTOCOL = 1;
  const VERSION = "1.7.0";
  const THEMES = ["dark", "light"];
  const DRAFT_DEBOUNCE_MS = 150;
  const ICON_BASE = "/sdk/v1/icons/";

  function escape(s) {
    return String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  }

  function inline(s) {
    return s
      .replace(/`([^`]+)`/g, (_, c) => `<code class="inl">${c}</code>`)
      .replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
      .replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, "$1<i>$2</i>")
      .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>');
  }

  /* A small, safe markdown subset: paragraphs, bold, italic, inline code,
     fenced code, bullet and numbered lists, http links. Escapes first. */
  function markdown(src) {
    const lines = escape(src || "").split("\n");
    let html = "", para = [], list = null, code = null;
    const flushPara = () => { if (para.length) { html += `<p>${inline(para.join(" "))}</p>`; para = []; } };
    const flushList = () => { if (list) { html += `<${list.tag}>${list.items.map((i) => `<li>${inline(i)}</li>`).join("")}</${list.tag}>`; list = null; } };
    for (const line of lines) {
      if (code !== null) {
        if (/^```/.test(line)) { html += `<pre>${code.join("\n")}</pre>`; code = null; } else code.push(line);
        continue;
      }
      if (/^```/.test(line)) { flushPara(); flushList(); code = []; continue; }
      const li = line.match(/^\s*(?:[-*]|(\d+)\.)\s+(.*)$/);
      if (li) {
        flushPara();
        const tag = li[1] ? "ol" : "ul";
        if (!list || list.tag !== tag) { flushList(); list = { tag, items: [] }; }
        list.items.push(li[2]);
        continue;
      }
      if (!line.trim()) { flushPara(); flushList(); continue; }
      flushList();
      para.push(line.trim());
    }
    if (code !== null) html += `<pre>${code.join("\n")}</pre>`;
    flushPara(); flushList();
    return html;
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
      settings: {}
    };
    let draftTimer = null;
    let stopObserving = null;

    const post = (msg) => env.post(Object.assign({ wicket: PROTOCOL }, msg), state.shellOrigin || "*");

    function startResize() {
      if (resizeMode === "fill") {
        post({ type: "resize", height: "fill" });
      } else if (resizeMode === "auto" && !stopObserving && env.observeSize) {
        stopObserving = env.observeSize((height) => post({ type: "resize", height }));
      }
    }

    function collect() {
      if (state.readonly) return;
      if (handlers.onCollect) handlers.onCollect();
    }

    // The plugin's own settings, as the manifest declares them and the
    // person set them; anything but an object means none.
    const settingsOf = (value) => (value && typeof value === "object" && !Array.isArray(value) ? value : {});

    function handle(data, origin) {
      if (!data || data.wicket !== PROTOCOL || typeof data.type !== "string") return;
      if (state.shellOrigin && origin !== state.shellOrigin) return;
      switch (data.type) {
        case "init":
          if (data.shell_origin) state.shellOrigin = data.shell_origin;
          state.gate = data.gate;
          state.previous = data.previous || null;
          state.readonly = !!data.readonly;
          state.settings = settingsOf(data.settings);
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
      }
    }

    env.listen(handle);
    if (handlers.shortcut !== false && env.onShortcut) env.onShortcut(collect);
    post({ type: "ready" });

    return {
      get gate() { return state.gate; },
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
    if (!doc) throw new Error("Wicket.layout needs a document");

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
    const found = /(?:^|[#&])wicket-theme=([a-z]+)/.exec((win.location && win.location.hash) || "");
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
      onShortcut: (fn) => win.addEventListener("keydown", (e) => {
        if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); fn(); }
      }),
    };
  }

  const Wicket = {
    version: VERSION,
    protocol: PROTOCOL,
    connect: (handlers) => createPlugin(browserEnv(root), handlers),
    createPlugin,
    layout,
    icon,
    escape,
    markdown,
    previousVerdict,
  };

  // Before anything else this file does, and before the view's own script
  // runs: a plugin that loads the SDK with a plain <script> tag never paints
  // in the wrong theme.
  if (root.document && root.document.documentElement) {
    const initial = themeFromUrl(root);
    if (initial) root.document.documentElement.dataset.theme = initial;
  }

  root.Wicket = Wicket;
  if (typeof module !== "undefined" && module.exports) module.exports = Wicket;
})(typeof window !== "undefined" ? window : globalThis);
