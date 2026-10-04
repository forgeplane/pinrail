// __TITLE__: one question, yes or no. The answer
// is held here; the app's hand-over button (or ⌘/Ctrl+Enter) sends `collect`
// and this view submits. Replace render() and handOver() with your own.
//
// The SDK is on the window from the script tag in index.html; the types
// come from the package, so `plugin.review.payload` is your payload.
import type { Init } from "@forgeplane/pinrail-plugin/types";
import { createElement, Check, X, type IconNode } from "lucide";

/** A Lucide icon as markup, for the HTML this view builds as a string; the
 *  class sizes it to the text, as the framework packages' icons are. */
const svg = (icon: IconNode) => createElement(icon, { class: "lucide", "aria-hidden": "true" }).outerHTML;

type Payload = { message: string };
type Decision = { ok: boolean };

const { Pinrail } = window;
const view = Pinrail.layout();
view.content.className = "plugin-content dim";
view.content.textContent = "waiting for the shell…";
let choice: boolean | null = null;

const plugin = Pinrail.connect<Payload, Decision>({
  onInit({ draft }: Init<Payload, Decision>) {
    choice = draft && typeof draft.ok === "boolean" ? draft.ok : null;
    render();
  },
  onSubmitted() {
    render();
  },
  onViolations(errors) {
    errorsEl().textContent = errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
  },
  onCollect() {
    handOver();
  },
});

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const errorsEl = () => el<HTMLDivElement>("errors");

function handOver() {
  if (choice === null) {
    errorsEl().textContent = "Choose yes or no first.";
    return;
  }
  plugin.submit({ ok: choice });
}

function pick(value: boolean) {
  choice = choice === value ? null : value;
  plugin.draft({ ok: choice }, { flush: true });
  render();
}

function render() {
  const review = plugin.review!;
  const decided = review.decision?.data;
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
         <button type="button" class="btn" id="yes" aria-pressed="${choice === true}">${svg(Check)} Yes</button>
         <button type="button" class="btn" id="no" aria-pressed="${choice === false}">${svg(X)} No</button>
       </div>
       <div id="errors" class="errors" role="alert"></div>`);
  if (focused) document.getElementById(focused)?.focus();
  if (plugin.readonly) return;
  el<HTMLButtonElement>("yes").onclick = () => pick(true);
  el<HTMLButtonElement>("no").onclick = () => pick(false);
  plugin.status({ label: choice === null ? "Hand over" : `Hand over: ${choice ? "yes" : "no"}` });
}
