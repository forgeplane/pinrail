// Shell preferences never touch LiveView or reload an opaque plugin frame.
const root = document.documentElement
const read = (key) => { try { return localStorage.getItem(key) } catch { return null } }
const write = (key, value) => { try { localStorage.setItem(key, value) } catch { /* Optional preference. */ } }
root.dataset.theme = read("wicket:theme") === "light" ? "light" : "dark"
root.dataset.sidebar = read("wicket:sidebar") || (matchMedia("(max-width: 760px)").matches ? "collapsed" : "expanded")

function syncChrome() {
  document.querySelector("[data-sidebar-toggle]")?.setAttribute("aria-expanded", String(root.dataset.sidebar !== "collapsed"))
  document.querySelector("[data-theme-toggle]")?.setAttribute("aria-label", `Switch to ${root.dataset.theme === "dark" ? "light" : "dark"} theme`)
  const label = document.querySelector("[data-connection-label]")
  if (label) label.textContent = root.dataset.connected === "true" ? "Connected to localhost" : "Reconnecting…"
}
function sidebar() {
  root.dataset.sidebar = root.dataset.sidebar === "collapsed" ? "expanded" : "collapsed"
  write("wicket:sidebar", root.dataset.sidebar)
  syncChrome()
  window.dispatchEvent(new Event("resize"))
}
function theme() {
  root.dataset.theme = root.dataset.theme === "dark" ? "light" : "dark"
  write("wicket:theme", root.dataset.theme)
  syncChrome()
  window.dispatchEvent(new CustomEvent("wicket:appearance", {detail: {theme: root.dataset.theme}}))
}
let returnFocus
function shortcuts() {
  const dialog = document.getElementById("keyboard-help")
  if (!dialog || dialog.open) return
  returnFocus = document.activeElement
  dialog.showModal()
  dialog.addEventListener("close", () => returnFocus?.isConnected && returnFocus.focus(), {once: true})
}
document.addEventListener("click", (event) => {
  if (event.target.closest("[data-sidebar-toggle]")) sidebar()
  if (event.target.closest("[data-theme-toggle]")) theme()
  if (event.target.closest("[data-shortcuts]")) shortcuts()
  if (event.target.closest("[data-close-dialog]")) event.target.closest("dialog")?.close()
})
document.addEventListener("keydown", (event) => {
  if (event.defaultPrevented || event.isComposing || document.querySelector("dialog[open], [role=dialog]")) return
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "b") {
    event.preventDefault(); sidebar(); return
  }
  if (event.metaKey || event.ctrlKey || event.altKey || event.target.closest("input,textarea,select,[contenteditable=true]")) return
  if (event.key === "t") theme()
  if (event.key === "?") shortcuts()
  if (event.key === "/") {
    const input = document.getElementById("inbox_q")
    if (input) { event.preventDefault(); input.focus() }
  }
  if (["j", "k"].includes(event.key)) {
    const rows = [...document.querySelectorAll("a[data-gate-row]")]
    if (!rows.length) return
    const current = rows.indexOf(document.activeElement)
    const next = current < 0 ? (event.key === "j" ? 0 : rows.length - 1) : (current + (event.key === "j" ? 1 : -1) + rows.length) % rows.length
    event.preventDefault(); rows[next].focus()
  }
})
window.addEventListener("storage", (event) => {
  if (event.key === "wicket:theme" && ["dark", "light"].includes(event.newValue)) {
    root.dataset.theme = event.newValue
    window.dispatchEvent(new CustomEvent("wicket:appearance", {detail: {theme: event.newValue}}))
  }
  if (event.key === "wicket:sidebar" && ["expanded", "collapsed"].includes(event.newValue)) root.dataset.sidebar = event.newValue
  syncChrome()
})
window.addEventListener("phx:page-loading-stop", syncChrome)
queueMicrotask(syncChrome)

export function connectionChanged(connected) {
  root.dataset.connected = String(connected)
  syncChrome()
}
