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
 *     onInit({ gate, previous, readonly, draft }) { … },
 *     onViolations(errors) { … },     // [{ path, message }]
 *     onSubmitted(decision) { … },    // the decision was accepted; render read-only
 *     onCollect() { … },              // Cmd/Ctrl+Enter in the shell or in this frame
 *     onAppearance(theme) { … },      // optional; "dark" | "light", already applied
 *   });
 *   plugin.submit(data);
 *   plugin.draft(data);               // debounced; { flush: true } posts at once
 *
 * The same code runs in Node for tests through Wicket.createPlugin(env, handlers).
 */
(function (root) {
  "use strict";

  const PROTOCOL = 1;
  const VERSION = "1.1.0";
  const THEMES = ["dark", "light"];
  const DRAFT_DEBOUNCE_MS = 150;

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

  function createPlugin(env, handlers) {
    handlers = handlers || {};
    const resizeMode = handlers.resize || "auto";
    const state = {
      shellOrigin: null,
      gate: null,
      previous: null,
      readonly: false,
      initialised: false,
      theme: "dark"
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

    function handle(data, origin) {
      if (!data || data.wicket !== PROTOCOL || typeof data.type !== "string") return;
      if (state.shellOrigin && origin !== state.shellOrigin) return;
      switch (data.type) {
        case "init":
          if (data.shell_origin) state.shellOrigin = data.shell_origin;
          state.gate = data.gate;
          state.previous = data.previous || null;
          state.readonly = !!data.readonly;
          state.initialised = true;
          if (handlers.onInit) handlers.onInit({ gate: state.gate, previous: state.previous, readonly: state.readonly, draft: data.draft || null });
          startResize();
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
      submit(data) { post({ type: "submit", data }); },
      draft(data, opts) {
        if (state.readonly) return;
        env.clearTimeout(draftTimer);
        const send = () => post({ type: "draft", data });
        if (opts && opts.flush) send();
        else draftTimer = env.setTimeout(send, DRAFT_DEBOUNCE_MS);
      },
      resize(height) { post({ type: "resize", height }); },
      collect,
    };
  }

  function browserEnv(win) {
    const doc = win.document;
    return {
      post: (msg, targetOrigin) => win.parent.postMessage(msg, targetOrigin),
      listen: (fn) => win.addEventListener("message", (e) => fn(e.data, e.origin)),
      setTimeout: (fn, ms) => win.setTimeout(fn, ms),
      clearTimeout: (t) => win.clearTimeout(t),
      observeSize: (cb) => {
        const emit = () => cb(doc.documentElement.scrollHeight);
        const ro = new win.ResizeObserver(emit);
        ro.observe(doc.body);
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
    escape,
    markdown,
    previousVerdict,
  };

  root.Wicket = Wicket;
  if (typeof module !== "undefined" && module.exports) module.exports = Wicket;
})(typeof window !== "undefined" ? window : globalThis);
