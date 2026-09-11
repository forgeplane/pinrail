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
  const document = {
    documentElement: {dataset: {theme: "light"}},
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
  return {hook, message, handlers, pushed, sent, storage, listeners, contentWindow}
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

test("foreign frames are ignored and invalid resize values cannot corrupt frame height", () => {
  const {hook, listeners, message, pushed} = setup()
  listeners.get("message")({source: {}, data: {wicket: 1, type: "submit", data: {ok: true}}})
  assert.equal(pushed.filter(x => x.name === "submit").length, 0)
  message({type: "resize", height: 500})
  message({type: "resize", height: Infinity})
  assert.equal(hook.el.style.height, "500px")
  hook.destroyed()
  assert.equal(listeners.size, 0)
})
