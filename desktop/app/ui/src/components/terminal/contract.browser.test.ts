// The renderer contract, against the reference renderer that checks the
// contract itself, and against every registered renderer, with the binary
// snapshot the host writes (screen/tests/snapshot.rs).
import { describeContract } from "./contract";
import { RENDERERS } from "./renderers";
import snapshotUrl from "./testing/ghostty-snapshot.bin?url";
import host from "./testing/ghostty-snapshot.json";
import { referenceRenderer } from "./testing/reference";

const snapshot = new Uint8Array(await (await fetch(snapshotUrl)).arrayBuffer());

describeContract("reference", async () => referenceRenderer);

for (const [id, make] of Object.entries(RENDERERS)) {
  describeContract(id, make, { snapshots: { [host.format]: snapshot } });
}
