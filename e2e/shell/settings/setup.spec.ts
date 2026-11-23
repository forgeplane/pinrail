import fs from "node:fs";
import { expect, test, type Page } from "../fixtures";
import { core } from "../helpers";

async function openSetup(page: Page) {
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill("Set up Pinrail");
  await palette.getByRole("option", { name: /Set up Pinrail/ }).click();
  return page.getByRole("dialog", { name: "Set up Pinrail" });
}

type Installed = { name: string; install: { source_kind: string; source: string; link: boolean } | null };

test("the plugins view adds a plugin to the ones the setup installs", async ({ page }) => {
  // a person who has installed nothing yet; the plugins other specs
  // installed are put back at the end, unless their folder is gone
  const before = (await (await page.request.get(`${core}/api/v1/plugins`)).json()).plugins as Installed[];
  for (const p of before) {
    expect((await page.request.delete(`${core}/api/v1/plugins/${p.name}`)).status()).toBe(200);
  }

  try {
    await page.goto("/");
    const setup = await openSetup(page);
    const summary = setup.getByTestId("setup-summary");
    await expect(summary).toContainText("adds the Action list and Feedback plugins");

    await setup.getByRole("button", { name: "Add more plugins" }).click();
    await expect(setup.getByRole("heading", { name: "Add more plugins" })).toBeVisible();
    const plugin = (name: string) => setup.getByTestId(`setup-plugin-${name}`);
    // the recommended ones are included and stay so
    for (const name of ["list", "feedback"]) {
      await expect(plugin(name)).toContainText("Included");
      await expect(plugin(name)).toHaveAttribute("aria-pressed", "true");
      await expect(plugin(name)).toBeDisabled();
    }
    // nothing in the view is wider than its box, which would show a
    // scrollbar across it where scrollbars always show
    const wide = await setup.evaluate((dialog) =>
      [dialog, ...dialog.querySelectorAll("*")]
        .filter((el) => getComputedStyle(el).overflowX !== "visible" && el.scrollWidth > el.clientWidth)
        .map((el) => el.className),
    );
    expect(wide).toEqual([]);
    // the others are offered, not picked
    await expect(plugin("notes")).toHaveAttribute("aria-pressed", "false");
    await expect(setup.getByRole("button", { name: "Done", exact: true })).toBeVisible();

    // a plugin picked and then left with Back is not added
    await plugin("notes").click();
    await setup.getByRole("button", { name: "Back", exact: true }).click();
    await expect(summary).toContainText("adds the Action list and Feedback plugins");

    await setup.getByRole("button", { name: "Add more plugins" }).click();
    await plugin("notes").click();
    await expect(plugin("notes")).toHaveAttribute("aria-pressed", "true");
    await setup.getByRole("button", { name: "Add 1 plugin", exact: true }).click();
    await expect(summary).toContainText("adds the Action list, Feedback and Notes plugins");

    // the setup keeps what was picked
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    await expect(setup.getByTestId("setup-plan-plugins")).toHaveText(
      "Plugins to add: Action list, Feedback and Notes.",
    );
    await setup.getByRole("button", { name: "Skip setup" }).click();
    await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
  } finally {
    for (const p of before) {
      if (!p.install) continue;
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

test("the welcome shows only the agents found, and leaves out the one clicked", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("main").waitFor();
  await page.evaluate(
    (agents) => {
      const w = window as unknown as Record<string, unknown>;
      w.__TAURI_INTERNALS__ = {
        invoke: async (command: string) => {
          if (command === "agents_status") return agents;
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
      agent("opencode", "OpenCode", true, "outdated", ["Claude Code"]),
    ],
  );
  const setup = await openSetup(page);
  const agents = setup.getByRole("group", { name: "Your agents" });
  // the main agents first, then the others by name, without the one not found
  await expect(agents.getByRole("button")).toHaveCount(4);
  expect(
    await agents.getByRole("button").evaluateAll((tiles) => tiles.map((t) => t.getAttribute("aria-label"))),
  ).toEqual(["Claude Code", "Codex", "OpenCode", "Cursor"]);
  const sentence = setup.getByTestId("setup-agents-sentence");
  await expect(sentence).toHaveText("Pinrail found Claude Code, Codex, OpenCode and Cursor on this computer.");

  // every agent is picked to begin with, and a click leaves one out
  const cursor = agents.getByRole("button", { name: "Cursor" });
  await expect(cursor).toHaveAttribute("aria-pressed", "true");
  await cursor.click();
  await expect(cursor).toHaveAttribute("aria-pressed", "false");
  await expect(sentence).toHaveText("Pinrail connects Claude Code, Codex and OpenCode. It leaves Cursor out.");

  // an agent already connected stays as it is
  const claude = agents.getByRole("button", { name: "Claude Code" });
  await expect(claude).toHaveAttribute("aria-pressed", "true");
  await expect(claude).toBeDisabled();

  // the setup keeps what was picked: the agents to connect, without the
  // one left out and the one connected already
  await setup.getByRole("button", { name: "Set up Pinrail" }).click();
  await expect(setup.getByTestId("setup-plan-agents")).toHaveText("Agents to connect: Codex and OpenCode.");
  await setup.getByRole("button", { name: "Back", exact: true }).click();
  await expect(cursor).toHaveAttribute("aria-pressed", "false");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
});

test("with more agents than tiles, a list of every agent opens from the +N tile", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("main").waitFor();
  await page.evaluate(
    (agents) => {
      (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
        invoke: async (command: string) => {
          if (command === "agents_status") return agents;
          if (command === "notification_status") return null;
          throw new Error(`${command} is not answered here`);
        },
      };
    },
    [
      agent("claude-code", "Claude Code", true, "absent"),
      agent("codex", "Codex", true, "absent"),
      agent("cursor", "Cursor", true, "absent"),
      agent("goose", "Goose", true, "absent"),
      agent("grok", "Grok CLI", true, "absent"),
      agent("kiro", "Kiro CLI", true, "absent"),
      agent("opencode", "OpenCode", true, "absent"),
      agent("pi", "Pi", true, "connected"),
    ],
  );
  const setup = await openSetup(page);
  const agents = setup.getByRole("group", { name: "Your agents" });
  // the five main agents as tiles, and the other three behind the +N tile
  const tiles = agents.getByRole("button", { pressed: true });
  await expect(tiles).toHaveCount(5);
  expect(await tiles.evaluateAll((all) => all.map((t) => t.getAttribute("aria-label")))).toEqual([
    "Claude Code",
    "Codex",
    "OpenCode",
    "Grok CLI",
    "Pi",
  ]);
  const more = agents.getByRole("button", { name: "3 more agents" });
  await expect(more).toHaveText("+3");
  const sentence = setup.getByTestId("setup-agents-sentence");
  await expect(sentence).toHaveText("Pinrail found 8 agents on this computer.");

  await more.click();
  const list = page.getByRole("menu", { name: "Agents" });
  await expect(list.getByRole("menuitem")).toHaveCount(8);
  await expect(list.getByRole("menuitem", { name: /^Pi/ })).toBeDisabled();
  // the list stays open while agents are left out, those without a tile
  // and those with one
  await list.getByRole("menuitem", { name: "OpenCode" }).click();
  await list.getByRole("menuitem", { name: "Cursor" }).click();
  await expect(list).toBeVisible();
  await expect(agents.getByRole("button", { name: "OpenCode" })).toHaveAttribute("aria-pressed", "false");
  await expect(sentence).toHaveText("Pinrail connects 6 agents. It leaves OpenCode and Cursor out.");

  // Esc closes the list, not the setup
  await page.keyboard.press("Escape");
  await expect(list).toHaveCount(0);
  await expect(setup).toBeVisible();

  // so does a click elsewhere in the setup
  await more.click();
  await expect(list).toBeVisible();
  await setup.getByText("You", { exact: true }).click();
  await expect(list).toHaveCount(0);
  await expect(setup).toBeVisible();
  await setup.getByRole("button", { name: "Set up Pinrail" }).click();
  await expect(setup.getByTestId("setup-plan-agents")).toHaveText(
    "Agents to connect: Claude Code, Codex, Grok CLI, Goose and Kiro CLI.",
  );
});
