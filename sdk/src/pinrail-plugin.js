/*
 * pinrail plugin SDK, protocol 1.
 *
 * Served by the app at /sdk/v1/pinrail-plugin.js. A plugin loads it with one
 * script tag and calls Pinrail.connect(handlers); everything the protocol
 * requires (ready, origin pinning, drafts, the hand-over, submitted,
 * violations and the Cmd/Ctrl+Enter shortcut) is handled here.
 *
 *   const plugin = Pinrail.connect({
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
 *   plugin.draft(data);               // kept at once; it comes back in onInit
 *   plugin.handOverLabel("…");        // what the app's hand-over button reads
 *   plugin.open("https://example.com"); // the app asks the person, then opens it in the browser
 *   plugin.settings;                  // the plugin's own settings, as the manifest declares them
 *   await plugin.setSetting("diff", "split"); // asks the app to keep one: the settings, or why not
 *
 * Pinrail.icon("check") returns the plugin's own icons/check.svg, beside the
 * view, as markup that takes the colour of the text around it:
 *
 *   `<button class="pinrail-btn">${Pinrail.icon("check")} Accept</button>`
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
  /** The key of the method that subscribes the microphone buttons to dictation changes. */
  const HEARING = Symbol.for("pinrail.hearing");
  const THEMES = ["dark", "light"];

  function escape(s) {
    return String(s ?? "").replace(
      /[&<>"']/g,
      (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
    );
  }

  /* Markdown is rendered by the SDK's second script, /sdk/v1/markdown.js,
     which a view that renders Markdown loads beside this one. It may load
     before or after this file: the renderer is looked up on each call. */
  const MARKDOWN = Symbol.for("pinrail.markdown");
  /** The addresses a view may send someone to. */
  const SAFE_HREF = /^(https?:|mailto:)/i;

  function render(method, src) {
    const md = root[MARKDOWN];
    if (!md) {
      throw new Error(
        'Pinrail.markdown needs the SDK\'s Markdown script: add <script src="/sdk/v1/markdown.js"></script> to the view',
      );
    }
    return src == null ? "" : md[method](String(src));
  }

  const markdown = (src) => render("render", src);
  const markdownInline = (src) => render("renderInline", src);

  /* Code is highlighted by the SDK's third script, /sdk/v1/highlight.js,
     loaded the same way and looked up on each call too. */
  const HIGHLIGHT = Symbol.for("pinrail.highlight");

  function highlighter() {
    const hl = root[HIGHLIGHT];
    if (!hl) {
      throw new Error(
        'Pinrail.highlight needs the SDK\'s highlighting script: add <script src="/sdk/v1/highlight.js"></script> to the view',
      );
    }
    return hl;
  }

  const highlight = (code, language) => highlighter().highlight(code, language);
  const highlightLines = (code, language) => highlighter().highlightLines(code, language);
  const languageOf = (path) => highlighter().languageOf(path);

  /* A file a review carries is named in its payload as
     { "$attachment": "pivot.glb" }. attachmentName reads the name back out,
     or gives null for anything else. The package's
     schemas/attachment.schema.json describes the object for a payload
     schema. */
  function attachmentName(ref) {
    return ref && typeof ref === "object" && typeof ref.$attachment === "string" ? ref.$attachment : null;
  }

  function createPlugin(env, handlers) {
    handlers = handlers || {};
    const state = {
      appOrigin: null,
      review: null,
      previous: null,
      readonly: false,
      theme: (env.initialTheme && env.initialTheme()) || "dark",
      settings: {},
      capabilities: [],
    };
    // the active dictation: its request number, the field it writes into,
    // the text before and after the cursor at its start, and how to resolve it
    let speaking = null;
    // listeners called when a dictation starts or ends, for the microphone buttons
    const hearing = new Set();
    const heard = () => {
      for (const fn of hearing) fn(speaking ? speaking.field : null);
    };
    // file requests and settings changes waiting on the app, by request
    // number, which the two share
    const asked = new Map();
    const changing = new Map();
    // copies waiting on the app, by request number
    const copying = new Map();
    let nextAsk = 1;

    const post = (msg) => env.post(Object.assign({ pinrail: PROTOCOL }, msg), state.appOrigin || "*");
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
      post({ type: "submit", req, data });
    }

    // The plugin's own settings, as the manifest declares them and the
    // person set them; anything but an object means none.
    const settingsOf = (value) => (value && typeof value === "object" && !Array.isArray(value) ? value : {});

    function handle(data, origin) {
      if (!data || data.pinrail !== PROTOCOL || typeof data.type !== "string") return;
      if (state.appOrigin && origin !== state.appOrigin) return;
      switch (data.type) {
        case "init":
          // the origin the browser vouches for; the one the message names
          // must agree with it
          if (data.app_origin && origin && data.app_origin !== origin) return;
          if (!state.appOrigin) state.appOrigin = origin || data.app_origin || null;
          state.review = data.review;
          state.previous = data.previous || null;
          state.readonly = !!data.readonly;
          state.settings = settingsOf(data.settings);
          state.capabilities = Array.isArray(data.capabilities) ? data.capabilities.slice() : [];
          // a review that became read-only accepts no more dictation
          if (state.readonly && speaking) stopDictating();
          // every JSON value a view kept comes back as it was, false and 0 too
          call("onInit", {
            review: state.review,
            previous: state.previous,
            readonly: state.readonly,
            draft: data.draft === undefined ? null : data.draft,
            settings: state.settings,
          });
          break;
        case "settings": {
          // the answer to setSetting, by its request number
          if (typeof data.req === "number") {
            const waiting = changing.get(data.req);
            if (!waiting) break;
            changing.delete(data.req);
            if (data.ok) {
              state.settings = Object.assign({}, state.settings, waiting.patch);
              waiting.resolve(state.settings);
            } else {
              const errors = Array.isArray(data.errors) ? data.errors : [];
              const error = new Error(
                `the app did not keep the setting: ${errors.map((e) => `${e.path || "/"}: ${e.message}`).join("; ")}`,
              );
              error.violations = errors;
              waiting.reject(error);
            }
            break;
          }
          // A change in Settings, or through this view: the values as they
          // stand now, every key the manifest declares.
          state.settings = settingsOf(data.settings);
          call("onSettings", state.settings);
          break;
        }
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
            if (env.applyTokens) env.applyTokens(data.tokens);
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
        case "find": {
          // The app's find bar: the view's text searched for the query, the
          // next match on each repeat, and nothing highlighted for an empty
          // query. The count goes back at once, and again whenever the
          // view's content changes while the bar is open.
          if (typeof data.req !== "number" || typeof data.query !== "string") break;
          const req = data.req;
          const answer = (found) => post({ type: "found", req, count: found.count, current: found.current });
          answer(env.find ? env.find(data.query, !!data.backwards, answer) : { count: 0, current: 0 });
          break;
        }
        case "clipboard": {
          // the app's answer to copy(): copied, or why not
          const waiting = copying.get(data.req);
          if (!waiting) break;
          copying.delete(data.req);
          if (data.ok) waiting.resolve();
          else waiting.reject(new Error(typeof data.error === "string" ? data.error : "the app could not copy"));
          break;
        }
        case "dictation":
          if (!speaking || data.req !== speaking.req) break;
          if (typeof data.text === "string") hear(data.text);
          if (data.done) settle(typeof data.error === "string" ? data.error : null);
          break;
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
    post({ type: "ready" });

    /* The bytes of a file the review carries, from the shell: a view's
       frame can fetch nothing, so it asks, and the shell answers with the
       bytes and nothing else. `round: "previous"` asks for a file of the
       round this one revises. */
    function attachment(name, opts) {
      const round = opts && opts.round === "previous" ? "previous" : "current";
      const review = round === "previous" ? state.previous : state.review;
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

    /* The view's frame cannot reach the clipboard, so it asks the app to
       copy for it: `{ text }`, or `{ attachment }` for an image the review
       carries, by its name. Only a plugin whose manifest declares the
       "clipboard" capability is let copy; the promise resolves once the app
       has copied, and rejects with why it did not. */
    function copy(content) {
      if (!state.capabilities.includes("clipboard"))
        return Promise.reject(
          new Error('the app does not let this plugin copy: declare the "clipboard" capability in its manifest'),
        );
      const text = content && typeof content.text === "string" ? content.text : null;
      const name = content && typeof content.attachment === "string" ? content.attachment : null;
      if ((text === null) === (name === null))
        return Promise.reject(new TypeError("Pinrail: copy takes either { text } or { attachment }"));
      if (name !== null) {
        const listed = state.review && Array.isArray(state.review.attachments) ? state.review.attachments : [];
        if (!listed.some((a) => a && a.name === name))
          return Promise.reject(new Error(`no attachment "${name}" on this review`));
      }
      return new Promise((resolve, reject) => {
        const req = nextAsk++;
        copying.set(req, { resolve, reject });
        post(Object.assign({ type: "clipboard_write", req }, text !== null ? { text } : { attachment: name }));
      });
    }

    /* The app runs speech recognition, and the recognised text is inserted
       into a field of the view at the cursor, replacing the selection. The
       field receives an `input` event for each change, so the view's own
       handlers save its draft. Dictating into another field ends the active
       dictation, and dictating into the same field again stops it. */
    function dictate(field) {
      if (!field) return Promise.reject(new TypeError("Pinrail: dictate needs a text field"));
      if (speaking && speaking.field === field) {
        stopDictating();
        return speaking.done;
      }
      if (!state.capabilities.includes("dictation") || state.readonly)
        return Promise.resolve({ error: "this app cannot dictate" });
      if (speaking) {
        post({ type: "dictation_stop", req: speaking.req });
        settle(null);
      }
      const value = String(field.value == null ? "" : field.value);
      const start = typeof field.selectionStart === "number" ? field.selectionStart : value.length;
      const end = typeof field.selectionEnd === "number" ? field.selectionEnd : start;
      const before = value.slice(0, start);
      let resolve;
      const done = new Promise((r) => (resolve = r));
      speaking = {
        req: nextAsk++,
        field,
        // insert a space before the dictated text unless one precedes it
        before: before && !/\s$/.test(before) ? before + " " : before,
        after: value.slice(end),
        done,
        resolve,
      };
      post({ type: "dictation_start", req: speaking.req });
      heard();
      return done;
    }

    /** Asks the app to stop the dictation; the final text still arrives. */
    function stopDictating() {
      if (speaking) post({ type: "dictation_stop", req: speaking.req });
    }

    /* A view that re-renders its field, as many do on each change, replaces
       it with a new element with the same id, and the dictation moves to
       that element. Returns true when it moved. */
    function follow() {
      if (!speaking || speaking.field.isConnected !== false || !env.again) return false;
      const next = env.again(speaking.field);
      if (!next || next === speaking.field) return false;
      speaking.field = next;
      return true;
    }

    /* A field's value, set through the setter of its element's prototype.
       React keeps a controlled field's value in a `value` property of the
       element itself: set through that, the value React holds already
       matches, and the input event that follows changes nothing in the
       view's state. */
    function setValue(field, value) {
      let proto = Object.getPrototypeOf(field);
      while (proto && !Object.getOwnPropertyDescriptor(proto, "value")) proto = Object.getPrototypeOf(proto);
      const own = proto && Object.getOwnPropertyDescriptor(proto, "value");
      if (own && own.set) own.set.call(field, value);
      else field.value = value;
    }

    function hear(text) {
      if (follow()) heard();
      const { field, before, after } = speaking;
      // a field removed from the page receives no more text
      if (field.isConnected === false) {
        if (!speaking.stopping) stopDictating();
        speaking.stopping = true;
        return;
      }
      const said = text.trimStart();
      const gap = said && after && !/^\s/.test(after) ? " " : "";
      setValue(field, before + said + gap + after);
      const caret = (before + said).length;
      if (typeof field.setSelectionRange === "function") field.setSelectionRange(caret, caret);
      if (env.changed) env.changed(field);
    }

    function settle(error) {
      const session = speaking;
      speaking = null;
      session.resolve(error ? { error } : {});
      heard();
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
      /** whether the app copies to the clipboard for this view, as copy() asks */
      get canCopy() {
        return state.capabilities.includes("clipboard");
      },
      copy,
      /** whether the app can run dictation for this view */
      get canDictate() {
        return state.capabilities.includes("dictation") && !state.readonly;
      },
      /** the field a dictation writes into, or null */
      get dictating() {
        follow();
        return speaking ? speaking.field : null;
      },
      /* Starts dictation into a text field at its cursor, or stops it when
         that field is already being dictated into. Resolves when the
         dictation ends, with `error` when it failed. */
      dictate,
      stopDictating,
      /* Asks the app to keep a setting of this plugin's. The app checks it
         against the manifest's settings schema: the promise resolves with
         the settings as they now stand, or rejects with an error whose
         `violations` say why the app would not keep it. */
      setSetting(key, value) {
        return new Promise((resolve, reject) => {
          const req = nextAsk++;
          const patch = { [key]: value };
          changing.set(req, { resolve, reject, patch });
          post({ type: "settings_set", req, patch });
        });
      },
      /* Keeps the person's work in progress, at once, so a reload right
         after a choice keeps it. What JSON cannot hold, such as a cycle or
         undefined, is a mistake in the view: it throws here, at the line
         that kept it. */
      draft(data) {
        if (state.readonly) return;
        if (data === undefined) throw new TypeError("Pinrail: a draft must be a JSON value, not undefined");
        let value;
        try {
          value = JSON.parse(JSON.stringify(data));
        } catch (error) {
          throw new TypeError(`Pinrail: a draft must be a JSON value: ${error.message}`, { cause: error });
        }
        post({ type: "draft", data: value });
      },
      /** asks the app to open a link in the system browser, as a click on one in the view does; the app asks the person first unless they allowed the site */
      open,
      /** what the app's hand-over button reads, such as "Hand over 3 of 5" */
      handOverLabel(text) {
        post({ type: "status", label: String(text) });
      },
      /* Undoes what the client set up: its listeners. For tests, which make a client per case; a view keeps its
         connection for as long as its page lives. */
      [TEARDOWN]() {
        for (const stop of undo.splice(0)) stop();
      },
      /* Subscribes a listener to the field a dictation writes into, which
         is null when no dictation is active. The microphone buttons use it. */
      [HEARING](fn) {
        hearing.add(fn);
        return () => hearing.delete(fn);
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
  const CONFIRMATION_TONES = ["warning", "info", "danger"];

  const documentOf = (options, name) => {
    const doc = (options && options.document) || (typeof document === "undefined" ? null : document);
    if (!doc) throw new Error(`Pinrail.${name} needs a document`);
    return doc;
  };

  const elementIn = (doc) => (tag, className) => {
    const node = doc.createElement(tag);
    node.className = className;
    return node;
  };

  /* Strings as spans, elements as they are, in place of what `node` held. */
  const fillIn = (doc) => (node, value) => {
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

  /*
   * What the view asks the person to confirm before the next hand-over, such
   * as items left undecided: a bar under the body, above the app's hand-over
   * button, the same in every plugin. Pinrail.layout() places one; a view
   * with a layout of its own places `element` itself, below what scrolls.
   * Hidden until `confirmation({ text, tone, actions })` asks: a warning
   * unless `tone` is "info" or "danger", with the actions, such as a button
   * that keeps the person deciding, at its end. null hides it.
   */
  function confirmationBar(options) {
    const doc = documentOf(options, "confirmationBar");
    const make = elementIn(doc);
    const fill = fillIn(doc);
    const element = make("div", "pinrail-confirmation");
    element.hidden = true;
    element.role = "status";
    const text = make("span", "pinrail-confirmation-text");
    const actions = make("span", "pinrail-confirmation-actions");
    element.append(text, actions);
    const bar = {
      element,
      confirmation(value) {
        if (!value) {
          element.hidden = true;
          text.textContent = "";
          fill(actions, null);
          return bar;
        }
        const tone = CONFIRMATION_TONES.includes(value.tone) ? value.tone : "warning";
        element.className = `pinrail-confirmation pinrail-confirmation-${tone}`;
        text.textContent = value.text == null ? "" : String(value.text);
        fill(actions, value.actions);
        element.hidden = false;
        return bar;
      },
    };
    return bar;
  }

  function layout(options) {
    options = options || {};
    const doc = documentOf(options, "layout");
    const make = elementIn(doc);
    const fill = fillIn(doc);

    const into = options.into || doc.body;
    into.className = into.className ? into.className + " pinrail-layout" : "pinrail-layout";

    const wantsHeader =
      options.header === true || options.title != null || options.meta != null || options.controls != null;

    let header = null;
    let titleNode = null;
    let metaNode = null;
    let controlsNode = null;

    if (wantsHeader) {
      header = make("header", "pinrail-header");
      titleNode = make("span", "pinrail-title");
      metaNode = make("span", "pinrail-meta");
      controlsNode = make("span", "pinrail-controls");
      header.append(titleNode, metaNode, controlsNode);
      into.append(header);
    }

    // The body scrolls, not the document, so a heading inside it can pin to
    // the top of the scroll without having to know the header's height.
    const scroll = make("div", "pinrail-scroll");
    const content = make("div", "pinrail-content");
    scroll.append(content);
    into.append(scroll);

    const bar = confirmationBar({ document: doc });
    into.append(bar.element);

    const view = {
      header,
      scroll,
      content,
      confirmationBar: bar.element,
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
      /* The confirmation bar: see confirmationBar(). */
      confirmation(value) {
        bar.confirmation(value);
        return view;
      },
    };

    if (options.title != null) view.title(options.title);
    if (options.meta != null) view.meta(options.meta);
    if (options.controls != null) view.controls(options.controls);

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

  /* A custom theme's colours, base64url JSON on the frame's URL for the same
     reason as the theme. `appearance` carries them from then on. */
  function tokensFromUrl(win) {
    const found = /(?:^|[#&])pinrail-tokens=([A-Za-z0-9_-]+)/.exec((win.location && win.location.hash) || "");
    if (!found) return null;
    try {
      return JSON.parse(win.atob(found[1].replace(/-/g, "+").replace(/_/g, "/")));
    } catch {
      return null;
    }
  }

  /* Sets a custom theme's colours on the root element, whose style wins over
     the stylesheet's, and clears the ones set before. Only Pinrail's tokens
     are set: a view's own variables are not the shell's to change. */
  let tokensSet = [];
  function setTokens(root, tokens) {
    for (const name of tokensSet) root.style.removeProperty(name);
    tokensSet = [];
    if (!tokens || typeof tokens !== "object") return;
    for (const [name, value] of Object.entries(tokens)) {
      if (!name.startsWith("--pinrail-") || typeof value !== "string") continue;
      root.style.setProperty(name, value);
      tokensSet.push(name);
    }
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
    const size = length ? `--pinrail-icon-size:${length};` : "";
    const extra = options.class ? " " + escape(options.class) : "";
    const described = options.label ? ` role="img" aria-label="${escape(options.label)}"` : ' aria-hidden="true"';
    const base = typeof document === "undefined" ? "http://plugin.invalid/view/" : document.baseURI;
    const url = new URL(`icons/${safe}.svg`, base).href;
    return `<span class="pinrail-icon${extra}" data-icon="${safe}" style="--pinrail-icon:url(&quot;${escape(url)}&quot;);${size}"${described}></span>`;
  }

  /* Sizes a comment field's text, a textarea in a .pinrail-compose: it grows
     with the text up to about eight lines, then scrolls, and the box is
     .pinrail-compose-wide once the text wraps or has a new line. A view
     calls it when the field is drawn and on each input; everything else in
     the field, its buttons among it, is the view's. */
  const FIELD_MAX = 150;
  function fitField(el) {
    if (!el || typeof el.closest !== "function") return;
    const box = el.closest(".pinrail-compose");
    if (box) {
      if (!el.value) box.classList.remove("pinrail-compose-wide");
      else if (!box.classList.contains("pinrail-compose-wide")) {
        el.style.height = "auto";
        const style = getComputedStyle(el);
        const oneLine =
          (parseFloat(style.lineHeight) || 18) + parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
        // the next measure, below, is at the full width
        if (el.value.includes("\n") || el.scrollHeight > oneLine * 1.4) box.classList.add("pinrail-compose-wide");
      }
    }
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, FIELD_MAX)}px`;
    el.style.overflowY = el.scrollHeight > FIELD_MAX ? "auto" : "hidden";
  }

  /* Find in the view, for the app's find bar. The text the view shows is
     searched, ignoring case, as one string per block, so a match may cross
     inline elements such as a link or a highlighted token. Text that is
     hidden, collapsed or scrolled out of an overflow box's reach still
     counts when it takes up space; text with no box at all does not. The
     matches are painted with the CSS Custom Highlight API where the webview
     has it, and the current one is selected where it does not. */
  const FIND_LIMIT = 1000;
  const FIND_SKIP = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEXTAREA", "OPTION"]);
  const FIND_STYLE = `::highlight(pinrail-find) { background-color: rgba(255, 210, 0, 0.45); }
::highlight(pinrail-find-current) { background-color: #ff9632; color: #1c1a17; }`;

  function finder(win) {
    const doc = win.document;
    const highlights = win.CSS && win.CSS.highlights && typeof win.Highlight === "function" ? win.CSS.highlights : null;
    let query = "";
    let backwards = false;
    let matches = [];
    let current = -1;
    let update = null;
    let observer = null;
    let timer = null;
    let selected = false;
    // registered once and changed in place: the system WebKit repaints a
    // highlight whose ranges change, but not the ranges of one replaced or
    // removed in the registry
    let all = null;
    let now = null;

    const result = () => ({ count: matches.length, current: current + 1 });

    function search() {
      const found = [];
      if (!query || !doc.body) return found;
      const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "giu");
      const display = new Map();
      const shown = new Map();
      const displayOf = (el) => {
        let value = display.get(el);
        if (value === undefined) display.set(el, (value = win.getComputedStyle(el).display));
        return value;
      };
      // the nearest ancestor that is not inline: text inside one is read as
      // one string
      const blockOf = (el) => {
        for (let node = el; node && node !== doc.body; node = node.parentElement) {
          const value = displayOf(node);
          if (value !== "inline" && value !== "contents") return node;
        }
        return doc.body;
      };
      const isShown = (el) => {
        let value = shown.get(el);
        if (value === undefined) shown.set(el, (value = win.getComputedStyle(el).visibility === "visible"));
        return value;
      };
      let block = null;
      let text = "";
      let pieces = [];
      const flush = () => {
        pattern.lastIndex = 0;
        let at = 0;
        let hit;
        while (found.length < FIND_LIMIT && (hit = pattern.exec(text))) {
          if (!hit[0]) break;
          const start = hit.index;
          const end = start + hit[0].length;
          while (at + 1 < pieces.length && pieces[at + 1].start <= start) at += 1;
          let last = at;
          while (last + 1 < pieces.length && pieces[last + 1].start < end) last += 1;
          const range = doc.createRange();
          range.setStart(pieces[at].node, start - pieces[at].start);
          range.setEnd(pieces[last].node, end - pieces[last].start);
          if (range.getClientRects().length > 0) found.push(range);
        }
        text = "";
        pieces = [];
      };
      const walker = doc.createTreeWalker(doc.body, win.NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const parent = node.parentElement;
        if (!parent || FIND_SKIP.has(parent.tagName) || !node.data.trim() || !isShown(parent)) continue;
        const here = blockOf(parent);
        if (here !== block) {
          flush();
          block = here;
        }
        pieces.push({ node, start: text.length });
        text += node.data;
      }
      flush();
      return found;
    }

    function paint() {
      if (highlights) {
        if (!doc.getElementById("pinrail-find-style")) {
          const style = doc.createElement("style");
          style.id = "pinrail-find-style";
          style.textContent = FIND_STYLE;
          (doc.head || doc.documentElement).append(style);
        }
        if (!all) {
          all = new win.Highlight();
          now = new win.Highlight();
          now.priority = 1;
          highlights.set("pinrail-find", all);
          highlights.set("pinrail-find-current", now);
        }
        for (const range of matches) all.add(range);
        if (current >= 0) now.add(matches[current]);
      } else if (current >= 0) {
        const selection = win.getSelection();
        selection.removeAllRanges();
        selection.addRange(matches[current].cloneRange());
        selected = true;
      }
    }

    function clear() {
      if (all) {
        all.clear();
        now.clear();
      } else if (selected) {
        win.getSelection().removeAllRanges();
      }
      selected = false;
    }

    // the current match, scrolled into the middle of its box unless every
    // box around it shows it already
    function reveal(range) {
      const rect = range.getBoundingClientRect();
      const inside = (box) =>
        rect.top >= box.top && rect.bottom <= box.bottom && rect.left >= box.left && rect.right <= box.right;
      let visible = inside({ top: 0, left: 0, bottom: win.innerHeight, right: win.innerWidth });
      for (let el = range.startContainer.parentElement; visible && el && el !== doc.body; el = el.parentElement) {
        const style = win.getComputedStyle(el);
        if (style.overflowX !== "visible" || style.overflowY !== "visible")
          visible = inside(el.getBoundingClientRect());
      }
      if (!visible) range.startContainer.parentElement.scrollIntoView({ block: "center", inline: "nearest" });
    }

    // the first match from the top of what shows, or the last before it
    // when going backwards
    function nearest() {
      if (!matches.length) return -1;
      const below = matches.findIndex((range) => range.getBoundingClientRect().bottom > 0);
      if (below === -1) return backwards ? matches.length - 1 : 0;
      return backwards ? (below === 0 ? matches.length - 1 : below - 1) : below;
    }

    // the view's content changed while the bar is open: the matches are
    // found again, and the count goes back, without scrolling
    function watch() {
      if (observer || typeof win.MutationObserver !== "function" || !doc.body) return;
      observer = new win.MutationObserver(() => {
        win.clearTimeout(timer);
        timer = win.setTimeout(() => {
          if (!query) return;
          matches = search();
          current = matches.length ? Math.min(Math.max(current, 0), matches.length - 1) : -1;
          clear();
          paint();
          if (update) update(result());
        }, 150);
      });
      observer.observe(doc.body, { childList: true, subtree: true, characterData: true });
    }

    function stop() {
      if (observer) observer.disconnect();
      observer = null;
      win.clearTimeout(timer);
    }

    return (text, back, onUpdate) => {
      update = onUpdate || null;
      backwards = back;
      if (!text) {
        query = "";
        matches = [];
        current = -1;
        stop();
        clear();
        return result();
      }
      const again = text === query && matches.length > 0;
      query = text;
      matches = search();
      if (!matches.length) current = -1;
      else if (again) current = (Math.max(current, 0) + (back ? matches.length - 1 : 1)) % matches.length;
      else current = nearest();
      clear();
      paint();
      if (current >= 0) reveal(matches[current]);
      watch();
      return result();
    };
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
      applyTheme: (theme) => {
        doc.documentElement.dataset.theme = theme;
      },
      applyTokens: (tokens) => setTokens(doc.documentElement, tokens),
      // sends an input event for dictated text, as typing would
      changed: (field) => field.dispatchEvent(new win.Event("input", { bubbles: true })),
      // finds, by its id, the field a view rendered in place of a removed one
      again: (field) => (field.id ? doc.getElementById(field.id) : null),
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
      // goes up from anywhere, a text field too: it is the hand-over. So
      // does ⌘D, Ctrl+D off macOS, which discards the review, and ⌘F or
      // Ctrl+F, which opens the find bar; on macOS, Ctrl+D in a text field
      // deletes forward, and stays in the view.
      onAppKey: (fn) => {
        const mac = /Mac/i.test((win.navigator && win.navigator.platform) || "");
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
          const mod = (mac ? e.metaKey : e.ctrlKey) && !e.altKey && !e.shiftKey;
          if (mod && (e.key === "d" || e.key === "D" || e.key === "f" || e.key === "F")) {
            e.preventDefault();
            const key = e.key.toLowerCase();
            fn({
              key,
              code: e.code || (key === "d" ? "KeyD" : "KeyF"),
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
      find: finder(win),
      logError: (error) => win.console.error(error),
      objectUrl: (bytes, type) => win.URL.createObjectURL(new win.Blob([bytes], { type })),
    };
  }

  // the shortcuts the app handles on the review screen, which a view
  // forwards to it: ? for help, [ and ] for the rounds, J and K to move
  // through the inbox, and Esc to close the inbox's panel. A view that
  // handles one itself, such as Esc to close its own dialog, calls
  // preventDefault to keep it.
  const APP_KEYS = ["?", "[", "]", "j", "k", "Escape"];

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
    const plugin = createPlugin(browserEnv(root), handlers);
    if (root.document && root.MutationObserver) microphones(root, plugin);
    // Esc stops a dictation before the view receives it; the next Esc goes to the view
    root.addEventListener(
      "keydown",
      (event) => {
        if (event.key !== "Escape" || event.isComposing || !plugin.dictating) return;
        event.preventDefault();
        event.stopImmediatePropagation();
        plugin.stopDictating();
      },
      true,
    );
    return plugin;
  }

  const MIC =
    '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 19v3"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><rect x="9" y="2" width="6" height="13" rx="3"/></svg>';

  /* A comment is written for a person to read as it was typed, so a comment
     field, a .pinrail-compose-input, has spelling, autocorrection,
     capitalisation and suggestions off. An attribute the view set itself
     is left as it is. */
  const PLAIN_TEXT = { spellcheck: "false", autocorrect: "off", autocapitalize: "off", autocomplete: "off" };
  function plainText(doc) {
    for (const field of doc.querySelectorAll(".pinrail-compose-input"))
      for (const [name, value] of Object.entries(PLAIN_TEXT))
        if (!field.hasAttribute(name)) field.setAttribute(name, value);
  }

  /* Adds a microphone button to every text field marked data-pinrail-dictate
     while the app supports dictation. A click starts dictation into the
     field, and a second click stops it. The button is placed in the element
     marked data-pinrail-dictate-for with the field's id, if there is one,
     and otherwise after the field. The buttons are updated when the view
     re-renders its fields, and show which field is being dictated into. */
  function microphones(win, plugin) {
    const doc = win.document;
    const fieldOf = new WeakMap();
    const buttonOf = new WeakMap();
    const slotFor = (field) => {
      if (!field.id) return null;
      const id = win.CSS && win.CSS.escape ? win.CSS.escape(field.id) : field.id.replace(/["\\]/g, "\\$&");
      return doc.querySelector(`[data-pinrail-dictate-for="${id}"]`);
    };
    function sync() {
      const now = plugin.dictating;
      for (const button of doc.querySelectorAll("button.pinrail-dictate")) {
        const on = fieldOf.get(button) === now && now !== null;
        button.setAttribute("aria-pressed", on ? "true" : "false");
        button.title = on ? "Stop dictating" : "Dictate";
        button.setAttribute("aria-label", button.title);
      }
    }
    function make(field) {
      const button = doc.createElement("button");
      button.type = "button";
      button.className = "pinrail-dictate";
      button.innerHTML = MIC;
      fieldOf.set(button, field);
      buttonOf.set(field, button);
      // keep the focus in the field while the button is pressed, so a view
      // that closes or cleans up a field on blur does not react
      button.addEventListener("mousedown", (event) => event.preventDefault());
      button.addEventListener("click", () => {
        void plugin.dictate(field);
        field.focus({ preventScroll: true });
      });
      return button;
    }
    function decorate() {
      const can = plugin.canDictate;
      for (const field of doc.querySelectorAll("[data-pinrail-dictate]")) {
        let button = buttonOf.get(field);
        if (!can || field.disabled || field.readOnly) {
          if (button) button.remove();
          continue;
        }
        const slot = slotFor(field);
        const placed =
          button && button.isConnected && (slot ? button.parentNode === slot : button.previousElementSibling === field);
        if (placed) continue;
        if (!button) button = make(field);
        if (slot) slot.replaceChildren(button);
        else field.after(button);
      }
      // remove a button whose field a re-render removed
      for (const button of doc.querySelectorAll("button.pinrail-dictate"))
        if (!fieldOf.get(button) || !fieldOf.get(button).isConnected) button.remove();
      sync();
    }
    let queued = false;
    const later = () => {
      if (queued) return;
      queued = true;
      win.queueMicrotask(() => {
        queued = false;
        decorate();
        plainText(doc);
      });
    };
    new win.MutationObserver(later).observe(doc.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["data-pinrail-dictate", "data-pinrail-dictate-for", "id", "disabled", "readonly"],
    });
    plugin[HEARING](sync);
    // the init message carries the app's capabilities
    win.addEventListener("message", later);
    later();
  }

  const Pinrail = {
    protocol: PROTOCOL,
    connect,
    layout,
    confirmationBar,
    fitField,
    icon,
    escape,
    markdown,
    markdownInline,
    highlight,
    highlightLines,
    languageOf,
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
    setTokens(root.document.documentElement, tokensFromUrl(root));
  }

  root.Pinrail = Pinrail;
  if (typeof module !== "undefined" && module.exports) module.exports = Pinrail;
})(typeof window !== "undefined" ? window : globalThis);
