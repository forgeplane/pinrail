// What to photograph. A scene gets the page (in one theme, clock frozen),
// the app, the seeded review ids by fixture key, and `shot(name, target?,
// options?)`, which saves <name>-<theme>.png. The plugin scenes show a
// decision under way: verdicts given, a note or a comment half written, and
// save the plugin's view alone as <name>-view as well, for the website.
// `site: true` marks the shots the landing pages use, in the light theme.

import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
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

/** A port nothing listens on. */
const freePort = () =>
  new Promise((resolve) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });

/** The agents Settings › Agents lists, as the desktop app reports them. */
const AGENTS = [
  ["claude", "Claude Code", true, ".claude/skills", "connected", null],
  ["codex", "Codex", true, ".codex/skills", "absent", null],
  ["cursor", "Cursor", true, ".cursor/skills", "connected", null],
  ["antigravity", "Antigravity CLI", false, ".gemini/config/skills", "absent", null],
  ["opencode", "OpenCode", true, ".config/opencode/skills", "covered", "Claude Code"],
  ["grok", "Grok CLI", false, ".grok/skills", "absent", null],
].map(([id, name, found, skills, state, covered_by]) => ({
  id,
  name,
  found,
  skill: `/Users/you/${skills}/pinrail/SKILL.md`,
  state,
  covered_by,
}));

/** Writes a note on a code review proposal, opening the field when a verdict did not. */
async function note(f, id, text) {
  if (!(await f.locator(`[data-note-ta="${id}"]`).count()))
    await f.locator(`[data-act="open-note"][data-id="${id}"]`).click();
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
      if (!(await f.locator('[data-note-ta="1"]').count()))
        await f.locator('[data-act="open-note"][data-id="1"]').click();
      await f
        .locator('[data-note-ta="1"]')
        .fill("Agreed. Re-enqueue with runAt = now + backoff(attempt), and keep MAX_ATTEMPTS on the job");
      await f.locator("#card-1").scrollIntoViewIfNeeded();
      await f.locator("#card-1").evaluate((el) => el.scrollIntoView({ block: "center" }));
      await settle(page);
      await shot("review");
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
      await f.locator("#sheet").evaluate((el) => {
        el.scrollTop = 0;
      });
      await settle(page, 600);
      await shot("model-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // the favourite boxed and pinned where it should change, another kept,
    // one dropped, and a note on the favourite as a whole
    name: "image",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["22-image-tern"]);
      await f.locator(".stage .art").waitFor();
      const verdict = async (index, action) => {
        await f.locator(".pick").nth(index).click();
        await f.locator(`.choice[data-action="${action}"]`).click();
        await settle(page, 150);
      };
      await verdict(1, "keep");
      await verdict(2, "drop");
      await verdict(0, "favorite");
      // a box over the cut-off cloud, and a pin on the plane's nose
      const art = await f.locator(".stage .art").boundingBox();
      const at = (x, y) => [art.x + art.width * x, art.y + art.height * y];
      await page.mouse.move(...at(0.9, 0.4));
      await page.mouse.down();
      await page.mouse.move(...at(0.99, 0.55), { steps: 6 });
      await page.mouse.up();
      await f.locator("#region-note").fill("Remove this cloud: it is cut off by the edge");
      await f.locator("#region-note").press("Enter");
      await page.mouse.click(...at(0.86, 0.3));
      await f.locator("#region-note").fill("Tilt the nose up a little, towards the sun");
      await f.locator("#region-note").press("Enter");
      await f.getByRole("button", { name: "Add a note" }).click();
      await f.locator("#note").fill("The direction. Keep the palette **exactly** as it is.");
      await f.locator("#note").press("Escape");
      await f.locator("#sheet").evaluate((el) => {
        el.scrollTop = 0;
      });
      await settle(page, 600);
      await shot("image");
      await shot("image-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // a word and a stretch commented, a cut, the take favourited
    name: "audio",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["24-audio-fieldnotes"]);
      await f.locator(".mini canvas").nth(3).waitFor();
      await f.locator(".pick").nth(1).click();
      await f
        .locator("[data-w]")
        .filter({ hasText: /^Nguyen$/ })
        .click();
      await f.locator("body").press("c");
      await f.locator("#mark-note").fill('Mispronounced: it is "Win", one syllable');
      await f.locator("#mark-note").press("Enter");
      const words = f.locator("[data-w]");
      await words.nth(9).hover();
      await page.mouse.down();
      await words.nth(15).hover();
      await page.mouse.up();
      await f.locator("body").press("c");
      await f.locator("#mark-note").fill("Too fast here; give the bridge a beat");
      await f.locator("#mark-note").press("Enter");
      await words.nth(5).hover();
      await page.mouse.down();
      await words.nth(6).hover();
      await page.mouse.up();
      await f.locator("body").press("Backspace");
      await f.locator("body").press("f");
      await words.nth(12).click();
      await settle(page, 600);
      await shot("audio-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // the first two requests fixed, the third not, with a note
    name: "visual-diff",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["25-visual-diff-acme"]);
      await f.locator(".pick .thumb img").nth(1).waitFor();
      // the frame takes the keys from here
      await f.locator("#pair-name").click();
      await page.keyboard.press("f");
      await page.keyboard.press("n");
      await page.keyboard.press("f");
      await page.keyboard.press("n");
      await page.keyboard.press("x");
      await f.locator('[data-note-for="2"]').fill("Still misspelled, now as “projcts”.");
      // back to the top, where the two images are compared
      await f.locator("html").evaluate(() => {
        for (const el of document.querySelectorAll("*")) if (el.scrollTop) el.scrollTop = 0;
      });
      await settle(page, 600);
      await shot("visual-diff-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // five seconds in: an area of the frame commented, and a comment at the moment
    name: "video",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["26-video-promo"]);
      await f.locator("#timeline[data-duration]").waitFor();
      // a click on the header gives the view the keyboard
      await f.locator(".top").click({ position: { x: 2, y: 2 } });
      for (let i = 0; i < 5; i++) await page.keyboard.press("Shift+ArrowRight");
      const b = await f.locator("#overlay").boundingBox();
      await page.mouse.move(b.x + b.width * 0.6, b.y + b.height * 0.48);
      await page.mouse.down();
      await page.mouse.move(b.x + b.width * 0.88, b.y + b.height * 0.96, { steps: 6 });
      await page.mouse.up();
      await f.locator("#mark-note").fill("Smaller: the mark crowds the line it stops");
      await f.locator("#mark-note").press("Enter");
      await page.keyboard.press("c");
      await f.locator("#mark-note").fill("Hold this a beat longer before the cut");
      await f.locator("#mark-note").press("Enter");
      await settle(page, 600);
      await shot("video-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    // a change asked for on a section and a question on the diagram
    name: "markdown",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["23-markdown-retries"]);
      await f.locator(".diagram svg").waitFor({ timeout: 15_000 });
      const comment = async (open, kind, text) => {
        await open();
        if (kind === "question") await f.getByRole("radio", { name: "Question" }).click();
        await f.locator("#compose-body").fill(text);
        await f.locator("#compose-body").press("Enter");
        await settle(page, 150);
      };
      await comment(
        async () => {
          await f.locator(".rendered h3").hover();
          await f.locator(".rendered h3 [data-comment-section]").click();
        },
        "change",
        "Show the delays up to the eighth try, so the hour adds up.",
      );
      await comment(
        async () => {
          // the button shows on hover, which the app's frame does not
          // always register here: hover for the look, then press it
          await f.locator(".diagram").scrollIntoViewIfNeeded();
          await f.locator(".diagram").hover();
          await f.locator("[data-comment-diagram]").evaluate((button) => button.click());
        },
        "question",
        "Where does Retry-After come in?",
      );
      // the Design section at the top: its diagram, with the question on it
      await f.locator(".rendered h2", { hasText: "Design" }).evaluate((el) => el.scrollIntoView({ block: "start" }));
      await settle(page, 600);
      await shot("markdown");
    },
  },
  {
    // two comments pinned, a third element picked and its comment being typed
    name: "artifact",
    async run({ page, app, reviews, shot }) {
      const f = await openReview(page, app, reviews["12-artifact-landing"]);
      const comment = async (selector, text, save = true) => {
        if (!(await f.locator("[data-select]").getAttribute("class"))?.includes("is-on"))
          await f.locator("[data-select]").click();
        await f.locator(`[data-artifact] ${selector}`).first().click();
        await f.locator("[data-comment-text]").fill(text);
        if (save) await f.locator("[data-save]").click();
        await settle(page, 200);
      };
      await comment(
        "#hero h1",
        "Lead with the outcome: “Show the right shipping price at checkout.” Keep the carrier count as the subline.",
      );
      await comment(
        "#pricing .price",
        "Add a second column for volume pricing past 1M requests; enterprise buyers ask first.",
      );
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
      const select = (wanted) =>
        quarry.locator("[data-body]").evaluate((body, wanted) => {
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
      await select(
        "I wanted to reach out and let you know that we have been working hard on a brand new analytics experience, and we think it could be a great fit for Quarry.",
      );
      await page.keyboard.insertText(
        "You upvoted per-endpoint reporting on our roadmap board. It's built, and it opens as a beta on 6 October.",
      );
      // an instruction on a passage, being written in its popover, with the
      // edit above it still in view
      await f.locator(".sheet").evaluate((el) => el.scrollTo({ top: 300, behavior: "instant" }));
      await select("Would you be interested in joining the beta?");
      await f.locator("#pick button").click();
      await f.locator("[data-mark-text]").fill("Ask for a yes: “Shall I turn it on for Quarry?”");
      await settle(page);
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
      await f.locator('.pinrail-item[data-id="2"]').evaluate((el) => el.scrollIntoView({ block: "start" }));
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
      await f
        .locator(".question-group")
        .first()
        .evaluate((el) => el.scrollIntoView({ block: "start" }));
      await settle(page);
      await shot("feedback");
      await shot("feedback-view", page.locator("#plugin-frame"), { site: true });
    },
  },
  {
    name: "discard",
    async run({ page, app, reviews, shot }) {
      await openReview(page, app, reviews["16-list-cloud"]);
      await page
        .getByRole("button", { name: /Discard/ })
        .first()
        .click();
      await page
        .getByRole("dialog")
        .locator("textarea, input")
        .first()
        .fill("Staging-2 is the load-test cluster for the Q4 launch; keep it until November");
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
      // written as a person would, by the command this checkout builds
      const cli = path.join(app.root, "cli");
      execFileSync("cargo", ["build", "--quiet", "--manifest-path", path.join(cli, "Cargo.toml")], {
        stdio: "inherit",
      });
      execFileSync(path.join(cli, "target", "debug", "pinrail"), ["plugins", "new", "ticket_triage", "--dir", dir], {
        stdio: "ignore",
      });
      const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
      fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ ...manifest, icon: "ticket" }, null, 2));
      await page.goto(`${app.ui}/#/`);
      await page.locator(".inbox-repo").first().waitFor();
      await page.keyboard.press("Meta+,");
      await page.locator('[data-section="plugins"]').click();
      await page.getByRole("textbox", { name: "Search official plugins, or give a source path" }).fill(dir);
      await page.getByText("ticket_triage").first().waitFor();
      await settle(page, 500);
      await shot("install");
    },
  },
  {
    // the browser has no native side to find agents: the scene answers the
    // one call the section makes with agents in each state it shows
    name: "settings-agents",
    async run({ page, app, shot }) {
      await page.goto(`${app.ui}/#/`);
      await page.locator(".inbox-repo").first().waitFor();
      await page.evaluate((agents) => {
        window.__TAURI_INTERNALS__ = {
          invoke: async (command) => {
            if (command === "agents_status") return agents;
            throw new Error(`${command} is not answered in screenshots`);
          },
        };
      }, AGENTS);
      await page.keyboard.press("Meta+,");
      await page.locator('[data-section="agents"]').click();
      await page.locator("[data-agent]").first().waitFor();
      await settle(page, 500);
      await shot("settings-agents");
    },
  },
  {
    // the setup over the inbox: the browser has no native side, so the
    // scene answers the calls Connect makes, with the command installed
    name: "setup",
    async run({ page, app, shot }) {
      await page.goto(`${app.ui}/#/`);
      await page.locator(".inbox-repo").first().waitFor();
      await page.evaluate((agents) => {
        window.__TAURI_INTERNALS__ = {
          invoke: async (command) => {
            if (command === "agents_status") return agents;
            if (command === "notification_status") return null;
            if (command === "cli_status")
              return {
                mode: "link",
                bundled: "/Applications/Pinrail.app/Contents/MacOS/pinrail",
                link: "/Users/you/.local/bin/pinrail",
                installed: true,
                outdated: false,
                occupied_by: null,
                runs: "/Users/you/.local/bin/pinrail",
                dir_on_path: true,
              };
            throw new Error(`${command} is not answered in screenshots`);
          },
        };
      }, AGENTS);
      await page.keyboard.press("Meta+k");
      await page.getByRole("textbox").fill("Set up Pinrail");
      await page.getByRole("option", { name: /Set up Pinrail/ }).click();
      const setup = page.getByRole("dialog", { name: "Set up Pinrail" });
      await setup.locator('[data-welcome-agent="claude"]').waitFor();
      await settle(page, 500);
      await shot("setup-connect");
    },
  },
  {
    // `pinrail-sdk dev` on Ship it?, the React plugin the docs build: Ship
    // chosen and a note written, so the shell shows the status and the draft
    name: "dev-shell",
    async run({ page, app, theme, shot }) {
      const dir = path.join(app.root, "docs", "examples", "ship-it", "react");
      if (!fs.existsSync(path.join(dir, "view", "index.html"))) {
        throw new Error(`${dir} has no view: run npm ci && npm run build there`);
      }
      const port = await freePort();
      const bin = path.join(app.root, "sdk", "bin", "pinrail-sdk.mjs");
      const shell = spawn(process.execPath, [bin, "dev", dir, "--port", String(port), "--no-open"], {
        stdio: "ignore",
      });
      try {
        const url = `http://127.0.0.1:${port}/`;
        for (let i = 0; !(await fetch(`${url}dev/manifest`).catch(() => null))?.ok; i++) {
          if (i > 100) throw new Error("pinrail-sdk dev did not start");
          await settle(page, 100);
        }
        // the shell keeps its own theme, dark until it is switched
        await page.addInitScript((theme) => localStorage.setItem("pinrail-shell:theme", theme), theme);
        await page.goto(url);
        const view = page.frameLocator("#frame");
        await view.getByRole("group", { name: "Verdict" }).getByRole("button", { name: /Ship/ }).click();
        await view.locator("textarea").fill("Watch the canary for an hour after it goes out");
        await page.waitForLoadState("networkidle");
        await settle(page, 600);
        await shot("dev-shell");
      } finally {
        shell.kill();
      }
    },
  },
  ...["general", "plugins"].map((section) => ({
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
