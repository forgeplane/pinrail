import { expect, test } from "@playwright/test";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fixture, mountPlugin, reviewFrom } from "pinrail-sdk/testing";

// The view alone, under the harness: the retry policy document, with a
// mermaid diagram, a table and a code block.
const dir = path.resolve(__dirname, "..");
const round = () => fixture(path.join(dir, "fixtures", "retries.json"));
const decidedRound = () => fixture(path.join(dir, "fixtures", "retries.decided.json"));
const source: string = round().payload.markdown;
const lineOf = (start: string) => source.split("\n").findIndex((l) => l.startsWith(start)) + 1;

// the schemas, checked the way the app checks them
const sdk = createRequire(fs.realpathSync(path.join(dir, "..", "node_modules", "pinrail-sdk", "package.json")));
const Ajv2020 = sdk("ajv/dist/2020").default;
const ajv = new Ajv2020({ allErrors: true, strict: false });
const read = (f: string) => JSON.parse(fs.readFileSync(path.join(dir, f), "utf8"));
const validPayload = ajv.compile(read("schemas/payload.schema.json"));
const validDecision = ajv.compile(read("schemas/decision.schema.json"));

async function mount(page, opts: Record<string, any> = {}) {
  const plugin = await mountPlugin(page, dir, { review: round(), ...opts });
  await expect(plugin.frame.locator(".diagram svg")).toBeVisible({ timeout: 15_000 });
  return plugin;
}

/** Selects the text of an element in the frame, as a drag would. */
async function select(plugin, selector: string) {
  await plugin.frame
    .locator(selector)
    .first()
    .evaluate((el) => {
      const range = document.createRange();
      range.selectNodeContents(el);
      const selection = window.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(range);
      document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });
}

test("the payloads it ships with pass its schema, and so does the decided round", () => {
  expect(validPayload(read("samples/markdown.json").payload)).toBe(true);
  expect(validPayload(read("fixtures/retries.json").payload)).toBe(true);
  expect(validDecision(read("fixtures/retries.decided.json").decision.data)).toBe(true);
});

test("Mermaid is fetched only for a document with a diagram", async ({ page }) => {
  const fetched: string[] = [];
  page.on("request", (r) => {
    if (/mermaid/i.test(r.url())) fetched.push(r.url());
  });
  const plain = round();
  plain.payload.markdown = "# Retries\n\nNo diagram here, only **text**.\n";
  const plugin = await mountPlugin(page, dir, { review: plain });
  await expect(plugin.frame.getByText("No diagram here").first()).toBeVisible();
  await page.waitForTimeout(500);
  expect(fetched).toEqual([]);

  const withDiagram = await mountPlugin(page, dir, { review: round() });
  await expect(withDiagram.frame.locator(".diagram svg")).toBeVisible({ timeout: 15_000 });
  expect(fetched.length).toBeGreaterThan(0);
});

test("questions alone ask the agent to explain, and change nothing", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await f.locator(".diagram").hover();
  await f.locator("[data-comment-diagram]").click();
  await f.getByRole("radio", { name: "Question" }).click();
  await f.locator("#compose-body").fill("Where does Retry-After come in?");
  await f.locator("#compose-body").press("Enter");
  await expect.poll(() => plugin.lastStatus()).toBe("Ask · 1 question");
  await plugin.collect();
  const decision = await plugin.nextSubmit();
  expect(decision.verdict).toBe("questions");
  expect(decision.comments[0].kind).toBe("question");
  expect(validDecision(decision)).toBe(true);

  // editing it keeps it a question, and it can become a change
  await f.locator("[data-edit]").click();
  await expect(f.getByRole("radio", { name: "Question" })).toBeChecked();
  await f.getByRole("radio", { name: "Change" }).click();
  await f.locator("#compose-save").click();
  await expect.poll(() => plugin.lastStatus()).toBe("Request changes · 1 change");
});

test("a document with no comments is approved at the hand-over", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  await expect.poll(() => plugin.lastStatus()).toBe("Approve");
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ verdict: "approve", comments: [] });
});

test("the document can come as an attached file instead of in the payload", async ({ page }) => {
  const review = fixture(path.join(dir, "fixtures", "retries-file.json"));
  expect(validPayload(review.payload)).toBe(true);
  const plugin = await mountPlugin(page, dir, { review });
  const f = plugin.frame;
  await expect(f.locator("#outline-items a").first()).toContainText("Retry policy for webhook deliveries");
  await expect(f.locator(".diagram svg")).toBeVisible({ timeout: 15_000 });

  // one or the other, never both, never neither
  expect(validPayload({ ...review.payload, markdown: "# Twice" })).toBe(false);
  const { file: _file, ...neither } = review.payload;
  expect(validPayload(neither)).toBe(false);
});

test("the outline folds away from the header, and the choice is kept as a setting", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  const outline = f.locator("#outline");
  await expect(outline).toBeVisible();
  await f.getByRole("button", { name: "Hide the outline" }).click();
  await expect(outline).toBeHidden();
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ outline_open: false });
  await f.getByRole("button", { name: "Show the outline" }).click();
  await expect(outline).toBeVisible();

  // a setting from the app starts it folded
  const folded = await mountPlugin(page, dir, { review: round(), settings: { outline_open: false } });
  await expect(folded.frame.locator("#outline")).toBeHidden();
});

test("the comments fold away from the header, and the choice is kept as a setting", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  const side = f.getByRole("complementary", { name: "Comments" });
  const doc = f.locator("#docs");
  await expect(side).toBeVisible();
  const narrow = (await doc.boundingBox())!.width;
  await f.getByRole("button", { name: "Hide the comments" }).click();
  await expect(side).toBeHidden();
  // the document takes the width the comments had
  await expect.poll(async () => (await doc.boundingBox())!.width).toBeGreaterThan(narrow + 200);
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ comments_open: false });
  await f.getByRole("button", { name: "Show the comments" }).click();
  await expect(side).toBeVisible();

  // a setting from the app starts them folded
  const folded = await mountPlugin(page, dir, { review: round(), settings: { comments_open: false } });
  await expect(folded.frame.getByRole("complementary", { name: "Comments" })).toBeHidden();
});

test("a decision the app refuses says why in the confirmation bar", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await plugin.sendViolations([{ path: "/comments/0/body", message: "is too long" }]);
  const bar = f.locator(".pinrail-confirmation");
  await expect(bar).toHaveClass(/pinrail-confirmation-danger/);
  await expect(bar).toContainText("/comments/0/body: is too long");
});

test("renders the document with its diagram, its outline and its raw source", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  await expect(f.locator("#path")).toHaveText("docs/retries.md");
  await expect(f.locator("#context")).toContainText("Mostly the Design section needs a look.");
  await expect(f.locator(".rendered h2")).toHaveText(["GoalsComment", "DesignComment", "Open questionsComment"]);
  await expect(f.locator(".rendered table td").first()).toHaveText("1");
  await expect(f.locator("#outline-items a")).toHaveText([
    "Retry policy for webhook deliveries",
    "Goals",
    "Design",
    "Backoff",
    "Open questions",
  ]);

  // one pane at a time: the rendered document, or its raw source
  await expect(f.locator(".modes [data-mode]:visible")).toHaveText(["Rendered", "Raw"]);
  await f.locator('[data-mode="raw"]').click();
  await expect(f.locator("#rendered-pane")).toBeHidden();
  await expect(f.locator('.raw-line[data-line="1"] .src')).toHaveText("# Retry policy for webhook deliveries");
  await expect.poll(async () => (await plugin.lastDraft())?.mode).toBe("raw");
  await f.locator('[data-mode="rendered"]').click();
  await expect(f.locator("#rendered-pane")).toBeVisible();
  await expect(f.locator("#raw-pane")).toBeHidden();
});

test("a draft that showed both panes opens on the rendered document", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round(), draft: { mode: "split" } });
  const f = plugin.frame;
  await expect(f.locator('[data-mode="rendered"]')).toHaveAttribute("aria-pressed", "true");
  await expect(f.locator("#rendered-pane")).toBeVisible();
  await expect(f.locator("#raw-pane")).toBeHidden();
});

test("comments on a section, a diagram and selected text, and hands them over with lines", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;

  // a section, from its heading
  await f.locator(".rendered h3").hover();
  await f.locator(".rendered h3 [data-comment-section]").click();
  // the dialog opens in the document, under the heading, and the list stays a list
  await expect(f.locator("#rendered-pane #dialog")).toBeVisible();
  await expect(f.locator("#list textarea")).toHaveCount(0);
  const heading = await f.locator(".rendered h3").boundingBox();
  const dialog = await f.locator("#dialog").boundingBox();
  expect(dialog!.y).toBeGreaterThan(heading!.y + heading!.height - 1);
  expect(dialog!.y).toBeLessThan(heading!.y + heading!.height + 40);
  await f.locator("#compose-body").fill("Show the delays up to 8.");
  await f.locator("#compose-body").press("Enter");

  // the diagram
  await f.locator(".diagram").hover();
  await f.locator("[data-comment-diagram]").click();
  // a question, not a change: it asks the agent to explain
  await expect(f.getByRole("radio", { name: "Change" })).toBeChecked();
  await f.getByRole("radio", { name: "Question" }).click();
  await f.locator("#compose-body").fill("Where does Retry-After come in?");
  await f.locator("#compose-save").click();

  // a passage, selected in the rendered document
  await select(plugin, ".rendered li");
  await f.locator("#pick").click();
  await f.locator("#compose-body").fill("Slow, or failing?");
  await f.locator("#compose-body").press("Enter");

  // and a line, selected in the raw source
  await f.locator('[data-mode="raw"]').click();
  const jitter = lineOf("Jitter");
  await select(plugin, `.raw-line[data-line="${jitter}"] .src`);
  await f.locator("#pick").click();
  await expect(f.locator("#raw-pane #dialog")).toBeVisible();
  await f.getByRole("radio", { name: "Question" }).click();
  await f.locator("#compose-body").fill("Why 20 %?");
  await f.locator("#compose-body").press("Enter");
  await expect(f.locator("#dialog")).toBeHidden();

  await expect(f.locator(".card")).toHaveCount(4);
  await expect(f.locator("#tally")).toHaveText("4 comments");
  await expect(f.locator(".raw-line.commented").first()).toBeVisible();
  // the outline counts the comments under each heading, Design's with its Backoff
  await expect(f.locator('#outline-items a[data-heading="2"] .count')).toHaveText("3");
  await expect(f.locator('#outline-items a[data-heading="3"] .count')).toHaveText("2");

  // with a change among them, the hand-over requests changes; nothing to choose
  await expect(f.locator("[data-verdict]")).toHaveCount(0);
  await expect(f.locator(".card.is-question")).toHaveCount(2);
  await expect.poll(() => plugin.lastStatus()).toBe("Request changes · 2 changes, 2 questions");
  await plugin.collect();
  const decision = await plugin.nextSubmit();

  expect(decision.verdict).toBe("request_changes");
  expect(decision).not.toHaveProperty("note");
  expect(decision.comments.map((c) => [c.body, c.kind])).toEqual(
    expect.arrayContaining([
      ["Show the delays up to 8.", "change"],
      ["Where does Retry-After come in?", "question"],
      ["Why 20 %?", "question"],
    ]),
  );
  const byKind = Object.fromEntries(decision.comments.map((c) => [c.target.kind + c.body.slice(0, 4), c.target]));
  const backoff = lineOf("### Backoff");
  expect(byKind["sectionShow"]).toMatchObject({
    start_line: backoff,
    end_line: lineOf("## Open questions") - 1,
    section: "Retry policy for webhook deliveries › Design › Backoff",
  });
  expect(byKind["diagramWher"]).toMatchObject({
    start_line: lineOf("```mermaid"),
    quote: "flowchart LR",
    section: "Retry policy for webhook deliveries › Design",
  });
  expect(byKind["textSlow"]).toMatchObject({
    start_line: lineOf("- A slow customer"),
    end_line: lineOf("- A slow customer"),
    quote: "A slow customer endpoint must not stall other deliveries.",
  });
  expect(byKind["textWhy "]).toMatchObject({ start_line: jitter, end_line: jitter });
  // in the order of the document
  const starts = decision.comments.map((c) => c.target.start_line);
  expect(starts).toEqual([...starts].sort((a, b) => a - b));
});

test("a long heading path and a long passage stay inside the comment's dialog", async ({ page }) => {
  const long = "A section whose heading goes on for a long while, longer than the dialog is wide";
  const markdown = `# ${long}\n\n## ${long}, again\n\n### ${long}, once more\n\n${"A paragraph that runs on. ".repeat(40)}\n`;
  const plugin = await mountPlugin(page, dir, { review: { ...round(), payload: { ...round().payload, markdown } } });
  const f = plugin.frame;
  await select(plugin, ".rendered p");
  await f.locator("#pick").click();
  await expect(f.locator("#dialog")).toBeVisible();

  const box = (await f.locator("#dialog").boundingBox())!;
  for (const inside of ["#compose-body", "#compose-save", ".dialog .where"]) {
    const b = (await f.locator(inside).boundingBox())!;
    expect(b.x + b.width, inside).toBeLessThanOrEqual(box.x + box.width);
  }
  // the heading path is cut short, not the dialog widened
  const where = f.locator(".dialog .where .section");
  expect(await where.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(true);
});

test("the button to comment on a selection sits at the end of the selection", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  await select(plugin, ".rendered table td");
  const end = await f
    .locator(".rendered table td")
    .first()
    .evaluate((el) => {
      const range = document.createRange();
      range.selectNodeContents(el);
      const rects = range.getClientRects();
      const last = rects[rects.length - 1];
      return { x: last.right, y: last.bottom };
    });
  const pick = await f.locator("#pick").evaluate((el) => el.getBoundingClientRect().toJSON());
  // centred on where the selection ends, whatever width the font gives it
  expect(Math.abs(pick.left + pick.width / 2 - end.x), JSON.stringify({ pick, end })).toBeLessThan(4);
  expect(pick.top - end.y, JSON.stringify({ pick, end })).toBeGreaterThanOrEqual(0);
  expect(pick.top - end.y).toBeLessThan(20);
});

test("a comment has edit and delete at the right of its header, and a click on its text edits it", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  await f.locator(".rendered h2").first().hover();
  await f.locator(".rendered h2 [data-comment-section]").first().click();
  await f.locator("#compose-body").fill("First");
  await f.locator("#compose-body").press("Enter");
  const head = f.locator(".card .where");
  await expect(head.getByRole("button", { name: "Edit the comment" })).toBeVisible();
  await expect(head.getByRole("button", { name: "Delete the comment" })).toBeVisible();

  await f.locator(".card .body").click();
  await expect(f.locator("#compose-body")).toHaveValue("First");
  await f.locator("#compose-body").press("Escape");
  await head.getByRole("button", { name: "Delete the comment" }).click();
  await expect(f.locator(".card")).toHaveCount(0);
});

test("a comment can be edited and deleted, and the draft keeps them", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  await f.locator(".rendered h2").first().hover();
  await f.locator(".rendered h2 [data-comment-section]").first().click();
  await f.locator("#compose-body").fill("First");
  await f.locator("#compose-body").press("Enter");
  await f.locator("[data-edit]").click();
  await expect(f.locator("#rendered-pane #dialog")).toBeVisible();
  await expect(f.locator("#compose-body")).toHaveValue("First");
  await f.locator("#compose-body").fill("Edited");
  await f.locator("#compose-body").press("Enter");
  await expect(f.locator(".card .body")).toHaveText("Edited");
  await expect.poll(async () => (await plugin.lastDraft())?.comments?.[0]?.body).toBe("Edited");

  // the draft comes back after a reload
  await plugin.reinit();
  await expect(f.locator(".card .body")).toHaveText("Edited");
  await f.locator("[data-delete]").click();
  await expect(f.locator(".card")).toHaveCount(0);
});

test("a comment's dialog closes with Escape, and a comment in the list leads to its place", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  await f.locator(".diagram").hover();
  await f.locator("[data-comment-diagram]").click();
  await f.locator("#compose-body").fill("Not this one");
  await f.locator("#compose-body").press("Escape");
  await expect(f.locator("#dialog")).toBeHidden();
  await expect(f.locator(".card")).toHaveCount(0);

  await f.locator(".rendered h2").last().hover();
  await f.locator(".rendered h2 [data-comment-section]").last().click();
  await f.locator("#compose-body").fill("Answer these first.");
  await f.locator("#compose-body").press("Enter");

  // back at the top, a click on the comment brings its heading into view
  await f.locator("#rendered-pane").evaluate((pane) => (pane.scrollTop = 0));
  await expect(f.locator(".rendered h2").last()).not.toBeInViewport();
  await f.locator(".card .body").click();
  await expect(f.locator(".rendered h2").last()).toBeInViewport();
  await expect(f.locator(".rendered h2").last()).toHaveClass(/flash/);
});

test("a decided review shows its verdict and comments, read-only", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: decidedRound(), readonly: true });
  const f = plugin.frame;
  await expect(f.locator(".decided")).toContainText("Changes requested");
  await expect(f.locator(".card")).toHaveCount(2);
  await expect(f.locator("[data-edit]")).toHaveCount(0);
  await expect(f.locator("#tally")).toHaveText("2 comments");
  await expect(f.locator(".rendered .commented").first()).toBeVisible();
});

test("a diagram that does not parse shows its error and its source", async ({ page }) => {
  const review = round();
  review.payload.markdown = "# Broken\n\n```mermaid\nflowchart LR\n  A -->\n```\n";
  const plugin = await mountPlugin(page, dir, { review });
  await expect(plugin.frame.locator(".diagram-error")).toContainText("does not render");
});

test("in a narrow frame the document takes the width, and the outline and the comments open as drawers", async ({
  page,
}) => {
  await page.setViewportSize({ width: 420, height: 800 });
  const plugin = await mount(page);
  const f = plugin.frame;
  const outline = f.getByRole("navigation", { name: "Outline" });
  const side = f.getByRole("complementary", { name: "Comments" });
  await expect(outline).toBeHidden();
  await expect(side).toBeHidden();
  const article = await f.locator("#rendered").boundingBox();
  expect(article!.width).toBeGreaterThan(400);

  // add a comment in the document, then open the list with the header's count
  await select(plugin, ".rendered li");
  await f.locator("#pick").click();
  await f.locator("#compose-body").fill("Slow, or failing?");
  await f.locator("#compose-body").press("Enter");
  const toggle = f.getByRole("button", { name: /1 comment/ });
  await toggle.click();
  await expect(side).toBeVisible();
  await expect(side.locator(".card")).toContainText("Slow, or failing?");
  await page.keyboard.press("Escape");
  await expect(side).toBeHidden();
  await expect(toggle).toBeFocused();

  // a click on a comment closes the drawer and scrolls to the text it refers to
  await toggle.click();
  await side.locator(".card .body").click();
  await expect(side).toBeHidden();

  // the outline opens over the document, and a click on a heading scrolls to it
  await f.getByRole("button", { name: "Show the outline" }).click();
  await expect(outline).toBeVisible();
  await outline.getByText("Open questions").click();
  await expect(outline).toBeHidden();
  await expect(f.locator(".rendered h2", { hasText: "Open questions" })).toBeInViewport();
});

test("between 760 and 1100 pixels the comments stay beside the document, and the outline is a drawer", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1000, height: 800 });
  const plugin = await mount(page);
  const f = plugin.frame;
  await expect(f.getByRole("complementary", { name: "Comments" })).toBeVisible();
  await expect(f.getByRole("navigation", { name: "Outline" })).toBeHidden();
  await f.getByRole("button", { name: "Show the outline" }).click();
  await expect(f.getByRole("navigation", { name: "Outline" })).toBeVisible();
  await f.locator("#scrim").click({ position: { x: 600, y: 300 } });
  await expect(f.getByRole("navigation", { name: "Outline" })).toBeHidden();
});

test("a comment can be dictated, from a microphone in the field's corner", async ({ page }) => {
  const plugin = await mount(page, { dictation: true });
  const f = plugin.frame;
  await select(plugin, ".rendered li");
  await f.locator("#pick").click();
  const dialog = f.getByRole("dialog", { name: "Comment" });
  await dialog.locator("[data-pinrail-dictate-for]").getByRole("button", { name: "Dictate" }).click();
  await plugin.say("slow or failing");
  await plugin.say("slow or failing, which one");
  await expect(f.locator("#compose-body")).toHaveValue("slow or failing, which one");
  await dialog.getByRole("button", { name: "Stop dictating" }).click();
  await expect.poll(() => plugin.dictating()).toBe(false);
  // the dialog stays open, and Save adds the comment with the field's text
  await f.locator("#compose-save").click();
  await expect(f.locator(".card .body")).toHaveText("slow or failing, which one");
});

test("without the app's dictation the comment dialog has no microphone", async ({ page }) => {
  const plugin = await mount(page);
  await select(plugin, ".rendered li");
  await plugin.frame.locator("#pick").click();
  await expect(plugin.frame.locator("#compose-body")).toBeVisible();
  await expect(plugin.frame.getByRole("button", { name: "Dictate" })).toHaveCount(0);
});

/** The fixture's document as an earlier round had it: six attempts, not
 *  eight, and no line on logging a give-up. */
const earlier = () =>
  reviewFrom({
    title: "Retry policy for webhook deliveries",
    payload: {
      markdown: source
        .replace("at most eight times", "at most six times")
        .replace("- Every give-up is logged with the last status.\n", ""),
    },
  });

test("a revised document shows its changes since the previous round, with the unchanged lines folded", async ({
  page,
}) => {
  const plugin = await mount(page, { previous: earlier() });
  const f = plugin.frame;
  await f.locator('.modes [data-mode="changes"]').click();
  await expect(f.locator("#changes-pane")).toBeVisible();
  await expect(f.locator("#rendered-pane")).toBeHidden();

  // the changed line, old and new, with the changed word marked
  await expect(f.locator(".diff-line.del")).toContainText("at most six times");
  await expect(f.locator(".diff-line.add").first()).toContainText("at most eight times");
  await expect(f.locator(".diff-line.del mark")).toHaveText("six");
  await expect(f.locator(".diff-line.add mark").first()).toHaveText("eight");
  // the added line, and where the change is
  await expect(f.locator(".diff-line.add", { hasText: "Every give-up is logged" })).toHaveCount(1);
  await expect(f.locator(".diff-section").first()).toContainText("Goals");

  // what did not change is folded, and opens on a click
  const fold = f.locator(".diff-fold").last();
  await expect(fold).toContainText("unchanged lines");
  const before = await f.locator(".diff-line.same").count();
  await fold.click();
  expect(await f.locator(".diff-line.same").count()).toBeGreaterThan(before);

  // the mode is kept with the draft
  await expect.poll(async () => (await plugin.lastDraft())?.mode).toBe("changes");
});

test("without a previous round there is no Changes mode", async ({ page }) => {
  const plugin = await mount(page);
  await expect(plugin.frame.locator('.modes [data-mode="changes"]')).toBeHidden();
});

test("in the Changes mode, a comment's place opens its fold, and editing it goes to Raw", async ({ page }) => {
  const plugin = await mount(page, { previous: earlier() });
  const f = plugin.frame;
  // a comment on the last section, far from the changes
  await f.locator(".rendered h2").last().hover();
  await f.locator(".rendered h2 [data-comment-section]").last().click();
  await f.locator("#compose-body").fill("Answer these first.");
  await f.locator("#compose-body").press("Enter");
  await f.locator('.modes [data-mode="changes"]').click();

  const heading = f.locator("#changes .diff-line", { hasText: "## Open questions" });
  await expect(heading).toHaveCount(0);
  await f.locator(".card .where").click();
  await expect(heading).toBeInViewport();

  await f.locator(".card .body").click();
  await expect(f.locator("#raw-pane")).toBeVisible();
  await expect(f.locator("#raw-pane #dialog")).toBeVisible();
  await expect(f.locator("#compose-body")).toHaveValue("Answer these first.");
});

test("in the Changes mode, words are marked only in a line that is mostly the same", async ({ page }) => {
  // a paragraph rewritten and rewrapped: its lines no longer pair up
  const rewritten = source.replace(
    "The worker takes a delivery from the queue and posts it once. A failure",
    "Each delivery is taken from the queue by a worker, which posts it a single time; on failure the delivery",
  );
  const plugin = await mount(page, {
    review: reviewFrom({ title: "x", payload: { markdown: rewritten } }),
    previous: earlier(),
  });
  const f = plugin.frame;
  await f.locator('.modes [data-mode="changes"]').click();
  // the rewritten line is shown whole, with nothing marked in it
  const added = f.locator(".diff-line.add", { hasText: "Each delivery is taken" });
  await expect(added).toHaveCount(1);
  await expect(added.locator("mark")).toHaveCount(0);
  // a small change in a line still has its word marked
  await expect(f.locator(".diff-line.add mark").first()).toHaveText("eight");
});

test("in the Changes mode, the previous round's comments show at their place, changed or not", async ({ page }) => {
  const before: string = earlier().payload.markdown;
  const at = (start: string) => before.split("\n").findIndex((l) => l.startsWith(start)) + 1;
  const six = at("- A delivery is tried at most six times");
  const open = at("## Open questions");
  const previous = {
    ...earlier(),
    status: "decided",
    decision: {
      data: {
        verdict: "request_changes",
        comments: [
          {
            id: 1,
            kind: "change",
            target: { kind: "text", start_line: six, end_line: six, quote: "six times" },
            body: "Make it eight.",
          },
          {
            id: 2,
            kind: "change",
            target: { kind: "section", start_line: open, end_line: before.split("\n").length },
            body: "Answer these first.",
          },
        ],
      },
    },
  };
  const plugin = await mount(page, { previous });
  const f = plugin.frame;
  await f.locator('.modes [data-mode="changes"]').click();

  // the comment on a changed line sits after the change, marked as changed
  const changed = f.locator('#changes [data-prev-comment="1"]');
  await expect(changed).toContainText("Make it eight.");
  await expect(changed).toContainText("Changed");
  const after = await changed.evaluate((el) => el.previousElementSibling?.textContent ?? "");
  expect(after).toContain("Every give-up is logged");

  // the one on lines that did not change is shown with them, and says so
  const unchanged = f.locator('#changes [data-prev-comment="2"]');
  await expect(unchanged).toContainText("Answer these first.");
  await expect(unchanged).toContainText("Unchanged");

  // the side panel's list says the same, and a click goes to the comment's place
  await f.getByRole("tab", { name: /Previous round/ }).click();
  const side = f.locator(".previous .card");
  await expect(side.nth(0)).toContainText("Changed");
  await expect(side.nth(1)).toContainText("Unchanged");
  await f.locator('.modes [data-mode="rendered"]').click();
  await side.nth(1).click();
  await expect(f.locator("#changes-pane")).toBeVisible();
  await expect(unchanged).toBeInViewport();
});

test("a previous comment on the first of several changed lines sits after the whole change", async ({ page }) => {
  const before: string = earlier().payload.markdown;
  const first = before.split("\n").findIndex((l) => l.startsWith("The worker takes a delivery")) + 1;
  // both lines of the paragraph rewritten
  const now = source
    .replace(
      "The worker takes a delivery from the queue and posts it once. A failure",
      "A worker posts each delivery once.",
    )
    .replace(
      "re-enqueues the delivery with a `run_at` in the future instead of sleeping.",
      "On a failure it is queued again for later.",
    );
  const previous = {
    ...earlier(),
    status: "decided",
    decision: {
      data: {
        verdict: "request_changes",
        comments: [
          {
            id: 1,
            kind: "change",
            target: { kind: "text", start_line: first, end_line: first, quote: "The worker" },
            body: "Shorter.",
          },
        ],
      },
    },
  };
  const plugin = await mount(page, { review: reviewFrom({ title: "x", payload: { markdown: now } }), previous });
  const f = plugin.frame;
  await f.locator('.modes [data-mode="changes"]').click();
  const card = f.locator('#changes [data-prev-comment="1"]');
  const before_ = await card.evaluate((el) => el.previousElementSibling?.textContent ?? "");
  expect(before_).toContain("On a failure it is queued again for later.");
});

test("this round's and the previous round's comments are in tabs, and every card has a straight bar", async ({
  page,
}) => {
  const before: string = earlier().payload.markdown;
  const six = before.split("\n").findIndex((l) => l.startsWith("- A delivery is tried at most six times")) + 1;
  const previous = {
    ...earlier(),
    status: "decided",
    decision: {
      data: {
        verdict: "request_changes",
        comments: [
          {
            id: 1,
            kind: "change",
            target: { kind: "text", start_line: six, end_line: six, quote: "six" },
            body: "Make it eight.",
          },
          {
            id: 2,
            kind: "question",
            target: { kind: "text", start_line: six, end_line: six, quote: "six" },
            body: "Why six?",
          },
        ],
      },
    },
  };
  const plugin = await mount(page, { previous });
  const f = plugin.frame;
  const current = f.getByRole("tab", { name: /This round/ });
  const last = f.getByRole("tab", { name: /Previous round/ });
  await expect(current).toHaveAttribute("aria-selected", "true");
  await expect(current).toContainText("0");
  await expect(last).toContainText("2");
  await expect(f.locator(".previous .card")).toHaveCount(0);

  await last.click();
  await expect(f.locator(".previous .card")).toHaveCount(2);
  // a change and a question have the same shape: a bar on a square left edge
  for (const card of [f.locator(".previous .card").nth(0), f.locator(".previous .card").nth(1)]) {
    const shape = await card.evaluate((el) => {
      const s = getComputedStyle(el);
      return [s.borderLeftWidth, s.borderTopLeftRadius, s.borderBottomLeftRadius];
    });
    expect(shape).toEqual(["3px", "0px", "0px"]);
  }
  // nothing makes the list scroll sideways
  const [scroll, client] = await f.locator("#list").evaluate((el) => [el.scrollWidth, el.clientWidth]);
  expect(scroll).toBeLessThanOrEqual(client);

  // a new comment brings this round's tab back
  await f.locator(".rendered h2").first().hover();
  await f.locator(".rendered h2 [data-comment-section]").first().click();
  await f.locator("#compose-body").fill("Name the queue.");
  await f.locator("#compose-body").press("Enter");
  await expect(current).toHaveAttribute("aria-selected", "true");
  await expect(current).toContainText("1");
  await expect(f.locator(".card", { hasText: "Name the queue." })).toBeVisible();
});

test("the comment tabs stay at the top while the list scrolls", async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 500 });
  const before: string = earlier().payload.markdown;
  const comments = Array.from({ length: 12 }, (_, i) => ({
    id: i + 1,
    kind: "change",
    target: { kind: "text", start_line: 3, end_line: 3, quote: "How" },
    body: `Comment ${i + 1}: something to change in this paragraph.`,
  }));
  const previous = { ...earlier(), status: "decided", decision: { data: { verdict: "request_changes", comments } } };
  expect(before.length).toBeGreaterThan(0);
  const plugin = await mount(page, { previous });
  const f = plugin.frame;
  await f.getByRole("tab", { name: /Previous round/ }).click();
  const tabs = f.locator(".side-tabs");
  const top = (await tabs.boundingBox())!.y;
  await f.locator("#list").evaluate((el) => el.scrollTo(0, el.scrollHeight));
  await expect.poll(async () => (await tabs.boundingBox())!.y).toBe(top);
  await expect(tabs).toBeInViewport();
});

test("a long quote shows two whole lines and an ellipsis", async ({ page }) => {
  const quote = "How the delivery worker retries a webhook that fails, and when it gives up. ".repeat(4);
  const previous = {
    ...earlier(),
    status: "decided",
    decision: {
      data: {
        verdict: "request_changes",
        comments: [
          { id: 1, kind: "change", target: { kind: "text", start_line: 3, end_line: 3, quote }, body: "Shorter." },
        ],
      },
    },
  };
  const plugin = await mount(page, { previous });
  const f = plugin.frame;
  await f.getByRole("tab", { name: /Previous round/ }).click();
  const q = f.locator(".previous .card .quote");
  const [clamp, height, line] = await q.evaluate((el) => {
    const s = getComputedStyle(el);
    return [s.webkitLineClamp, el.getBoundingClientRect().height, parseFloat(s.lineHeight)];
  });
  expect(clamp).toBe("2");
  expect(Math.round(height / line)).toBe(2);
});
