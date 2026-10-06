import fs from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { clearInbox, core } from "./helpers";

async function openSetup(page: Page) {
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill("Set up Pinrail");
  await palette.getByRole("option", { name: /Set up Pinrail/ }).click();
  return page.getByRole("dialog", { name: "Set up Pinrail" });
}

type Installed = { name: string; install: { source_kind: string; source: string; link: boolean } | null };

test("Try it opens once a plugin is installed, and lists a prompt and a sample for each", async ({ page }) => {
  // a person who has installed nothing yet; the plugins other specs
  // installed are put back at the end, unless their folder is gone
  const before = (await (await page.request.get(`${core}/api/v1/plugins`)).json()).plugins as Installed[];
  for (const p of before) {
    expect((await page.request.delete(`${core}/api/v1/plugins/${p.name}`)).status()).toBe(200);
  }
  await clearInbox(page.request);

  try {
    await page.goto("/");
    const setup = await openSetup(page);
    const tryIt = setup.locator('[data-welcome-step="Try it"]');
    await expect(tryIt).toBeDisabled();
    await expect(tryIt).toContainText("Install a plugin first");

    await setup.locator("[data-welcome-next]").click();
    const plugins = setup.locator("[data-welcome-plugins]");
    for (const name of ["list", "feedback"]) {
      await expect(plugins.locator(`[data-welcome-plugin="${name}"]`)).toContainText("Recommended");
      await expect(plugins.locator(`[data-welcome-plugin="${name}"] input`)).toBeChecked();
    }
    // the others are offered, not chosen
    await expect(plugins.locator('[data-welcome-plugin="notes"] input')).not.toBeChecked();
    await expect(setup.locator("[data-welcome-install]")).toHaveText("Install 2 plugins");

    // with nothing chosen and nothing installed, there is no way on
    for (const name of ["list", "feedback"]) await plugins.locator(`[data-welcome-plugin="${name}"] input`).uncheck();
    await expect(setup.locator("[data-welcome-install]")).toHaveCount(0);
    await expect(setup.locator("[data-welcome-next]")).toBeDisabled();

    // installing moves on to Try it, which lists what was installed
    for (const name of ["list", "feedback"]) await plugins.locator(`[data-welcome-plugin="${name}"] input`).check();
    await setup.locator("[data-welcome-install]").click();
    await expect(setup.getByRole("heading", { name: "Try it" })).toBeVisible();
    await expect(tryIt).toBeEnabled();
    await expect(setup.locator("[data-welcome-try]")).toHaveCount(2);
    await expect(setup.locator('[data-welcome-try="list"]')).toContainText("which TODOs in this repository");
    await expect(setup.getByText("Waiting for a review to arrive")).toBeVisible();

    // a sample is a first review like any other
    await setup.locator('[data-welcome-try="feedback"]').getByRole("button", { name: "Sample" }).click();
    await expect(setup.getByRole("status")).toContainText("Arrived:");
    await setup.getByRole("button", { name: "Open the review" }).click();
    await expect(page).toHaveURL(/#\/reviews\/r_/);
    await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
  } finally {
    await clearInbox(page.request);
    for (const p of before) {
      if (["list", "feedback"].includes(p.name) || !p.install) continue;
      if (p.install.source_kind !== "index" && !fs.existsSync(p.install.source)) continue;
      const body =
        p.install.source_kind === "index"
          ? { id: p.install.source }
          : { source: p.install.source, link: p.install.link };
      expect((await page.request.post(`${core}/api/v1/plugins/install`, { data: body })).status()).toBe(200);
    }
  }
});

/** The agents as the desktop app reports them: the browser build has no
 *  native side, so the test answers its calls. */
const agent = (id: string, name: string, found: boolean, state: string, covered_by: string | null = null) => ({
  id,
  name,
  found,
  skill: `/Users/you/.${id}/skills/pinrail/SKILL.md`,
  state,
  covered_by,
});

test("Connect lists only the agents found, and connects the ones chosen", async ({ page }) => {
  await page.goto("/");
  await page.locator(".app-main").waitFor();
  await page.evaluate(
    (agents) => {
      const w = window as unknown as Record<string, unknown>;
      w.__connected = [] as string[];
      w.__TAURI_INTERNALS__ = {
        invoke: async (command: string, args: { id?: string }) => {
          if (command === "agents_status") return agents;
          if (command === "connect_agent") {
            (w.__connected as string[]).push(args.id!);
            const a = agents.find((a) => a.id === args.id)!;
            a.state = "connected";
            return agents;
          }
          if (command === "notification_status") return null;
          throw new Error(`${command} is not answered here`);
        },
      };
    },
    [
      agent("claude", "Claude Code", true, "connected"),
      agent("codex", "Codex", true, "absent"),
      agent("cursor", "Cursor", true, "absent"),
      agent("opencode", "OpenCode", true, "covered", "Claude Code"),
      agent("grok", "Grok CLI", false, "absent"),
    ],
  );
  const setup = await openSetup(page);
  const agents = setup.locator('[data-welcome-section="agents"]');
  await expect(agents.locator("[data-welcome-agent]")).toHaveCount(4);
  await expect(agents.locator('[data-welcome-agent="grok"]')).toHaveCount(0);

  // connected ones stay ticked; the others found are chosen to begin with
  await expect(agents.locator('[data-welcome-agent="claude"] input')).toBeChecked();
  await expect(agents.locator('[data-welcome-agent="claude"] input')).toBeDisabled();
  await expect(agents.locator('[data-welcome-agent="opencode"]')).toContainText("Uses Claude Code's skill");
  await expect(agents.locator("[data-welcome-connect]")).toHaveText("Connect 2 agents");

  await agents.locator('[data-welcome-agent="cursor"] input').uncheck();
  await agents.locator("[data-welcome-connect]").click();
  await expect(agents.locator('[data-welcome-agent="codex"]')).toContainText("Connected");
  await expect(agents.locator('[data-welcome-agent="cursor"]')).not.toContainText("Connected");
  expect(await page.evaluate(() => (window as unknown as { __connected: string[] }).__connected)).toEqual(["codex"]);
  await expect(agents.locator("[data-welcome-connect]")).toHaveCount(0);
  await page.keyboard.press("Escape");
});
