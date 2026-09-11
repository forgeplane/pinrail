// The shell side of the plugin protocol. The element is the sandboxed
// <iframe>; it has an opaque origin, so messages to it use "*" and messages
// from it are trusted only when event.source is its window.
//
// Every message is {wicket: 1, type, ...}.
//   plugin -> shell: ready | resize {height | "fill"} | draft {data} | submit {data}
//   shell -> plugin: init {gate, previous, readonly, draft, shell_origin} |
//                    violations {errors} | submitted {decision} | collect |
//                    appearance {theme}

const DRAFT_PREFIX = "wicket:draft:"

export const PluginBridge = {
  mounted() {
    this.gateId = this.el.dataset.gateId
    this.minHeight = parseInt(this.el.dataset.minHeight || "400", 10)
    this.connected = true
    this.submitting = false
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
          this.pushEvent("plugin_ready", {})
          break
        case "resize":
          // The page is the same shape for every gate: a viewport-height
          // column with the frame taking what is left of it. "fill" uses all
          // of that; a reported height uses only as much as it needs, and is
          // capped, so the view scrolls inside rather than growing the page.
          if (msg.height === "fill") {
            this.el.style.height = "100%"
          } else if (typeof msg.height === "number" && Number.isFinite(msg.height)) {
            const height = Math.min(50000, Math.max(this.minHeight, Math.ceil(msg.height)))
            this.el.style.height = `min(${height}px, 100%)`
          }
          break
        case "draft":
          this.saveDraft(msg.data)
          break
        case "submit":
          if (!this.init || this.init.readonly || this.submitting) return
          if (!this.connected) {
            this.post({type: "violations", errors: [{path: "", message: "Reconnect before submitting. Your selections are preserved."}]})
            return
          }
          this.submitting = true
          // Read the DOM at commit time, not the last debounced server assign.
          const agentNote = document.getElementById("agent_note")?.value || ""
          this.pushEvent("submit", {data: msg.data, agent_note: agentNote}, () => { this.submitting = false })
          break
      }
    }
    window.addEventListener("message", this.onMessage)

    this.onKey = (event) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault()
        if (this.connected && this.init && !this.init.readonly && !this.submitting) this.post({type: "collect"})
      }
    }
    window.addEventListener("keydown", this.onKey)

    this.handleEvent("gate:init", (payload) => {
      this.init = payload
      this.sendInit()
    })
    this.handleEvent("gate:violations", ({errors}) => {
      this.submitting = false
      this.post({type: "violations", errors})
    })
    // The shell owns the theme; tell the view whenever it changes.
    this.onAppearance = () => this.sendAppearance()
    window.addEventListener("wicket:appearance", this.onAppearance)

    this.onInput = (event) => {
      if (event.target.id !== "agent_note") return
      try { sessionStorage.setItem(this.draftKey() + ":note", event.target.value) } catch { /* Optional draft. */ }
    }
    document.addEventListener("input", this.onInput)
    const note = document.getElementById("agent_note")
    try { if (note) note.value = sessionStorage.getItem(this.draftKey() + ":note") ?? note.value } catch { /* Optional draft. */ }
    // Only now start loading the bundle: the listener above is in place, so
    // the plugin's "ready" cannot race the hook.
    this.el.src = this.el.dataset.src

    this.handleEvent("gate:submitted", ({decision}) => {
      this.submitting = false
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
    window.removeEventListener("wicket:appearance", this.onAppearance)
    document.removeEventListener("input", this.onInput)
  },

  disconnected() { this.connected = false },

  reconnected() {
    this.connected = true
    this.submitting = false
    this.pushEvent("plugin_ready", {})
  },

  sendAppearance() {
    this.post({type: "appearance", theme: document.documentElement.dataset.theme === "light" ? "light" : "dark"})
  },

  sendInit() {
    if (!this.ready || !this.init) return
    const {gate, previous, readonly} = this.init
    this.sendAppearance()
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
    if (this.init?.readonly) return
    try {
      sessionStorage.setItem(this.draftKey(), JSON.stringify(data))
    } catch (_e) {
      // storage unavailable: drafts are a convenience
    }
  },

  clearDraft() {
    try {
      sessionStorage.removeItem(this.draftKey())
      sessionStorage.removeItem(this.draftKey() + ":note")
    } catch (_e) {
      // ignore
    }
  },
}
