import { expect, test } from "./fixtures";
import { core, settingsSection } from "./helpers";

// The browser build has no native side: the section is there, and says the
// desktop app does the connecting. The skill itself is tested in the app's
// own crate.
test("Agents lists the skills, and the command line stays in Data", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Agents").click();
  const body = page.getByTestId("settings-body");
  await expect(body.getByRole("heading", { name: "Agents", level: 2 })).toBeVisible();
  await expect(body.getByRole("heading", { name: "Command line" })).toHaveCount(0);
  await expect(body.getByRole("heading", { name: "Connect your agents" })).toBeVisible();
  await expect(body.getByText("The desktop app finds your agents and connects them")).toBeVisible();

  await settingsSection(page, "Data").click();
  await expect(body.getByRole("heading", { name: "Command line" })).toBeVisible();
  await expect(body.getByText("Install the CLI")).toBeVisible();
});

// A long message, such as an outdated skill's with its folder, wraps inside
// the card rather than widening the section past the dialog.
test("Agents fits its width, whatever an agent's message says", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 700 });
  await page.goto("/#/");
  await page.getByRole("main").waitFor();
  await page.evaluate(() => {
    const agent = (id: string, name: string, skills: string) => ({
      id,
      name,
      found: true,
      skill: `/Users/someone-with-a-long-name/${skills}/pinrail/SKILL.md`,
      state: "outdated",
    });
    const agents = [
      agent("claude-code", "Claude Code", ".claude/skills"),
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
  await settingsSection(page, "Agents").click();
  await page.getByTestId("agent-antigravity").waitFor();
  const body = page.getByTestId("settings-body");
  const overflow = await body.evaluate((el) => el.scrollWidth - el.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
});

test("Update all updates every agent whose skill is outdated", async ({ page }) => {
  await page.goto("/#/");
  await page.getByRole("main").waitFor();
  await page.evaluate(() => {
    const agent = (id: string, name: string, state: string) => ({
      id,
      name,
      found: true,
      skill: `/Users/you/.${id}/skills/pinrail/SKILL.md`,
      state,
    });
    const agents = [
      agent("claude-code", "Claude Code", "outdated"),
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
  await settingsSection(page, "Agents").click();
  const updateAll = page.getByRole("button", { name: /^Update all/ });
  await expect(updateAll).toHaveText("Update all 2");
  await updateAll.click();
  await expect(page.getByTestId("agent-codex")).toHaveClass(/\bis-connected\b/);
  await expect(page.getByTestId("agent-claude-code")).toHaveClass(/\bis-connected\b/);
  // an agent that was never connected stays as it was
  await expect(page.getByTestId("agent-cursor")).toHaveClass(/\bis-absent\b/);
  expect(await page.evaluate(() => (window as unknown as { __connected: string[] }).__connected)).toEqual([
    "claude-code",
    "codex",
  ]);
  await expect(updateAll).toHaveCount(0);
});

test("OpenCode can be connected beside Claude Code, whose skill it already uses", async ({ page }) => {
  await page.goto("/#/");
  await page.getByRole("main").waitFor();
  await page.evaluate(() => {
    const agents = [
      {
        id: "claude-code",
        name: "Claude Code",
        found: true,
        skill: "/Users/you/.claude/skills/pinrail/SKILL.md",
        state: "connected",
        also_reads: [],
      },
      {
        id: "opencode",
        name: "OpenCode",
        found: true,
        skill: "/Users/you/.config/opencode/skills/pinrail/SKILL.md",
        state: "absent",
        also_reads: ["Claude Code"],
      },
    ];
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string, args: { id?: string }) => {
        if (command === "agents_status") return agents;
        if (command === "connect_agent") {
          agents.find((a) => a.id === args.id)!.state = "connected";
          return agents;
        }
        throw new Error(`${command} is not answered here`);
      },
    };
  });
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Agents").click();
  const opencode = page.getByTestId("agent-opencode");
  await expect(opencode).toContainText("Already uses Claude Code's skill");
  await opencode.getByRole("button", { name: "Connect" }).click();
  await expect(opencode).toHaveClass(/\bis-connected\b/);
  await expect(opencode).toContainText("~/.config/opencode/skills/pinrail");
  await expect(opencode).toContainText("Reads Claude Code's skills too");
});

test("Agents groups the agents into connected, not connected and not found, each by name", async ({ page }) => {
  await page.goto("/#/");
  await page.getByRole("main").waitFor();
  await page.evaluate(() => {
    const agent = (id: string, name: string, found: boolean, state: string) => ({
      id,
      name,
      found,
      skill: `/Users/you/.${id}/skills/pinrail/SKILL.md`,
      state,
    });
    const agents = [
      agent("grok", "Grok CLI", true, "connected"),
      agent("claude-code", "Claude Code", true, "connected"),
      agent("pi", "Pi", true, "absent"),
      agent("codex", "Codex", true, "absent"),
      agent("kiro", "Kiro CLI", false, "absent"),
      agent("amp", "Amp", false, "absent"),
    ];
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string) => {
        if (command === "agents_status") return agents;
        throw new Error(`${command} is not answered here`);
      },
    };
  });
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Agents").click();
  const ids = (group: string) =>
    page
      .getByTestId(`agent-group-${group}`)
      .locator("[data-testid^=agent-]:not([data-testid^=agent-group])")
      .evaluateAll((cards) => cards.map((c) => c.getAttribute("data-testid")));
  await expect.poll(() => ids("Connected")).toEqual(["agent-claude-code", "agent-grok"]);
  expect(await ids("Not connected")).toEqual(["agent-codex", "agent-pi"]);

  // the agents not found stay folded away until asked for
  const missing = page.getByTestId("agent-group-Not found");
  await expect(missing.getByRole("button")).toHaveText("Not found on this computer (2)");
  expect(await ids("Not found")).toEqual([]);
  await missing.getByRole("button").click();
  await expect.poll(() => ids("Not found")).toEqual(["agent-amp", "agent-kiro"]);
});

test("an agent that reads a connected agent's skill says it already uses it", async ({ page }) => {
  await page.goto("/#/");
  await page.getByRole("main").waitFor();
  await page.evaluate(() => {
    const agent = (id: string, name: string, state: string, also_reads: string[] = []) => ({
      id,
      name,
      found: true,
      skill: `/Users/you/.${id}/skills/pinrail/SKILL.md`,
      state,
      also_reads,
    });
    const agents = [
      agent("claude-code", "Claude Code", "connected"),
      agent("cursor", "Cursor", "absent"),
      agent("grok", "Grok CLI", "absent", ["Claude Code", "Cursor"]),
      agent("opencode", "OpenCode", "absent", ["Claude Code"]),
      agent("amp", "Amp", "absent", ["Cursor"]),
    ];
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string) => {
        if (command === "agents_status") return agents;
        throw new Error(`${command} is not answered here`);
      },
    };
  });
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Agents").click();
  const notConnected = page.getByTestId("agent-group-Not connected");
  // only the connected agent counts; it stays under Not connected, with Connect
  await expect(notConnected.getByTestId("agent-grok")).toContainText("Already uses Claude Code's skill");
  await expect(notConnected.getByTestId("agent-opencode")).toContainText("Already uses Claude Code's skill");
  await expect(notConnected.getByTestId("agent-opencode").getByRole("button", { name: "Connect" })).toBeVisible();
  // Cursor is not connected, so Amp only reads its skills
  await expect(notConnected.getByTestId("agent-amp")).toContainText("Reads Cursor's skills too");
});

test("Update skills automatically is off at first, and turned on it updates the outdated skills", async ({ page }) => {
  await page.request.patch(`${core}/api/v1/settings`, { data: { agents: { auto_update: false } } });
  await page.goto("/#/");
  await page.getByRole("main").waitFor();
  await page.evaluate(() => {
    const agent = (id: string, name: string, state: string) => ({
      id,
      name,
      found: true,
      skill: `/Users/you/.${id}/skills/pinrail/SKILL.md`,
      state,
      also_reads: [],
    });
    const agents = [agent("claude-code", "Claude Code", "outdated"), agent("codex", "Codex", "connected")];
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
        return null;
      },
    };
  });
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Agents").click();
  const toggle = page.getByRole("switch", { name: "Update agent skills automatically" });
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await expect(page.getByTestId("agent-claude-code")).toHaveClass(/\bis-connected\b/);
  expect(await page.evaluate(() => (window as unknown as { __connected: string[] }).__connected)).toEqual([
    "claude-code",
  ]);
  await expect
    .poll(async () => (await (await page.request.get(`${core}/api/v1/settings`)).json()).agents?.auto_update)
    .toBe(true);
  await page.request.patch(`${core}/api/v1/settings`, { data: { agents: { auto_update: false } } });
});
