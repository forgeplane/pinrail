// The four builds of Ship it? are one plugin: the same manifest, schemas,
// fixtures, example and test in each, and the same page around the view,
// which differs only in the script it loads. Only the view's code is the
// framework's own.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

const here = path.dirname(new URL(import.meta.url).pathname);
const FRAMEWORKS = ["vanilla", "react", "vue", "svelte"];
const SHARED = [
  "manifest.json",
  "example.json",
  "playwright.config.ts",
  ".gitignore",
  "schemas/payload.schema.json",
  "schemas/decision.schema.json",
  "fixtures/deploy.json",
  "fixtures/deploy.decided.json",
  "tests/ship_it.spec.ts",
];
const read = (framework, file) => fs.readFileSync(path.join(here, framework, file), "utf8");

for (const file of SHARED) {
  test(`${file} is the same in every framework`, () => {
    for (const framework of FRAMEWORKS.slice(1))
      assert.equal(read(framework, file), read("vanilla", file), `${framework}/${file}`);
  });
}

test("the page around the view differs only in the script it loads", () => {
  const page = (framework) => read(framework, "src/index.html").replace(/src="\.\/main\.tsx?"/, 'src="./main"');
  for (const framework of FRAMEWORKS.slice(1)) assert.equal(page(framework), page("vanilla"), framework);
});
