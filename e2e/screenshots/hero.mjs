// The views in the website's hero window: each plugin under the SDK's
// harness at the width the app gives it (1240 px, at 2x), grown to its whole
// length so the page can scroll through it, and shown there scaled to the
// pane; light theme, into website/src/assets/hero/<name>.png.
//
//   node screenshots/hero.mjs [name…]
//
// The app's plugins come from plugins/, the official ones from a checkout of
// forgeplane/pinrail-plugins beside this repository, or the folder that
// PINRAIL_PLUGINS_DIR names.
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { chromium } from "@playwright/test";
import { root } from "./app.mjs";

const require = createRequire(import.meta.url);
const { fixture, mountPlugin } = require(path.join(root, "sdk", "testing", "index.cjs"));
const official = path.join(process.env.PINRAIL_PLUGINS_DIR ?? path.join(root, "..", "pinrail-plugins"), "plugins");
const appFixture = (name) => path.join(root, "e2e", "screenshots", "fixtures", `${name}.json`);
const out = path.join(root, "website", "src", "assets", "hero");
const W = 1240;
const H = 860;
const MAX = 3200;

const keys = async (f, ...list) => {
  for (const k of list) await f.locator("body").press(k);
};
const drag = async (page, a, b) => {
  await page.mouse.move(...a);
  await page.mouse.down();
  await page.mouse.move(...b, { steps: 6 });
  await page.mouse.up();
};

const scenes = {
  // a finding accepted with a note being written, another rejected with a reason
  review: {
    plugin: path.join(root, "plugins", "code-review"),
    review: appFixture("10-review-webhooks"),
    act: async (f) => {
      const note = async (id, text) => {
        if (!(await f.locator(`[data-note-ta="${id}"]`).count()))
          await f.locator(`[data-act="open-note"][data-id="${id}"]`).click();
        await f.locator(`[data-note-ta="${id}"]`).fill(text);
      };
      await f.locator("#card-3 button", { hasText: "Accept" }).click();
      await f.locator("#card-4 button", { hasText: "Reject" }).click();
      await note(4, "Fine as it is; the log already has the delivery id.");
      await f.locator('[data-act="save-note"][data-id="4"]').click();
      await f.locator("#card-1 button", { hasText: "Accept" }).click();
      await note(1, "Agreed. Re-enqueue with runAt = now + backoff(attempt), and keep MAX_ATTEMPTS on the job");
    },
  },
  // two regions on the plane, a third note being written
  image: {
    plugin: path.join(root, "plugins", "image"),
    review: path.join(root, "plugins", "image", "fixtures", "tern.json"),
    act: async (f, page) => {
      await f.locator(".pick .thumb img").first().waitFor();
      await f.locator(".stage .art:not([hidden])").waitFor();
      await f.locator("body").click({ position: { x: 400, y: 5 } });
      await keys(f, "f");
      const s = await f.locator(".stage .art:not([hidden])").boundingBox();
      const at = (x, y) => [s.x + x * s.width, s.y + y * s.height];
      await drag(page, at(0.89, 0.39), at(0.995, 0.56));
      await f.locator("#region-note").fill("Remove this cloud: it is cut off by the edge");
      await f.locator("#region-note").press("Enter");
      await drag(page, at(0.38, 0.67), at(0.55, 0.76));
      await f.locator("#region-note").fill("A stray piece of the trail; remove it");
      await f.locator("#region-note").press("Enter");
    },
  },
  // answers given, a comment being written
  feedback: {
    plugin: path.join(root, "plugins", "feedback"),
    review: appFixture("14-feedback-pagination"),
    act: async (f) => {
      await f.getByRole("radio", { name: /Cursor-based/ }).check();
      await f.getByRole("radio", { name: "100", exact: false }).first().check();
      await f.getByRole("radio", { name: "No", exact: true }).check();
      await f.getByRole("radio", { name: /90 days/ }).check();
      const style = f.locator('[data-question="style"]');
      await style.getByRole("button", { name: "Add a comment" }).click();
      await style.getByLabel("Comment on this question").fill("Keep `page` working as an alias for one release");
    },
  },
  // the safe upgrades accepted, one held back with a reason, a note on another
  list: {
    plugin: path.join(root, "plugins", "list"),
    review: appFixture("13-list-deps"),
    act: async (f) => {
      for (const id of [1, 2, 3]) await f.locator(`[data-id="${id}"] button`, { hasText: "Accept" }).click();
      await f.locator('[data-id="6"] button', { hasText: "Reject" }).click();
      await f.locator('[data-note="6"]').fill("Wait for the stable release");
      await f.locator('[data-id="4"] button', { hasText: "Accept" }).click();
      await f.locator('[data-note="4"]').fill("Run the codemod in its own PR first");
    },
  },
  // a word and a stretch commented, a cut, the take favourited
  audio: {
    plugin: path.join(official, "audio"),
    review: path.join(official, "audio", "fixtures", "fieldnotes.json"),
    act: async (f, page) => {
      await f.locator(".mini canvas").nth(3).waitFor();
      await f.locator(".pick").nth(1).click();
      await f
        .locator("[data-w]")
        .filter({ hasText: /^Nguyen$/ })
        .click();
      await keys(f, "c");
      await f.locator("#mark-note").fill('Mispronounced: it is "Win", one syllable');
      await f.locator("#mark-note").press("Enter");
      const words = f.locator("[data-w]");
      await words.nth(9).hover();
      await page.mouse.down();
      await words.nth(15).hover();
      await page.mouse.up();
      await keys(f, "c");
      await f.locator("#mark-note").fill("Too fast here; give the bridge a beat");
      await f.locator("#mark-note").press("Enter");
      await words.nth(5).hover();
      await page.mouse.down();
      await words.nth(6).hover();
      await page.mouse.up();
      await keys(f, "Backspace", "f");
      await f.locator("[data-w]").nth(12).click();
    },
  },
  // five seconds in: an area of the frame commented, and a comment at the moment
  video: {
    plugin: path.join(official, "video"),
    review: path.join(official, "video", "samples", "video.json"),
    act: async (f, page) => {
      await f.locator("#timeline[data-duration]").waitFor();
      // a click on the header gives the view the keyboard
      await f.locator(".top").click({ position: { x: 2, y: 2 } });
      for (let i = 0; i < 5; i++) await page.keyboard.press("Shift+ArrowRight");
      await drag(page, ...(await boxAt(f.locator("#overlay"), [0.6, 0.48], [0.88, 0.96])));
      await f.locator("#mark-note").fill("Smaller: the mark crowds the line it stops");
      await f.locator("#mark-note").press("Enter");
      await page.keyboard.press("c");
      await f.locator("#mark-note").fill("Hold this a beat longer before the cut");
      await f.locator("#mark-note").press("Enter");
    },
  },
};

/** Two points in a locator's box, as fractions of its size. */
async function boxAt(locator, ...points) {
  const b = await locator.boundingBox();
  return points.map(([x, y]) => [b.x + x * b.width, b.y + y * b.height]);
}

// how much taller the frame must be for the view's main scrolling area not to scroll
const overflow = (f) =>
  f.locator("html").evaluate(() => {
    let most = 0;
    for (const el of document.querySelectorAll("*")) {
      const style = getComputedStyle(el);
      if (!/(auto|scroll)/.test(style.overflowY) && el !== document.scrollingElement) continue;
      most = Math.max(most, el.scrollHeight - el.clientHeight);
    }
    return most;
  });

const only = process.argv.slice(2);
fs.mkdirSync(out, { recursive: true });
const browser = await chromium.launch();
let failed = false;
for (const [name, scene] of Object.entries(scenes)) {
  if (only.length && !only.includes(name)) continue;
  const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 2 });
  try {
    await page.addInitScript(() =>
      document.addEventListener("DOMContentLoaded", () => {
        document.body.style.margin = "0";
      }),
    );
    const plugin = await mountPlugin(page, scene.plugin, { review: fixture(scene.review), theme: "light" });
    await plugin.setFrameHeight(H);
    await page.waitForTimeout(500);
    await scene.act(plugin.frame, page);
    // grow until nothing inside scrolls, a few passes since growing can reflow
    let height = H;
    for (let i = 0; i < 4; i++) {
      const more = await overflow(plugin.frame);
      if (more <= 1 || height >= MAX) break;
      height = Math.min(MAX, height + more);
      await page.setViewportSize({ width: W, height });
      await plugin.setFrameHeight(height);
      await page.waitForTimeout(300);
    }
    await page.waitForTimeout(400);
    const file = path.join(out, `${name}.png`);
    await page.locator("#plugin-frame").screenshot({ path: file });
    console.log(`hero: ${path.relative(root, file)}  ${W}×${height}`);
  } catch (error) {
    failed = true;
    console.error(`hero: ${name} failed: ${error.message.split("\n")[0]}`);
  }
  await page.close();
}
await browser.close();
if (failed) process.exit(1);
