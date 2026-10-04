// The test harness against the conformance view: it hosts a view the way the
// app does, for the parts a test does not drive itself.

import { expect, test } from "@playwright/test";
import { spawn } from "node:child_process";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";
import conformance from "./conformance/conformance.cjs";

const dir = fileURLToPath(new URL("./conformance", import.meta.url));
const received = (frame: import("@playwright/test").Frame) =>
  frame.evaluate(() => (window as unknown as { received: any[] }).received);

test("the harness hosts a view as the app does", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, {
    review: fixture(path.join(dir, "fixtures", "basic.json")),
    settings: { mode: "a" },
  });
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
  await expect
    .poll(async () => (await received(frame)).filter((m) => m.type === "init").at(-1)?.draft)
    .toEqual({ step: 2 });

  // another page in the view's place speaks first: it gets nothing
  const before = (await received(frame)).length;
  await frame.evaluate(() => {
    const send = (window as unknown as { send: (m: object) => void }).send;
    send({ type: "ready" });
    send({ type: "attachment", req: 9, name: "note.txt" });
  });
  await page.waitForTimeout(500);
  expect(conformance.secondReadyProblems(await received(frame), before)).toEqual([]);
});

const bin = fileURLToPath(new URL("../bin/pinrail-plugin.mjs", import.meta.url));
const freePort = () =>
  new Promise<number>((resolve) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as net.AddressInfo;
      server.close(() => resolve(port));
    });
  });

async function devFrame(page: import("@playwright/test").Page) {
  const handle = await page.locator("#frame").elementHandle();
  let frame: import("@playwright/test").Frame | null = null;
  await expect
    .poll(
      async () =>
        (frame = await handle!.contentFrame()) !== null &&
        (await received(frame).catch(() => [])).some((m: any) => m.type === "init"),
    )
    .toBe(true);
  return frame!;
}

test("the dev shell hosts a view as the app does", async ({ page }) => {
  const port = await freePort();
  const shell = spawn(process.execPath, [bin, "dev", dir, "--port", String(port), "--no-open"], { stdio: "pipe" });
  try {
    await expect
      .poll(async () => (await fetch(`http://127.0.0.1:${port}/dev/manifest`).catch(() => null))?.status)
      .toBe(200);
    await page.goto(`http://127.0.0.1:${port}/`);
    let frame = await devFrame(page);
    expect(conformance.handshakeProblems(await received(frame))).toEqual([]);

    const send = (message: object) =>
      frame.evaluate((m) => (window as unknown as { send: (m: object) => void }).send(m), message);
    await send({ type: "attachment", req: 7, name: "note.txt" });
    await send({ type: "attachment", req: 8, name: "missing.txt" });
    await expect.poll(async () => (await received(frame)).filter((m) => m.type === "attachment").length).toBe(2);
    expect(conformance.attachmentProblems(await received(frame))).toEqual([]);
    expect(conformance.refusedAttachmentProblems(await received(frame))).toEqual([]);

    await send({ type: "settings_set", patch: { mode: "b" } });
    await expect
      .poll(async () => (await received(frame)).find((m) => m.type === "settings")?.settings)
      .toEqual({ mode: "b" });
    await send({ type: "settings_set", patch: { mode: "z" } });
    await expect.poll(async () => (await received(frame)).some((m) => m.type === "violations")).toBe(true);

    await send({ type: "draft", data: { step: 2 } });
    await page.waitForTimeout(500);
    await page.reload();
    frame = await devFrame(page);
    expect((await received(frame)).find((m) => m.type === "init")?.draft).toEqual({ step: 2 });

    // a submit nobody asked for decides nothing
    await send({ type: "submit", req: 1, data: { ok: true } });
    await page.waitForTimeout(500);
    expect((await received(frame)).filter((m) => m.type === "violations" || m.type === "submitted")).toEqual([]);

    // the Collect button is the app's hand-over: the view answers its request
    const answer = async (data: unknown) => {
      const asked = (await received(frame)).filter((m) => m.type === "collect").length;
      await page.locator("#collect").click();
      await expect.poll(async () => (await received(frame)).filter((m) => m.type === "collect").length).toBe(asked + 1);
      await send({ type: "submit", req: conformance.lastRequest(await received(frame)), data });
    };
    await answer({ ok: "yes" });
    await expect.poll(async () => conformance.violationsProblems(await received(frame))).toEqual([]);
    await answer({ ok: true });
    await expect
      .poll(async () => (await received(frame)).find((m) => m.type === "submitted")?.decision?.data)
      .toEqual({ ok: true });
    expect(conformance.collectProblems(await received(frame))).toEqual([]);

    // another page in the view's place speaks first: it gets nothing
    const before = (await received(frame)).length;
    await send({ type: "ready" });
    await send({ type: "attachment", req: 9, name: "note.txt" });
    await page.waitForTimeout(500);
    expect(conformance.secondReadyProblems(await received(frame), before)).toEqual([]);
  } finally {
    shell.kill();
  }
});
