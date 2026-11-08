import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { mountPlugin } from "../testing/index.cjs";
import { scratch } from "./scratch.cjs";

// A view with text to find: once in a paragraph, once split by a link, once
// far down, and twice where it does not show.
function view(): string {
  const dir = scratch("pinrail-find-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "find", version: "1.0.0", title: "Find" }));
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<p id="ready">waiting</p>
<p id="one">A Pivot table</p>
<p id="two">The <a href="#">piv</a>ot of it</p>
<p id="styled"><strong>Quokka</strong> <em>loader</em></p>
<p hidden>pivot, hidden</p>
<p style="visibility: hidden">pivot, invisible</p>
<div style="height: 3000px"></div>
<p id="far">the last pivot</p>
<textarea id="field"></textarea>
<script>
  Pinrail.connect({ onInit() { document.getElementById("ready").textContent = "ready"; } });
</script>`,
  );
  return dir;
}

test("the view finds the text that shows, steps through it, and clears it", async ({ page }) => {
  const plugin = await mountPlugin(page, view(), { review: { payload: {} } });
  const f = plugin.frame;
  await expect(f.locator("#ready")).toHaveText("ready");
  const found = async (req: number) =>
    (await plugin.messages())
      .filter((m: any) => m.type === "found" && m.req === req)
      .map((m: any) => [m.count, m.current]);
  const highlighted = () =>
    f.locator("body").evaluate(() => {
      const all = (CSS as any).highlights.get("pinrail-find");
      const now = (CSS as any).highlights.get("pinrail-find-current");
      return [all ? all.size : 0, (now && [...now][0]?.toString()) ?? null];
    });

  await plugin.send({ type: "find", req: 1, query: "pivot", backwards: false });
  await expect.poll(() => found(1)).toEqual([[3, 1]]);
  expect(await highlighted()).toEqual([3, "Pivot"]);

  await plugin.send({ type: "find", req: 2, query: "pivot", backwards: false });
  await expect.poll(() => found(2)).toEqual([[3, 2]]);
  // the match that crosses the link
  expect(await highlighted()).toEqual([3, "pivot"]);

  await plugin.send({ type: "find", req: 3, query: "pivot", backwards: false });
  await expect.poll(() => found(3)).toEqual([[3, 3]]);
  // the last one is scrolled to
  await expect(f.locator("#far")).toBeInViewport();

  await plugin.send({ type: "find", req: 4, query: "pivot", backwards: false });
  await expect.poll(() => found(4)).toEqual([[3, 1]]);
  await plugin.send({ type: "find", req: 5, query: "pivot", backwards: true });
  await expect.poll(() => found(5)).toEqual([[3, 3]]);

  await plugin.send({ type: "find", req: 6, query: "nothing like it", backwards: false });
  await expect.poll(() => found(6)).toEqual([[0, 0]]);

  await plugin.send({ type: "find", req: 7, query: "", backwards: false });
  await expect.poll(() => found(7)).toEqual([[0, 0]]);
  expect(await highlighted()).toEqual([0, null]);
});

test("the count goes back again when the view's content changes", async ({ page }) => {
  const plugin = await mountPlugin(page, view(), { review: { payload: {} } });
  const f = plugin.frame;
  await expect(f.locator("#ready")).toHaveText("ready");
  const found = async () =>
    (await plugin.messages()).filter((m: any) => m.type === "found" && m.req === 1).map((m: any) => m.count);

  await plugin.send({ type: "find", req: 1, query: "pivot", backwards: false });
  await expect.poll(found).toEqual([3]);
  await f
    .locator("#one")
    .evaluate((el) => el.after(Object.assign(document.createElement("p"), { textContent: "pivot" })));
  await expect.poll(found).toEqual([3, 4]);
});

test("⌘F or Ctrl+F goes up from anywhere in a view, a text field too", async ({ page }) => {
  const plugin = await mountPlugin(page, view(), { review: { payload: {} } });
  const f = plugin.frame;
  await expect(f.locator("#ready")).toHaveText("ready");
  const finds = async () =>
    (await plugin.messages()).filter((m: any) => m.type === "key" && m.key === "f" && (m.metaKey || m.ctrlKey)).length;

  await f.locator("#ready").click();
  await page.keyboard.press("ControlOrMeta+f");
  await f.locator("#field").click();
  await page.keyboard.press("ControlOrMeta+f");
  await expect.poll(finds).toBe(2);
});

// what shows, not what the registry holds: a highlight a webview does not
// repaint stays on the screen
test("a longer query leaves nothing painted from the shorter one", async ({ page }) => {
  const dir = scratch("pinrail-find-paint-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "fp", version: "1.0.0", title: "FP" }));
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<p id="ready" style="font: 40px monospace">waiting</p><p id="other" style="font: 40px monospace">pa</p>
<script>Pinrail.connect({ onInit() { document.getElementById("ready").textContent = "pi"; } });</script>`,
  );
  const plugin = await mountPlugin(page, dir, { review: { payload: {} } });
  const other = plugin.frame.locator("#other");
  await expect(plugin.frame.locator("#ready")).toHaveText("pi");
  const before = await other.screenshot();
  const found = async (req: number) => (await plugin.messages()).some((m: any) => m.type === "found" && m.req === req);
  await plugin.send({ type: "find", req: 1, query: "p", backwards: false });
  await expect.poll(() => found(1)).toBe(true);
  await page.waitForTimeout(100);
  expect((await other.screenshot()).equals(before)).toBe(false);
  await plugin.send({ type: "find", req: 2, query: "pi", backwards: false });
  await expect.poll(() => found(2)).toBe(true);
  await page.waitForTimeout(100);
  expect((await other.screenshot()).equals(before)).toBe(true);
});

test("the view finds text across the space between two inline elements", async ({ page }) => {
  const plugin = await mountPlugin(page, view(), { review: { payload: {} } });
  await expect(plugin.frame.locator("#ready")).toHaveText("ready");
  const count = async (req: number) =>
    (await plugin.messages()).filter((m: any) => m.type === "found" && m.req === req).map((m: any) => m.count);
  await plugin.send({ type: "find", req: 1, query: "Quokka loader", backwards: false });
  await expect.poll(() => count(1)).toEqual([1]);
  await plugin.send({ type: "find", req: 2, query: "Quokkaloader", backwards: false });
  await expect.poll(() => count(2)).toEqual([0]);
});
