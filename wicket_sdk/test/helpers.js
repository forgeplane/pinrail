// A fake shell environment for the SDK: records what the plugin posts,
// delivers messages as if from a shell, and drives timers by hand.
const Wicket = require("../src/wicket-plugin.js");

function fakeEnv() {
  const env = {
    posted: [],
    listeners: [],
    timers: [],
    nextTimer: 1,
    shortcuts: [],
    observers: [],
    themes: [],
    post(msg, target) { env.posted.push({ msg, target }); },
    listen(fn) { env.listeners.push(fn); },
    setTimeout(fn, ms) { const id = env.nextTimer++; env.timers.push({ id, fn, ms }); return id; },
    clearTimeout(id) { env.timers = env.timers.filter((t) => t.id !== id); },
    observeSize(cb) { env.observers.push(cb); cb(321); return () => {}; },
    applyTheme(theme) { env.themes.push(theme); },
    onShortcut(fn) { env.shortcuts.push(fn); },
    // helpers
    deliver(data, origin = "http://shell.test") { env.listeners.forEach((fn) => fn(data, origin)); },
    tick() { const due = env.timers; env.timers = []; due.forEach((t) => t.fn()); },
    types() { return env.posted.map((p) => p.msg.type); },
    last(type) { return [...env.posted].reverse().find((p) => p.msg.type === type); },
    pressShortcut() { env.shortcuts.forEach((fn) => fn()); },
  };
  return env;
}

/* Enough of a document for Wicket.layout: elements that remember their
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

const shell = (msg) => Object.assign({ wicket: 1 }, msg);
const gate = (extra = {}) => Object.assign({ id: "g_1", type: "t", type_version: 1, title: "t", status: "pending", payload: {}, decision: null }, extra);
const init = (extra = {}) => shell(Object.assign({ type: "init", gate: gate(), previous: null, readonly: false, draft: null, shell_origin: "http://shell.test" }, extra));

module.exports = { Wicket, fakeEnv, fakeDocument, shell, gate, init };
