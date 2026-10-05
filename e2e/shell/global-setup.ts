import { core } from "./helpers";

/** The official plugins the specs use, installed once the core is up, as a
 *  person would choose them: the app installs none on its own. */
export default async function globalSetup() {
  for (const id of ["forgeplane/list", "forgeplane/feedback"]) {
    const response = await fetch(`${core}/api/v1/plugins/install`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id }),
    });
    if (!response.ok) throw new Error(`installing ${id}: ${response.status} ${await response.text()}`);
  }
}
