import { expect, test, type Page } from "@playwright/test";

const service = "https://feedback.test/v1/feedback";

/** The feedback service: answers every report with `status` and hands back what it got. */
async function feedbackService(page: Page, status = 200, answer: object = { ok: true }) {
  const got: { headers: Record<string, string>; body: string }[] = [];
  await page.route(service, async (route) => {
    const request = route.request();
    got.push({ headers: request.headers(), body: request.postDataBuffer()?.toString("latin1") ?? "" });
    await route.fulfill({ status, json: answer, headers: { "access-control-allow-origin": "*" } });
  });
  return got;
}

async function openFeedback(page: Page) {
  await page.goto("/#/");
  await page.getByRole("complementary", { name: "Workspace" }).getByRole("button", { name: "Send feedback" }).click();
  return page.getByRole("dialog", { name: "Send feedback" });
}

test("feedback goes with its address, subject, message and files, chosen or dropped", async ({ page }) => {
  const got = await feedbackService(page);
  const dialog = await openFeedback(page);
  const send = dialog.getByRole("button", { name: "Send" });
  await expect(send).toBeDisabled();

  await dialog.getByLabel("Email").fill("maya@example.com");
  await dialog.getByLabel("Subject").fill("Scroll position is lost");
  await dialog.getByLabel("Message").fill("After a review the inbox jumps to the top.");
  await dialog.getByLabel("Attach files").setInputFiles({
    name: "inbox.png",
    mimeType: "image/png",
    buffer: Buffer.from([137, 80, 78, 71]),
  });
  // a file dropped on the dialog is attached too
  await dialog.evaluate((element) => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(["line 1\n"], "notes.txt", { type: "text/plain" }));
    element.dispatchEvent(new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: transfer }));
    element.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer }));
  });
  const attached = dialog.getByRole("list", { name: "Attachments" }).getByRole("listitem");
  await expect(attached).toHaveCount(2);

  await send.click();
  await expect(dialog).toBeHidden();
  await expect(page.getByText("Thank you. Your feedback was sent.")).toBeVisible();

  expect(got).toHaveLength(1);
  const { headers, body } = got[0];
  expect(headers["x-pinrail-client"]).toMatch(/^pinrail\//);
  const field = (name: string) => body.match(new RegExp(`name="${name}"\\r\\n\\r\\n([^\\r]*)\\r\\n`))?.[1];
  expect(field("email")).toBe("maya@example.com");
  expect(field("subject")).toBe("Scroll position is lost");
  expect(field("message")).toBe("After a review the inbox jumps to the top.");
  expect(body).toContain('name="file"; filename="inbox.png"');
  expect(body).toContain('name="file"; filename="notes.txt"');
  // the diagnostics are included unless the person leaves them out
  expect(body).toMatch(/name="diagnostics"\r\n\r\nPinrail: \d/);
  expect(body).toContain("Plugins (");

  // the address is kept for the next time
  const again = await openFeedback(page);
  await expect(again.getByLabel("Email")).toHaveValue("maya@example.com");
});

test("diagnostics can be read before sending, and left out", async ({ page }) => {
  const got = await feedbackService(page);
  const dialog = await openFeedback(page);
  await dialog.getByRole("button", { name: "Show" }).click();
  await expect(dialog.getByLabel("Diagnostics", { exact: true })).toContainText("Settings:");

  await dialog.getByLabel("Include diagnostics").uncheck();
  await dialog.getByLabel("Email").fill("maya@example.com");
  await dialog.getByLabel("Subject").fill("Hello");
  await dialog.getByLabel("Message").fill("Hi");
  await dialog.getByRole("button", { name: "Send" }).click();
  await expect(dialog).toBeHidden();
  expect(got[0].body).not.toContain('name="diagnostics"');
});

test("a refused report keeps the dialog open with the service's reason", async ({ page }) => {
  await feedbackService(page, 429, {
    error: "rate_limited",
    message: "too much feedback from this address; try again in a minute",
  });
  const dialog = await openFeedback(page);
  await dialog.getByLabel("Email").fill("maya@example.com");
  await dialog.getByLabel("Subject").fill("Hello");
  await dialog.getByLabel("Message").fill("Hi");
  await dialog.getByLabel("Message").press("ControlOrMeta+Enter");
  await expect(dialog.getByText("try again in a minute")).toBeVisible();
  await expect(dialog.getByLabel("Message")).toHaveValue("Hi");
});

test("the command palette opens the feedback dialog", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill("send feedback");
  await expect(palette.getByRole("option", { name: /Send feedback/ })).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog", { name: "Send feedback" })).toBeVisible();
});
