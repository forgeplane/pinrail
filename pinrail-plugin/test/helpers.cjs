// A fake shell environment for the SDK: records what the plugin posts,
// delivers messages as if from a shell, and drives timers by hand.
// The SDK is a script for a <script> tag, not a module: run it as one, as a
// browser would, and take Pinrail off the global it leaves it on.
const path = require("node:path");
const vm = require("node:vm");
const { sdkScript } = require("../lib/paths.cjs");
// the script the app serves, parser and all, not the source half of it
const sdkFile = path.join(__dirname, "..", "src", "pinrail-plugin.js");
vm.runInThisContext(sdkScript(path.join(__dirname, "..")), { filename: sdkFile });
// the client with a fake environment, which the global keeps off its
// documented members
const Pinrail = Object.assign(Object.create(globalThis.Pinrail), {
  createPlugin: globalThis.Pinrail[Symbol.for("pinrail.createPlugin")],
});

function fakeEnv() {
  const env = {
    posted: [],
    listeners: [],
    timers: [],
    nextTimer: 1,
    shortcuts: [],
    links: [],
    observers: [],
    themes: [],
    keys: [],
    appKeys: [],
    errors: [],
    post(msg, target) {
      env.posted.push({ msg, target });
    },
    // each registration can be undone, as the browser's can
    listen(fn) {
      env.listeners.push(fn);
      return () => (env.listeners = env.listeners.filter((f) => f !== fn));
    },
    setTimeout(fn, ms) {
      const id = env.nextTimer++;
      env.timers.push({ id, fn, ms });
      return id;
    },
    clearTimeout(id) {
      env.timers = env.timers.filter((t) => t.id !== id);
    },
    observeSize(cb) {
      env.observers.push(cb);
      env.observing += 1;
      cb(321);
      return () => (env.observing -= 1);
    },
    observing: 0,
    applyTheme(theme) {
      env.themes.push(theme);
    },
    dispatchKey(key) {
      env.keys.push(key);
    },
    onShortcut(fn) {
      env.shortcuts.push(fn);
    },
    onAppKey(fn) {
      env.appKeys.push(fn);
      return () => (env.appKeys = env.appKeys.filter((f) => f !== fn));
    },
    logError(error) {
      env.errors.push(error);
    },
    onLink(fn) {
      env.links.push(fn);
      return () => (env.links = env.links.filter((f) => f !== fn));
    },
    objectUrl(bytes, type) {
      return `blob:test/${type}/${bytes.byteLength}`;
    },
    // helpers
    /** what the client still has set up: listeners and observers */
    active() {
      return env.listeners.length + env.appKeys.length + env.links.length + env.observing;
    },
    deliver(data, origin = "http://shell.test") {
      env.listeners.forEach((fn) => fn(data, origin));
    },
    tick() {
      const due = env.timers;
      env.timers = [];
      due.forEach((t) => t.fn());
    },
    types() {
      return env.posted.map((p) => p.msg.type);
    },
    last(type) {
      return [...env.posted].reverse().find((p) => p.msg.type === type);
    },
    pressAppKey(key) {
      env.appKeys.forEach((fn) => fn(key));
    },
    clickLink(url) {
      env.links.forEach((fn) => fn(url));
    },
  };
  return env;
}

/* Enough of a document for Pinrail.layout: elements that remember their
   class, their text and their children. */
function fakeDocument() {
  const make = (tag) => ({
    tag,
    className: "",
    textContent: "",
    children: [],
    append(...nodes) {
      this.children.push(...nodes);
    },
    replaceChildren(...nodes) {
      this.children = nodes;
    },
  });
  return { body: make("body"), createElement: make };
}

const shell = (msg) => Object.assign({ pinrail: 1 }, msg);
const review = (extra = {}) =>
  Object.assign(
    {
      id: "g_1",
      plugin: "t",
      plugin_version: "1.0.0",
      plugin_bundle: null,
      title: "t",
      status: "pending",
      payload: {},
      decision: null,
    },
    extra,
  );
const init = (extra = {}) =>
  shell(
    Object.assign(
      {
        type: "init",
        review: review(),
        previous: null,
        readonly: false,
        draft: null,
        app_origin: "http://shell.test",
      },
      extra,
    ),
  );

module.exports = { Pinrail, fakeEnv, fakeDocument, shell, review, init };
