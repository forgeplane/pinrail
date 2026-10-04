import { expect, test, type APIRequestContext } from "@playwright/test";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { clearInbox, core, linkPlugin } from "./helpers";

// The harness checks payloads and decisions with Ajv, and the app with a
// validator of its own. Both are given the official plugins' fixtures, and
// an empty payload and decision beside them, and must agree on each.
const root = path.resolve(__dirname, "..", "..");
const sdk = createRequire(path.join(root, "pinrail-plugin", "package.json"));
const { schemaChecker } = sdk("./harness/schemas.cjs");
const { resolveAttachments } = sdk("./harness/attachments.cjs");

type Fixture = { file: string; title: string; payload: unknown; decision?: { data: unknown }; attachments?: unknown };

/** The official plugins, each with the fixtures that hold a review. */
function plugins() {
  const dir = path.join(root, "plugins");
  return fs
    .readdirSync(dir)
    .filter((name) => fs.existsSync(path.join(dir, name, "manifest.json")))
    .map((folder) => {
      const pluginDir = path.join(dir, folder);
      const { name } = JSON.parse(fs.readFileSync(path.join(pluginDir, "manifest.json"), "utf8"));
      const fixturesDir = path.join(pluginDir, "fixtures");
      const fixtures: Fixture[] = fs.existsSync(fixturesDir)
        ? fs
            .readdirSync(fixturesDir)
            .filter((f) => f.endsWith(".json") && !f.endsWith(".summary.json"))
            .map((f) => ({ file: path.join(fixturesDir, f), ...JSON.parse(fs.readFileSync(path.join(fixturesDir, f), "utf8")) }))
            .filter((f) => f.payload !== undefined)
        : [];
      return { name, pluginDir, fixtures };
    });
}

/** The fixture's files as the app lists them, uploaded to the core. */
async function upload(request: APIRequestContext, fixture: Fixture) {
  if (!fixture.attachments) return undefined;
  const { list, files } = resolveAttachments(fixture.attachments, path.dirname(fixture.file));
  for (const entry of list) {
    const response = await request.put(`${core}/api/v1/attachments/${entry.sha256}`, {
      data: fs.readFileSync(files[entry.name].path),
    });
    expect(response.ok(), await response.text()).toBe(true);
  }
  return Object.fromEntries(
    list.map((e: { name: string; sha256: string; size: number; media_type: string }) => [
      e.name,
      { sha256: e.sha256, size: e.size, media_type: e.media_type },
    ]),
  );
}

/** Whether the app took a value, from its answer: a refusal must be about the
 *  value under `pointer` and nothing else. */
async function took(response: import("@playwright/test").APIResponse, pointer: string) {
  if (response.status() === 200) return true;
  const body = await response.json();
  expect(response.status(), JSON.stringify(body)).toBe(422);
  for (const v of body.violations) expect(v.path, JSON.stringify(body)).toMatch(new RegExp(`^${pointer}(/|$)`));
  return false;
}

const envelope = (plugin: string, fixture: Fixture, attachments: unknown) => ({
  plugin,
  title: fixture.title,
  origin: { repo: "acme/api" },
  requested_by: "spec",
  ...(attachments ? { attachments } : {}),
});

/** Whether the app takes `payload` for `plugin`. */
async function appTakesPayload(request: APIRequestContext, plugin: string, fixture: Fixture, payload: unknown) {
  const attachments = await upload(request, fixture);
  const response = await request.post(`${core}/api/v1/reviews/validate`, {
    data: { ...envelope(plugin, fixture, attachments), payload },
  });
  return took(response, "/payload");
}

let asked = 0;

/** Whether the app takes `data` as the decision on a review of the fixture,
 *  a new one each time: the app answers the same submission with the same review. */
async function appTakesDecision(request: APIRequestContext, plugin: string, fixture: Fixture, data: unknown) {
  const attachments = await upload(request, fixture);
  const created = await request.post(`${core}/api/v1/reviews`, {
    data: { ...envelope(plugin, fixture, attachments), title: `${fixture.title} (${++asked})`, payload: fixture.payload },
  });
  expect(created.status(), await created.text()).toBe(201);
  const { id } = await created.json();
  return took(await request.post(`${core}/api/v1/reviews/${id}/decision`, { data: { data } }), "");
}

test("the harness and the app agree on the official plugins' fixtures", async ({ request }) => {
  test.setTimeout(120_000);
  await clearInbox(request);
  const installed = new Set(
    ((await (await request.get(`${core}/api/v1/plugins`)).json()).plugins as { name: string }[]).map((p) => p.name),
  );
  const linked: string[] = [];
  const verdicts: { case: string; harness: boolean; app: boolean }[] = [];
  try {
    for (const { name, pluginDir, fixtures } of plugins()) {
      if (fixtures.length === 0) continue;
      if (!installed.has(name)) {
        await linkPlugin(request, pluginDir, name);
        linked.push(name);
      }
      const payloadOk = schemaChecker(pluginDir, "payload");
      const decisionOk = schemaChecker(pluginDir, "decision");
      for (const fixture of fixtures) {
        const label = `${name}/${path.basename(fixture.file)}`;
        for (const [what, payload] of [["payload", fixture.payload], ["empty payload", {}]] as const) {
          verdicts.push({
            case: `${label}: ${what}`,
            harness: payloadOk(payload).length === 0,
            app: await appTakesPayload(request, name, fixture, payload),
          });
        }
        const decisions = fixture.decision ? [["decision", fixture.decision.data] as const] : [];
        for (const [what, data] of [...decisions, ["empty decision", {}] as const]) {
          verdicts.push({
            case: `${label}: ${what}`,
            harness: decisionOk(data).length === 0,
            app: await appTakesDecision(request, name, fixture, data),
          });
        }
      }
    }
  } finally {
    await clearInbox(request);
    for (const name of linked) await request.delete(`${core}/api/v1/plugins/${name}`);
  }

  expect(verdicts.length).toBeGreaterThan(20);
  expect(verdicts.filter((v) => v.harness !== v.app)).toEqual([]);
});
