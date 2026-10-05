import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { withDevShell } from "./dev-shell";
import { scratch } from "./scratch.cjs";

// `pinrail-plugin dev`, the shell in a browser: a plugin from create, with
// its sample and a decided fixture beside it.
const sdk = path.resolve(import.meta.dirname, "..");
const bin = path.join(sdk, "bin", "pinrail-plugin.mjs");

test("the menu offers the samples and the fixtures, marks a decided one, and loads the one chosen", async ({
  page,
}) => {
  const dir = path.join(scratch("pinrail-dev-"), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });
  const sample = JSON.parse(fs.readFileSync(path.join(dir, "samples", "triage.json"), "utf8"));
  // a decided round, with the pending one's title
  fs.mkdirSync(path.join(dir, "fixtures"));
  fs.writeFileSync(
    path.join(dir, "fixtures", "decided.json"),
    JSON.stringify({
      ...sample,
      decision: { decided_by: "you", decided_at: "2026-09-16T09:00:00Z", data: { ok: false } },
    }),
  );

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    const menu = page.locator("#fixture");
    // the sample first, as the one the app would show
    await expect(menu.locator("option")).toHaveText([sample.title, `${sample.title} (decided)`]);
    const view = page.frameLocator("#frame");
    await expect(view.getByRole("button", { name: "Yes" })).toBeVisible();

    await menu.selectOption("fixtures/decided.json");
    await expect(view.locator("body")).toContainText("Decided: no");
    // the frame shows the same page for every fixture; choosing another still loads it again
    await menu.selectOption("samples/triage.json");
    await expect(view.getByRole("button", { name: "Yes" })).toBeVisible();
  });
});

test("the view is set in the app's typeface, and nothing is fetched from off the machine", async ({ page }) => {
  const dir = path.join(scratch("pinrail-dev-"), "typeface");
  execFileSync(process.execPath, [bin, "create", "typeface", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });
  const away: string[] = [];
  page.on("request", (r) => new URL(r.url()).hostname !== "127.0.0.1" && away.push(r.url()));

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    await expect(page.frameLocator("#frame").getByRole("button", { name: "Yes" })).toBeVisible();
    const view = page.frames().find((f) => f.url().includes("/plugin/"))!;

    const loaded = await view.evaluate(async () => {
      await document.fonts.ready;
      return [...document.fonts].some((f) => f.family.replace(/"/g, "") === "Inter Variable" && f.status === "loaded");
    });
    expect(loaded).toBe(true);
    expect(away).toEqual([]);
  });
});

test("select picks an element of the view to comment on, and the comments are copied as Markdown", async ({
  page,
  context,
}) => {
  const dir = path.join(scratch("pinrail-dev-"), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });
  const sample = JSON.parse(fs.readFileSync(path.join(dir, "samples", "triage.json"), "utf8"));
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    const view = page.frameLocator("#frame");
    const yes = view.getByRole("button", { name: "Yes" });
    await expect(yes).toBeVisible();

    // selecting, a click picks the element and the view does not see it
    await page.locator("#select").click();
    await yes.click();
    const pop = page.locator("#comment-pop");
    await expect(pop).toBeVisible();
    await expect(pop.locator(".el")).toContainText('button "Yes"');
    await pop.locator("textarea").fill("Make Yes the primary button.");
    await pop.getByRole("button", { name: "Comment" }).click();
    await expect(page.locator("#s-draft")).toHaveText("—");

    const list = page.locator("#comments .comment");
    await expect(list).toHaveCount(1);
    await expect(list.first()).toContainText("Make Yes the primary button.");
    await expect(view.locator("[data-pinrail-review-pin]")).toHaveText("1");

    // copied as Markdown, under the review and mode it was made in
    await page.locator("#copy").click();
    const copied = await page.evaluate(() => navigator.clipboard.readText());
    expect(copied).toContain("# Review of triage");
    expect(copied).toContain(`## ${sample.title} · pending · dark`);
    expect(copied).toMatch(/1\. `button` "Yes" \(`[^`]+`\)\n {3}Make Yes the primary button\./);

    // not selecting, the view takes clicks again
    await page.locator("#select").click();
    await yes.click();
    await expect(page.locator("#s-draft")).not.toHaveText("—");

    // the comments outlive a reload
    await page.reload();
    await expect(page.locator("#comments .comment")).toHaveCount(1);
  });
});

test("the payload switch shows the review's payload as coloured JSON in place of the view", async ({ page }) => {
  const dir = path.join(scratch("pinrail-dev-"), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });
  const sample = JSON.parse(fs.readFileSync(path.join(dir, "samples", "triage.json"), "utf8"));

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    await expect(page.frameLocator("#frame").getByRole("button", { name: "Yes" })).toBeVisible();

    await page.locator("#show-payload").click();
    const json = page.locator("#payload");
    await expect(json).toBeVisible();
    await expect(page.locator("#frame")).toBeHidden();
    // the payload alone, pretty printed, its keys and values marked for colour
    expect(JSON.parse((await json.textContent()) ?? "")).toEqual(sample.payload);
    await expect(json.locator(".json-key").first()).toBeVisible();

    await page.locator("#show-payload").click();
    await expect(page.locator("#frame")).toBeVisible();
    await expect(json).toBeHidden();
  });
});

test("a request to the dev server clears the page's comments, as an agent does once it has made the changes", async ({
  page,
}) => {
  const dir = path.join(scratch("pinrail-dev-"), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    const yes = page.frameLocator("#frame").getByRole("button", { name: "Yes" });
    await expect(yes).toBeVisible();
    await page.locator("#select").click();
    await yes.click();
    await page.locator("#comment-pop textarea").fill("Make Yes the primary button.");
    await page.locator("#comment-pop").getByRole("button", { name: "Comment" }).click();
    await expect(page.locator("#comments .comment")).toHaveCount(1);

    const cleared = await fetch(new URL("/dev/clear-comments", url), { method: "POST" });
    expect(cleared.status).toBe(204);
    await expect(page.locator("#comments .comment")).toHaveCount(0);
    await page.reload();
    await expect(page.locator("#comments .comment")).toHaveCount(0);
  });
});

test("the app's composer sits under the view, hands over, and takes comments of its own", async ({ page }) => {
  const dir = path.join(scratch("pinrail-dev-"), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    const view = page.frameLocator("#frame");
    await view.getByRole("button", { name: "Yes" }).click();
    const handOver = page.locator("#handover");
    // the label the view gives, as on the app's button
    await expect(handOver).toContainText(await page.locator("#s-status").innerText());

    // with Select on, the button is commented on, not pressed
    await page.locator("#select").click();
    await handOver.click();
    const pop = page.locator("#comment-pop");
    await expect(pop.locator(".el")).toContainText("app › hand-over button");
    await pop.locator("textarea").fill("Say what is handed over.");
    await pop.getByRole("button", { name: "Comment" }).click();
    await expect(page.locator("#comments .comment .el")).toHaveText(/button "[^"]+"/);
    await expect(page.locator("#s-submit")).toHaveText("—");

    // pressed, it asks the view for the decision as the app's button does
    await page.locator("#select").click();
    await handOver.click();
    await expect(page.locator("#s-submit")).not.toHaveText("—");

    // a review that cannot be decided has no composer, as in the app
    await page.locator("#readonly").check();
    await expect(page.locator("#composer")).toBeHidden();
  });
});

test("a plugin with no settings or shortcuts says so across the side panel", async ({ page }) => {
  const dir = path.join(scratch("pinrail-dev-"), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    for (const id of ["#settings", "#shortcuts"]) {
      const said = page.locator(`${id} .empty`);
      await expect(said).toContainText("The manifest declares none");
      // the sentence runs across the panel, not down the 60-pixel label column of a row
      const text = await said.evaluate((el) => {
        const range = document.createRange();
        range.selectNodeContents(el);
        return range.getBoundingClientRect().width;
      });
      expect(text).toBeGreaterThan(100);
    }
  });
});
