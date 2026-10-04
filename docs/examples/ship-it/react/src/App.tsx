// Ship it? in React. main.tsx connects to the app and renders this with what
// the app handed over (the review, whether it is read-only, the draft). The
// app's hand-over button asks for the decision, and `view` answers with it.
// The SDK is on the window from the script tag in index.html; the types come
// from the package.
import { CircleCheck, CircleX, Hand, Rocket } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Init, Plugin } from "@forgeplane/pinrail-plugin/types";

export type Payload = {
  service: string;
  version: string;
  environment: string;
  changes: { title: string; risky?: boolean }[];
  checks: { name: string; passed: boolean; detail?: string }[];
};
type Verdict = "ship" | "hold";
export type Decision = { verdict: Verdict; note?: string };
/** what is kept while the person decides: a verdict may not be chosen yet */
type Draft = { verdict: Verdict | null; note: string };

/** What the connection in main.tsx asks of the view on screen. */
export const view = {
  /** the decision, or nothing while there is no verdict to hand over */
  collect: (): Decision | undefined => undefined,
};

/** A draft kept by an earlier release may have another shape: use only what
 *  reads as this one's. */
function draftOf(kept: unknown): Draft {
  const d = kept && typeof kept === "object" ? (kept as Partial<Draft>) : {};
  return {
    verdict: d.verdict === "ship" || d.verdict === "hold" ? d.verdict : null,
    note: typeof d.note === "string" ? d.note : "",
  };
}

export function App({ plugin, init }: { plugin: Plugin<Payload, Decision>; init: Init<Payload, Decision> }) {
  // the app closes the view once the decision is accepted: what it shows
  // is the review as init handed it over
  const { review, readonly } = init;
  const [draft, setDraft] = useState<Draft>(() => draftOf(init.draft));
  const [error, setError] = useState("");
  // the connection calls `view` at any time, so it reads the draft from here
  const latest = useRef(draft);
  latest.current = draft;

  view.collect = () => {
    const { verdict, note } = latest.current;
    if (!verdict) {
      setError("Choose ship or hold first.");
      return;
    }
    return note.trim() ? { verdict, note: note.trim() } : { verdict };
  };

  function choose(verdict: Verdict) {
    const next = { ...latest.current, verdict };
    setDraft(next);
    setError("");
    plugin.draft(next);
  }

  function writeNote(note: string) {
    const next = { ...latest.current, note };
    setDraft(next);
    plugin.draft(next);
  }

  // what the app's hand-over button says follows the choice
  useEffect(() => {
    if (readonly) return;
    const label =
      draft.verdict === "ship"
        ? `Ship ${review.payload.version}`
        : draft.verdict === "hold"
          ? "Hold the deploy"
          : "Choose ship or hold";
    plugin.handOverLabel(label);
  }, [plugin, review, readonly, draft.verdict]);

  // s and h decide. The app forwards them too when it has the focus, as a
  // keydown on the document itself, so the target is not always an element.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof Element && e.target.closest("textarea, input");
      if (readonly || e.metaKey || e.ctrlKey || e.altKey || typing) return;
      if (e.key === "s") choose("ship");
      if (e.key === "h") choose("hold");
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [readonly]);

  const { payload } = review;
  const decided = review.decision?.data;
  return (
    <main className="plugin-content ship">
      <p className="eyebrow">Deploy to {payload.environment}</p>
      <h1>
        {payload.service} <span className="meta">{payload.version}</span>
      </h1>
      <section>
        <h2 className="eyebrow">Changes</h2>
        <ul aria-label="Changes">
          {payload.changes.map((c) => (
            <li key={c.title}>
              {c.title}
              {c.risky ? (
                <>
                  {" "}
                  <span className="sev sev-major">risky</span>
                </>
              ) : null}
            </li>
          ))}
        </ul>
      </section>
      <section>
        <h2 className="eyebrow">Checks</h2>
        <ul aria-label="Checks">
          {payload.checks.map((c) => (
            <li key={c.name} data-passed={String(c.passed)}>
              {c.passed ? <CircleCheck /> : <CircleX />} {c.name}
              {c.detail ? (
                <>
                  {" "}
                  <span className="detail">{c.detail}</span>
                </>
              ) : null}
            </li>
          ))}
        </ul>
      </section>
      {readonly ? (
        <p className="decided">
          <b>{decided?.verdict === "ship" ? "Shipped" : "Held"}</b>
          {decided?.note ? `: ${decided.note}` : null}
        </p>
      ) : (
        <>
          <div className="choice" role="group" aria-label="Verdict">
            <button
              type="button"
              className="btn"
              aria-pressed={draft.verdict === "ship"}
              onClick={() => choose("ship")}
            >
              <Rocket /> Ship <kbd>s</kbd>
            </button>
            <button
              type="button"
              className="btn"
              aria-pressed={draft.verdict === "hold"}
              onClick={() => choose("hold")}
            >
              <Hand /> Hold <kbd>h</kbd>
            </button>
          </div>
          <textarea
            className="note"
            aria-label="Note to the agent"
            placeholder="A note for the agent (optional)"
            value={draft.note}
            onChange={(e) => writeNote(e.target.value)}
          />
          <div className="errors" role="alert">
            {error}
          </div>
        </>
      )}
    </main>
  );
}
