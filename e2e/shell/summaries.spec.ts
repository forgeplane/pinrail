import { expect, test } from "@playwright/test";
import path from "node:path";
import { clearInbox, createReview, decide, linkPlugin } from "./helpers";

// A review is summed up as its plugin declares: what it asks on the inbox
// row and in its header while it waits, what was decided in its header and
// in history once it is. The list plugin counts proposals by severity and
// verdicts by action.

const payload = {
  intro: "Three proposals.",
  groups: [
    {
      title: "lib/acme/tickets.ex",
      items: [
        { id: 1, severity: "major", title: "do_save dedups without reversing" },
        { id: 2, severity: "major", title: "missing index" },
        { id: 3, severity: "nit", title: "moduledoc typo" },
      ],
    },
  ],
};

test("the inbox row and the header say what a review asks, in its plugin's tones", async ({ page }) => {
  await clearInbox(page.request);
  const tag = `asks${Date.now()}`;
  const { id } = await createReview(page.request, { title: `${tag} tickets`, payload });

  await page.goto("/#/");
  const row = page.locator("[data-review-row]", { hasText: tag });
  await expect(row.locator(".summary-count")).toHaveText(["2 major", "1 nit"]);
  await expect(row.locator(".summary-count", { hasText: "2 major" })).toHaveClass(/tone-warning/);

  await page.goto(`/#/reviews/${id}`);
  await expect(page.locator(".review-strip .summary-count")).toHaveText(["2 major", "1 nit"]);
});

test("once decided, the header and history say what was decided", async ({ page }) => {
  const tag = `decided${Date.now()}`;
  const { id } = await createReview(page.request, { title: `${tag} tickets`, payload });
  await decide(page.request, id, {
    decisions: [
      { id: 1, action: "accept" },
      { id: 2, action: "accept" },
      { id: 3, action: "reject" },
    ],
    undecided: [],
  });

  await page.goto(`/#/reviews/${id}`);
  await expect(page.locator(".review-strip .summary-count")).toHaveText(["2 accepted", "1 rejected"]);

  await page.goto(`/#/history?q=${tag}`);
  const row = page.locator("[data-history-row]", { hasText: tag });
  await expect(row.locator(".summary-count")).toHaveText(["2 accepted", "1 rejected"]);
  // the list plugin declares no verdict, so the badge keeps the status
  await expect(row.locator(".status-badge")).toHaveText("decided");
});

test("a plugin's declared verdict names how the review ended", async ({ page }) => {
  // the sampler plugin declares its verdict: approve reads "approved"
  await linkPlugin(page.request, path.resolve(__dirname, "../../plugins/sampler"), "sampler");
  const tag = `verdict${Date.now()}`;
  const { id } = await createReview(page.request, {
    title: `${tag} notes`,
    plugin: "sampler",
    payload: { note: "Check the version." },
  });
  await decide(page.request, id, { verdict: "approve" });

  await page.goto(`/#/history?q=${tag}`);
  const badge = page.locator("[data-history-row]", { hasText: tag }).locator(".status-badge");
  await expect(badge).toHaveText("approved");
  await expect(badge).toHaveClass(/status-success/);
});
