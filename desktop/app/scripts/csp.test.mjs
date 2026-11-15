// The window's Content-Security-Policy, in development and in release
// builds, lets the libghostty renderer compile its WebAssembly. Without
// it, the renderer fails to load and the window falls back to xterm.js
// without a word.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";

const security = JSON.parse(readFileSync(join(process.cwd(), "src-tauri/tauri.conf.json"), "utf8")).app.security;

/** The sources of one directive of a policy. */
function directive(policy, name) {
  const found = policy
    .split(";")
    .map((d) => d.trim().split(/\s+/))
    .find(([n]) => n === name);
  return found ? found.slice(1) : [];
}

describe.each([
  ["release", security.csp],
  ["development", security.devCsp],
])("the %s policy", (_, policy) => {
  test("lets the window compile WebAssembly", () => {
    expect(directive(policy, "script-src")).toContain("'wasm-unsafe-eval'");
  });
});
