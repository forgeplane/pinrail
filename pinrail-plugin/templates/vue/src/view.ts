// What the connection in main.ts asks of the view on screen, and the shapes
// of this plugin's payload, decision and draft.
export type Payload = { message: string };
export type Decision = { ok: boolean };
/** what is kept between reloads: the decision so far, answer still open */
export type Draft = { ok: boolean | null };

export const view = {
  /** the decision, or nothing while there is no answer to hand over */
  collect: (): Decision | undefined => undefined,
};

/** A draft kept by an earlier release may have another shape: use it only
 *  when it reads as this one's. */
export const draftOf = (kept: unknown): Draft =>
  kept && typeof kept === "object" && typeof (kept as Draft).ok === "boolean"
    ? { ok: (kept as Draft).ok }
    : { ok: null };
