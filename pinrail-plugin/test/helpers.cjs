// A fake shell environment for the SDK: records what the plugin posts,
// delivers messages as if from a shell, and drives timers by hand.
// The SDK is a script for a <script> tag, not a module: run it as one, as a
// browser would, and take Pinrail off the global it leaves it on.
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { sdkScript } = require("../lib/paths.cjs");
// the script the app serves, parser and all, not the source half of it
const sdkFile = path.join(__dirname, "..", "src", "pinrail-plugin.js");
vm.runInThisContext(sdkScript(path.join(__dirname, "..")), { filename: sdkFile });
const Pinrail = globalThis.Pinrail;

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
    post(msg, target) { env.posted.push({ msg, target }); },
    listen(fn) { env.listeners.push(fn); },
    setTimeout(fn, ms) { const id = env.nextTimer++; env.timers.push({ id, fn, ms }); return id; },
    clearTimeout(id) { env.timers = env.timers.filter((t) => t.id !== id); },
    observeSize(cb) { env.observers.push(cb); cb(321); return () => {}; },
    applyTheme(theme) { env.themes.push(theme); },
    dispatchKey(key) { env.keys.push(key); },
    onShortcut(fn) { env.shortcuts.push(fn); },
    onLink(fn) { env.links.push(fn); },
    // helpers
    deliver(data, origin = "http://shell.test") { env.listeners.forEach((fn) => fn(data, origin)); },
    tick() { const due = env.timers; env.timers = []; due.forEach((t) => t.fn()); },
    types() { return env.posted.map((p) => p.msg.type); },
    last(type) { return [...env.posted].reverse().find((p) => p.msg.type === type); },
    pressShortcut() { env.shortcuts.forEach((fn) => fn()); },
    clickLink(url) { env.links.forEach((fn) => fn(url)); },
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
    append(...nodes) { this.children.push(...nodes); },
    replaceChildren(...nodes) { this.children = nodes; },
  });
  return { body: make("body"), createElement: make };
}

const shell = (msg) => Object.assign({ pinrail: 1 }, msg);
const gate = (extra = {}) => Object.assign({ id: "g_1", type: "t", type_version: 1, title: "t", status: "pending", payload: {}, decision: null }, extra);
const init = (extra = {}) => shell(Object.assign({ type: "init", gate: gate(), previous: null, readonly: false, draft: null, shell_origin: "http://shell.test" }, extra));

module.exports = { Pinrail, fakeEnv, fakeDocument, shell, gate, init };
