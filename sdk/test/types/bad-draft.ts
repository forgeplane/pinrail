// A restored draft used without a check: it must not compile.
type Payload = { message: string };
type Decision = { ok: boolean };

Pinrail.connect<Payload, Decision>({
  onInit({ draft }) {
    const ok: boolean = draft.ok;
    void ok;
  },
});
