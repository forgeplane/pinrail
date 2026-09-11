// The note to the agent is one line until it needs more, then grows with what
// is typed until the stylesheet's ceiling, after which it scrolls.
//
// The height is set from the content rather than guessed, so it is reapplied
// after every LiveView patch as well as on input: the form re-renders on
// change, and a patched textarea comes back without it.

export const NoteAutosize = {
  mounted() {
    this.fit = () => {
      const borders = this.el.offsetHeight - this.el.clientHeight
      this.el.style.height = "auto"
      this.el.style.height = `${this.el.scrollHeight + borders}px`
    }
    this.el.addEventListener("input", this.fit)
    this.fit()
  },

  updated() {
    this.fit()
  },

  destroyed() {
    this.el.removeEventListener("input", this.fit)
  },
}
