// Ship it? in TypeScript and nothing else: the page is drawn from the
// payload with template strings, and drawn again whenever the choice changes.
// The SDK is on the window from the script tag in index.html; the types come
// from the package.
import type { Init } from "@forgeplane/pinrail-plugin/types";
import { createElement, CircleCheck, CircleX, Hand, Rocket, type IconNode } from "lucide";

/** A Lucide icon as markup, for the HTML this view builds as a string; the
 *  class sizes it to the text, as the framework packages' icons are. */
const svg = (icon: IconNode) => createElement(icon, { class: "lucide", "aria-hidden": "true" }).outerHTML;

type Payload = {
  service: string;
  version: string;
  environment: string;
  changes: { title: string; risky?: boolean }[];
  checks: { name: string; passed: boolean; detail?: string }[];
};
type Verdict = "ship" | "hold";
type Decision = { verdict: Verdict; note?: string };
/** what is kept while the person decides: a verdict may not be chosen yet */
type Draft = { verdict: Verdict | null; note: string };

const { Pinrail } = window;
const app = document.getElementById("app")!;
const esc = Pinrail.escape;
let draft: Draft = { verdict: null, note: "" };
let error = "";

const plugin = Pinrail.connect<Payload, Decision>({
  onInit({ draft: kept }: Init<Payload, Decision>) {
    if (kept) draft = kept as Draft;
    render();
  },
  onCollect() {
    if (!draft.verdict) {
      error = "Choose ship or hold first.";
      return render();
    }
    const note = draft.note.trim();
    plugin.submit(note ? { verdict: draft.verdict, note } : { verdict: draft.verdict });
  },
  onViolations(errors) {
    error = errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
    render();
  },
  onSubmitted() {
    render();
  },
});

function choose(verdict: Verdict) {
  draft = { ...draft, verdict };
  error = "";
  plugin.draft(draft, { flush: true });
  render();
}

function render() {
  const { payload, decision } = plugin.review!;
  const status =
    draft.verdict === "ship"
      ? `Ship ${payload.version}`
      : draft.verdict === "hold"
        ? "Hold the deploy"
        : "Choose ship or hold";
  if (!plugin.readonly) plugin.status({ label: status });

  const decided = decision?.data;
  app.innerHTML = `
    <main class="plugin-content ship">
      <p class="eyebrow">Deploy to ${esc(payload.environment)}</p>
      <h1>${esc(payload.service)} <span class="meta">${esc(payload.version)}</span></h1>
      <section>
        <h2 class="eyebrow">Changes</h2>
        <ul aria-label="Changes">
          ${payload.changes.map((c) => `<li>${esc(c.title)}${c.risky ? ' <span class="sev sev-major">risky</span>' : ""}</li>`).join("")}
        </ul>
      </section>
      <section>
        <h2 class="eyebrow">Checks</h2>
        <ul aria-label="Checks">
          ${payload.checks.map((c) => `<li data-passed="${c.passed}">${svg(c.passed ? CircleCheck : CircleX)} ${esc(c.name)}${c.detail ? ` <span class="detail">${esc(c.detail)}</span>` : ""}</li>`).join("")}
        </ul>
      </section>
      ${
        plugin.readonly
          ? `<p class="decided"><b>${decided?.verdict === "ship" ? "Shipped" : "Held"}</b>${decided?.note ? `: ${esc(decided.note)}` : ""}</p>`
          : `<div class="choice" role="group" aria-label="Verdict">
            <button type="button" class="btn" data-verdict="ship" aria-pressed="${draft.verdict === "ship"}">${svg(Rocket)} Ship <kbd>s</kbd></button>
            <button type="button" class="btn" data-verdict="hold" aria-pressed="${draft.verdict === "hold"}">${svg(Hand)} Hold <kbd>h</kbd></button>
          </div>
          <textarea class="note" aria-label="Note to the agent" placeholder="A note for the agent (optional)">${esc(draft.note)}</textarea>
          <div class="errors" role="alert">${esc(error)}</div>`
      }
    </main>`;
}

app.addEventListener("click", (e) => {
  const button = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-verdict]");
  if (button && !plugin.readonly) choose(button.dataset.verdict as Verdict);
});
app.addEventListener("input", (e) => {
  const note = e.target as HTMLTextAreaElement;
  if (note.tagName !== "TEXTAREA") return;
  draft = { ...draft, note: note.value };
  plugin.draft(draft);
});
// s and h decide. The app forwards them too when it has the focus, as a
// keydown on the document itself, so the target is not always an element.
document.addEventListener("keydown", (e) => {
  const typing = e.target instanceof Element && e.target.closest("textarea, input");
  if (plugin.readonly || e.metaKey || e.ctrlKey || e.altKey || typing) return;
  if (e.key === "s") choose("ship");
  if (e.key === "h") choose("hold");
});
