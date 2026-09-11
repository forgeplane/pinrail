// The shell side of the plugin protocol. The element is the sandboxed
// <iframe>; it has an opaque origin, so messages to it use "*" and messages
// from it are trusted only when event.source is its window.
//
// Every message is {wicket: 1, type, ...}.
//   plugin -> shell: ready | resize {height} | draft {data} | submit {data}
//   shell -> plugin: init {gate, previous, readonly, draft, shell_origin} |
//                    violations {errors} | submitted {decision} | collect

const DRAFT_PREFIX = "wicket:draft:"

export const PluginBridge = {
  mounted() {
    this.gateId = this.el.dataset.gateId
    this.minHeight = parseInt(this.el.dataset.minHeight || "400", 10)
    this.ready = false
    this.init = null

    this.onMessage = (event) => {
      if (event.source !== this.el.contentWindow) return
      const msg = event.data
      if (!msg || msg.wicket !== 1) return
      switch (msg.type) {
        case "ready":
          this.ready = true
          this.sendInit()
          break
        case "resize":
          if (typeof msg.height === "number") {
            this.el.style.height = Math.max(this.minHeight, Math.ceil(msg.height)) + "px"
          }
          break
        case "draft":
          this.saveDraft(msg.data)
          break
        case "submit":
          this.pushEvent("submit", {data: msg.data})
          break
      }
    }
    window.addEventListener("message", this.onMessage)

    this.onKey = (event) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault()
        this.post({type: "collect"})
      }
    }
    window.addEventListener("keydown", this.onKey)

    this.handleEvent("gate:init", (payload) => {
      this.init = payload
      this.sendInit()
    })
    this.handleEvent("gate:violations", ({errors}) => this.post({type: "violations", errors}))
    // Only now start loading the bundle: the listener above is in place, so
    // the plugin's "ready" cannot race the hook.
    this.el.src = this.el.dataset.src

    this.handleEvent("gate:submitted", ({decision}) => {
      this.clearDraft()
      // keep the stored init current, so a frame reload after the decision
      // comes back read-only instead of editable
      if (this.init) {
        this.init = {...this.init, readonly: true, gate: {...this.init.gate, status: "decided", decision}}
      }
      this.post({type: "submitted", decision})
    })
  },

  destroyed() {
    window.removeEventListener("message", this.onMessage)
    window.removeEventListener("keydown", this.onKey)
  },

  sendInit() {
    if (!this.ready || !this.init) return
    const {gate, previous, readonly} = this.init
    this.post({
      type: "init",
      gate,
      previous,
      readonly,
      draft: readonly ? null : this.loadDraft(),
      shell_origin: window.location.origin,
    })
  },

  post(msg) {
    this.el.contentWindow.postMessage({wicket: 1, ...msg}, "*")
  },

  draftKey() {
    return DRAFT_PREFIX + this.gateId
  },

  loadDraft() {
    try {
      const raw = sessionStorage.getItem(this.draftKey())
      return raw ? JSON.parse(raw) : null
    } catch (_e) {
      return null
    }
  },

  saveDraft(data) {
    try {
      sessionStorage.setItem(this.draftKey(), JSON.stringify(data))
    } catch (_e) {
      // storage unavailable: drafts are a convenience
    }
  },

  clearDraft() {
    try {
      sessionStorage.removeItem(this.draftKey())
    } catch (_e) {
      // ignore
    }
  },
}
