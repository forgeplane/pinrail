// The app against the conformance view, which speaks the raw plugin protocol:
// the host a plugin ships into answers every message as protocol.md says.

import { expect, test, type Frame, type Page } from "@playwright/test";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import conformance from "../../pinrail-plugin/test/conformance/conformance.cjs";
import { clearInbox, core, linkPlugin } from "./helpers";

const dir = path.resolve(__dirname, "..", "..", "pinrail-plugin", "test", "conformance");
const received = (frame: Frame) => frame.evaluate(() => (window as unknown as { received: any[] }).received);
const send = (frame: Frame, message: object) =>
  frame.evaluate((m) => (window as unknown as { send: (m: object) => void }).send(m), message);

/** The conformance view's frame once it has its init; `after` is a frame it must not be. */
async function viewFrame(page: Page, after?: Frame): Promise<Frame> {
  let frame: Frame | undefined;
  await expect
    .poll(
      () =>
        (frame = page.frames().find((f) => f !== after && !f.isDetached() && f.url().includes("/bundles/"))) !==
        undefined,
    )
    .toBe(true);
  await expect.poll(async () => (await received(frame!)).some((m) => m.type === "init")).toBe(true);
  return frame!;
}

/** A review for the conformance view, with the file its payload names. */
async function conformanceReview(page: Page, title = "Conformance"): Promise<string> {
  const bytes = fs.readFileSync(path.join(dir, "fixtures", "note.txt"));
  const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
  const put = await page.request.put(`${core}/api/v1/attachments/${sha256}`, {
    headers: { "content-type": "application/octet-stream" },
    data: bytes,
  });
  expect([200, 201]).toContain(put.status());
  const created = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "conformance",
      title,
      payload: { note: { $attachment: "note.txt" } },
      attachments: { "note.txt": { sha256, size: bytes.length, media_type: "text/plain" } },
    },
  });
  expect(created.status(), await created.text()).toBe(201);
  const { id } = await created.json();
  return id;
}

test("the app hosts a view as the protocol says", async ({ page }) => {
  await linkPlugin(page.request, dir, "conformance");
  await clearInbox(page.request);
  await page.request.patch(`${core}/api/v1/settings`, { data: { plugins: { conformance: { mode: "a" } } } });
  const id = await conformanceReview(page);

  await page.goto(`/#/reviews/${id}`);
  let frame = await viewFrame(page);
  expect(conformance.handshakeProblems(await received(frame))).toEqual([]);

  // files the review carries, and a refusal for one it does not
  await send(frame, { type: "attachment", req: 7, name: "note.txt" });
  await send(frame, { type: "attachment", req: 8, name: "missing.txt" });
  await expect.poll(async () => (await received(frame)).filter((m) => m.type === "attachment").length).toBe(2);
  expect(conformance.attachmentProblems(await received(frame))).toEqual([]);
  expect(conformance.refusedAttachmentProblems(await received(frame))).toEqual([]);

  // a setting the schema allows comes back as settings; one it refuses, as violations
  await send(frame, { type: "settings_set", patch: { mode: "b" } });
  await expect
    .poll(async () => (await received(frame)).find((m) => m.type === "settings")?.settings)
    .toEqual({ mode: "b" });
  await send(frame, { type: "settings_set", patch: { mode: "z" } });
  await expect.poll(async () => (await received(frame)).some((m) => m.type === "violations")).toBe(true);

  // a draft comes back in init after a reload
  await send(frame, { type: "draft", data: { step: 2 } });
  await expect
    .poll(() => page.evaluate((key) => sessionStorage.getItem(key), `pinrail:draft:${id}`))
    .toBe(JSON.stringify({ step: 2 }));
  await page.reload();
  frame = await viewFrame(page);
  expect((await received(frame)).find((m) => m.type === "init")?.draft).toEqual({ step: 2 });

  // a decision the schema refuses gets violations; one it accepts, submitted
  await send(frame, { type: "submit", data: { ok: "yes" } });
  await expect.poll(async () => conformance.violationsProblems(await received(frame))).toEqual([]);
  // the app closes the view as it returns to the inbox, so what the view
  // is told from here on is read from its console
  const told: { type: string; decision?: { data: unknown } }[] = [];
  page.on("console", (m) => {
    const text = m.text();
    if (text.startsWith("view got ")) told.push(JSON.parse(text.slice("view got ".length)));
  });
  await frame.evaluate(() =>
    window.addEventListener("message", (e) => console.log(`view got ${JSON.stringify(e.data)}`)),
  );
  await send(frame, { type: "submit", data: { ok: true } });
  // the view that handed the review over is told submitted, and not init
  // again, before the app returns to the inbox
  await expect(page).toHaveURL(/#\/$/);
  expect(told.find((m) => m.type === "submitted")?.decision?.data).toEqual({ ok: true });
  expect(told.filter((m) => m.type === "init")).toEqual([]);
  const review = await (await page.request.get(`${core}/api/v1/reviews/${id}`)).json();
  expect(review.status).toBe("decided");
});

test("a decision that lands after moving to another review stays with its own", async ({ page }) => {
  await linkPlugin(page.request, dir, "conformance");
  await clearInbox(page.request);
  const first = await conformanceReview(page, "Conformance: first");
  const second = await conformanceReview(page, "Conformance: second");
  const draftOf = (id: string) => page.evaluate((key) => sessionStorage.getItem(key), `pinrail:draft:${id}`);

  // the second review has a draft of the person's
  await page.goto(`/#/reviews/${second}`);
  let frame = await viewFrame(page);
  await send(frame, { type: "draft", data: { step: 5 } });
  await expect.poll(() => draftOf(second)).not.toBeNull();

  // the first review's decision is held back while the person moves on
  await page.goto(`/#/reviews/${first}`);
  frame = await viewFrame(page, frame);
  let released!: () => void;
  const held = new Promise<void>((resolve) => (released = resolve));
  let asked!: () => void;
  const sent = new Promise<void>((resolve) => (asked = resolve));
  await page.route(`${core}/api/v1/reviews/${first}/decision`, async (route) => {
    asked();
    await held;
    await route.continue();
  });
  await send(frame, { type: "submit", data: { ok: true } });
  await sent;
  await page.evaluate((id) => (location.hash = `#/reviews/${id}`), second);
  await expect(page.locator(".crumb-title")).toHaveText("Conformance: second");
  frame = await viewFrame(page, frame);
  const answered = page.waitForResponse(`${core}/api/v1/reviews/${first}/decision`);
  released();
  await answered;
  await expect
    .poll(async () => (await (await page.request.get(`${core}/api/v1/reviews/${first}`)).json()).status)
    .toBe("decided");

  // anything the app sent the second view about it has arrived by the time
  // a later request is answered
  await send(frame, { type: "attachment", req: 11, name: "note.txt" });
  await expect
    .poll(async () => (await received(frame)).some((m) => m.type === "attachment" && m.req === 11))
    .toBe(true);
  // the screen still shows the second review, and its view was told nothing
  await expect(page.locator(".crumb-title")).toHaveText("Conformance: second");
  expect((await received(frame)).filter((m) => m.type === "submitted")).toEqual([]);
  expect(await draftOf(second)).not.toBeNull();
  await expect(page).toHaveURL(new RegExp(`/reviews/${second}$`));
  await expect(page.locator(".review-strip .status-badge")).toHaveText(/pending/i);
  await expect(page.getByText("Decision recorded")).toHaveCount(0);
});

test("a second ready from the frame is another page, which the app answers nothing", async ({ page }) => {
  await linkPlugin(page.request, dir, "conformance");
  await clearInbox(page.request);
  const id = await conformanceReview(page);
  await page.goto(`/#/reviews/${id}`);
  const frame = await viewFrame(page);
  const before = (await received(frame)).length;
  await send(frame, { type: "ready" });
  await send(frame, { type: "attachment", req: 9, name: "note.txt" });
  await page.waitForTimeout(500);
  expect(conformance.secondReadyProblems(await received(frame), before)).toEqual([]);
});

test("a view whose review ends elsewhere is sent init again, read-only", async ({ page }) => {
  await linkPlugin(page.request, dir, "conformance");
  await clearInbox(page.request);
  const id = await conformanceReview(page);
  await page.goto(`/#/reviews/${id}`);
  const frame = await viewFrame(page);
  const withdrawn = await page.request.post(`${core}/api/v1/reviews/${id}/withdraw`, {
    data: { reason: "not needed" },
  });
  expect(withdrawn.status(), await withdrawn.text()).toBe(200);
  await expect
    .poll(async () => (await received(frame)).filter((m) => m.type === "init").map((m) => m.readonly))
    .toEqual([false, true]);
});

// The preview checks a hand-over and decides nothing: it keeps no drafts
// or settings, and never says submitted.
test("the preview hosts a view as the protocol says, and decides nothing", async ({ page }) => {
  await linkPlugin(page.request, dir, "conformance");
  const bytes = fs.readFileSync(path.join(dir, "fixtures", "note.txt"));
  const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
  const put = await page.request.put(`${core}/api/v1/attachments/${sha256}`, {
    headers: { "content-type": "application/octet-stream" },
    data: bytes,
  });
  expect([200, 201]).toContain(put.status());
  const created = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "conformance",
      title: "Preview",
      payload: { note: { $attachment: "note.txt" } },
      attachments: { "note.txt": { sha256, size: bytes.length, media_type: "text/plain" } },
    },
  });
  expect(created.status(), await created.text()).toBe(201);
  const { id } = await created.json();

  await page.goto(`${core}/preview/reviews/${id}`);
  const frame = await viewFrame(page);
  expect(conformance.handshakeProblems(await received(frame))).toEqual([]);

  await send(frame, { type: "attachment", req: 7, name: "note.txt" });
  await send(frame, { type: "attachment", req: 8, name: "missing.txt" });
  await expect.poll(async () => (await received(frame)).filter((m) => m.type === "attachment").length).toBe(2);
  expect(conformance.attachmentProblems(await received(frame))).toEqual([]);
  expect(conformance.refusedAttachmentProblems(await received(frame))).toEqual([]);

  await send(frame, { type: "submit", data: { ok: "yes" } });
  await expect.poll(async () => conformance.violationsProblems(await received(frame))).toEqual([]);
  await send(frame, { type: "submit", data: { ok: true } });
  await expect(page.locator("body")).toContainText("The decision passes");
  expect((await received(frame)).some((m) => m.type === "submitted")).toBe(false);
  const review = await (await page.request.get(`${core}/api/v1/reviews/${id}`)).json();
  expect(review.status).toBe("pending");

  // another page in the view's place speaks first: it gets nothing
  const before = (await received(frame)).length;
  await send(frame, { type: "ready" });
  await send(frame, { type: "attachment", req: 9, name: "note.txt" });
  await page.waitForTimeout(500);
  expect(conformance.secondReadyProblems(await received(frame), before)).toEqual([]);
});
