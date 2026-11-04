// Fills the app with the reviews in fixtures/, in the order and at the ages
// they say, then pins every time and id. A fixture is
// `{ plugin, title, origin, requested_by, age, payload }`, with `kind` in
// place of `plugin` for questions, and, for one that has ended, `decision`
// (with an optional `note` to the agent), `discard` or `withdraw` (the
// reason), with `decided` saying when. `revises` names the
// fixture a round answers. A fixture with `status` in place of a review is
// what the agent `requested_by` reported in its `session`, with `pinrail
// status`, with `start` when it starts the task the status names,
// `done` when it said it had finished, and `note` for a note, whose
// `attachments` are uploaded as a review's are; it goes into the
// session's thread in the order of the fixtures' names. `age` and `decided`
// are how long before NOW, as
// "4m", "2h" or "3d". `attachments` names files the review carries by path,
// relative to the fixture, `{ "pivot.glb": { "path": "…" } }`: each is
// uploaded first. The plugins they use are installed first: the official
// ones the app carries by their ids, and the rest from a checkout of
// forgeplane/pinrail-plugins beside this repository, or from
// PINRAIL_PLUGINS_DIR, with artifact and model built.

import crypto from "node:crypto";
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
    .map((file) => ({
      key: file.replace(/\.json$/, ""),
      ...JSON.parse(fs.readFileSync(path.join(dir, file), "utf8")),
    }));
}

/** The official plugins the app carries that the fixtures use. */
const CARRIED = [
  "forgeplane/list",
  "forgeplane/feedback",
  "forgeplane/code-review",
  "forgeplane/image",
  "forgeplane/markdown",
  "forgeplane/change-map",
];

/** The plugins the fixtures use from forgeplane/pinrail-plugins. */
const OPTIONAL = [
  "email",
  "artifact",
  "db-schema",
  "canvas",
  "logo",
  "calendar",
  "model-3d",
  "audio",
  "visual-diff",
  "video",
];

/** Where the sample plugins are: the plugins folder of their checkout. */
const samplePlugins = (app) =>
  path.join(process.env.PINRAIL_PLUGINS_DIR ?? path.join(app.root, "..", "pinrail-plugins"), "plugins");

/** Installs one of the sample plugins; artifact's and model-3d's views must be built first. */
async function install(app, name) {
  const source = path.join(samplePlugins(app), name);
  if (!fs.existsSync(path.join(source, "manifest.json"))) {
    throw new Error(
      `${source} is not a plugin: check out forgeplane/pinrail-plugins beside this repository, or set PINRAIL_PLUGINS_DIR, and build artifact and model-3d there`,
    );
  }
  await app.api("POST", "/api/v1/plugins/install", { source });
}

/** Uploads the files a fixture names, and says what the review carries. */
async function upload(app, spec) {
  const carried = {};
  for (const [name, entry] of Object.entries(spec || {})) {
    const bytes = fs.readFileSync(path.resolve(dir, entry.path));
    const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
    const res = await fetch(`${app.core}/api/v1/attachments/${sha256}`, {
      method: "PUT",
      headers: { "content-type": "application/octet-stream" },
      body: bytes,
    });
    if (!res.ok) throw new Error(`uploading ${name}: ${res.status} ${await res.text()}`);
    carried[name] = {
      sha256,
      size: bytes.length,
      media_type: entry.media_type ?? (name.endsWith(".glb") ? "model/gltf-binary" : "application/octet-stream"),
    };
  }
  return carried;
}

/** Seeds every fixture and pins the run. Returns fixture key → review id. */
export async function seed(app) {
  for (const id of CARRIED) {
    console.log(`screenshots: installing ${id}`);
    await app.api("POST", "/api/v1/plugins/install", { id });
  }
  for (const name of OPTIONAL) {
    console.log(`screenshots: installing ${name}`);
    await install(app, name);
  }
  const plan = {};
  const created = {};
  const statuses = [];
  for (const f of fixtures()) {
    if (f.status !== undefined) {
      await app.api("POST", `/api/v1/sessions/${f.requested_by}/${f.session}/status`, {
        text: f.status,
        kind: f.note ? "note" : f.done ? "done" : f.start ? "start" : "step",
        origin: f.origin,
        ...(f.attachments ? { attachments: await upload(app, f.attachments) } : {}),
      });
      statuses.push(ms(f.age));
      continue;
    }
    const review = await app.api("POST", "/api/v1/reviews", {
      // questions asked with `pinrail ask` have a kind and no plugin
      ...(f.kind ? { kind: f.kind } : { plugin: f.plugin }),
      title: f.title,
      origin: f.origin,
      requested_by: f.requested_by,
      ...(f.session ? { session: f.session } : {}),
      payload: f.payload,
      ...(f.attachments ? { attachments: await upload(app, f.attachments) } : {}),
      ...(f.revises ? { revises: created[f.revises] } : {}),
    });
    created[f.key] = review.id;
    // `decided_with`, for a decision not made in the app, names the client
    // that sent it and the user and session that the client reports
    if (f.decision)
      await app.api(
        "POST",
        `/api/v1/reviews/${review.id}/decision`,
        { data: f.decision, ...(f.note ? { agent_note: f.note } : {}), ...(f.decided_with?.says ?? {}) },
        f.decided_with?.client,
      );
    if (f.discard) await app.api("POST", `/api/v1/reviews/${review.id}/discard`, { reason: f.discard });
    // the requesting agent withdraws the review with the pinrail command
    if (f.withdraw)
      await app.api("POST", `/api/v1/reviews/${review.id}/withdraw`, { reason: f.withdraw, by: f.requested_by }, "cli");
    plan[review.id] = { created: ms(f.age), ...(f.decided ? { outcome: ms(f.decided) } : {}) };
  }
  const ids = await app.pin(plan, statuses);
  return Object.fromEntries(Object.entries(created).map(([key, id]) => [key, ids.get(id)]));
}
