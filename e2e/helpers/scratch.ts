import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const made: string[] = [];
process.on("exit", () => {
  for (const dir of made) fs.rmSync(dir, { recursive: true, force: true });
});

/** A fresh folder in the system's temp directory, removed when the process
 * running the tests exits. */
export function scratch(prefix: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  made.push(dir);
  return dir;
}
