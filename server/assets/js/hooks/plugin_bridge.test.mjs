import {test} from "node:test"
import assert from "node:assert/strict"
import {readFileSync} from "node:fs"
import vm from "node:vm"

const source = readFileSync(new URL("./plugin_bridge.js", import.meta.url), "utf8")
  .replace("export const PluginBridge =", "globalThis.PluginBridge =")

function setup() {
  const listeners = new Map(), handlers = new Map(), pushed = [], sent = [], storage = new Map()
  const note = {value: "latest unsent note"}
  const contentWindow = {postMessage: (message) => sent.push(message)}
  const window = {
    addEventListener: (name, fn) => listeners.set(name, fn),
    removeEventListener: name => listeners.delete(name),
    location: {origin: "http://localhost:4747"}, innerHeight: 900, scrollY: 0
  }
  const body = {
    dataset: {},
    removeAttribute: (name) => delete body.dataset[name.replace(/^data-(.*)$/, (_, rest) => rest.replace(/-(.)/g, (_, c) => c.toUpperCase()))]
  }
  const document = {
    documentElement: {dataset: {theme: "light"}},
    body,
    getElementById: () => note,
    addEventListener() {}, removeEventListener() {}
  }
  const context = vm.createContext({window, document, console, setTimeout,
    sessionStorage: {getItem: k => storage.get(k) ?? null, setItem: (k,v) => storage.set(k,v), removeItem: k => storage.delete(k)}})
  vm.runInContext(source, context)
  const hook = Object.assign({}, context.PluginBridge, {
    el: {dataset: {gateId: "gate-1", minHeight: "400", src: "/plugin.html"}, contentWindow, style: {}, getBoundingClientRect: () => ({top: 100})},
    handleEvent: (name, fn) => handlers.set(name, fn),
    pushEvent: (name, payload, callback) => pushed.push({name, payload, callback})
  })
  hook.mounted()
  const message = data => listeners.get("message")({source: contentWindow, data: {wicket: 1, ...data}})
  handlers.get("gate:init")({gate: {id: "gate-1"}, readonly: false})
  message({type: "ready"})
  return {hook, message, handlers, pushed, sent, storage, listeners, contentWindow, body}
}

test("submission includes the current note and suppresses a second in-flight submit", () => {
  const {message, pushed} = setup()
  message({type: "submit", data: {ok: true}})
  message({type: "submit", data: {ok: false}})
  const submits = pushed.filter(x => x.name === "submit")
  assert.equal(submits.length, 1)
  assert.equal(submits[0].payload.agent_note, "latest unsent note")
})

test("disconnected and read-only gates cannot submit; reconnect requests fresh state", () => {
  const {hook, message, pushed, handlers, sent} = setup()
  hook.disconnected()
  message({type: "submit", data: {ok: true}})
  assert.equal(pushed.filter(x => x.name === "submit").length, 0)
  assert.equal(sent.at(-1).type, "violations")
  hook.reconnected()
  assert.equal(pushed.at(-1).name, "plugin_ready")
  handlers.get("gate:init")({gate: {id: "gate-1"}, readonly: true})
  message({type: "submit", data: {ok: true}})
  assert.equal(pushed.filter(x => x.name === "submit").length, 0)
})

test("a draft is stored per gate and cleared once the decision is in", () => {
  const {handlers, message, storage} = setup()
  message({type: "draft", data: {choice: "reject"}})
  assert.equal(storage.get("wicket:draft:gate-1"), JSON.stringify({choice: "reject"}))
  handlers.get("gate:submitted")({decision: {data: {}}})
  assert.equal(storage.has("wicket:draft:gate-1"), false)
})

test("a reported height is used but capped, and fill takes the whole slot", () => {
  const {hook, message} = setup()

  message({type: "resize", height: 620})
  assert.equal(hook.el.style.height, "min(620px, 100%)", "short content stays short")

  message({type: "resize", height: 120})
  assert.equal(hook.el.style.height, "min(400px, 100%)", "never below the view's minimum")

  message({type: "resize", height: "fill"})
  assert.equal(hook.el.style.height, "100%")
})

test("the view is told the theme, without a re-init or a lost draft", () => {
  const {hook, message, sent, storage} = setup()
  message({type: "draft", data: {choice: "reject"}})
  const draft = storage.get("wicket:draft:gate-1")
  const inits = sent.filter(x => x.type === "init").length

  hook.onAppearance()

  assert.equal(sent.at(-1).type, "appearance")
  assert.equal(sent.at(-1).theme, "light")
  assert.equal(sent.filter(x => x.type === "init").length, inits)
  assert.equal(storage.get("wicket:draft:gate-1"), draft)
})

test("foreign frames are ignored and invalid resize values cannot corrupt frame height", () => {
  const {hook, listeners, message, pushed} = setup()
  listeners.get("message")({source: {}, data: {wicket: 1, type: "submit", data: {ok: true}}})
  assert.equal(pushed.filter(x => x.name === "submit").length, 0)
  message({type: "resize", height: 500})
  message({type: "resize", height: Infinity})
  assert.equal(hook.el.style.height, "min(500px, 100%)")
  hook.destroyed()
  assert.equal(listeners.size, 0)
})
