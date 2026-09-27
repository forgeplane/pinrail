// A person deciding through the app: the plugin's view in the review
// screen, the Hand over button, and the core behind them. The plugin suites
// test each view against a stand-in shell; this is the real one.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { clearInbox, core, createReview, linkPlugin } from "./helpers";
import { scratch } from "../helpers/scratch";

const root = path.resolve(__dirname, "..", "..");
const hello = path.join(root, "plugins", "hello");

/**
 * A plugin whose view counts the keys it is sent and, on hand-over, submits
 * `{ ok: "yes" }`, which its own decision schema refuses.
 */
function probe(): string {
  const dir = scratch("pinrail-probe-");
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({
      name: "probe",
      version: "1.0.0",
      title: "Probe",
      entry: "index.html",
      payload_schema: {},
      decision_schema: { type: "object", required: ["ok"], properties: { ok: { type: "boolean" } } },
      shortcuts: [{ keys: "j", does: "Count" }],
    }),
  );
  fs.writeFileSync(
    path.join(dir, "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<p id="keys">0</p><pre id="errors"></pre>
<script>
  let keys = 0;
  document.addEventListener("keydown", (e) => { if (e.key === "j") document.getElementById("keys").textContent = String(++keys); });
  const plugin = Pinrail.connect({
    onInit() {},
    onViolations(errors) { document.getElementById("errors").textContent = errors.map((e) => (e.path || "/") + ": " + e.message).join("\\n"); },
    onCollect() { plugin.submit({ ok: "yes" }); },
  });
</script>`,
  );
  return dir;
}

async function review(request: APIRequestContext, id: string) {
  return (await (await request.get(`${core}/api/v1/reviews/${id}`)).json()) as { status: string; decision?: { data: unknown } };
}

/** Opens a review in the app and waits for its view to be drawn. */
async function open(page: Page, id: string, ready: string) {
  await page.goto(`/#/reviews/${id}`);
  const view = page.frameLocator("#plugin-frame");
  await expect(view.locator(ready)).toBeVisible();
  return view;
}

/** Takes focus out of the view, as a click on the shell around it would. */
async function shellFocus(page: Page) {
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await expect.poll(() => page.evaluate(() => document.activeElement === document.body)).toBe(true);
}

test.beforeAll(async ({ request }) => {
  await linkPlugin(request, hello, "hello");
  await linkPlugin(request, probe(), "probe");
});

test.beforeEach(async ({ page }) => {
  await clearInbox(page.request);
});

test("the answer chosen in the view is handed over and becomes the decision", async ({ page }) => {
  const { id } = await createReview(page.request, { plugin: "hello", title: "Decide: push", payload: { message: "Push the branch?" } });
  const view = await open(page, id, "#yes");

  await view.locator("#comment").fill("after the rebase");
  await view.locator("#yes").click();
  const handover = page.locator("[data-handover]");
  await expect(handover).toContainText("Hand over: yes");
  await handover.click();

  await expect.poll(async () => (await review(page.request, id)).status).toBe("decided");
  expect((await review(page.request, id)).decision?.data).toEqual({ ok: true, comment: "after the rebase" });
  // the view turns to what was decided, and there is nothing left to hand over
  await expect(view.locator("p").last()).toContainText("Decided: yes — after the rebase");
  await expect(handover).toHaveCount(0);
});

test("⌘Enter hands over from the shell", async ({ page }) => {
  const { id } = await createReview(page.request, { plugin: "hello", title: "Decide: keys", payload: { message: "Deploy?" } });
  const view = await open(page, id, "#no");
  await view.locator("#no").click();
  await shellFocus(page);

  await page.keyboard.press("ControlOrMeta+Enter");
  await expect.poll(async () => (await review(page.request, id)).decision?.data).toEqual({ ok: false });
});

test("a decision the core refuses is shown with its violations and decides nothing", async ({ page }) => {
  const { id } = await createReview(page.request, { plugin: "probe", title: "Decide: refused", payload: {} });
  const view = await open(page, id, "#keys");

  await page.locator("[data-handover]").click();
  await expect(page.locator(".violations")).toContainText("/ok");
  await expect(view.locator("#errors")).toContainText("/ok");
  expect((await review(page.request, id)).status).toBe("pending");
});

test("a declared key pressed with the shell in focus reaches the view", async ({ page }) => {
  const { id } = await createReview(page.request, { plugin: "probe", title: "Decide: shortcut", payload: {} });
  const view = await open(page, id, "#keys");
  await shellFocus(page);

  await page.keyboard.press("j");
  await expect(view.locator("#keys")).toHaveText("1");
});

test("a choice survives a reload of the app", async ({ page }) => {
  const { id } = await createReview(page.request, { plugin: "hello", title: "Decide: reload", payload: { message: "Merge?" } });
  const view = await open(page, id, "#no");
  await view.locator("#no").click();
  await view.locator("#comment").fill("not yet");
  // the shell keeps the view's draft for the session; reload once it has it
  await expect
    .poll(() => page.evaluate((key) => sessionStorage.getItem(key), `pinrail:draft:${id}`))
    .toBe(JSON.stringify({ ok: false, comment: "not yet" }));

  await page.reload();
  await expect(view.locator("#no")).toHaveAttribute("aria-pressed", "true");
  await expect(view.locator("#comment")).toHaveValue("not yet");
});

test("a review withdrawn elsewhere turns the open view read-only", async ({ page }) => {
  const { id } = await createReview(page.request, { plugin: "hello", title: "Decide: withdrawn", payload: { message: "Release?" } });
  const view = await open(page, id, "#yes");

  const withdrawn = await page.request.post(`${core}/api/v1/reviews/${id}/withdraw`, { data: { reason: "superseded" } });
  expect(withdrawn.status(), await withdrawn.text()).toBe(200);
  await expect(view.locator("p").last()).toContainText("Closed without a decision (withdrawn)");
  await expect(view.locator("#yes")).toHaveCount(0);
  await expect(page.locator("[data-handover]")).toHaveCount(0);
});

test("an agent waiting in the CLI wakes with what the person decided in the app", async ({ page }) => {
  const cli = path.join(root, "cli", "target", "debug", "pinrail");
  const payload = path.join(scratch("pinrail-decide-"), "payload.json");
  fs.writeFileSync(payload, JSON.stringify({ message: "Ship it?" }));
  const agent = spawn(cli, ["submit", "hello", "--title", "Decide: from the CLI", "--data", payload, "--wait"], {
    cwd: os.tmpdir(),
    env: { ...process.env, PINRAIL_URL: core, PINRAIL_JSON: "1", PINRAIL_CONFIG_DIR: path.dirname(payload) },
  });
  let stdout = "";
  let stderr = "";
  agent.stdout.on("data", (d) => (stdout += d));
  agent.stderr.on("data", (d) => (stderr += d));
  const exited = new Promise<number | null>((resolve) => agent.on("close", resolve));

  await expect.poll(() => stderr.match(/review (r_[0-9A-Z]+) submitted/)?.[1], { message: stderr }).toBeTruthy();
  const id = stderr.match(/review (r_[0-9A-Z]+) submitted/)![1];
  const view = await open(page, id, "#yes");
  await view.locator("#yes").click();
  await page.locator("[data-handover]").click();

  expect(await exited, stderr).toBe(0);
  expect(JSON.parse(stdout).decision.data).toEqual({ ok: true });
});
