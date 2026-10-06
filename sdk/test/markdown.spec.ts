import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { reviewFrom, mountPlugin } from "pinrail-sdk/testing";
import { scratch } from "./scratch.cjs";

// Markdown, in the place it runs: a view's frame, which has an opaque origin,
// no network of its own, and inline scripts allowed. The last of those is why
// the escaping test below matters — HTML that reached the DOM would run.

/** A plugin whose whole view renders `Pinrail.markdown` of its payload, with
 *  the renderer's script or without it. */
function renderer({ withMarkdown = true } = {}): string {
  const dir = scratch("pinrail-markdown-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({ name: "markdown", version: "1.0.0", title: "Markdown" }),
  );
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<script src="/sdk/v1/pinrail-plugin.js"></script>
${withMarkdown ? '<script src="/sdk/v1/markdown.js"></script>' : ""}
<div id="out"></div>
<div id="inline"></div>
<script>
  Pinrail.connect({
    onInit({ review }) {
      try {
        document.getElementById("out").innerHTML = Pinrail.markdown(review.payload.source);
        document.getElementById("inline").innerHTML = Pinrail.markdownInline(review.payload.source);
      } catch (e) {
        document.getElementById("out").textContent = "threw: " + e.message;
      }
    },
  });
</script>`,
  );
  return dir;
}

async function render(page: any, source: string, opts = {}) {
  const plugin = await mountPlugin(page, renderer(opts), {
    review: reviewFrom({ title: "Markdown", payload: { source } }),
  });
  return plugin.frame;
}

test("renders the whole of markdown, not a subset of it", async ({ page }) => {
  const frame = await render(
    page,
    ["# Title", "", "| a | b |", "| --- | --- |", "| 1 | 2 |", "", "> quoted", "", "- one", "  - nested"].join("\n"),
  );

  // the point of loading a parser: headings, tables, blockquotes, nested lists
  await expect(frame.locator("#out h1")).toHaveText("Title");
  await expect(frame.locator("#out table td").first()).toHaveText("1");
  await expect(frame.locator("#out blockquote")).toContainText("quoted");
  await expect(frame.locator("#out ul li ul li")).toHaveText("nested");
});

test("escapes raw HTML rather than passing it through", async ({ page }) => {
  const frame = await render(page, "before <b>bold</b> <img src=x onerror=1> after");

  // the parser is configured with html:false, so tags in the source are text:
  // a view's frame runs inline scripts, and markup here would be the way in
  await expect(frame.locator("#out b")).toHaveCount(0);
  await expect(frame.locator("#out img")).toHaveCount(0);
  await expect(frame.locator("#out")).toContainText("<b>bold</b>");
});

test("keeps only the addresses a view may follow", async ({ page }) => {
  const frame = await render(
    page,
    "[a](javascript:alert(1)) [b](https://example.com) [c](mailto:x@example.com) [d](ftp://example.com/f)",
  );

  // a javascript: address is the one that would run on a click; the parser
  // refuses to make a link of it at all, and the text stays as written
  await expect(frame.locator('#out a[href^="javascript"]')).toHaveCount(0);
  await expect(frame.locator("#out")).toContainText("[a](javascript:alert(1))");
  // the two a view may legitimately show keep theirs
  await expect(frame.locator("#out a").nth(0)).toHaveAttribute("href", "https://example.com");
  await expect(frame.locator("#out a").nth(1)).toHaveAttribute("href", "mailto:x@example.com");
  // anything else the parser would allow loses its address but keeps its text
  await expect(frame.locator("#out a").nth(2)).toHaveAttribute("href", "#");
  await expect(frame.locator("#out a").nth(2)).toHaveText("d");
  // and a link that leaves carries no referrer and does not navigate the view
  await expect(frame.locator("#out a").nth(0)).toHaveAttribute("rel", "noreferrer");
  await expect(frame.locator("#out a").nth(0)).toHaveAttribute("target", "_blank");
});

test("renders one line without a paragraph around it", async ({ page }) => {
  const frame = await render(page, "a *little* line");

  // markdownInline is what a heading or a summary wants
  await expect(frame.locator("#inline em")).toHaveText("little");
  await expect(frame.locator("#inline p")).toHaveCount(0);
});

/** The paths of the scripts the view's frame asks for. */
function viewScripts(page: any): string[] {
  const scripts: string[] = [];
  // the view's own scripts, not the harness page's
  page.on(
    "request",
    (r: any) =>
      r.resourceType() === "script" && r.frame() !== page.mainFrame() && scripts.push(new URL(r.url()).pathname),
  );
  return scripts;
}

test("is ready in onInit once the view loads its script", async ({ page }) => {
  const scripts = viewScripts(page);
  const frame = await render(page, "# Title");

  // a view renders in onInit with no await of its own
  await expect(frame.locator("#out h1")).toHaveText("Title");
  expect(scripts).toEqual(["/sdk/v1/pinrail-plugin.js", "/sdk/v1/markdown.js"]);
});

test("a view without the script loads no parser, and is told which script it needs", async ({ page }) => {
  const scripts = viewScripts(page);
  const frame = await render(page, "# Title", { withMarkdown: false });

  await expect(frame.locator("#out")).toContainText("threw:");
  await expect(frame.locator("#out")).toContainText('<script src="/sdk/v1/markdown.js">');
  expect(scripts).toEqual(["/sdk/v1/pinrail-plugin.js"]);
});

test("a link in rendered markdown asks the shell to open it", async ({ page }) => {
  const plugin = await mountPlugin(page, renderer(), {
    review: reviewFrom({
      title: "Markdown",
      payload: { source: "[docs](https://example.com/docs) [d](ftp://example.com/f)" },
    }),
  });

  // the frame is sandboxed without allow-popups, so target="_blank" opens
  // nothing: the click becomes a message and the shell opens the link
  await plugin.frame.locator("#out").getByRole("link", { name: "docs" }).click();
  await expect.poll(() => plugin.lastOpen()).toBe("https://example.com/docs");

  // the one whose address was dropped stays where it is
  await plugin.frame.locator("#out").getByRole("link", { name: "d", exact: true }).click();
  await expect.poll(() => plugin.lastOpen()).toBe("https://example.com/docs");
});
