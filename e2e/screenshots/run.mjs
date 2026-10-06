// Screenshots of the app for the docs and the website, in light and dark,
// made the same way every time:
//
//   mise run screenshots                     # every scene
//   mise run screenshots -- --only inbox     # scenes whose name starts with it
//   mise run screenshots -- --no-build       # reuse the desktop binary as it is
//
// A clean app is started on a scratch data directory, seeded from
// fixtures/ through the API, and pinned: every time and id is rewritten to
// a fixed value, and the browser's clock is frozen at the same moment. Each
// scene then runs once per theme and saves <name>-light.png and
// <name>-dark.png where they are used: into website/public/screenshots when
// a docs page shows it as screenshot:<name>, and into
// website/src/assets/screenshots when the scene marks it `site: true`, for
// the landing pages, where Astro optimizes it. A shot used by neither fails
// the run. With --out, every shot goes into that folder instead.
//
// made.json records the files a full run made. The next full run that
// succeeds removes the ones it no longer makes, such as a renamed scene's;
// a file no run made, such as a shot taken by hand, is left alone.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";
import { NOW, root, startApp } from "./app.mjs";
import { seed } from "./seed.mjs";
import { scenes } from "./scenes.mjs";

/**
 * What only a browser shows, or only this run knows, made to read as the
 * app does for anyone: the scratch data directory reads as the app's
 * default, and notes the shell adds outside the app are dropped or say what
 * the app would.
 */
async function scrub(page, paths) {
  const fix = ({ paths }) => {
    const clean = (text) => paths.reduce((t, [from, to]) => t.split(from).join(to), text);
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = clean(node.textContent);
      if (text !== node.textContent) node.textContent = text;
      if (node.textContent === "What macOS allows shows here in the app")
        node.textContent = "Allowed: Banners, sound on, badge";
    }
    for (const input of document.querySelectorAll("input")) input.value = clean(input.value);
    for (const note of document.querySelectorAll(".settings-note"))
      if (note.textContent === "Only in the app") note.remove();
  };
  for (const frame of page.frames()) await frame.evaluate(fix, { paths }).catch(() => {});
}

const args = process.argv.slice(2);
const flag = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};
const elsewhere = flag("--out");
const out = path.resolve(elsewhere ?? path.join(root, "website", "public", "screenshots"));
// shots the landing pages use go here, where Astro optimizes them
const siteAssets = path.join(root, "website", "src", "assets", "screenshots");

/** The shots the docs show, as `![…](screenshot:<name>)`. */
function docsShots(dir = path.join(root, "docs")) {
  const names = new Set();
  for (const entry of fs.readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (!entry.isFile() || !entry.name.endsWith(".md")) continue;
    const text = fs.readFileSync(path.join(entry.parentPath, entry.name), "utf8");
    for (const [, name] of text.matchAll(/\]\(screenshot:([a-z0-9-]+)/g)) names.add(name);
  }
  return names;
}
const inDocs = docsShots();
const only = flag("--only");
const chosen = scenes.filter((s) => !only || s.name.startsWith(only));
if (!chosen.length) throw new Error(`no scene starts with ${only}`);

// the files a full run made, relative to the repository
const record = path.join(path.dirname(fileURLToPath(import.meta.url)), "made.json");
const made = new Set();

const app = await startApp({ build: !args.includes("--no-build") });
// WebGL in software, for the views that draw with it (the model plugin)
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
let failed = false;
try {
  const reviews = await seed(app);
  fs.mkdirSync(out, { recursive: true });
  fs.mkdirSync(siteAssets, { recursive: true });

  for (const theme of ["light", "dark"]) {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 2,
      colorScheme: theme,
      locale: "en-US",
      timezoneId: "UTC",
      reducedMotion: "reduce",
    });
    for (const scene of chosen) {
      const page = await context.newPage();
      await page.clock.setFixedTime(NOW);
      // `site: true` saves the shot into the website's assets, for the landing pages
      const shot = async (name, target = page, { site = false, ...options } = {}) => {
        const folders = elsewhere ? [out] : [...(inDocs.has(name) ? [out] : []), ...(site ? [siteAssets] : [])];
        if (!folders.length) throw new Error(`${name} is shown by no docs page, and not marked site: true`);
        await page.evaluate(() => document.fonts.ready);
        await scrub(page, [
          [app.data, "~/.local/share/pinrail"],
          [app.code, "~/code"],
        ]);
        const [file, ...copies] = folders.map((folder) => path.join(folder, `${name}-${theme}.png`));
        await target.screenshot({ path: file, animations: "disabled", caret: "hide", ...options });
        for (const copy of copies) fs.copyFileSync(file, copy);
        for (const each of [file, ...copies]) {
          made.add(path.relative(root, each));
          console.log(`screenshots: ${path.relative(root, each)}`);
        }
      };
      try {
        await scene.run({ page, app, reviews, theme, shot });
      } catch (error) {
        failed = true;
        console.error(`screenshots: ${scene.name} (${theme}) failed: ${error.stack ?? error}`);
      }
      await page.close();
    }
    await context.close();
  }
} finally {
  await browser.close();
  await app.stop();
}
if (failed) process.exit(1);

// a full run into the website, every scene made: what an earlier run made
// and this one did not goes, and the record is this run's
if (!only && !elsewhere) {
  const before = fs.existsSync(record) ? JSON.parse(fs.readFileSync(record, "utf8")) : [];
  for (const file of before) if (!made.has(file)) fs.rmSync(path.join(root, file), { force: true });
  fs.writeFileSync(record, JSON.stringify([...made].sort(), null, 2) + "\n");
}
