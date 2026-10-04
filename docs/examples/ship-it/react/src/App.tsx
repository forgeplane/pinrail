// Ship it? in React. The SDK is connected once, when the view mounts; what it
// hands over (the review, whether it is read-only, the draft) becomes state,
// and the page renders from it. The SDK is on the window from the script tag
// in index.html; the types come from the package.
import { CircleCheck, CircleX, Hand, Rocket } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Review, Plugin } from "@forgeplane/pinrail-plugin/types";

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

export function App() {
  const [review, setReview] = useState<Review<Payload, Decision> | null>(null);
  const [readonly, setReadonly] = useState(false);
  const [draft, setDraft] = useState<Draft>({ verdict: null, note: "" });
  const [error, setError] = useState("");
  const plugin = useRef<Plugin<Payload, Decision> | null>(null);
  // the SDK's callbacks are made once, so they read the draft from here
  const latest = useRef(draft);
  latest.current = draft;

  useEffect(() => {
    plugin.current = Pinrail.connect<Payload, Decision>({
      onInit({ review, readonly, draft }) {
        setReview(review);
        setReadonly(readonly);
        if (draft) setDraft(draft as Draft);
      },
      // the decision, or nothing while there is no verdict to hand over
      onCollect() {
        const { verdict, note } = latest.current;
        if (!verdict) {
          setError("Choose ship or hold first.");
          return;
        }
        return note.trim() ? { verdict, note: note.trim() } : { verdict };
      },
      onViolations(errors) {
        setError(errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n"));
      },
      onSubmitted() {
        setReview({ ...plugin.current!.review! });
        setReadonly(true);
      },
    });
  }, []);

  function choose(verdict: Verdict) {
    const next = { ...latest.current, verdict };
    setDraft(next);
    setError("");
    plugin.current!.draft(next, { flush: true });
  }

  function writeNote(note: string) {
    const next = { ...latest.current, note };
    setDraft(next);
    plugin.current!.draft(next);
  }

  // what the app's hand-over button says follows the choice
  useEffect(() => {
    if (!review || readonly) return;
    const label =
      draft.verdict === "ship"
        ? `Ship ${review.payload.version}`
        : draft.verdict === "hold"
          ? "Hold the deploy"
          : "Choose ship or hold";
    plugin.current!.status({ label });
  }, [review, readonly, draft.verdict]);

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

  if (!review) return null;
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
