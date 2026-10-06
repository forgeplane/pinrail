// A plugin as `pinrail plugins new` writes it with the plain template,
// from the CLI's templates in this repository, for the tests that need a
// whole plugin folder to serve. It fills in the name and the title as the
// CLI does, without the CLI.
const fs = require("node:fs");
const path = require("node:path");

const templates = path.resolve(__dirname, "..", "..", "cli", "templates");

/** "ticket_triage" → "Ticket triage" */
const titleOf = (name) => name.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());

/** Writes the plain plugin `name` into `dir`, and returns `dir`. */
function plainPlugin(name, dir) {
  for (const layer of ["common", "plain"]) {
    const root = path.join(templates, layer);
    for (const rel of fs.readdirSync(root, { recursive: true })) {
      const from = path.join(root, rel);
      if (fs.statSync(from).isDirectory()) continue;
      const to = path.join(dir, rel.replace(/__NAME__/g, name));
      fs.mkdirSync(path.dirname(to), { recursive: true });
      const text = fs
        .readFileSync(from, "utf8")
        .replace(/__NAME__/g, name)
        .replace(/__TITLE__/g, titleOf(name));
      fs.writeFileSync(to, text);
    }
  }
  fs.copyFileSync(path.resolve(__dirname, "..", "types.d.ts"), path.join(dir, "pinrail-plugin.d.ts"));
  return dir;
}

module.exports = { plainPlugin };
