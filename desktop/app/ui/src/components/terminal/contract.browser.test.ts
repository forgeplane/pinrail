// The renderer contract, against the reference renderer that checks the
// contract itself, and against every registered renderer.
import { describeContract } from "./contract";
import { RENDERERS } from "./renderers";
import { referenceRenderer } from "./testing/reference";

describeContract("reference", async () => referenceRenderer);

for (const [id, make] of Object.entries(RENDERERS)) {
  describeContract(id, make);
}
