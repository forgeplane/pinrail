// The Content-Security-Policy the fake core sends with the bench page: the
// app's own, read from the Tauri configuration, so a renderer that needs
// more than the app allows shows it as violations in its results. It
// already allows the fake core's WebSockets on 127.0.0.1. Open a case with
// `&csp=off` to load it without a policy.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const config = join(dirname(fileURLToPath(import.meta.url)), "../../../../src-tauri/tauri.conf.json");

/** The policy for the page, or null without one. */
export function policy(mode = "app") {
  if (mode === "off") return null;
  return JSON.parse(readFileSync(config, "utf8")).app.security.csp;
}
