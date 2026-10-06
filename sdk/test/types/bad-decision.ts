// A decision of the wrong shape: it must not compile.
type Payload = { message: string };
type Decision = { ok: boolean };

Pinrail.connect<Payload, Decision>({
  onCollect() {
    return { ok: "yes" };
  },
});
