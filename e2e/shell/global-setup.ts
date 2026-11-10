import { core } from "./helpers";

/** The official plugins the specs use, installed once the core is up, as a
 *  person would choose them: the app installs none on its own. The usage
 *  data notice is answered. */
export default async function globalSetup() {
  for (const id of ["forgeplane/list", "forgeplane/feedback", "forgeplane/code-review"]) {
    const response = await fetch(`${core}/api/v1/plugins/install`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id }),
    });
    if (!response.ok) throw new Error(`installing ${id}: ${response.status} ${await response.text()}`);
  }
  // the usage data notice answered, as on a computer in use; its own spec
  // brings it back
  const answered = await fetch(`${core}/api/v1/settings`, {
    method: "PATCH",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ telemetry: { notice_shown: "2026-01-01T00:00:00Z" } }),
  });
  if (!answered.ok) throw new Error(`answering the usage data notice: ${answered.status}`);
}
