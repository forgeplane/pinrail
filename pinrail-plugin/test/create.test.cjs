const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const load = () => import("../lib/create.mjs");
const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-create-"));

const filesUnder = (dir) => {
  const out = [];
  const walk = (d, rel) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const next = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) walk(path.join(d, e.name), next);
      else out.push(next);
    }
  };
  walk(dir, "");
  return out.sort();
};

test("the plain template is a whole plugin, named throughout", async () => {
  const { scaffold, titleOf } = await load();
  const dir = path.join(tmp(), "ticket_triage");
  const { written } = scaffold("ticket_triage", { dir, sdk: "file:../sdk" });

  assert.deepEqual(filesUnder(dir), [
    ".github/workflows/release.yml",
    ".gitignore",
    "README.md",
    "example.json",
    "fixtures/basic.json",
    "manifest.json",
    "package.json",
    "playwright.config.ts",
    "schemas/decision.schema.json",
    "schemas/payload.schema.json",
    "tests/ticket_triage.spec.ts",
    "view/index.html",
  ]);
  assert.deepEqual([...written].sort(), filesUnder(dir));

  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
  assert.equal(manifest.name, "ticket_triage");
  assert.equal(manifest.title, "Ticket triage");
  assert.equal(titleOf("ticket_triage"), "Ticket triage");
  assert.equal(manifest.version, "0.1.0");
  assert.equal(manifest.entry, "view/index.html");
  assert.equal(manifest.build, undefined);
  for (const ref of [manifest.payload_schema.$ref, manifest.decision_schema.$ref]) {
    assert.ok(fs.existsSync(path.join(dir, ref)), `${ref} exists`);
  }

  const pkg = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
  assert.equal(pkg.devDependencies["@forgeplane/pinrail-plugin"], "file:../sdk");

  // no placeholder survives, in any file
  for (const file of filesUnder(dir)) {
    const text = fs.readFileSync(path.join(dir, file), "utf8");
    assert.ok(!/__(NAME|TITLE|SDK_DEP)__/.test(text), `${file} has no placeholder`);
  }
});

test("the vite template adds the build, its sources and the config", async () => {
  const { scaffold } = await load();
  const dir = path.join(tmp(), "fancy");
  scaffold("fancy", { dir, template: "vite", sdk: "file:../sdk" });

  const files = filesUnder(dir);
  for (const f of ["src/index.html", "src/main.ts", "vite.config.ts", "tsconfig.json", "tests/fancy.spec.ts"]) {
    assert.ok(files.includes(f), `${f} written`);
  }
  assert.ok(!files.includes("view/index.html"), "the view is the build's to write");
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
  assert.equal(manifest.build.command, "npm ci && npm run build");
  assert.match(fs.readFileSync(path.join(dir, ".gitignore"), "utf8"), /^\/view\/$/m);
});

test("the react template writes the view in React, with its build", async () => {
  const { scaffold } = await load();
  const dir = path.join(tmp(), "fancy");
  scaffold("fancy", { dir, template: "react", sdk: "file:../sdk" });

  const files = filesUnder(dir);
  for (const f of ["src/index.html", "src/main.tsx", "src/App.tsx", "vite.config.ts", "tsconfig.json", "tests/fancy.spec.ts"]) {
    assert.ok(files.includes(f), `${f} written`);
  }
  assert.ok(!files.includes("src/main.ts"), "no TypeScript entry of the vite template");
  const pkg = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
  assert.ok(pkg.dependencies.react && pkg.devDependencies["@vitejs/plugin-react"], "React and its Vite plugin");
  assert.equal(JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8")).build.command, "npm ci && npm run build");
  for (const file of files) {
    assert.ok(!/__(NAME|TITLE|SDK_DEP)__/.test(fs.readFileSync(path.join(dir, file), "utf8")), `${file} has no placeholder`);
  }
});

test("without --sdk the dependency is the release tarball of this version", async () => {
  const { defaultSdkDep } = await load();
  const { version } = require("../package.json");
  assert.equal(defaultSdkDep(), `https://github.com/forgeplane/pinrail/releases/download/sdk-v${version}/pinrail-plugin-${version}.tgz`);
});

test("a bad name, an unknown template and a folder in use are refused", async () => {
  const { scaffold } = await load();
  assert.throws(() => scaffold("Bad", { dir: path.join(tmp(), "x") }), /\[a-z\]\[a-z0-9_-\]\*/);
  assert.throws(() => scaffold("ok", { dir: path.join(tmp(), "x"), template: "svelte" }), /no template "svelte"/);
  const used = tmp();
  fs.writeFileSync(path.join(used, "keep.txt"), "");
  assert.throws(() => scaffold("ok", { dir: used }), /is not empty/);
});
