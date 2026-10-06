import { expect, test } from "@playwright/test";

// The browser build has no native side: the section is there, and says the
// desktop app does the connecting. The skill itself is tested in the app's
// own crate.
test("Agents lists the skills, and the command line stays in Data", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await page.locator('[data-section="agents"]').click();
  const body = page.locator(".settings-body");
  await expect(body.getByRole("heading", { name: "Agents", level: 2 })).toBeVisible();
  await expect(body.getByRole("heading", { name: "Command line" })).toHaveCount(0);
  await expect(body.getByRole("heading", { name: "Connect your agents" })).toBeVisible();
  await expect(body.getByText("The desktop app finds your agents and connects them")).toBeVisible();

  await page.locator('[data-section="data"]').click();
  await expect(body.getByRole("heading", { name: "Command line" })).toBeVisible();
  await expect(body.getByText("Install the CLI")).toBeVisible();
});

// A long message, such as an outdated skill's with its folder, wraps inside
// the card rather than widening the section past the dialog.
test("Agents fits its width, whatever an agent's message says", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 700 });
  await page.goto("/#/");
  await page.locator(".app-main").waitFor();
  await page.evaluate(() => {
    const agent = (id: string, name: string, skills: string) => ({
      id,
      name,
      found: true,
      skill: `/Users/someone-with-a-long-name/${skills}/pinrail/SKILL.md`,
      state: "outdated",
      covered_by: null,
    });
    const agents = [
      agent("claude", "Claude Code", ".claude/skills"),
      agent("antigravity", "Antigravity CLI", ".gemini/config/skills"),
    ];
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string) => {
        if (command === "agents_status") return agents;
        throw new Error(`${command} is not answered here`);
      },
    };
  });
  await page.keyboard.press("ControlOrMeta+,");
  await page.locator('[data-section="agents"]').click();
  await page.locator('[data-agent="antigravity"]').waitFor();
  const body = page.locator(".settings-body");
  const overflow = await body.evaluate((el) => el.scrollWidth - el.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
});

test("Update all updates every agent whose skill is outdated", async ({ page }) => {
  await page.goto("/#/");
  await page.locator(".app-main").waitFor();
  await page.evaluate(() => {
    const agent = (id: string, name: string, state: string) => ({
      id,
      name,
      found: true,
      skill: `/Users/you/.${id}/skills/pinrail/SKILL.md`,
      state,
      covered_by: null,
    });
    const agents = [
      agent("claude", "Claude Code", "outdated"),
      agent("codex", "Codex", "outdated"),
      agent("cursor", "Cursor", "absent"),
    ];
    const w = window as unknown as Record<string, unknown>;
    w.__connected = [] as string[];
    w.__TAURI_INTERNALS__ = {
      invoke: async (command: string, args: { id?: string }) => {
        if (command === "agents_status") return agents;
        if (command === "connect_agent") {
          (w.__connected as string[]).push(args.id!);
          agents.find((a) => a.id === args.id)!.state = "connected";
          return agents;
        }
        throw new Error(`${command} is not answered here`);
      },
    };
  });
  await page.keyboard.press("ControlOrMeta+,");
  await page.locator('[data-section="agents"]').click();
  const updateAll = page.locator("[data-agents-update-all]");
  await expect(updateAll).toHaveText("Update all 2");
  await updateAll.click();
  await expect(page.locator('[data-agent="codex"]')).toHaveAttribute("data-agent-state", "connected");
  await expect(page.locator('[data-agent="claude"]')).toHaveAttribute("data-agent-state", "connected");
  // an agent that was never connected stays as it was
  await expect(page.locator('[data-agent="cursor"]')).toHaveAttribute("data-agent-state", "absent");
  expect(await page.evaluate(() => (window as unknown as { __connected: string[] }).__connected)).toEqual([
    "claude",
    "codex",
  ]);
  await expect(updateAll).toHaveCount(0);
});
