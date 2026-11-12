import { expect, test, type APIRequestContext, type Locator } from "@playwright/test";
import { clearInbox, core, decide, onePayload } from "./helpers";

const decision = { decisions: [{ id: 1, action: "accept" }], undecided: [] };

/** Names this attempt's reviews apart from an earlier attempt's, which stay
 *  in the history when a test is retried. */
const run = Date.now().toString(36);

/** Hovers once the web font has loaded: text drawn in the fallback font
 *  first moves when it loads, and can move what is hovered from under the
 *  pointer before its tooltip opens. */
async function hover(target: Locator) {
  await target.page().evaluate(() => document.fonts.ready);
  await target.hover();
}

/** A review submitted from an agent's session, as the CLI submits one. */
async function submitFrom(request: APIRequestContext, title: string, session: string) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: { plugin: "list", title: `${title} ${run}`, origin: { repo: "acme/api" }, payload: onePayload, session },
  });
  expect(response.status(), await response.text()).toBe(201);
  return ((await response.json()) as { id: string }).id;
}

/** A decision as a client other than the app sends it. */
async function decideAs(
  request: APIRequestContext,
  id: string,
  headers: Record<string, string>,
  extra: { session?: string; by?: string } = {},
) {
  const response = await request.post(`${core}/api/v1/reviews/${id}/decision`, {
    data: { data: decision, ...extra },
    headers,
  });
  expect(response.status(), await response.text()).toBe(200);
}

test("a decision made outside the app says how, and warns when the asking session made it", async ({ page }) => {
  await clearInbox(page.request);
  const own = await submitFrom(page.request, "Source: decided by the asker", "s-1");
  await decideAs(page.request, own, { "x-pinrail-client": "cli" }, { session: "s-1", by: "claude-code" });
  const scripted = await submitFrom(page.request, "Source: decided by a script", "s-1");
  await decideAs(page.request, scripted, {});
  const inApp = await submitFrom(page.request, "Source: decided in the app", "s-1");
  await decide(page.request, inApp, decision);

  // the source is a badge after who decided, with how in its tooltip
  const badge = page.getByTestId("source-badge");
  const decided = page.locator(".decision-box .dim");
  await page.goto(`/#/reviews/${own}`);
  await expect(decided).toContainText("Decided by claude-code");
  await expect(badge).toHaveText("CLI");
  await expect(badge).toHaveClass(/is-own/);
  await hover(badge);
  await expect(page.getByRole("tooltip")).toHaveText(
    "Decided with pinrail decide, from the agent session that submitted the review",
  );

  // a client that names nobody: nobody is named
  await page.goto(`/#/reviews/${scripted}`);
  await expect(decided).toHaveText(/^Decided \S+ ago/);
  await expect(badge).toHaveText("API");
  await expect(badge).not.toHaveClass(/is-own/);
  await hover(badge);
  await expect(page.getByRole("tooltip")).toHaveText("Decided by a program through the API");

  await page.goto(`/#/reviews/${inApp}`);
  await expect(decided).toContainText("Decided by");
  await expect(badge).toHaveCount(0);

  // the history shows the same badges on the line under the time
  await page.goto("/#/history");
  const source = (title: string) =>
    page.getByTestId("history-row").filter({ hasText: title }).getByTestId("source-badge");
  await expect(source("decided by the asker")).toHaveText("CLI");
  await expect(source("decided by the asker")).toHaveClass(/is-own/);
  await expect(source("decided by a script")).toHaveText("API");
  await expect(source("decided in the app")).toHaveCount(0);
});

test("a withdrawn or discarded review shows who ended it and why", async ({ page }) => {
  await clearInbox(page.request);
  const withdrawn = await submitFrom(page.request, "Source: withdrawn by the asker", "s-1");
  const response = await page.request.post(`${core}/api/v1/reviews/${withdrawn}/withdraw`, {
    data: { reason: "the branch was force-pushed", session: "s-1", by: "claude-code" },
    headers: { "x-pinrail-client": "cli" },
  });
  expect(response.status(), await response.text()).toBe(200);
  const discarded = await submitFrom(page.request, "Source: discarded by a script", "s-1");
  const discard = await page.request.post(`${core}/api/v1/reviews/${discarded}/discard`, {
    data: { reason: "wrong branch" },
  });
  expect(discard.status(), await discard.text()).toBe(200);

  // the agent withdrawing its own review is what withdrawing is for: no warning
  const badge = page.getByTestId("source-badge");
  await page.goto(`/#/reviews/${withdrawn}`);
  await expect(page.locator(".notice").filter({ hasText: "withdrew" })).toContainText(
    /^claude-code withdrew this review \S+ ago: the branch was force-pushed\./,
  );
  await expect(badge).toHaveText("CLI");
  await expect(badge).not.toHaveClass(/is-own/);
  await hover(badge);
  await expect(page.getByRole("tooltip")).toHaveText(
    "Withdrawn with pinrail withdraw, from the agent session that submitted the review",
  );

  await page.goto("/#/history");
  const row = (title: string) => page.getByTestId("history-row").filter({ hasText: title });
  await expect(row("withdrawn by the asker")).toContainText("Withdrawn by claude-code");
  await expect(row("withdrawn by the asker").getByTestId("source-badge")).toHaveText("CLI");
  // the reason is in a tooltip on who ended the review, not in the row
  await expect(row("withdrawn by the asker")).not.toContainText("force-pushed");
  await hover(row("withdrawn by the asker").getByTestId("history-reason"));
  await expect(page.getByRole("tooltip")).toHaveText("the branch was force-pushed");
  await expect(row("discarded by a script")).toContainText("Discarded");
  await expect(row("discarded by a script").getByTestId("source-badge")).toHaveText("API");
  await hover(row("discarded by a script").getByTestId("history-reason"));
  await expect(page.getByRole("tooltip")).toHaveText("wrong branch");
});
