// @ts-check
/// <reference path="../pinrail-plugin.d.ts" />
// __TITLE__: one question, yes or no, with an optional comment. The answer
// is held here; the app's hand-over button (or ⌘/Ctrl+Enter) sends `collect`
// and this view submits. Replace render() and handOver() with your own.
"use strict";

// the shapes the schemas give, so the editor knows them too
/** @typedef {{ message: string }} Payload */
/** @typedef {{ ok: boolean, comment?: string }} Decision */

const view = Pinrail.layout();
view.content.className = "plugin-content dim";
view.content.textContent = "waiting for the shell…";
let choice = null;

/** @type {import("../pinrail-plugin").Plugin<Payload, Decision>} */
const plugin = Pinrail.connect({
  onInit({ draft }) {
    choice = draft && typeof draft.ok === "boolean" ? draft.ok : null;
    render(draft);
  },
  onSubmitted() { render(); },
  onViolations(errors) {
    document.getElementById("errors").textContent = errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
  },
  onCollect() { handOver(); },
});

const comment = () => /** @type {HTMLInputElement} */ (document.getElementById("comment")).value.trim();

function handOver() {
  if (choice === null) {
    document.getElementById("errors").textContent = "Choose yes or no first.";
    return;
  }
  const note = comment();
  plugin.submit(note ? { ok: choice, comment: note } : { ok: choice });
}

function pick(value) {
  choice = choice === value ? null : value;
  plugin.draft({ ok: choice, comment: comment() }, { flush: true });
  render({ ok: choice, comment: comment() });
}

function render(draft) {
  const gate = plugin.gate;
  const decided = gate.decision && gate.decision.data;
  view.content.className = "plugin-content";
  view.content.innerHTML = Pinrail.markdown(gate.payload.message) + (plugin.readonly
    ? `<p class="dim">Decided: <b>${decided && decided.ok ? "yes" : "no"}</b>${decided && decided.comment ? " — " + Pinrail.escape(decided.comment) : ""}</p>`
    : `<div class="choice">
         <button type="button" class="btn" id="yes" aria-pressed="${choice === true}">${Pinrail.icon("check")} Yes</button>
         <button type="button" class="btn" id="no" aria-pressed="${choice === false}">${Pinrail.icon("x")} No</button>
       </div>
       <input class="field" id="comment" placeholder="comment (optional)" aria-label="comment" value="${Pinrail.escape((draft && draft.comment) || "")}">
       <div id="errors" class="errors"></div>`);
  if (plugin.readonly) return;
  document.getElementById("comment").oninput = () => plugin.draft({ ok: choice, comment: comment() });
  document.getElementById("yes").onclick = () => pick(true);
  document.getElementById("no").onclick = () => pick(false);
  plugin.status({ label: choice === null ? "Hand over" : `Hand over: ${choice ? "yes" : "no"}` });
}
