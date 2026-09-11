import {test} from "node:test"
import assert from "node:assert/strict"
import {readFileSync} from "node:fs"
import vm from "node:vm"
const source = readFileSync(new URL("./ui.js", import.meta.url), "utf8").replace("export function", "function")
function setup(initial = {}) {
  const stored = new Map(Object.entries(initial)), listeners = {}, attrs = {}, events = []
  // The root layout stamps these onto the document before this file runs, in
  // time for the first paint; from here on ui.js reads and writes them.
  const root = {dataset: {
    theme: stored.get("wicket:theme") === "light" ? "light" : "dark",
    sidebar: stored.get("wicket:sidebar") === "collapsed" ? "collapsed" : "expanded",
  }}
  const toggle = {setAttribute: (key, value) => {attrs[key] = value}}
  const document = {documentElement: root,
    addEventListener: (name, fn) => {listeners[name] = fn},
    querySelector: selector => selector === "[data-sidebar-toggle]" ? toggle : null,
    querySelectorAll: () => []}
  const context = vm.createContext({document,
    localStorage: {getItem: key => stored.get(key), setItem: (key, value) => stored.set(key, value)},
    matchMedia: () => ({matches: false}), queueMicrotask: fn => fn(),
    Event: class {constructor(type) {this.type = type}},
    CustomEvent: class {constructor(type, options) {this.type = type;this.detail = options.detail}},
    window: {addEventListener() {}, dispatchEvent: event => events.push(event)}})
  vm.runInContext(source, context)
  return {stored, root, attrs, listeners, events, context}
}
test("sidebar toggles, persists and updates its accessibility state", () => {
  const s = setup()
  vm.runInContext("sidebar()", s.context)
  assert.equal(s.root.dataset.sidebar, "collapsed")
  assert.equal(s.stored.get("wicket:sidebar"), "collapsed")
  assert.equal(s.attrs["aria-expanded"], "false")
  const restored = setup(Object.fromEntries(s.stored))
  assert.equal(restored.attrs["aria-expanded"], "false", "and the button says so on the next load")
})
test("theme changes notify plugins without replacing the page", () => {
  const s = setup()
  vm.runInContext("theme()", s.context)
  assert.equal(s.root.dataset.theme, "light")
  assert.equal(s.stored.get("wicket:theme"), "light")
  assert.equal(s.events.at(-1).type, "wicket:appearance")
})
test("single-key shortcuts do not capture typing or browser modifier shortcuts", () => {
  const s = setup()
  s.listeners.keydown({key: "t", target: {closest: () => ({})}})
  assert.equal(s.root.dataset.theme, "dark")
  s.listeners.keydown({key: "t", ctrlKey: true, target: {closest: () => null}})
  assert.equal(s.root.dataset.theme, "dark")
})
