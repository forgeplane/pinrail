import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

const dir = path.resolve(__dirname, "..");
const review = (name: string) => fixture(path.join(dir, "fixtures", name));

/** The view alone, on a fixture; `opts` takes `previous` and `readonly` as the app sends them. */
async function mount(page: any, name = "01-incident.json", opts: Record<string, unknown> = {}) {
  const plugin = await mountPlugin(page, dir, { review: review(name), ...opts });
  await expect(plugin.frame.getByRole("heading", { level: 1 })).toBeVisible();
  return plugin;
}

/** One question's fieldset, by the id the payload gave it. */
const q = (frame: any, id: string) => frame.locator(`[data-question="${id}"]`);

/** The answers the incident fixture needs before it will hand over. */
async function rollback(frame: any) {
  await q(frame, "approach")
    .getByRole("radio", { name: /Roll back to release/ })
    .check();
  await q(frame, "preserve_logs").getByRole("checkbox").check();
  await q(frame, "notify").getByRole("radio", { name: "No", exact: true }).check();
}

test("nothing is preselected, and a choice opens the questions that follow from it", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const plugin = await mount(page);
  const f = plugin.frame;

  // an agent's recommendation is shown, never filled in: the answer is the person's
  await expect(f.locator("input:checked")).toHaveCount(0);
  await expect(q(f, "checks")).toHaveCount(0);

  await q(f, "approach")
    .getByRole("radio", { name: /Apply the proposed patch/ })
    .check();
  await expect(q(f, "checks")).toBeVisible();
  await q(f, "checks")
    .getByRole("checkbox", { name: /Replay failed requests/ })
    .check();
  await expect(q(f, "replay_details")).toBeVisible();

  // what was typed into a follow-up survives the answer above it changing and changing back
  await q(f, "replay_details").getByRole("textbox").fill("Use anonymized requests only.");
  await q(f, "checks").getByRole("button", { name: "Add a comment" }).click();
  await q(f, "checks").getByLabel("Comment on this question").fill("No raw customer data.");
  await q(f, "approach")
    .getByRole("radio", { name: /Roll back to release/ })
    .check();
  await expect(q(f, "checks")).toHaveCount(0);
  await q(f, "approach")
    .getByRole("radio", { name: /Apply the proposed patch/ })
    .check();
  await expect(q(f, "replay_details").getByRole("textbox")).toHaveValue("Use anonymized requests only.");
  await expect(q(f, "checks").getByLabel("Comment on this question")).toHaveValue("No raw customer data.");

  expect(errors).toEqual([]);
});

test("a required answer stops the hand-over; what goes over is exactly what was answered", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;

  await plugin.collect();
  await expect(f.locator(".pinrail-confirmation")).toContainText("highlighted");
  expect((await plugin.messages()).filter((m: any) => m.type === "submit")).toHaveLength(0);

  await rollback(f);
  await q(f, "approach").getByRole("button", { name: "Add a comment" }).click();
  await q(f, "approach").getByLabel("Comment on this question").fill("Preserve logs first.");
  await plugin.collect();

  // a false and an unticked box are answers; what the conditions hid is not
  expect(await plugin.nextSubmit()).toMatchObject({
    answers: [
      { question_id: "approach", answer: "rollback", comment: "Preserve logs first." },
      { question_id: "preserve_logs", answer: true, comment: "" },
      { question_id: "notify", answer: false, comment: "" },
    ],
    unanswered: ["other_context"],
  });
  expect((await plugin.nextSubmit()).excluded).toContain("checks");

  await plugin.sendSubmitted({ data: await plugin.nextSubmit() });
  await expect(q(f, "approach").getByRole("radio").first()).toBeDisabled();
  await expect(f.locator(".header-count")).toContainText("Read-only");
});

test("missing answers are named in the layout's confirmation bar, which leads to the first", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  const bar = f.locator(".pinrail-confirmation");
  await expect(bar).toBeHidden();

  await plugin.collect();
  await expect(bar).toBeVisible();
  await expect(bar).toHaveClass(/pinrail-confirmation-warning/);
  await expect(bar).toContainText("Complete 2 highlighted questions before handing over.");
  await expect(f.locator(".validation-banner")).toHaveCount(0);

  await f.locator("#questions").evaluate((el: HTMLElement) => el.scrollTo(0, el.scrollHeight));
  await bar.getByRole("button", { name: "Go to the first" }).click();
  await expect(q(f, "approach").getByRole("radio").first()).toBeFocused();

  await rollback(f);
  await expect(bar).toBeHidden();
});

test("the rail marks the required questions still to answer", async ({ page }) => {
  const plugin = await mount(page);
  const rail = plugin.frame.locator(".sidebar nav");
  await expect(rail.getByRole("button", { name: /^How should I proceed\?.*required$/ })).toBeVisible();
  await expect(rail.getByRole("button", { name: "Anything else I should know?", exact: true })).toBeVisible();

  await rollback(plugin.frame);
  await expect(rail.getByRole("button", { name: "How should I proceed?", exact: true })).toBeVisible();
  await expect(rail.locator(".rail-required")).toHaveCount(0);
});

test("at its limit, a question's other options wait until one is unticked", async ({ page }) => {
  const plugin = await mount(page, "02-launch.json");
  const channels = q(plugin.frame, "launch_channels");
  for (const name of ["Personal invitation emails", "Product announcement", "Guided demo"]) {
    await channels.getByRole("checkbox", { name: new RegExp(name) }).check();
  }
  await expect(channels.getByRole("checkbox", { name: /Social posts/ })).toBeDisabled();
  await expect(channels).toContainText("3 of 3 chosen");

  await channels.getByRole("checkbox", { name: /Guided demo/ }).uncheck();
  await expect(channels.getByRole("checkbox", { name: /Social posts/ })).toBeEnabled();
});

test("an answer a condition hid never reaches the decision, and a draft comes back", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;

  await q(f, "approach")
    .getByRole("radio", { name: /Apply the proposed patch/ })
    .check();
  await q(f, "checks")
    .getByRole("checkbox", { name: /Replay failed requests/ })
    .check();
  await q(f, "replay_details").getByRole("textbox").fill("HIDDEN ANSWER");
  await q(f, "checks").getByRole("button", { name: "Add a comment" }).click();
  await q(f, "checks").getByLabel("Comment on this question").fill("HIDDEN COMMENT");
  await rollback(f);

  // the draft keeps it, so answering the other way again does not lose the typing
  await expect.poll(async () => JSON.stringify(await plugin.lastDraft())).toContain("HIDDEN ANSWER");
  await plugin.reinit({ draft: await plugin.lastDraft() });
  await expect(q(f, "notify").getByRole("radio", { name: "No", exact: true })).toBeChecked();

  await plugin.collect();
  const decision = await plugin.nextSubmit();
  expect(JSON.stringify(decision)).not.toContain("HIDDEN");
  expect(decision.excluded).toContain("replay_details");
});

test("compound conditions, limits and a group's own condition all hold in the browser", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;

  await q(f, "notify").getByRole("radio", { name: /^Yes/ }).check();
  await expect(q(f, "channels")).toBeVisible();
  // the follow-up needs both answers, not just the first
  await expect(q(f, "message_focus")).toHaveCount(0);
  await q(f, "channels").getByRole("checkbox", { name: "Email affected customers" }).check();
  await expect(q(f, "message_focus")).toBeVisible();

  await q(f, "message_focus").getByRole("textbox").fill("short");
  await plugin.collect();
  await expect(q(f, "message_focus")).toContainText("at least 10");
});

test("a group hidden by its own condition takes its questions with it", async ({ page }) => {
  const plugin = await mount(page, "02-launch.json");
  const f = plugin.frame;

  await q(f, "audience")
    .getByRole("radio", { name: /Public launch/ })
    .check();
  await expect(q(f, "timing")).toBeVisible();
  await q(f, "audience")
    .getByRole("radio", { name: /Everyone on the waitlist/ })
    .check();
  await expect(q(f, "timing")).toHaveCount(0);
});

test("a comment alone is an answer's context, not an answer", async ({ page }) => {
  const plugin = await mount(page, "03-handoff.json");
  const f = plugin.frame;

  await q(f, "include_appendix").getByRole("radio", { name: "No", exact: true }).check();
  // an acknowledgment ticked and unticked again is not an acknowledgment
  await q(f, "reviewed").getByRole("checkbox").check();
  await q(f, "reviewed").getByRole("checkbox").uncheck();
  await plugin.collect();
  await expect(q(f, "reviewed")).toContainText("Confirm this acknowledgment");

  await q(f, "reviewed").getByRole("checkbox").check();
  await q(f, "extra").getByRole("button", { name: "Add a comment" }).click();
  await q(f, "extra").getByLabel("Comment on this question").fill("Check recipient names again.");
  await plugin.collect();

  const decision = await plugin.nextSubmit();
  expect(decision.answers.at(-1)).toEqual({
    question_id: "extra",
    answer: null,
    comment: "Check recipient names again.",
  });
  expect(decision.unanswered).toContain("extra");
  // a free-text question carries its own words; a second box for them would be noise
  await expect(q(f, "note").getByRole("button", { name: "Add a comment" })).toHaveCount(0);
});

test("a decided review is read-only, and a new round starts empty with the last one for reference", async ({
  page,
}) => {
  const decided = await mount(page, "04-incident.decided.json", { readonly: true });
  await expect(q(decided.frame, "notify").getByRole("radio", { name: "No", exact: true })).toBeChecked();
  await expect(q(decided.frame, "notify").getByRole("radio", { name: "No", exact: true })).toBeDisabled();

  const round2 = await mount(page, "01-incident.json", { previous: review("04-incident.decided.json") });
  const f = round2.frame;
  // the previous round is shown where it is asked for, and nowhere else
  await expect(f.locator("input:checked")).toHaveCount(0);
  await q(f, "approach").getByText("Previous response", { exact: true }).click();
  await expect(q(f, "approach").locator(".previous")).toContainText("Preserve the logs first.");
});

test("typing survives a theme change, and the shell's own objections keep the draft", async ({ page }) => {
  const plugin = await mount(page, "03-handoff.json");
  const f = plugin.frame;

  const no = q(f, "include_appendix").getByRole("radio", { name: "No", exact: true });
  await no.focus();
  await no.press("Space");
  const text = q(f, "note").getByRole("textbox").first();
  await text.fill("Start: ");
  await text.press("End");
  await text.pressSequentially("keep me");
  await expect(text).toHaveValue("Start: keep me");

  await plugin.send({ type: "appearance", theme: "light" });
  await expect(f.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(text).toHaveValue("Start: keep me");

  await plugin.sendViolations([{ path: "/answers", message: "Please confirm the recipient list again." }]);
  await expect(f.getByRole("alert")).toContainText("recipient list");
  await expect(no).toBeChecked();
});

test("a condition that cannot be evaluated fails closed, and agent text is escaped", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  const broken = review("01-incident.json");
  broken.title = "<img src=x onerror=alert(1)>";

  broken.payload.groups[0].questions[0].prompt = "<script>alert(1)</script>";
  await plugin.send({ type: "init", review: broken, draft: null, readonly: false });
  await expect(f.getByRole("heading", { level: 1 })).toHaveText(broken.title);
  await expect(f.locator("img")).toHaveCount(0);

  // a question whose condition reads an answer given later cannot be ordered:
  // the view refuses the whole payload rather than guessing what to show
  broken.payload.groups[0].questions[0].when = { question_id: "notify", operator: "answered" };
  await plugin.send({ type: "init", review: broken, draft: null, readonly: false });
  await expect(f.getByRole("alert")).toContainText("earlier questions");
  await plugin.collect();
  expect((await plugin.messages()).filter((m: any) => m.type === "submit")).toHaveLength(0);
});

for (const narrow of [false, true]) {
  test(`answering never moves the page under the person (${narrow ? "narrow" : "wide"})`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "no-preference" });
    if (narrow) await page.setViewportSize({ width: 420, height: 800 });
    const plugin = await mount(page);
    const f = plugin.frame;
    await plugin.setFrameHeight(narrow ? 700 : 900);
    await q(f, "approach")
      .getByRole("radio", { name: /Apply the proposed patch/ })
      .check();

    // The view re-renders on every answer. Scroll each control into the middle
    // of the scroller, act on it, and watch two dozen frames: the scroll
    // position must not move, or answering a long form fights the person.
    const results = await f.locator("#questions").evaluate(async () => {
      const seen = [];
      for (const id of ["answer-checks-0", "answer-checks-1", "comment-toggle-checks", "comment-checks"]) {
        const input = document.getElementById(id) as HTMLInputElement;
        const scroller = document.querySelector(window.innerWidth <= 600 ? ".workspace" : "#questions")!;
        const offset = input.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
        scroller.scrollTo({ top: scroller.scrollTop + offset - 180, behavior: "instant" });
        input.focus({ preventScroll: true });
        const start = scroller.scrollTop;
        const samples: number[] = [];
        if (input.tagName === "TEXTAREA") {
          input.value = "Preserve the logs before proceeding.";
          input.dispatchEvent(new Event("input", { bubbles: true }));
        } else input.click();
        for (let i = 0; i < 24; i++) {
          await new Promise((resolve) => requestAnimationFrame(resolve));
          samples.push(scroller.scrollTop);
        }
        seen.push({ id, start, samples });
      }
      return seen;
    });

    for (const result of results) {
      expect(result.start).toBeGreaterThan(100);
      expect(Math.max(...result.samples.map((y: number) => Math.abs(y - result.start))), result.id).toBeLessThanOrEqual(
        1,
      );
    }
    await expect(f.locator("#answer-checks-0")).toBeChecked();
  });
}

test("the rail can be folded away, and the shell is asked to remember it", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  const rail = f.locator(".sidebar");
  await expect(rail).toBeVisible();

  await f.getByRole("button", { name: "Hide the group list" }).click();
  await expect(rail).toBeHidden();
  // the choice is the shell's to keep, so the next set of questions opens the same way
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ rail_open: false });

  // and it survives the re-render every answer causes
  await q(f, "approach")
    .getByRole("radio", { name: /Roll back to release/ })
    .check();
  await expect(rail).toBeHidden();

  await f.getByRole("button", { name: "Show the group list" }).click();
  await expect(rail).toBeVisible();
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ rail_open: true });
});

test("a comment shows as Markdown, and a click on it edits it", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  const approach = q(f, "approach");
  const box = approach.getByLabel("Comment on this question");
  const shown = approach.locator(".comment-view");

  await approach.getByRole("radio", { name: /Roll back to release/ }).check();
  await approach.getByRole("button", { name: "Add a comment" }).click();
  await expect(box).toBeFocused();
  await box.fill("because of **the logs**\n\n- and the replay");

  // with both an answer and a comment, both can be taken off
  await expect(approach.getByRole("button", { name: "Clear answer" })).toBeVisible();
  await expect(approach.getByRole("button", { name: "Remove comment" })).toBeVisible();

  // leaving the box shows the comment, whole and formatted, and nothing offers a second one
  await f.locator(".request-description").click();
  await expect(box).toBeHidden();
  await expect(shown.locator("strong")).toHaveText("the logs");
  await expect(shown.locator("li")).toHaveText("and the replay");
  await expect(approach.getByRole("button", { name: "Add a comment" })).toHaveCount(0);

  await shown.click();
  await expect(box).toBeFocused();
  await expect(box).toHaveValue("because of **the logs**\n\n- and the replay");
  await box.press("Escape");
  await expect(box).toBeHidden();

  // from the keyboard too
  await shown.focus();
  await shown.press("Enter");
  await expect(box).toBeFocused();
  await box.press("Escape");

  // a click elsewhere while editing still lands where it was aimed
  await shown.click();
  await approach.getByRole("radio", { name: /Keep investigating/ }).click();
  await expect(approach.getByRole("radio", { name: /Keep investigating/ })).toBeChecked();
  await expect(box).toBeHidden();

  // clearing the answer leaves the comment, which still has something to say
  await approach.getByRole("button", { name: "Clear answer" }).click();
  await expect(approach.getByRole("button", { name: "Clear answer" })).toHaveCount(0);
  await expect(shown).toContainText("because of the logs");

  await shown.click();
  await approach.getByRole("button", { name: "Remove comment" }).click();
  await expect(box).toBeHidden();
  await expect(shown).toHaveCount(0);
  await expect(approach.getByRole("button", { name: "Add a comment" })).toBeFocused();
  await expect
    .poll(async () => JSON.stringify(await plugin.lastDraft()))
    .toBe(JSON.stringify({ values: {}, comments: {} }));
});

test("a comment's box grows as it is typed in, up to a limit, then scrolls", async ({ page }) => {
  const plugin = await mount(page);
  const approach = q(plugin.frame, "approach");
  await approach.getByRole("button", { name: "Add a comment" }).click();
  const box = approach.getByLabel("Comment on this question");
  const size = () => box.evaluate((el: HTMLElement) => [el.clientHeight, el.scrollHeight]);
  const [empty] = await size();

  await box.fill("line\n".repeat(6));
  const [six, sixContent] = await size();
  expect(six).toBeGreaterThan(empty);
  expect(sixContent).toBeLessThanOrEqual(six);

  await box.fill("line\n".repeat(40));
  const [many, manyContent] = await size();
  expect(many).toBeLessThan(260);
  expect(manyContent).toBeGreaterThan(many);
});

test("a comment left empty is dropped", async ({ page }) => {
  const plugin = await mount(page);
  const approach = q(plugin.frame, "approach");
  await approach.getByRole("button", { name: "Add a comment" }).click();
  await approach.getByLabel("Comment on this question").fill("   ");
  await approach.getByLabel("Comment on this question").press("Escape");
  await expect(approach.locator(".comment-view")).toHaveCount(0);
  await expect(approach.getByRole("button", { name: "Add a comment" })).toBeVisible();
});

test("a long comment wraps rather than widening the page", async ({ page }) => {
  const plugin = await mount(page);
  const approach = q(plugin.frame, "approach");
  await approach.getByRole("button", { name: "Add a comment" }).click();
  await approach.getByLabel("Comment on this question").fill("a-long-comment-".repeat(40));
  await approach.getByLabel("Comment on this question").press("Escape");
  await expect(approach.locator(".comment-view")).toBeVisible();
  const scroll = plugin.frame.locator(".questions-scroll");
  const width = await scroll.evaluate((el: HTMLElement) => [el.scrollWidth, el.clientWidth]);
  expect(width[0]).toBe(width[1]);
});

test("the agent's recommendation is one click away, and says so once it is the answer", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  const approach = q(f, "approach");
  await expect(approach.locator(".agent-card")).toContainText("Roll back to release 2.7");
  await approach.getByRole("button", { name: "Use this answer" }).click();
  await expect(approach.getByRole("radio", { name: /Roll back to release/ })).toBeChecked();
  await expect(approach.getByRole("button", { name: "Use this answer" })).toHaveCount(0);
  await expect(approach.locator(".agent-card")).toContainText("Your answer");

  // a multiple choice takes every recommended option
  await approach.getByRole("radio", { name: /Apply the proposed patch/ }).check();
  const checks = q(f, "checks");
  await checks.getByRole("button", { name: "Use this answer" }).click();
  await expect(checks.getByRole("checkbox", { name: /Integration tests/ })).toBeChecked();
  await expect(checks.getByRole("checkbox", { name: /Replay failed requests/ })).toBeChecked();
  await expect(checks.getByRole("checkbox", { name: /Staging smoke test/ })).not.toBeChecked();
});

test("j and k move between questions, focus the answer, and leave typing alone", async ({ page }) => {
  const plugin = await mount(page);
  const f = plugin.frame;
  await f.locator("h1").click();
  await f.locator("body").press("j");
  await expect(q(f, "approach")).toHaveClass(/is-current/);
  await expect(q(f, "approach").getByRole("radio").first()).toBeFocused();

  // from a focused radio, j goes on: letters do not answer a radio
  await f.locator("body").press("j");
  await expect(q(f, "notify")).toHaveClass(/is-current/);
  await expect(q(f, "approach")).not.toHaveClass(/is-current/);
  await expect(f.locator("input:checked")).toHaveCount(0);
  await f.locator("body").press("k");
  await expect(q(f, "approach")).toHaveClass(/is-current/);

  // typing a j into a text answer types it
  const text = q(f, "other_context").getByRole("textbox");
  await text.click();
  await expect(q(f, "other_context")).toHaveClass(/is-current/);
  await text.press("j");
  await expect(text).toHaveValue("j");
  await expect(q(f, "other_context")).toHaveClass(/is-current/);
});
