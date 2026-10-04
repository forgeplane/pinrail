// __TITLE__: one question, yes or no, in React.
// main.tsx connects to the app and renders this with what the app handed
// over (the review, whether it is read-only, the draft). The app's
// hand-over button (or ⌘/Ctrl+Enter) asks for the decision, and `view`
// answers with it. Replace the markup and the decision with your own.
//
// The SDK is on the window from the script tag in index.html; the types
// come from the package, so `review.payload` is your payload.
import { Check, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Init, Plugin } from "@forgeplane/pinrail-plugin/types";

export type Payload = { message: string };
export type Decision = { ok: boolean };
/** what is kept between reloads: the decision so far, answer still open */
type Draft = { ok: boolean | null };

const { Pinrail } = window;

/** What the connection in main.tsx asks of the view on screen. */
export const view = {
  /** the decision, or nothing while there is no answer to hand over */
  collect: (): Decision | undefined => undefined,
};

/** A draft kept by an earlier release may have another shape: use it only
 *  when it reads as this one's. */
const draftOf = (kept: unknown): Draft =>
  kept && typeof kept === "object" && typeof (kept as Draft).ok === "boolean"
    ? { ok: (kept as Draft).ok }
    : { ok: null };

export function App({ plugin, init }: { plugin: Plugin<Payload, Decision>; init: Init<Payload, Decision> }) {
  // the app closes the view once the decision is accepted: what it shows
  // is the review as init handed it over
  const { review, readonly } = init;
  const [draft, setDraft] = useState<Draft>(() => draftOf(init.draft));
  const [errors, setErrors] = useState("");
  // the connection calls `view` at any time, so it reads the draft from here
  const latest = useRef(draft);
  latest.current = draft;

  view.collect = () => {
    const { ok } = latest.current;
    if (ok === null) {
      setErrors("Choose yes or no first.");
      return;
    }
    return { ok };
  };

  // what the app's hand-over button says follows the answer
  useEffect(() => {
    if (readonly) return;
    plugin.handOverLabel(draft.ok === null ? "Hand over" : `Hand over: ${draft.ok ? "yes" : "no"}`);
  }, [plugin, readonly, draft.ok]);

  function pick(value: boolean) {
    const next = { ok: latest.current.ok === value ? null : value };
    setDraft(next);
    setErrors("");
    plugin.draft(next);
  }

  const decided = review.decision?.data;
  return (
    <main className="plugin-content">
      <div dangerouslySetInnerHTML={{ __html: Pinrail.markdown(review.payload.message) }} />
      {readonly && decided ? (
        <p className="dim">
          Decided: <b>{decided.ok ? "yes" : "no"}</b>
        </p>
      ) : readonly ? (
        // withdrawn or expired: nobody answered
        <p className="dim">Closed without a decision ({review.status})</p>
      ) : (
        <>
          <div className="choice">
            <button type="button" className="btn" id="yes" aria-pressed={draft.ok === true} onClick={() => pick(true)}>
              <Check /> Yes
            </button>
            <button type="button" className="btn" id="no" aria-pressed={draft.ok === false} onClick={() => pick(false)}>
              <X /> No
            </button>
          </div>
          <div id="errors" className="errors">
            {errors}
          </div>
        </>
      )}
    </main>
  );
}
