// The previous round's payload read as this release's: it must not compile.
type Payload = { message: string };
type Decision = { ok: boolean };

Pinrail.connect<Payload, Decision>({
  onInit({ previous }) {
    const message: string = previous!.payload.message;
    void message;
  },
});
