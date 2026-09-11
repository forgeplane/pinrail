import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "../../../wicket_sdk/testing/playwright";

const dir = path.resolve(__dirname, "..");
const push = () => fixture(path.join(dir, "fixtures", "push.json"));

test("the answer is chosen here and handed over by the shell", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  await expect(plugin.frame.locator("p").first()).toHaveText(push().payload.message);
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over");

  await plugin.frame.getByPlaceholder("comment (optional)").fill("after the rebase");
  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: yes");
  expect((await plugin.messages()).filter((m) => m.type === "submit")).toHaveLength(0);

  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: true, comment: "after the rebase" });
});

test("no without a comment hands over only ok", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  await plugin.frame.getByRole("button", { name: "No" }).click();
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: false });
});

test("handing over without an answer asks for one; violations are shown; submitted renders read-only", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  await plugin.collect();
  await expect(plugin.frame.locator("#errors")).toHaveText("Choose yes or no first.");
  expect((await plugin.messages()).filter((m) => m.type === "submit")).toHaveLength(0);

  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: true });

  await plugin.sendViolations([{ path: "/ok", message: "value is not of type boolean" }]);
  await expect(plugin.frame.locator("#errors")).toHaveText("/ok: value is not of type boolean");

  await plugin.sendSubmitted({ decided_by: "alice", decided_at: "2026-09-11T10:00:00Z", data: { ok: true, comment: "go" } });
  await expect(plugin.frame.locator("p").last()).toContainText("Decided: yes — go");
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toHaveCount(0);
});

test("the answer and the comment survive a reload", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  await plugin.frame.getByRole("button", { name: "No" }).click();
  await plugin.frame.getByPlaceholder("comment (optional)").fill("keep this");
  await expect.poll(() => plugin.lastDraft()).toEqual({ ok: false, comment: "keep this" });

  await plugin.reload();
  await plugin.reinit();
  await expect(plugin.frame.getByRole("button", { name: "No" })).toHaveAttribute("aria-pressed", "true");
  await expect(plugin.frame.getByPlaceholder("comment (optional)")).toHaveValue("keep this");
});

test("follows the shell's theme without losing what was typed", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  const comment = plugin.frame.getByPlaceholder("comment (optional)");
  await comment.fill("keep this");

  await plugin.send({ type: "appearance", theme: "light" });
  await expect(plugin.frame.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(comment).toHaveValue("keep this");

  await plugin.send({ type: "appearance", theme: "dark" });
  await expect(plugin.frame.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("renders a decided gate read-only", async ({ page }) => {
  const gate = { ...push(), decision: { decided_by: "alice", decided_at: "2026-09-11T10:00:00Z", data: { ok: false } } };
  const plugin = await mountPlugin(page, dir, { gate, readonly: true });
  await expect(plugin.frame.locator("p").last()).toContainText("Decided: no");
  await expect(plugin.frame.locator("button")).toHaveCount(0);
});

test("the view is in the shell's theme with no message from it at all", async ({ page }) => {
  // The fake shell never sends `appearance`, so the only theme a view can be
  // in here is the one the frame's URL carried. A view that waited for the
  // message would paint in the wrong theme first.
  const plugin = await mountPlugin(page, dir, { gate: push(), theme: "light" });
  await expect(plugin.frame.locator("html")).toHaveAttribute("data-theme", "light");
  expect((await plugin.messages()).some((m) => m.type === "appearance")).toBe(false);

  const background = await plugin.frame
    .locator("body")
    .evaluate((body) => getComputedStyle(body).backgroundColor);
  expect(background).toBe("rgb(255, 255, 255)");
});

test("a dark shell leaves the view dark", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push(), theme: "dark" });
  await expect(plugin.frame.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("an icon paints inside the sandbox and takes the colour of its button", async ({ page }) => {
  const fetched: Record<string, number> = {};
  const blocked: string[] = [];
  page.on("response", (r) => {
    if (r.url().includes("/sdk/v1/icons/")) fetched[r.url().split("/").pop()!] = r.status();
  });
  page.on("console", (m) => {
    if (/content security policy/i.test(m.text())) blocked.push(m.text());
  });

  const plugin = await mountPlugin(page, dir, { gate: push(), theme: "light" });
  const yes = plugin.frame.getByRole("button", { name: "Yes" });
  await expect(yes).toBeVisible();

  const mark = yes.locator(".wi");
  await expect(mark).toHaveAttribute("data-icon", "check");
  // Decorative: the button already says Yes, so the icon is not announced.
  await expect(mark).toHaveAttribute("aria-hidden", "true");

  const painted = await mark.evaluate((el) => {
    const box = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return {
      width: Math.round(box.width),
      height: Math.round(box.height),
      colour: style.backgroundColor,
      textColour: getComputedStyle(el.closest("button")!).color,
      mask: (style.maskImage || (style as any).webkitMaskImage || "").includes("/sdk/v1/icons/check.svg"),
    };
  });
  expect(painted.width).toBeGreaterThan(8);
  expect(painted.height).toBeGreaterThan(8);
  expect(painted.mask).toBe(true);
  expect(painted.colour).toBe(painted.textColour);

  expect(fetched["check.svg"]).toBe(200);
  expect(fetched["x.svg"]).toBe(200);
  expect(blocked).toEqual([]);
});

test("a name with no icon behind it renders nothing and says which name", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  const missing = await plugin.frame.locator("body").evaluate((body, markup) => {
    body.insertAdjacentHTML("beforeend", markup);
    const el = body.querySelector('[data-icon="not-an-icon"]') as HTMLElement;
    const box = el.getBoundingClientRect();
    return { name: el.dataset.icon, width: Math.round(box.width) };
  }, `<span class="wi" data-icon="not-an-icon" style="--wi:url(/sdk/v1/icons/not-an-icon.svg)"></span>`);
  expect(missing.name).toBe("not-an-icon");
  expect(missing.width).toBeGreaterThan(8);
});
