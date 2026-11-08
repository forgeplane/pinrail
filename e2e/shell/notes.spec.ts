// Notes an agent sends with `pinrail status --note`: Markdown shown as a card
// in the session's thread, which informs and changes nothing else.

import { expect, test, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { core } from "./helpers";

let n = 0;
/** A session of its own for each test, so threads never share statuses. */
const fresh = () => `s-note-${Date.now()}-${++n}`;

async function report(page: Page, session: string, body: Record<string, unknown>) {
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data: body });
  expect(response.status(), await response.text()).toBe(200);
}

test("a note shows in the thread as Markdown, and a status after it as a status", async ({ page }) => {
  const session = fresh();
  await report(page, session, { text: "Add rate limiting", kind: "start" });
  await report(page, session, { text: "Running the tests" });
  await report(page, session, {
    text: "## What I found\n\n- **3** callers of the old endpoint\n- all in `billing/`\n\nSee [the docs](https://example.com/limits).",
    kind: "note",
  });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const note = page.getByTestId("note");
  await expect(note.getByRole("heading", { name: "What I found" })).toBeVisible();
  await expect(note.getByRole("listitem")).toHaveText(["3 callers of the old endpoint", "all in billing/"]);
  await expect(note.locator("strong")).toHaveText("3");
  await expect(note.getByRole("link", { name: "the docs" })).toHaveAttribute("href", "https://example.com/limits");
  // the step before it is a status of its own
  await expect(page.getByTestId("status")).toHaveText([/Running the tests/]);

  // a note sent while the thread is open shows at once
  await report(page, session, { text: "Pushed the branch.", kind: "note" });
  await expect(page.getByTestId("note")).toHaveCount(2);
  await expect(page.getByTestId("note").last()).toContainText("Pushed the branch.");
});

test("a long note folds behind Show more", async ({ page }) => {
  const session = fresh();
  const lines = Array.from({ length: 40 }, (_, i) => `- line ${i + 1}`).join("\n");
  await report(page, session, { text: lines, kind: "note" });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const body = page.getByTestId("note").locator(".thread-note-body");
  const folded = (await body.boundingBox())?.height ?? 0;
  expect(folded).toBeLessThan(300);
  await page.getByTestId("note").getByRole("button", { name: "Show more" }).click();
  expect((await body.boundingBox())?.height ?? 0).toBeGreaterThan(folded * 2);
  await page.getByTestId("note").getByRole("button", { name: "Show less" }).click();
  expect((await body.boundingBox())?.height ?? 0).toBeLessThan(300);
});

test("a note is not the latest status on the Sessions page", async ({ page }) => {
  const session = fresh();
  await report(page, session, { text: "Running the tests" });
  await report(page, session, { text: "A summary nobody should see as the status", kind: "note" });
  await page.goto(`/#/sessions?q=${session}`);
  const row = page.getByTestId("session-row").filter({ hasText: session });
  await expect(row.getByTestId("session-status")).toHaveText(/Running the tests/);
});

/** Uploads bytes as an attachment, and answers their hash. */
async function upload(page: Page, bytes: Buffer) {
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  const response = await page.request.put(`${core}/api/v1/attachments/${sha256}`, {
    data: bytes,
    headers: { "content-type": "application/octet-stream" },
  });
  expect(response.ok(), await response.text()).toBe(true);
  return sha256;
}

// a 2 by 2 PNG, enough for the browser to draw
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFklEQVR4nGNkYPj/n4GBgYGJgYEBAAomAgFTJp+1AAAAAElFTkSuQmCC",
  "base64",
);

test("a thread opened at its end stays there as a note's image loads", async ({ page }) => {
  const session = fresh();
  for (let i = 1; i <= 30; i++) await report(page, session, { text: `Stage ${i}`, kind: "start" });
  const chart = await upload(page, PNG);
  await report(page, session, {
    kind: "note",
    text: "The chart:\n\n![chart](attachment:chart.png)",
    attachments: { "chart.png": { sha256: chart, size: PNG.length, media_type: "image/png" } },
  });
  // the image arrives late, and tall
  let served = false;
  await page.route("**/attachments/chart.png", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    served = true;
    await route.fulfill({
      contentType: "image/svg+xml",
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="200"></svg>',
    });
  });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const items = page.locator(".thread-items");
  await expect(page.getByTestId("note")).toBeVisible();
  await expect.poll(() => served).toBe(true);
  await expect
    .poll(() => page.getByRole("img", { name: "chart" }).evaluate((img: HTMLImageElement) => img.naturalHeight))
    .toBe(200);
  await expect.poll(() => items.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop)).toBeLessThan(2);
});

test("a note's image shows in place, and its files are chips that save", async ({ page }) => {
  const session = fresh();
  const chart = await upload(page, PNG);
  const log = Buffer.from("the test log\n");
  const logHash = await upload(page, log);
  await report(page, session, {
    kind: "note",
    text: "Latency fell:\n\n![p95 latency](attachment:latency.png)\n\nThe output is in [the log](attachment:run.log).",
    attachments: {
      "latency.png": { sha256: chart, size: PNG.length, media_type: "image/png" },
      "run.log": { sha256: logHash, size: log.length, media_type: "text/plain" },
    },
  });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const note = page.getByTestId("note");
  const image = note.locator(".thread-note-body img");
  await expect(image).toHaveAttribute("alt", "p95 latency");
  await expect.poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(2);
  await expect(note).toContainText("2 files");
  await expect(note.getByTestId("note-file")).toHaveText([/latency\.png/, /run\.log/]);

  // a chip's button and a link to the file both save it
  const saved = page.waitForEvent("download");
  await note.getByRole("button", { name: "Save run.log" }).click();
  expect((await saved).suggestedFilename()).toBe("run.log");
  const linked = page.waitForEvent("download");
  await note.getByRole("link", { name: "the log" }).click();
  expect((await linked).suggestedFilename()).toBe("run.log");
});
