const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { scratch } = require("./scratch.cjs");

const load = () => import("../lib/check.mjs");
const samples = path.resolve(__dirname, "..", "..", "plugins");

/** A plugin folder with the given manifest fields on top of the minimum, and an entry. */
function plugin(extra = {}, files = { "index.html": "<html></html>" }) {
  const dir = scratch("pinrail-check-");
  const manifest = { name: "sample", version: "1.0.0", payload_schema: {}, decision_schema: {}, ...extra };
  for (const [k, v] of Object.entries(manifest)) if (v === undefined) delete manifest[k];
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify(manifest));
  for (const [name, text] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
    fs.writeFileSync(path.join(dir, name), text);
  }
  return dir;
}

const keys = (list) => list.map((p) => p.key);

test("the sample plugins pass, and the review plugin's template is noted", async () => {
  const { checkPlugin } = await load();
  for (const name of ["hello", "email", "review"]) {
    const r = checkPlugin(path.join(samples, name));
    assert.equal(r.ok, true, `${name}: ${JSON.stringify(r.problems)}`);
    assert.equal(r.usable, true);
    assert.deepEqual(r.warnings, [], name);
    assert.equal(r.name, name);
    assert.equal(r.release, "1.0.0");
    assert.equal(r.major, 1);
  }
  assert.deepEqual(keys(checkPlugin(path.join(samples, "review")).notes), ["decision_template"]);
});

test("versions read as the app reads them", async () => {
  const { versionOf } = await load();
  assert.deepEqual(versionOf(3), { release: "3.0.0", major: 3 });
  assert.deepEqual(versionOf("0.1.0"), { release: "0.1.0", major: 0 });
  assert.deepEqual(versionOf(" 2.3.4 "), { release: "2.3.4", major: 2 });
  for (const bad of [0, -1, 1.5, "1.2", "v1.2.0", "1.2.x", true, null, undefined]) assert.equal(versionOf(bad), null, String(bad));
});

test("what refuses a plugin: manifest, name, version, entry, schemas, build", async () => {
  const { checkPlugin } = await load();
  const refused = (extra, files) => {
    const r = checkPlugin(plugin(extra, files));
    assert.equal(r.ok, false, JSON.stringify(extra));
    return keys(r.problems);
  };
  assert.deepEqual(keys(checkPlugin(scratch("pinrail-empty-")).problems), ["manifest"]);
  assert.deepEqual(refused({ name: "Bad" }), ["name"]);
  assert.deepEqual(refused({ name: undefined }), ["name"]);
  assert.deepEqual(refused({ version: "0.0.0" }), ["version"]);
  assert.deepEqual(refused({ version: undefined }), ["version"]);
  assert.deepEqual(refused({ entry: "/abs.html" }), ["entry"]);
  assert.deepEqual(refused({ entry: "view/index.html" }), ["entry"]);
  assert.deepEqual(refused({ decision_schema: undefined }), ["decision_schema"]);
  assert.deepEqual(refused({ payload_schema: [] }), ["payload_schema"]);
  assert.deepEqual(refused({ payload_schema: { $ref: "../out.json" } }), ["payload_schema"]);
  assert.deepEqual(refused({ payload_schema: { $ref: "missing.json" } }), ["payload_schema"]);
  assert.deepEqual(refused({ payload_schema: { $ref: "p.json" } }, { "index.html": "", "p.json": "{" }), ["payload_schema"]);
  assert.deepEqual(refused({ payload_schema: { type: "thing" } }), ["payload_schema"]);
  assert.deepEqual(refused({ build: { command: "" } }), ["build"]);
  assert.deepEqual(refused({ build: "npm run build" }), ["build"]);
  // the manifest schema's rules for the rest of the keys
  assert.deepEqual(refused({ title: 3 }), ["title"]);
  assert.deepEqual(refused({ description: ["a"] }), ["description"]);
  assert.deepEqual(refused({ min_height: 0 }), ["min_height"]);
  assert.deepEqual(refused({ min_height: "400" }), ["min_height"]);
  assert.deepEqual(refused({ dev: "yes" }), ["dev"]);
  assert.deepEqual(refused({ entry: "" }), ["entry"]);
  assert.deepEqual(refused({ version: "1.2" }), ["version"]);
  // everything wrong at once is listed at once
  assert.deepEqual(refused({ name: "-", version: 0 }), ["name", "version"]);
});

test("a source that builds is ok before its build and usable after", async () => {
  const { checkPlugin } = await load();
  const dir = plugin({ entry: "view/index.html", build: { command: "npm run build" } }, {});
  const before = checkPlugin(dir);
  assert.equal(before.ok, true);
  assert.equal(before.usable, false);
  assert.deepEqual(keys(before.notes), ["entry"]);
  fs.mkdirSync(path.join(dir, "view"));
  fs.writeFileSync(path.join(dir, "view", "index.html"), "");
  const after = checkPlugin(dir);
  assert.equal(after.usable, true);
  assert.deepEqual(after.notes, []);
});

test("what costs a feature: settings, shortcuts, the template, the icon; a missing title is only a note", async () => {
  const { checkPlugin } = await load();
  const warned = (extra, files) => {
    const r = checkPlugin(plugin({ title: "T", ...extra }, files));
    assert.equal(r.ok, true, JSON.stringify(r.problems));
    return keys(r.warnings);
  };
  assert.deepEqual(keys(checkPlugin(plugin()).warnings), ["title"]);
  // the icon: an SVG file in the folder, or the plugin shows the generic one
  assert.deepEqual(warned({ icon: "mail" }), ["icon"]);
  assert.deepEqual(warned({ icon: "missing.svg" }), ["icon"]);
  assert.deepEqual(warned({ icon: "icon.svg" }, { "index.html": "", "icon.svg": "hello" }), ["icon"]);
  assert.deepEqual(warned({ icon: "icon.svg" }, { "index.html": "", "icon.svg": '<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1"/></svg>' }), []);
  assert.deepEqual(warned({ settings_schema: [] }), ["settings_schema"]);
  assert.deepEqual(warned({ settings_schema: { type: "array" } }), ["settings_schema"]);
  assert.deepEqual(warned({ settings_schema: { properties: { a: { type: "object", default: {} } } } }), ["settings_schema"]);
  assert.deepEqual(warned({ settings_schema: { properties: { a: { type: "integer" } } } }), ["settings_schema"]);
  assert.deepEqual(warned({ settings_schema: { properties: { a: { type: "integer", default: "3" } } } }), ["settings_schema"]);
  assert.deepEqual(warned({ settings_schema: { properties: { a: { type: "string", default: "x", enum: [] } } } }), ["settings_schema"]);
  assert.deepEqual(warned({ settings_schema: { properties: { a: { type: "string", default: "x", oneOf: [{ title: "no const" }] } } } }), ["settings_schema"]);
  assert.deepEqual(warned({ settings_schema: { $ref: "../s.json" } }), ["settings_schema"]);
  assert.deepEqual(
    warned({ settings_schema: { properties: {
      diff: { type: "string", title: "Diff", oneOf: [{ const: "inline", title: "Inline" }, { const: "split", title: "Split" }], default: "inline" },
      wrap: { type: "boolean", default: true },
      context: { type: "integer", minimum: 0, maximum: 20, default: 3 },
    } } }),
    [],
  );
  assert.deepEqual(warned({ shortcuts: {} }), ["shortcuts"]);
  assert.deepEqual(warned({ shortcuts: [{ keys: "j" }] }), ["shortcuts"]);
  assert.deepEqual(warned({ shortcuts: [{ keys: "hyper+j", does: "x" }] }), ["shortcuts"]);
  assert.deepEqual(warned({ shortcuts: [{ keys: "j", does: "x", group: 1 }] }), ["shortcuts"]);
  assert.deepEqual(warned({ shortcuts: [{ keys: "Cmd+Shift+F", does: "Fold", group: "View" }, { keys: "escape", does: "Close" }] }), []);
  assert.deepEqual(warned({ decision_template: "../t.j2" }), ["decision_template"]);
  assert.deepEqual(warned({ decision_template: "templates/decision.md.j2" }), ["decision_template"]);
  assert.deepEqual(warned({ decision_template: "t.j2" }, { "index.html": "", "t.j2": "{{ note }}" }), []);
});

test("files: a kind the core cannot read refuses the plugin; a schema that never says where they go is a warning", async () => {
  const { checkPlugin } = await load();
  const attachment = { type: "object", required: ["$attachment"], properties: { $attachment: { type: "string" } } };
  const takes = { accept: [".glb", "image/*"] };

  const good = checkPlugin(plugin({ attachments: takes, payload_schema: { type: "object", properties: { file: attachment } } }));
  assert.equal(good.ok, true, JSON.stringify(good.problems));
  assert.deepEqual(keys(good.warnings).filter((k) => k !== "title"), []);

  const kind = checkPlugin(plugin({ attachments: { accept: ["glb"] } }));
  assert.equal(kind.ok, false);
  assert.match(kind.problems.find((p) => p.key === "attachments").message, /"glb" is neither an extension like \.glb nor a media type/);

  for (const block of [{ accepts: [".glb"] }, { accept: [] }, { accept: [".glb"], max_size: 200 * 1024 * 1024 }]) {
    assert.equal(checkPlugin(plugin({ attachments: block })).ok, false, JSON.stringify(block));
  }

  const nowhere = checkPlugin(plugin({ attachments: takes }));
  assert.equal(nowhere.ok, true);
  assert.match(nowhere.warnings.find((w) => w.key === "attachments").message, /never names \{"\$attachment": …\}/);

  const undeclared = checkPlugin(plugin({ payload_schema: { type: "object", properties: { file: attachment } } }));
  assert.match(undeclared.warnings.find((w) => w.key === "attachments").message, /declares no attachments/);
});
