// A view written against the types as an author would: it must compile.
import type { Init, Review } from "../../types";

type Payload = { message: string };
type Decision = { ok: boolean };
type Draft = { ok: boolean | null };

/** a draft from an earlier release may have another shape */
const draftOf = (kept: unknown): Draft =>
  kept && typeof kept === "object" && typeof (kept as Draft).ok === "boolean"
    ? { ok: (kept as Draft).ok }
    : { ok: null };

let draft: Draft = { ok: null };
let review: Review<Payload, Decision> | null = null;

const plugin = Pinrail.connect<Payload, Decision>({
  onInit(init: Init<Payload, Decision>) {
    review = init.review;
    draft = draftOf(init.draft);
    const message: string = init.review.payload.message;
    const names: string[] = init.review.attachments.map((a) => a.name);
    const decided: boolean | undefined = init.review.decision?.data.ok;
    // the previous round's decision is checked before it is read
    const earlier = init.previous?.decision?.data;
    if (earlier && typeof earlier === "object" && "ok" in earlier) void earlier.ok;
    void [message, names, decided];
  },
  onCollect() {
    if (draft.ok === null) return;
    return { ok: draft.ok };
  },
  onError(error) {
    void error.message;
  },
});
plugin.draft(draft);
void review;
