import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, reviewFrom, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

const dir = path.resolve(__dirname, "..");
const landing = () => fixture(path.join(dir, "fixtures", "landing.json"));

/* Selection happens on the artifact inside its shadow root; Playwright's
   locators pierce it, so the headline is a normal target. */
async function commentOn(plugin: Awaited<ReturnType<typeof mountPlugin>>, target: string, text: string) {
  const f = plugin.frame;
  if ((await f.locator("[data-select]").getAttribute("class"))?.includes("is-on") === false) await f.locator("[data-select]").click();
  await f.locator(target).click();
  await expect(f.locator("[data-popover]")).toBeVisible();
  await f.locator("[data-comment-text]").fill(text);
  await f.locator("[data-save]").click();
  await expect(f.locator("[data-popover]")).toHaveCount(0);
}

test("renders the artifact with its own styles, inert", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: landing() });
  const f = plugin.frame;
  await expect(f.locator("[data-artifact] h1")).toHaveText("Bookkeeping that closes itself");
  // the artifact's stylesheet reached its elements
  const size = await f.locator("[data-artifact] h1").evaluate((el) => getComputedStyle(el).fontSize);
  expect(size).toBe("48px");
  // a link in a mockup goes nowhere
  await f.locator("[data-artifact] a.cta").click();
  await expect(f.locator("[data-artifact] h1")).toBeVisible();
  // an empty review approves
  await expect.poll(() => plugin.lastStatus()).toBe("Approve");
});

test("the document can come as a file beside the payload, and one that cannot be read says so", async ({ page }) => {
  const review = landing();
  const { html, ...rest } = review.payload as { html: string };
  review.payload = { ...rest, file: { $attachment: "landing.html" } };
  const plugin = await mountPlugin(page, dir, { review, attachments: { "landing.html": path.join(dir, "fixtures", "landing.html") } });
  await expect(plugin.frame.locator("[data-artifact] h1")).toHaveText("Bookkeeping that closes itself");
  await expect(plugin.frame.locator("[data-load-error]")).toHaveCount(0);

  const missing = landing();
  const { html: _, ...others } = missing.payload as { html: string };
  missing.payload = { ...others, file: { $attachment: "gone.html" } };
  const broken = await mountPlugin(page, dir, { review: missing });
  await expect(broken.frame.locator("[data-load-error]")).toContainText("gone.html could not be read");
});

test("a comment hangs on the element by a selector and travels in the decision", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: landing() });
  const f = plugin.frame;
  await commentOn(plugin, "[data-artifact] h1", "Say what it does, not a slogan");
  await expect(f.locator("[data-pin]")).toHaveCount(1);
  await expect(f.locator("[data-comment]")).toHaveCount(1);
  await expect(f.locator("[data-comment] .mono")).toHaveText("#hero > h1");
  await expect.poll(() => plugin.lastStatus()).toBe("Request changes (1)");

  await commentOn(plugin, "[data-artifact] .card:nth-of-type(2) h3", "Keep this one");
  await expect(f.locator("[data-pin]")).toHaveCount(2);

  await plugin.collect();
  const data = await plugin.nextSubmit();
  expect(data.verdict).toBe("revise");
  expect(data.comments).toHaveLength(2);
  expect(data.comments[0]).toMatchObject({ selector: "#hero > h1", tag: "h1", kind: "change", text: "Say what it does, not a slogan", snippet: "Bookkeeping that closes itself" });
  expect(data.comments[1].selector).toBe("#features > div:nth-of-type(2) > h3");
});

test("comments can be edited and removed, and the verdict overridden", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: landing() });
  const f = plugin.frame;
  await commentOn(plugin, "[data-artifact] #pricing h2", "Two plans at least");
  await f.locator("[data-comment]").hover();
  await f.locator("[data-comment] [data-edit]").click();
  await expect(f.locator("[data-popover]")).toBeVisible();
  await f.locator("[data-comment-text]").fill("Two plans, and a free tier");
  await f.locator("[data-save]").click();
  await expect(f.locator("[data-comment] .comment-text")).toHaveText("Two plans, and a free tier");

  await f.locator('[data-verdict="approve"]').click();
  await expect.poll(() => plugin.lastStatus()).toBe("Approve");

  await f.locator("[data-comment]").hover();
  await f.locator("[data-comment] [data-remove]").click();
  await expect(f.locator("[data-comment]")).toHaveCount(0);
  await expect(f.locator("[data-pin]")).toHaveCount(0);
});

test("a draft comes back with the next init", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: landing() });
  const f = plugin.frame;
  await commentOn(plugin, "[data-artifact] h1", "Shorter");
  await expect.poll(() => plugin.lastDraft()).toMatchObject({ comments: [{ selector: "#hero > h1", text: "Shorter" }] });
  await plugin.reinit();
  await expect(f.locator("[data-comment]")).toHaveCount(1);
  await expect(f.locator("[data-pin]")).toHaveCount(1);
});

test("a decided review is read-only with its pins", async ({ page }) => {
  const review = landing();
  review.status = "decided";
  review.decision = { data: { verdict: "revise", comments: [{ id: "c1", selector: "#hero > h1", tag: "h1", kind: "change", text: "Shorter" }] } };
  const plugin = await mountPlugin(page, dir, { review, readonly: true });
  const f = plugin.frame;
  await expect(f.locator("[data-pin]")).toHaveCount(1);
  await expect(f.locator("[data-select]")).toHaveCount(0);
  await expect(f.locator(".decided")).toHaveText(/Changes requested/);
  await expect(f.locator("[data-comment] [data-edit]")).toHaveCount(0);
});

test("the viewport presets resize the artifact", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: landing() });
  const f = plugin.frame;
  await f.getByRole("button", { name: /Phone/ }).click();
  await expect.poll(() => f.locator(".frame").evaluate((el) => el.getBoundingClientRect().width)).toBe(390);
});

test("custom properties on :root, html and body reach the artifact's elements", async ({ page }) => {
  const html = `<!doctype html><html><head><style>
    :root { --ink: rgb(10, 20, 30); --paper: rgb(250, 240, 230); }
    html { --edge: rgb(1, 2, 3); }
    body { background: var(--paper); }
    .btn { color: var(--ink); border: 1px solid var(--edge); }
  </style></head><body><a class="btn" id="go">Go</a></body></html>`;
  const plugin = await mountPlugin(page, dir, { review: reviewFrom({ title: "tokens", payload: { html } }) });
  const btn = plugin.frame.locator("[data-artifact] #go");
  await expect(btn).toHaveCSS("color", "rgb(10, 20, 30)");
  await expect(btn).toHaveCSS("border-top-color", "rgb(1, 2, 3)");
  await expect(plugin.frame.locator("[data-artifact] .artifact-body")).toHaveCSS("background-color", "rgb(250, 240, 230)");
});

test("markup in the artifact runs nothing, so it cannot decide the review", async ({ page }) => {
  // The artifact is the agent's HTML, often pasted from somewhere else. A
  // handler in it would run inside the view the person trusts, where it
  // could hand over a decision the person never made.
  const submit = "parent.postMessage({pinrail:1,type:'submit',data:{comments:[]}},'*');window.ran=(window.ran||0)+1";
  const review = landing();
  review.payload = {
    ...(review.payload as object),
    html: `<!doctype html><html><body>
      <h1>Bookkeeping that closes itself</h1>
      <img src="x" onerror="${submit}">
      <svg><image href="x" onerror="${submit}"></image></svg>
      <details open ontoggle="${submit}"><summary>more</summary></details>
      <a href="javascript:${submit}" class="js">a link</a>
      <a href="java&#9;script:${submit}" class="hidden-js">a link with a tab in its scheme</a>
      <form action="javascript:${submit}"><button>send</button></form>
      <iframe srcdoc="<script>${submit.replace("parent.", "parent.parent.")}</script>"></iframe>
      <object data="data:text/html,<script>${submit}</script>"></object>
    </body></html>`,
  };
  const plugin = await mountPlugin(page, dir, { review });
  const f = plugin.frame;
  await expect(f.locator("[data-artifact] h1")).toHaveText("Bookkeeping that closes itself");
  await f.locator("[data-artifact] a.js").click();
  await f.locator("[data-artifact] a.hidden-js").click();
  await f.locator("[data-artifact] form button").click();
  await page.waitForTimeout(500);

  expect(await f.locator("body").evaluate(() => (window as unknown as { ran?: number }).ran ?? 0), "a handler in the artifact ran").toBe(0);
  const submits = (await plugin.messages()).filter((m) => m.type === "submit");
  expect(submits, "the artifact handed over a decision").toEqual([]);
});

test("a refused hand-over says why and keeps every comment", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: landing() });
  const f = plugin.frame;
  await commentOn(plugin, "[data-artifact] h1", "Say what it does, not a slogan");
  await plugin.collect();
  await plugin.nextSubmit();

  await plugin.sendViolations([{ path: "/comments/0/selector", message: "must name an element" }]);
  await expect(f.locator("body")).toContainText("/comments/0/selector: must name an element");
  await expect(f.locator("[data-comment]")).toHaveCount(1);
  await expect(f.locator("[data-pin]")).toHaveCount(1);
});
