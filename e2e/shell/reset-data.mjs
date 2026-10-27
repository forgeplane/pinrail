// Empties the shell suite's data folder, given as the argument, and starts it
// as a returning person's: the setup and the release notes already seen, so
// their dialogs do not cover what the tests click.
import fs from "node:fs";
import path from "node:path";

const [dir] = process.argv.slice(2);
fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(dir, { recursive: true });
fs.writeFileSync(
  path.join(dir, "settings.json"),
  JSON.stringify({ welcome: { seen: true }, whats_new: { seen: "999.0.0" } }),
);
