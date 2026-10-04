// __TITLE__: one question, yes or no, in React.
// The SDK is connected once, when the view mounts; what it hands over (the
// review, whether it is read-only, the draft) becomes state. The app's
// hand-over button (or ⌘/Ctrl+Enter) sends `collect` and this view submits.
// Replace the markup and handOver with your own.
//
// The SDK is on the window from the script tag in index.html; the types
// come from the package, so `review.payload` is your payload.
import { Check, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Review, Plugin } from "@forgeplane/pinrail-plugin/types";

type Payload = { message: string };
type Decision = { ok: boolean };
/** what is kept between reloads: the decision so far, answer still open */
type Draft = { ok: boolean | null };

const { Pinrail } = window;

export function App() {
  const [review, setReview] = useState<Review<Payload, Decision> | null>(null);
  const [readonly, setReadonly] = useState(false);
  const [draft, setDraft] = useState<Draft>({ ok: null });
  const [errors, setErrors] = useState("");
  const plugin = useRef<Plugin<Payload, Decision> | null>(null);
  // the SDK's callbacks are made once, so they read the draft from here
  const latest = useRef(draft);
  latest.current = draft;

  useEffect(() => {
    plugin.current = Pinrail.connect<Payload, Decision>({
      onInit({ review, readonly, draft }) {
        setReview(review);
        setReadonly(readonly);
        const kept = draft as Draft | null;
        if (kept) setDraft({ ok: typeof kept.ok === "boolean" ? kept.ok : null });
      },
      onCollect() {
        const { ok } = latest.current;
        if (ok === null) return setErrors("Choose yes or no first.");
        plugin.current!.submit({ ok });
      },
      onViolations(errors) {
        setErrors(errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n"));
      },
      onSubmitted() {
        setReview({ ...plugin.current!.review! });
        setReadonly(true);
      },
    });
  }, []);

  // what the app's hand-over button says follows the answer
  useEffect(() => {
    if (!review || readonly) return;
    plugin.current!.status({ label: draft.ok === null ? "Hand over" : `Hand over: ${draft.ok ? "yes" : "no"}` });
  }, [review, readonly, draft.ok]);

  function pick(value: boolean) {
    const next = { ok: latest.current.ok === value ? null : value };
    setDraft(next);
    setErrors("");
    plugin.current!.draft(next, { flush: true });
  }

  if (!review) return <p className="plugin-content dim">waiting for the shell…</p>;
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
