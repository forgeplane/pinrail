import fs from "node:fs";
import { expect, test, type Page } from "./fixtures";
import { clearInbox, core, onReviewPages } from "./helpers";

// a review opened from the inbox or the sidebar is followed on its own page
onReviewPages();

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
    const tryIt = setup.getByRole("button", { name: /Try it/ });
    await expect(tryIt).toBeDisabled();
    await expect(tryIt).toContainText("Install a plugin first");

    await setup.getByRole("button", { name: "Next", exact: true }).click();
    const plugin = (name: string) => setup.getByTestId(`welcome-plugin-${name}`);
    for (const name of ["list", "feedback"]) {
      await expect(plugin(name)).toContainText("Recommended");
      await expect(plugin(name).getByRole("checkbox")).toBeChecked();
    }
    // the others are offered, not chosen
    await expect(plugin("notes").getByRole("checkbox")).not.toBeChecked();
    await expect(setup.getByRole("button", { name: /^Install \d+ plugins?$/ })).toHaveText("Install 2 plugins");

    // with nothing chosen and nothing installed, there is no way on
    for (const name of ["list", "feedback"]) await plugin(name).getByRole("checkbox").uncheck();
    await expect(setup.getByRole("button", { name: /^Install \d+ plugins?$/ })).toHaveCount(0);
    await expect(setup.getByRole("button", { name: "Next", exact: true })).toBeDisabled();

    // installing moves on to Try it, which lists what was installed
    for (const name of ["list", "feedback"]) await plugin(name).getByRole("checkbox").check();
    await setup.getByRole("button", { name: /^Install \d+ plugins?$/ }).click();
    await expect(setup.getByRole("heading", { name: "Try it" })).toBeVisible();
    await expect(tryIt).toBeEnabled();
    await expect(setup.getByTestId(/^welcome-try-/)).toHaveCount(2);
    await expect(setup.getByTestId("welcome-try-list")).toContainText("which TODOs in this repository");
    await expect(setup.getByText("Waiting for a review to arrive")).toBeVisible();

    // a sample is a first review like any other
    await setup.getByTestId("welcome-try-feedback").getByRole("button", { name: "Sample" }).click();
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
const agent = (id: string, name: string, found: boolean, state: string, also_reads: string[] = []) => ({
  id,
  name,
  found,
  skill: `/Users/you/.${id}/skills/pinrail/SKILL.md`,
  state,
  also_reads,
});

test("Connect lists only the agents found, and connects the ones chosen", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("main").waitFor();
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
      agent("claude-code", "Claude Code", true, "connected"),
      agent("codex", "Codex", true, "absent"),
      agent("cursor", "Cursor", true, "absent"),
      agent("antigravity", "Antigravity CLI", false, "absent"),
      // OpenCode and Grok also read Claude Code's skills, but the skill is still installed for them
      agent("opencode", "OpenCode", true, "absent", ["Claude Code"]),
      agent("grok", "Grok CLI", true, "absent", ["Claude Code", "Cursor"]),
    ],
  );
  const setup = await openSetup(page);
  const agents = setup.getByRole("region", { name: "Your agents" });
  await expect(agents.getByTestId(/^welcome-agent-/)).toHaveCount(5);
  await expect(agents.getByTestId("welcome-agent-antigravity")).toHaveCount(0);
  // in alphabetical order of their names
  expect(
    await agents.getByTestId(/^welcome-agent-/).evaluateAll((rows) => rows.map((r) => r.getAttribute("data-testid"))),
  ).toEqual([
    "welcome-agent-claude-code",
    "welcome-agent-codex",
    "welcome-agent-cursor",
    "welcome-agent-grok",
    "welcome-agent-opencode",
  ]);

  // connected ones stay ticked; the others found are chosen to begin with
  await expect(agents.getByTestId("welcome-agent-claude-code").getByRole("checkbox")).toBeChecked();
  await expect(agents.getByTestId("welcome-agent-claude-code").getByRole("checkbox")).toBeDisabled();
  await expect(agents.getByRole("button", { name: /^Connect \d+ agents?$/ })).toHaveText("Connect 4 agents");

  await agents.getByTestId("welcome-agent-cursor").getByRole("checkbox").uncheck();
  await agents.getByRole("button", { name: /^Connect \d+ agents?$/ }).click();
  await expect(agents.getByTestId("welcome-agent-codex")).toContainText("Connected");
  await expect(agents.getByTestId("welcome-agent-grok")).toContainText("Connected");
  await expect(agents.getByTestId("welcome-agent-opencode")).toContainText("Connected");
  await expect(agents.getByTestId("welcome-agent-cursor")).not.toContainText("Connected");
  expect(await page.evaluate(() => (window as unknown as { __connected: string[] }).__connected)).toEqual([
    "codex",
    "grok",
    "opencode",
  ]);
  await expect(agents.getByRole("button", { name: /^Connect \d+ agents?$/ })).toHaveCount(0);
  await page.keyboard.press("Escape");
});
