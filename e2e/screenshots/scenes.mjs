// What to photograph. A scene gets the page (in one theme, clock frozen),
// the app, the seeded review ids by fixture key, and `shot(name, target?,
// options?)`, which saves <name>-<theme>.png. The plugin scenes show a
// decision under way: verdicts given, a note or a comment half written, and
// save the plugin's view alone as <name>-view as well, for the website.
// `site: true` marks the shots the landing pages use.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/** Opens a review and waits for its plugin's view to draw. */
async function openReview(page, app, id) {
  await page.goto(`${app.ui}/#/reviews/${id}`);
  const frame = page.frameLocator("#plugin-frame");
  await frame.locator("body").waitFor();
  await page.waitForTimeout(700);
  return frame;
}

const settle = (page, ms = 350) => page.waitForTimeout(ms);

/** Writes a note on a code review proposal, opening the field when a verdict did not. */
async function note(f, id, text) {
  if (!(await f.locator(`[data-note-ta="${id}"]`).count())) await f.locator(`[data-act="open-note"][data-id="${id}"]`).click();
  await f.locator(`[data-note-ta="${id}"]`).fill(text);
}

export const scenes = [
  {
    name: "inbox",
    async run({ page, app, shot }) {
      await page.goto(`${app.ui}/#/`);
      await page.locator(".inbox-repo").first().waitFor();
      await settle(page);
      await shot("inbox", page, { site: true });
    },
  },
  {
    name: "history",
    async run({ page, app, shot }) {
      await page.goto(`${app.ui}/#/history`);
      await page.waitForLoadState("networkidle");
      await settle(page, 500);
      await shot("history");
    },
  },
  {
    // a finding in focus: two verdicts given, and a note on this one being written
    name: "review",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["10-review-webhooks"]);
      await f.locator("#card-3 button", { hasText: "Accept" }).click();
      await f.locator("#card-4 button", { hasText: "Reject" }).click();
      await note(f, 4, "Fine as it is; the log already has the delivery id.");
      await f.locator('[data-act="save-note"][data-id="4"]').click();
      await f.locator("#card-1 button", { hasText: "Accept" }).click();
      if (!(await f.locator('[data-note-ta="1"]').count())) await f.locator('[data-act="open-note"][data-id="1"]').click();
      await f.locator('[data-note-ta="1"]').fill("Agreed. Re-enqueue with runAt = now + backoff(attempt), and keep MAX_ATTEMPTS on the job");
      await f.locator("#card-1").scrollIntoViewIfNeeded();
      await f.locator("#card-1").evaluate((el) => el.scrollIntoView({ block: "center" }));
      await settle(page);
      await shot("review", page, { site: true });
      await shot("review-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // round two, with the first round's verdicts beside it
    name: "rounds",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["21-review-ratelimit-r2"]);
      await f.locator("#card-4 button", { hasText: "Accept" }).click();
      await settle(page);
      await shot("rounds");
    },
  },
  {
    // one time chosen, one item sent back for another time with a note; then
    // the same plan in the day and the list views
    name: "calendar",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["18-calendar-week"]);
      await f.getByRole("button", { name: "Week", exact: true }).click();
      await f.locator('.event[data-option="doctor-mon"]').click();
      const dinner = f.locator('[data-activity="dinner"]');
      await dinner.getByRole("button", { name: "Another time" }).click();
      await dinner.locator("textarea").fill("Friday evening works better");
      await f.locator("#calendar-scroll").evaluate((el) => el.scrollTo({ top: 60, behavior: "instant" }));
      await settle(page);
      await shot("calendar-week");
      await f.getByRole("button", { name: "Day", exact: true }).click();
      await f.locator(".day-strip button").filter({ hasText: "22" }).click();
      await f.locator("#calendar-scroll").evaluate((el) => el.scrollTo({ top: 420, behavior: "instant" }));
      await settle(page);
      await shot("calendar-day");
      await f.getByRole("button", { name: "List", exact: true }).click();
      await settle(page);
      await shot("calendar-list");
      await shot("calendar-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // a verdict on three marks, the favourite with a change to one of its parts half written
    name: "logo",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["17-logo-tidemark"]);
      const verdict = async (index, action, note) => {
        await f.locator(".pick").nth(index).click();
        await f.locator(`.choice[data-action="${action}"]`).click();
        if (note) await f.locator("#note").fill(note);
        await settle(page, 150);
      };
      await verdict(0, "keep");
      await verdict(1, "drop", "Too close to every other ring mark");
      await verdict(3, "favorite", "Pixel-tune it at 16 px");
      await f.locator('.stage[data-stage="light"] svg rect').nth(2).click({ force: true });
      await f.locator("#part-note").fill("Lower the line a little, below the middle");
      await settle(page);
      await shot("logo");
      await shot("logo-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // the favourite on the stage, a change pinned to its base and another being written on its shade
    name: "model",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["19-model-halden"]);
      await f.locator(".pick .still img").first().waitFor();
      const verdict = async (index, action, note) => {
        await f.locator(".pick").nth(index).click();
        await f.locator(`.choice[data-action="${action}"]`).click();
        if (note) await f.locator("#note").fill(note);
        await settle(page, 150);
      };
      await verdict(1, "drop", "A lamp that cannot be aimed is not a desk lamp");
      await verdict(2, "keep");
      await verdict(0, "favorite", "Warmer overall; it reads a little cold next to Column");
      await f.locator("[data-part]").filter({ hasText: "Base" }).click();
      await f.locator("#part-note").fill("A darker oak, closer to the walnut of Column");
      await f.locator("#part-note").press("Enter");
      await f.locator("[data-part]").filter({ hasText: "Shade" }).click();
      await f.locator("#part-note").fill("Wider and shallower, so the bulb is hidden from the chair");
      // back to the top: the model's name and reasoning above the stage
      await f.locator("#sheet").evaluate((el) => { el.scrollTop = 0; });
      await settle(page, 600);
      await shot("model");
      await shot("model-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // two comments pinned, a third element picked and its comment being typed
    name: "artifact",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["12-artifact-landing"]);
      const comment = async (selector, text, save = true) => {
        if (!(await f.locator("[data-select]").getAttribute("class"))?.includes("is-on")) await f.locator("[data-select]").click();
        await f.locator(`[data-artifact] ${selector}`).first().click();
        await f.locator("[data-comment-text]").fill(text);
        if (save) await f.locator("[data-save]").click();
        await settle(page, 200);
      };
      await comment("#hero h1", "Lead with the outcome: “Show the right shipping price at checkout.” Keep the carrier count as the subline.");
      await comment("#pricing .price", "Add a second column for volume pricing past 1M requests; enterprise buyers ask first.");
      await comment("#features .card h3", "Rename to “One schema for every carrier”", false);
      // clicks scroll the page wherever timing leaves it; set it: the cards
      // at the top, the comment being written and the second pin below
      await f.locator("[data-artifact] #features").evaluate((el) => {
        el.scrollIntoView({ block: "start", behavior: "instant" });
        let scroller = (el.getRootNode().host ?? el).parentElement;
        while (scroller && scroller.scrollHeight <= scroller.clientHeight) scroller = scroller.parentElement;
        scroller?.scrollBy({ top: -24, behavior: "instant" });
      });
      await settle(page);
      await shot("artifact");
      await shot("artifact-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // an edit showing against the draft, and a passage commented
    name: "email",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["11-email-beta"]);
      await f.locator('[data-draft="lumen"]').getByRole("button", { name: "Send" }).click();
      await f.locator('[data-pick-id="quarry"]').click();
      const quarry = f.locator('[data-draft="quarry"]');
      // select a passage of the message the way a reader would
      const select = (wanted) => quarry.locator("[data-body]").evaluate((body, wanted) => {
        const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          if (node.parentElement.closest("del")) continue;
          const at = node.textContent.indexOf(wanted);
          if (at < 0) continue;
          body.focus();
          const range = document.createRange();
          range.setStart(node, at);
          range.setEnd(node, at + wanted.length);
          const selection = document.getSelection();
          selection.removeAllRanges();
          selection.addRange(range);
          document.dispatchEvent(new Event("selectionchange"));
          return;
        }
      }, wanted);
      // an edit in place, tracked against the agent's words
      await select("I wanted to reach out and let you know that we have been working hard on a brand new analytics experience, and we think it could be a great fit for Quarry.");
      await page.keyboard.insertText("You upvoted per-endpoint reporting on our roadmap board. It's built, and it opens as a beta on 6 October.");
      // an instruction on a passage, being written in its popover, with the
      // edit above it still in view
      await f.locator(".sheet").evaluate((el) => el.scrollTo({ top: 300, behavior: "instant" }));
      await select("Would you be interested in joining the beta?");
      await f.locator("#pick button").click();
      await f.locator("[data-mark-text]").fill("Ask for a yes: “Shall I turn it on for Quarry?”");
      await settle(page);
      await shot("email");
      await shot("email-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // verdicts on the safe upgrades, a reason on the hold, a note being written
    name: "list",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["13-list-deps"]);
      for (const id of [1, 2, 3]) await f.locator(`[data-id="${id}"] button`, { hasText: "Accept" }).click();
      await f.locator('[data-id="6"] button', { hasText: "Reject" }).click();
      await f.locator('[data-note="6"]').fill("Wait for the stable release");
      await f.locator('[data-id="4"] button', { hasText: "Accept" }).click();
      await f.locator('[data-note="4"]').fill("Run the codemod in its own PR first");
      await f.locator('.item[data-id="2"]').evaluate((el) => el.scrollIntoView({ block: "start" }));
      await settle(page);
      await shot("list");
      await shot("list-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // two answers given, a follow-up opened by one of them, a comment being written
    name: "feedback",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["14-feedback-pagination"]);
      await f.getByRole("radio", { name: /Cursor-based/ }).check();
      await f.getByRole("radio", { name: "100", exact: false }).first().check();
      await f.getByRole("radio", { name: "No", exact: true }).check();
      await f.getByRole("radio", { name: /90 days/ }).check();
      const style = f.locator('[data-question="style"]');
      await style.getByRole("button", { name: "Add a comment" }).click();
      await style.getByLabel("Comment on this question").fill("Keep `page` working as an alias for one release");
      // the group's heading at the top, and its first question under it
      await f.locator(".question-group").first().evaluate((el) => el.scrollIntoView({ block: "start" }));
      await settle(page);
      await shot("feedback");
      await shot("feedback-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    name: "discard",
    async run({ page, app, reviews, shot }) {
      await openReview(page, app, reviews["16-list-cloud"]);
      await page.getByRole("button", { name: /Discard/ }).first().click();
      await page.getByRole("dialog").locator("textarea, input").first().fill("Staging-2 is the load-test cluster for the Q4 launch; keep it until November");
      await settle(page);
      await shot("discard");
    },
  },
  {
    // everything at once: what waits, what was decided, the plugins
    name: "palette",
    async run({ page, app, shot }) {
      await page.goto(`${app.ui}/#/`);
      await page.locator(".inbox-repo").first().waitFor();
      await page.keyboard.press("Meta+k");
      const input = page.getByPlaceholder("Search reviews, plugins, actions…");
      await input.waitFor();
      await page.getByText("Recent decisions").waitFor();
      await page.waitForLoadState("networkidle");
      await settle(page, 400);
      await shot("palette");
      // then narrowed to one review: filled into the input itself, and
      // waited for until the other sections are gone
      await input.fill("webhook");
      await page.getByText("Recent decisions").waitFor({ state: "detached" });
      await page.waitForLoadState("networkidle");
      await settle(page, 400);
      await shot("palette-search");
    },
  },
  {
    name: "shortcuts",
    async run({ page, app, shot }) {
      await page.goto(`${app.ui}/#/`);
      await page.locator(".inbox-repo").first().waitFor();
      await page.keyboard.press("?");
      await settle(page, 500);
      await shot("shortcuts");
    },
  },
  {
    // a new plugin looked at before it is installed: what it is, where from
    name: "install",
    async run({ page, app, shot }) {
      const dir = path.join(app.code, "ticket_triage");
      fs.rmSync(dir, { recursive: true, force: true });
      fs.mkdirSync(app.code, { recursive: true });
      execFileSync("node", [path.join(app.root, "pinrail-plugin", "bin", "pinrail-plugin.mjs"), "create", "ticket_triage", "--dir", dir], { stdio: "ignore" });
      const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
      fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ ...manifest, icon: "ticket" }, null, 2));
      await page.goto(`${app.ui}/#/`);
      await page.locator(".inbox-repo").first().waitFor();
      await page.keyboard.press("Meta+,");
      await page.locator('[data-section="plugins"]').click();
      await page.getByRole("button", { name: /Install…/ }).click();
      await page.getByRole("textbox", { name: "Source", exact: true }).fill(dir);
      await page.locator("[data-install-look]").click();
      await page.getByText("ticket_triage").first().waitFor();
      await settle(page, 500);
      await shot("install");
    },
  },
  ...["general", "appearance", "shortcuts", "plugins"].map((section) => ({
    name: `settings-${section}`,
    async run({ page, app, shot }) {
      await page.goto(`${app.ui}/#/`);
      await page.locator(".inbox-repo").first().waitFor();
      await page.keyboard.press("Meta+,");
      await page.locator(`[data-section="${section}"]`).click();
      await settle(page, 500);
      await shot(`settings-${section}`);
    },
  })),
];
