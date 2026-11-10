// The text of a session's thread can be selected and copied: what the agent
// and the person wrote there. Its buttons stay unselectable, as elsewhere
// in the app.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core } from "./helpers";

let n = 0;
const fresh = () => `s-select-${Date.now()}-${++n}`;

async function post(request: APIRequestContext, path: string, data: unknown) {
  const response = await request.post(`${core}${path}`, { data });
  expect(response.ok(), await response.text()).toBe(true);
}

/** The text selected after a triple click, which selects a whole block. */
async function selectBlock(page: Page, text: string) {
  await page.locator(".thread-items").getByText(text, { exact: true }).click({ clickCount: 3 });
  return page.evaluate(() => window.getSelection()?.toString().trim() ?? "");
}

test("the text of a session's thread can be selected, and its buttons cannot", async ({ page, request }) => {
  const session = fresh();
  const status = `/api/v1/sessions/claude-code/${session}/status`;
  await post(request, status, { text: "Running the quokka suite" });
  await post(request, status, { text: "The quokka loader reads each file.", kind: "note" });
  await post(request, `/api/v1/sessions/claude-code/${session}/messages`, { text: "Keep the old quokka endpoint." });
  await post(request, "/api/v1/reviews", {
    kind: "question",
    title: "A question",
    origin: { repo: "acme/api", ref: "main" },
    requested_by: "claude-code",
    session,
    payload: {
      questions: [
        { id: "q1", question: "Rename the quokka table?", answer: "yes-no", context: "It is read in two places." },
      ],
    },
  });
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(page.getByText("Rename the quokka table?")).toBeVisible();

  for (const text of [
    "Running the quokka suite",
    "The quokka loader reads each file.",
    "Keep the old quokka endpoint.",
    "Rename the quokka table?",
    "It is read in two places.",
  ])
    expect(await selectBlock(page, text)).toBe(text);

  const thread = page.locator(".thread-items");
  const buttons = await thread
    .getByRole("button")
    .evaluateAll((all) => all.map((b) => getComputedStyle(b).userSelect || getComputedStyle(b).webkitUserSelect));
  expect(buttons.length).toBeGreaterThan(0);
  expect(new Set(buttons)).toEqual(new Set(["none"]));
});
