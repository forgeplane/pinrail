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
// <name>-dark.png into website/public/screenshots (or --out), for the docs;
// the ones the landing pages use are copied into website/src/assets too.
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
const out = path.resolve(flag("--out") ?? path.join(root, "website", "public", "screenshots"));
// shots the landing pages use go here as well, where Astro optimizes them
const siteAssets = path.join(root, "website", "src", "assets", "screenshots");
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
      // `site: true` copies the shot into the website's assets as well
      const shot = async (name, target = page, { site = false, ...options } = {}) => {
        await page.evaluate(() => document.fonts.ready);
        await scrub(page, [
          [app.data, "~/.local/share/pinrail"],
          [app.code, "~/code"],
        ]);
        const file = path.join(out, `${name}-${theme}.png`);
        await target.screenshot({ path: file, animations: "disabled", caret: "hide", ...options });
        made.add(path.relative(root, file));
        if (site) {
          const copy = path.join(siteAssets, path.basename(file));
          fs.copyFileSync(file, copy);
          made.add(path.relative(root, copy));
        }
        console.log(`screenshots: ${path.relative(root, file)}`);
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
if (!only && !flag("--out")) {
  const before = fs.existsSync(record) ? JSON.parse(fs.readFileSync(record, "utf8")) : [];
  for (const file of before) if (!made.has(file)) fs.rmSync(path.join(root, file), { force: true });
  fs.writeFileSync(record, JSON.stringify([...made].sort(), null, 2) + "\n");
}
