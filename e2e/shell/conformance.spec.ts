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
const send = (frame: Frame, message: object) => frame.evaluate((m) => (window as unknown as { send: (m: object) => void }).send(m), message);

async function viewFrame(page: Page): Promise<Frame> {
  let frame: Frame | undefined;
  await expect.poll(() => (frame = page.frames().find((f) => f.url().includes("/plugins/conformance/"))) !== undefined).toBe(true);
  await expect.poll(async () => (await received(frame!)).some((m) => m.type === "init")).toBe(true);
  return frame!;
}

test("the app hosts a view as the protocol says", async ({ page }) => {
  await linkPlugin(page.request, dir, "conformance");
  await clearInbox(page.request);
  await page.request.patch(`${core}/api/v1/settings`, { data: { plugins: { conformance: { mode: "a" } } } });
  const bytes = fs.readFileSync(path.join(dir, "fixtures", "note.txt"));
  const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
  const put = await page.request.put(`${core}/api/v1/attachments/${sha256}`, { headers: { "content-type": "application/octet-stream" }, data: bytes });
  expect([200, 201]).toContain(put.status());
  const created = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "conformance",
      title: "Conformance",
      payload: { note: { $attachment: "note.txt" } },
      attachments: { "note.txt": { sha256, size: bytes.length, media_type: "text/plain" } },
    },
  });
  expect(created.status(), await created.text()).toBe(201);
  const { id } = await created.json();

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
  await expect.poll(async () => (await received(frame)).find((m) => m.type === "settings")?.settings).toEqual({ mode: "b" });
  await send(frame, { type: "settings_set", patch: { mode: "z" } });
  await expect.poll(async () => (await received(frame)).some((m) => m.type === "violations")).toBe(true);

  // a draft comes back in init after a reload
  await send(frame, { type: "draft", data: { step: 2 } });
  await expect.poll(() => page.evaluate((key) => sessionStorage.getItem(key), `pinrail:draft:${id}`)).toBe(JSON.stringify({ step: 2 }));
  await page.reload();
  frame = await viewFrame(page);
  expect((await received(frame)).find((m) => m.type === "init")?.draft).toEqual({ step: 2 });

  // a decision the schema refuses gets violations; one it accepts, submitted
  await send(frame, { type: "submit", data: { ok: "yes" } });
  await expect.poll(async () => conformance.violationsProblems(await received(frame))).toEqual([]);
  await send(frame, { type: "submit", data: { ok: true } });
  await expect.poll(async () => (await received(frame)).find((m) => m.type === "submitted")?.decision?.data).toEqual({ ok: true });
  const review = await (await page.request.get(`${core}/api/v1/reviews/${id}`)).json();
  expect(review.status).toBe("decided");
});

// The preview checks a hand-over and decides nothing: it keeps no drafts
// or settings, and never says submitted.
test("the preview hosts a view as the protocol says, and decides nothing", async ({ page }) => {
  await linkPlugin(page.request, dir, "conformance");
  const bytes = fs.readFileSync(path.join(dir, "fixtures", "note.txt"));
  const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
  const put = await page.request.put(`${core}/api/v1/attachments/${sha256}`, { headers: { "content-type": "application/octet-stream" }, data: bytes });
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
});
