// __TITLE__: one question, yes or no, with an optional comment. The answer
// is held here; the app's hand-over button (or ⌘/Ctrl+Enter) sends `collect`
// and this view submits. Replace render() and handOver() with your own.
//
// The SDK is on the window from the script tag in index.html; the types
// come from the package, so `plugin.gate.payload` is your payload.
import type { Init } from "@forgeplane/pinrail-plugin/types";
import { createElement, Check, X, type IconNode } from "lucide";

/** A Lucide icon as markup, for the HTML this view builds as a string; the
 *  class sizes it to the text, as the framework packages' icons are. */
const svg = (icon: IconNode) => createElement(icon, { class: "lucide", "aria-hidden": "true" }).outerHTML;

type Payload = { message: string };
type Decision = { ok: boolean; comment?: string };
/** what is kept between reloads: the decision so far, answer still open */
type Draft = { ok: boolean | null; comment: string };

const { Pinrail } = window;
const view = Pinrail.layout();
view.content.className = "plugin-content dim";
view.content.textContent = "waiting for the shell…";
let choice: boolean | null = null;

const plugin = Pinrail.connect<Payload, Decision>({
  onInit({ draft }: Init<Payload, Decision>) {
    choice = draft && typeof draft.ok === "boolean" ? draft.ok : null;
    render(draft as Draft | null);
  },
  onSubmitted() { render(); },
  onViolations(errors) {
    errorsEl().textContent = errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
  },
  onCollect() { handOver(); },
});

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const errorsEl = () => el<HTMLDivElement>("errors");
const comment = () => el<HTMLInputElement>("comment").value.trim();

function handOver() {
  if (choice === null) {
    errorsEl().textContent = "Choose yes or no first.";
    return;
  }
  const note = comment();
  plugin.submit(note ? { ok: choice, comment: note } : { ok: choice });
}

function pick(value: boolean) {
  choice = choice === value ? null : value;
  plugin.draft({ ok: choice, comment: comment() }, { flush: true });
  render({ ok: choice, comment: comment() });
}

function render(draft?: Draft | null) {
  const gate = plugin.gate!;
  const decided = gate.decision?.data;
  view.content.className = "plugin-content";
  view.content.innerHTML = Pinrail.markdown(gate.payload.message) + (plugin.readonly
    ? (decided
      ? `<p class="dim">Decided: <b>${decided.ok ? "yes" : "no"}</b>${decided.comment ? " — " + Pinrail.escape(decided.comment) : ""}</p>`
      // withdrawn or expired: nobody answered
      : `<p class="dim">Closed without a decision (${Pinrail.escape(gate.status)})</p>`)
    : `<div class="choice">
         <button type="button" class="btn" id="yes" aria-pressed="${choice === true}">${svg(Check)} Yes</button>
         <button type="button" class="btn" id="no" aria-pressed="${choice === false}">${svg(X)} No</button>
       </div>
       <input class="field" id="comment" placeholder="comment (optional)" aria-label="comment" value="${Pinrail.escape(draft?.comment ?? "")}">
       <div id="errors" class="errors"></div>`);
  if (plugin.readonly) return;
  el<HTMLInputElement>("comment").oninput = () => plugin.draft({ ok: choice, comment: comment() });
  el<HTMLButtonElement>("yes").onclick = () => pick(true);
  el<HTMLButtonElement>("no").onclick = () => pick(false);
  plugin.status({ label: choice === null ? "Hand over" : `Hand over: ${choice ? "yes" : "no"}` });
}
