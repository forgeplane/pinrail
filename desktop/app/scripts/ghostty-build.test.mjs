// The window's libghostty-vt is the build the host links: the same tag
// and commit, so the two never drift apart, and the file is the one its
// BUILD_INFO describes. It checks what scripts/ghostty-wasm.sh built, so
// it runs where the app is built, with Zig, and is skipped elsewhere.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";

const app = process.cwd();
/** The `key = "value"` lines of a file. */
const fields = (path) =>
  Object.fromEntries(
    [...readFileSync(path, "utf8").matchAll(/^(\w+) = "(.*)"$/gm)].map(([, key, value]) => [key, value]),
  );

const built = existsSync(join(app, "ui/public/ghostty/BUILD_INFO"));

describe.skipIf(!built)("libghostty-vt for the window, once built", () => {
  // a skipped describe still runs its body, without the file
  const info = built ? fields(join(app, "ui/public/ghostty/BUILD_INFO")) : {};
  const lock = fields(join(app, "../screen/ghostty.lock"));

  test("is built from the release the host pins", () => {
    expect(info.tag).toBe(lock.tag);
    expect(info.commit).toBe(lock.commit);
    expect(info.zig.startsWith(`${lock.zig}.`)).toBe(true);
    expect(info.target).toBe("wasm32-freestanding");
  });

  test("is the file its BUILD_INFO describes", () => {
    const wasm = readFileSync(join(app, "ui/public/ghostty/ghostty-vt.wasm"));
    expect(createHash("sha256").update(wasm).digest("hex")).toBe(info.sha256);
  });
});
