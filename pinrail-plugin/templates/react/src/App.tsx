// __TITLE__: one question, yes or no, with an optional comment, in React.
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
type Decision = { ok: boolean; comment?: string };
/** what is kept between reloads: the decision so far, answer still open */
type Draft = { ok: boolean | null; comment: string };

const { Pinrail } = window;

export function App() {
  const [review, setGate] = useState<Review<Payload, Decision> | null>(null);
  const [readonly, setReadonly] = useState(false);
  const [draft, setDraft] = useState<Draft>({ ok: null, comment: "" });
  const [errors, setErrors] = useState("");
  const plugin = useRef<Plugin<Payload, Decision> | null>(null);
  // the SDK's callbacks are made once, so they read the draft from here
  const latest = useRef(draft);
  latest.current = draft;

  useEffect(() => {
    plugin.current = Pinrail.connect<Payload, Decision>({
      onInit({ review, readonly, draft }) {
        setGate(review);
        setReadonly(readonly);
        const kept = draft as Draft | null;
        if (kept) setDraft({ ok: typeof kept.ok === "boolean" ? kept.ok : null, comment: kept.comment ?? "" });
      },
      onCollect() {
        const { ok, comment } = latest.current;
        if (ok === null) return setErrors("Choose yes or no first.");
        plugin.current!.submit(comment.trim() ? { ok, comment: comment.trim() } : { ok });
      },
      onViolations(errors) {
        setErrors(errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n"));
      },
      onSubmitted() {
        setGate({ ...plugin.current!.review! });
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
    const next = { ...latest.current, ok: latest.current.ok === value ? null : value };
    setDraft(next);
    setErrors("");
    plugin.current!.draft(next, { flush: true });
  }

  function writeComment(comment: string) {
    const next = { ...latest.current, comment };
    setDraft(next);
    plugin.current!.draft(next);
  }

  if (!review) return <p className="plugin-content dim">waiting for the shell…</p>;
  const decided = review.decision?.data;
  return (
    <main className="plugin-content">
      <div dangerouslySetInnerHTML={{ __html: Pinrail.markdown(review.payload.message) }} />
      {readonly && decided ? (
        <p className="dim">
          Decided: <b>{decided.ok ? "yes" : "no"}</b>
          {decided.comment ? ` — ${decided.comment}` : null}
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
          <input
            className="field"
            id="comment"
            placeholder="comment (optional)"
            aria-label="comment"
            value={draft.comment}
            onChange={(e) => writeComment(e.target.value)}
          />
          <div id="errors" className="errors">
            {errors}
          </div>
        </>
      )}
    </main>
  );
}
