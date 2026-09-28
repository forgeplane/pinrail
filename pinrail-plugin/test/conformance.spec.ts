// The test harness against the conformance view: it hosts a view the way the
// app does, for the parts a test does not drive itself.

import { expect, test } from "@playwright/test";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";
import conformance from "./conformance/conformance.cjs";

const dir = fileURLToPath(new URL("./conformance", import.meta.url));
const received = (frame: import("@playwright/test").Frame) => frame.evaluate(() => (window as unknown as { received: any[] }).received);

test("the harness hosts a view as the app does", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: fixture(path.join(dir, "fixtures", "basic.json")), settings: { mode: "a" } });
  const frame = page.frames().find((f) => f.url().includes("/view/index.html"))!;
  await expect.poll(async () => (await received(frame)).some((m) => m.type === "init")).toBe(true);
  expect(conformance.handshakeProblems(await received(frame))).toEqual([]);

  await frame.evaluate(() => {
    const send = (window as unknown as { send: (m: object) => void }).send;
    send({ type: "attachment", req: 7, name: "note.txt" });
    send({ type: "attachment", req: 8, name: "missing.txt" });
    send({ type: "draft", data: { step: 2 } });
  });
  await expect.poll(async () => (await received(frame)).filter((m) => m.type === "attachment").length).toBe(2);
  expect(conformance.attachmentProblems(await received(frame))).toEqual([]);
  expect(conformance.refusedAttachmentProblems(await received(frame))).toEqual([]);

  // a draft comes back in init after a reload
  await expect.poll(() => plugin.lastDraft()).toEqual({ step: 2 });
  await plugin.reinit();
  await expect.poll(async () => (await received(frame)).filter((m) => m.type === "init").at(-1)?.draft).toEqual({ step: 2 });
});
