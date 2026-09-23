// Fills the app with the reviews in fixtures/, in the order and at the ages
// they say, then pins every time and id. A fixture is
// `{ plugin, title, origin, requested_by, age, payload }` and, for one that
// has ended, `decision` (with an optional `note` to the agent), `discard` or
// `withdraw` (the reason), with `decided` saying when. `revises` names the
// fixture a round answers. `age` and `decided` are how long before NOW, as
// "4m", "2h" or "3d". The optional plugins are installed from plugins/ first.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

const ms = (text) => {
  const m = /^(\d+)([smhd])$/.exec(text);
  if (!m) throw new Error(`an age is like "4m", not ${text}`);
  return Number(m[1]) * { s: 1_000, m: 60_000, h: 3_600_000, d: 86_400_000 }[m[2]];
};

/** The fixtures, by file name, in the order of their names. */
export function fixtures() {
  return fs
    .readdirSync(dir)
    .filter((f) => f.endsWith(".json"))
    .sort()
    .map((file) => ({ key: file.replace(/\.json$/, ""), ...JSON.parse(fs.readFileSync(path.join(dir, file), "utf8")) }));
}

/** The plugins the fixtures use beyond the built-in ones. */
const OPTIONAL = ["review", "email", "artifact"];

async function install(app, name) {
  const { job } = await app.api("POST", "/api/v1/plugins/install", { source: path.join(app.root, "plugins", name) });
  for (;;) {
    const state = await app.api("GET", `/api/v1/plugins/jobs/${job}`);
    if (state.status === "done") return;
    if (state.status === "failed") throw new Error(`installing ${name}: ${state.error}\n${state.log}`);
    await new Promise((r) => setTimeout(r, 250));
  }
}

/** Seeds every fixture and pins the run. Returns fixture key → review id. */
export async function seed(app) {
  for (const name of OPTIONAL) {
    console.log(`screenshots: installing ${name}`);
    await install(app, name);
  }
  const plan = {};
  const created = {};
  for (const f of fixtures()) {
    const review = await app.api("POST", "/api/v1/reviews", {
      plugin: f.plugin,
      title: f.title,
      origin: f.origin,
      requested_by: f.requested_by,
      payload: f.payload,
      ...(f.revises ? { revises: created[f.revises] } : {}),
    });
    created[f.key] = review.id;
    if (f.decision) await app.api("POST", `/api/v1/reviews/${review.id}/decision`, { data: f.decision, ...(f.note ? { agent_note: f.note } : {}) });
    if (f.discard) await app.api("POST", `/api/v1/reviews/${review.id}/discard`, { reason: f.discard });
    if (f.withdraw) await app.api("POST", `/api/v1/reviews/${review.id}/withdraw`, { reason: f.withdraw });
    plan[review.id] = { created: ms(f.age), ...(f.decided ? { outcome: ms(f.decided) } : {}) };
  }
  const ids = await app.pin(plan);
  return Object.fromEntries(Object.entries(created).map(([key, id]) => [key, ids.get(id)]));
}
