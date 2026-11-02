import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { reviewFrom, mountPlugin } from "pinrail-sdk/testing";
import { scratch } from "./scratch.cjs";

// What the SDK's stylesheet gives every view, checked in a view's frame.

/** A plugin whose view is a button and the SDK's text fields. */
function fields(): string {
  const dir = scratch("pinrail-styles-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "styles", version: "1.0.0" }));
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<button class="pinrail-btn" id="start">Start</button>
<input class="pinrail-field" id="field" aria-label="field">
<textarea class="pinrail-note" id="note" aria-label="note"></textarea>
<script>Pinrail.connect({ onInit() {} });</script>`,
  );
  return dir;
}

test("a text field reached from the keyboard shows the focus ring", async ({ page }) => {
  // the ring is how a keyboard user sees where they are
  const plugin = await mountPlugin(page, fields(), { review: reviewFrom({ title: "Styles", payload: {} }) });
  const f = plugin.frame;
  await f.locator("#start").focus();
  for (const id of ["field", "note"]) {
    await f.locator("body").press("Tab");
    await expect(f.locator(`#${id}`)).toBeFocused();
    const outline = await f.locator(`#${id}`).evaluate((el) => getComputedStyle(el).outlineStyle);
    expect(outline, `#${id} lost its focus ring`).not.toBe("none");
  }
});

/** A plugin whose view is `body`, under the stylesheets in `links`. */
function viewOf(body: string, links = ["/sdk/v1/pinrail-plugin.css"]): string {
  const dir = scratch("pinrail-styles-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "styles", version: "1.0.0" }));
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html>
<meta charset="utf-8">
${links.map((l) => `<link rel="stylesheet" href="${l}">`).join("\n")}
<script src="/sdk/v1/pinrail-plugin.js"></script>
${body}
<script>Pinrail.connect({ onInit() {} });</script>`,
  );
  return dir;
}

const review = () => reviewFrom({ title: "Styles", payload: {} });
const style = (frame, selector: string, property: string) =>
  frame.locator(selector).evaluate((el, p) => getComputedStyle(el).getPropertyValue(p).trim(), property);

for (const theme of ["dark", "light"] as const) {
  test(`a view's own --border and .btn are its own, beside the stylesheet, in the ${theme} theme`, async ({ page }) => {
    // the names a component library such as Bootstrap or shadcn/ui brings
    const dir = viewOf(`<style>
  :root { --border: rgb(4, 5, 6); }
  .btn { color: rgb(1, 2, 3); border: 1px solid var(--border); }
</style>
<button class="btn" id="theirs">Theirs</button>
<button class="pinrail-btn" id="ours">Ours</button>`);
    const plugin = await mountPlugin(page, dir, { review: review(), theme });
    const f = plugin.frame;
    await expect(f.locator("#theirs")).toBeVisible();
    expect(await style(f, "#theirs", "color")).toBe("rgb(1, 2, 3)");
    expect(await style(f, "#theirs", "border-top-color")).toBe("rgb(4, 5, 6)");
    // the stylesheet's own button reads its own token, not the view's
    expect(await style(f, "#ours", "border-top-color")).not.toBe("rgb(4, 5, 6)");
  });
}

test("a view's rule of the lowest specificity beats a component of the stylesheet", async ({ page }) => {
  const dir = viewOf(`<style>button { color: rgb(7, 8, 9); }</style>
<button class="pinrail-btn pinrail-btn-danger" id="b">Delete</button>`);
  const plugin = await mountPlugin(page, dir, { review: review() });
  await expect(plugin.frame.locator("#b")).toBeVisible();
  expect(await style(plugin.frame, "#b", "color")).toBe("rgb(7, 8, 9)");
});

test("tokens.css alone gives the palette, and no rule for any element", async ({ page }) => {
  const dir = viewOf(`<p id="p">Plain</p>`, ["/sdk/v1/tokens.css"]);
  const plugin = await mountPlugin(page, dir, { review: review(), theme: "light" });
  const f = plugin.frame;
  await expect(f.locator("#p")).toBeVisible();
  expect(await style(f, "html", "--pinrail-bg")).toBe("#ffffff");
  expect(await style(f, "html", "--pinrail-danger")).not.toBe("");
  // the browser's own body margin, and no font of ours
  expect(await style(f, "body", "margin-top")).toBe("8px");
  expect(await style(f, "body", "font-family")).not.toContain("Inter");
});

test.describe("with motion reduced", () => {
  test.use({ reducedMotion: "reduce" });

  test("a style set from script applies at once, so what measures it reads the new value", async ({ page }) => {
    // with every transition merely shortened, any property change animated,
    // and a diagram library measuring its labels read the old size
    const plugin = await mountPlugin(page, fields(), { review: reviewFrom({ title: "Styles", payload: {} }) });
    const size = await plugin.frame.locator("#start").evaluate((el) => {
      el.style.fontSize = "40px";
      return getComputedStyle(el).fontSize;
    });
    expect(size).toBe("40px");
  });
});

test("highlighted code takes the code tokens' colours, in either theme", async ({ page }) => {
  const dir = viewOf(`<script src="/sdk/v1/highlight.js"></script>
<pre id="code"></pre>
<script>document.getElementById("code").innerHTML = Pinrail.highlight("def x, do: 1 # why", "elixir");</script>`);
  const colours: Record<string, string[]> = {};
  for (const theme of ["dark", "light"] as const) {
    const plugin = await mountPlugin(page, dir, { review: review(), theme });
    const f = plugin.frame;
    const token = async (cls: string) => style(f, `#code .${cls}`, "color");
    const expected = async (name: string) =>
      f.locator("body").evaluate((body, n) => {
        const probe = document.createElement("span");
        probe.style.color = `var(--pinrail-code-${n})`;
        body.append(probe);
        const value = getComputedStyle(probe).color;
        probe.remove();
        return value;
      }, name);
    expect(await token("hljs-keyword")).toBe(await expected("keyword"));
    expect(await token("hljs-number")).toBe(await expected("number"));
    expect(await token("hljs-comment")).toBe(await expected("comment"));
    expect(await style(f, "#code .hljs-comment", "font-style")).toBe("italic");
    colours[theme] = [await token("hljs-keyword"), await token("hljs-number")];
  }
  // the light theme has colours of its own
  expect(colours.light).not.toEqual(colours.dark);
});

test("a custom theme's colours apply before the view's script and follow appearance", async ({ page }) => {
  const plugin = await mountPlugin(page, fields(), {
    review: reviewFrom({ title: "Styles", payload: {} }),
    tokens: { "--pinrail-bg": "#112233", "--a-view-variable": "#000000" },
  });
  const token = (name: string) =>
    plugin.frame.locator("html").evaluate((root, name) => getComputedStyle(root).getPropertyValue(name).trim(), name);
  expect(await token("--pinrail-bg")).toBe("#112233");
  expect(await token("--a-view-variable"), "only Pinrail's tokens are set").toBe("");

  await plugin.send({ type: "appearance", theme: "dark", tokens: { "--pinrail-text": "#abcdef" } });
  await expect.poll(() => token("--pinrail-text")).toBe("#abcdef");
  expect(await token("--pinrail-bg"), "the colours set before are cleared").toBe("#18191b");
});
