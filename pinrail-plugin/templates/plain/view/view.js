// @ts-check
/// <reference path="../pinrail-plugin.d.ts" />
// __TITLE__: one question, yes or no. The answer
// is held here; the app's hand-over button (or ⌘/Ctrl+Enter) asks for it,
// and onCollect returns it. Replace render() and handOver() with your own.
"use strict";

// the shapes the schemas give, so the editor knows them too
/** @typedef {{ message: string }} Payload */
/** @typedef {{ ok: boolean }} Decision */

const view = Pinrail.layout();
view.content.className = "plugin-content dim";
view.content.textContent = "waiting for the shell…";
let choice = null;

/** @type {import("../pinrail-plugin").Plugin<Payload, Decision>} */
const plugin = Pinrail.connect({
  onInit({ draft }) {
    // a draft kept by an earlier release may have another shape
    const kept = /** @type {{ ok?: unknown } | null} */ (draft);
    choice = kept && typeof kept.ok === "boolean" ? kept.ok : null;
    render();
  },
  onSubmitted() {
    render();
  },
  onViolations(errors) {
    document.getElementById("errors").textContent = errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
  },
  // the decision, or nothing while the view needs more from the person
  onCollect() {
    return handOver();
  },
});

/** The decision, or nothing while there is no answer to hand over. */
function handOver() {
  if (choice === null) {
    document.getElementById("errors").textContent = "Choose yes or no first.";
    return;
  }
  return { ok: choice };
}

function pick(value) {
  choice = choice === value ? null : value;
  plugin.draft({ ok: choice });
  render();
}

function render() {
  const review = plugin.review;
  const decided = review.decision && review.decision.data;
  view.content.className = "plugin-content";
  // the redraw replaces the buttons: the one that had focus gets it back
  const focused = document.activeElement && document.activeElement.id;
  view.content.innerHTML =
    Pinrail.markdown(review.payload.message) +
    (plugin.readonly
      ? decided
        ? `<p class="dim">Decided: <b>${decided.ok ? "yes" : "no"}</b></p>`
        : // withdrawn or expired: nobody answered
          `<p class="dim">Closed without a decision (${Pinrail.escape(review.status)})</p>`
      : `<div class="choice">
         <button type="button" class="btn" id="yes" aria-pressed="${choice === true}">${Pinrail.icon("check")} Yes</button>
         <button type="button" class="btn" id="no" aria-pressed="${choice === false}">${Pinrail.icon("x")} No</button>
       </div>
       <div id="errors" class="errors" role="alert"></div>`);
  if (focused) document.getElementById(focused)?.focus();
  if (plugin.readonly) return;
  document.getElementById("yes").onclick = () => pick(true);
  document.getElementById("no").onclick = () => pick(false);
  plugin.handOverLabel(choice === null ? "Hand over" : `Hand over: ${choice ? "yes" : "no"}`);
}
