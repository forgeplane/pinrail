import fs from "node:fs";
import { expect, test, type Page } from "../fixtures";
import { core, sessionId, settingSaved } from "../helpers";

// The shell's core has no terminal host, and the computer the specs run on
// may or may not have agents: by default Pinrail can start none, and has
// seen no folder. A test that starts a session answers these itself.
test.beforeEach(async ({ page }) => {
  await page.route(/\/api\/v1\/agents\/startable/, (route) => route.fulfill({ json: { agents: [] } }));
  await page.route(/\/api\/v1\/folders\/recent/, (route) => route.fulfill({ json: { folders: [] } }));
});
test.afterEach(({ page }) => page.unrouteAll({ behavior: "ignoreErrors" }));

async function openSetup(page: Page) {
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill("Set up Pinrail");
  await palette.getByRole("option", { name: /Set up Pinrail/ }).click();
  return page.getByRole("dialog", { name: "Set up Pinrail" });
}

type Installed = { name: string; install: { source_kind: string; source: string; link: boolean } | null };

const installed = async (page: Page) =>
  (await (await page.request.get(`${core}/api/v1/plugins`)).json()).plugins as Installed[];

/** Runs a test that sets Pinrail up, then puts back what the setup changes
 *  for the specs after it: the installed plugins, unless their folder is
 *  gone, and the usage data notice. */
async function restoring(page: Page, run: () => Promise<void>) {
  const before = await installed(page);
  const notice = (await (await page.request.get(`${core}/api/v1/settings`)).json()).telemetry.notice_shown;
  try {
    await run();
  } finally {
    const after = await installed(page);
    for (const p of after) {
      if (!before.some((b) => b.name === p.name)) await page.request.delete(`${core}/api/v1/plugins/${p.name}`);
    }
    for (const p of before) {
      if (!p.install || after.some((a) => a.name === p.name)) continue;
      if (p.install.source_kind !== "index" && !fs.existsSync(p.install.source)) continue;
      const body =
        p.install.source_kind === "index"
          ? { id: p.install.source }
          : { source: p.install.source, link: p.install.link };
      expect((await page.request.post(`${core}/api/v1/plugins/install`, { data: body })).status()).toBe(200);
    }
    await page.request.patch(`${core}/api/v1/settings`, { data: { telemetry: { notice_shown: notice } } });
  }
}

test("the plugins view adds a plugin to the ones the setup installs", async ({ page }) => {
  await restoring(page, async () => {
    // a person who has installed nothing yet
    for (const p of await installed(page)) {
      expect((await page.request.delete(`${core}/api/v1/plugins/${p.name}`)).status()).toBe(200);
    }

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

    // the setup installs what was picked, and goes on when Continue is pressed
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
    await setup.getByRole("button", { name: "Continue", exact: true }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is ready" })).toBeVisible();
    expect((await installed(page)).map((p) => p.name).sort()).toEqual(["feedback", "list", "notes"]);
    await setup.getByRole("button", { name: "Done", exact: true }).click();
    await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
  });
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

  // the setup has a step for each agent to connect, without the one left
  // out and the one connected already
  await restoring(page, async () => {
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    await expect(setup.getByTestId("setup-step-agent-codex")).toContainText("Codex");
    await expect(setup.getByTestId("setup-step-agent-opencode")).toContainText("OpenCode");
    await expect(setup.getByTestId(/^setup-step-agent-/)).toHaveCount(2);
  });
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
  // more agents than have a step each share one
  await restoring(page, async () => {
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    await expect(setup.getByTestId("setup-step-agents")).toContainText("5 agents");
  });
});

/** The desktop app's side of the setup, answered in the page: the command,
 *  the agents, their Report status and notifications. It records each call. */
async function native(
  page: Page,
  options: {
    agents: ReturnType<typeof agent>[];
    onPath?: boolean;
    failOnce?: string[];
    /** the file of the login shell's PATH; null for a shell Pinrail does not know */
    profile?: string | null;
    /** the folder the system's folder dialog answers */
    pickFolder?: string;
  },
) {
  await page.goto("/");
  await page.getByRole("main").waitFor();
  await page.evaluate(({ agents, onPath = true, failOnce = [], profile = "/Users/you/.zshrc", pickFolder = null }) => {
    const w = window as unknown as Record<string, unknown>;
    const calls: string[] = [];
    const state = { installed: false, onPath, asked: false, failed: new Set<string>() };
    w.__calls = calls;
    w.__native = state;
    const hooked = agents.map((a) => ({
      ...a,
      integration: { kind: "hooks", file: `/Users/you/.${a.id}/settings.json`, state: "absent" },
    }));
    const link = "/Users/you/.local/bin/pinrail";
    const cli = () => ({
      mode: "link",
      bundled: "/Applications/Pinrail.app/Contents/MacOS/pinrail",
      link,
      installed: state.installed,
      outdated: false,
      occupied_by: null,
      runs: state.installed && state.onPath ? link : null,
      dir_on_path: state.onPath,
      warning: null,
      profile,
      profile_line: profile ? 'export PATH="$HOME/.local/bin:$PATH"' : null,
    });
    const allowed = { alert_style: "banner", alerts: true, sound: true, badge: true };
    w.__TAURI_INTERNALS__ = {
      invoke: async (command: string, args?: { id?: string }) => {
        calls.push(args?.id ? `${command} ${args.id}` : command);
        const one = hooked.find((a) => a.id === args?.id);
        switch (command) {
          case "agents_status":
            return hooked;
          case "cli_status":
            return cli();
          case "install_cli":
            state.installed = true;
            return cli();
          case "plugin:dialog|open":
            return pickFolder;
          case "add_cli_to_path":
            // a new terminal reads the file, so it finds the command
            state.onPath = true;
            return cli();
          case "connect_agent":
            if (failOnce.includes(args!.id!) && !state.failed.has(args!.id!)) {
              state.failed.add(args!.id!);
              throw "cannot write the skill: permission denied";
            }
            one!.state = "connected";
            return hooked;
          case "set_agent_hooks":
            one!.integration.state = "current";
            return hooked;
          case "notification_status":
            return { ...allowed, authorization: state.asked ? "authorized" : "not_determined", shows: state.asked };
          case "request_notifications":
            state.asked = true;
            return { ...allowed, authorization: "authorized", shows: true };
          default:
            throw new Error(`${command} is not answered here`);
        }
      },
    };
  }, options);
  return {
    calls: () => page.evaluate(() => (window as unknown as { __calls: string[] }).__calls),
    putOnPath: () =>
      page.evaluate(() => {
        (window as unknown as { __native: { onPath: boolean } }).__native.onPath = true;
      }),
  };
}

test("Set up Pinrail installs the command, connects the agents and waits for Continue", async ({ page }) => {
  await restoring(page, async () => {
    await page.request.patch(`${core}/api/v1/settings`, { data: { telemetry: { notice_shown: null } } });
    const app = await native(page, {
      agents: [agent("claude-code", "Claude Code", true, "absent"), agent("codex", "Codex", true, "absent")],
    });
    const setup = await openSetup(page);
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    // every part is done before the setup goes on
    const step = (key: string) => setup.getByTestId(`setup-step-${key}`);
    await expect(step("command")).toContainText("Installed");
    await expect(step("agent-claude-code")).toContainText("Connected");
    await expect(step("agent-codex")).toContainText("Connected");
    await expect(step("notifications")).toContainText("On");
    await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
    // the setup waits for the person, however long they take
    await page.waitForTimeout(1500);
    await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
    await setup.getByRole("button", { name: "Continue", exact: true }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is ready" })).toBeVisible();

    // each agent is connected and its Report status turned on, and macOS
    // is asked for notifications
    const calls = await app.calls();
    for (const call of [
      "install_cli",
      "connect_agent claude-code",
      "set_agent_hooks claude-code",
      "connect_agent codex",
      "set_agent_hooks codex",
      "request_notifications",
    ])
      expect(calls).toContain(call);
    // the welcome told the person about usage data, so the inbox does not
    await settingSaved(page.request, (s) => typeof s.telemetry.notice_shown, "string");
    await setup.getByRole("button", { name: "Done", exact: true }).click();
    await expect(page.getByTestId("telemetry-notice")).toHaveCount(0);
  });
});

test("a part that fails keeps the setup on its screen, and Try again runs it alone", async ({ page }) => {
  await restoring(page, async () => {
    const app = await native(page, {
      agents: [agent("codex", "Codex", true, "absent"), agent("cursor", "Cursor", true, "absent")],
      failOnce: ["cursor"],
    });
    const setup = await openSetup(page);
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    const step = (key: string) => setup.getByTestId(`setup-step-${key}`);
    // the other parts are done all the same
    await expect(setup.getByRole("heading", { name: "One step is left" })).toBeVisible();
    await expect(step("agent-cursor")).toContainText("Not connected");
    await expect(step("agent-cursor")).toContainText("cannot write the skill: permission denied");
    await expect(step("command")).toContainText("Installed");
    await expect(step("agent-codex")).toContainText("Connected");
    await expect(step("notifications")).toContainText("On");
    await expect(setup.getByRole("button", { name: "Continue", exact: true })).toBeVisible();

    await setup.getByRole("button", { name: "Try again", exact: true }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
    await setup.getByRole("button", { name: "Continue", exact: true }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is ready" })).toBeVisible();
    const calls = await app.calls();
    expect(calls.filter((c) => c === "connect_agent cursor")).toHaveLength(2);
    expect(calls.filter((c) => c === "connect_agent codex")).toHaveLength(1);
    expect(calls.filter((c) => c === "install_cli")).toHaveLength(1);
  });
});

test("a command that is not on the PATH shows the line to add, and Check again looks once more", async ({ page }) => {
  await restoring(page, async () => {
    // a shell Pinrail does not know: the line is the person's to add
    const app = await native(page, {
      agents: [agent("codex", "Codex", true, "absent")],
      onPath: false,
      profile: null,
    });
    const setup = await openSetup(page);
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    await expect(setup.getByRole("heading", { name: "One step is left" })).toBeVisible();
    const command = setup.getByTestId("setup-step-command");
    await expect(command).toContainText("Not on your PATH");
    await expect(command).toContainText('export PATH="$HOME/.local/bin:$PATH"');
    await expect(command.getByRole("button", { name: "Copy the line" })).toBeVisible();
    await expect(command.getByRole("button", { name: /^Add it to/ })).toHaveCount(0);
    await expect(setup.getByTestId("setup-step-agent-codex")).toContainText("Connected");
    // nothing else failed, so there is nothing to try again
    await expect(setup.getByRole("button", { name: "Try again", exact: true })).toHaveCount(0);

    // still off the PATH: the screen stays
    await command.getByRole("button", { name: "Check again" }).click();
    await expect(command).toContainText("Not on your PATH");
    await app.putOnPath();
    await command.getByRole("button", { name: "Check again" }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
    await setup.getByRole("button", { name: "Continue", exact: true }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is ready" })).toBeVisible();
    // the command was installed once, and only looked at after that
    expect((await app.calls()).filter((c) => c === "install_cli")).toHaveLength(1);
  });
});

test("with no agent found, the setup says so and does the rest", async ({ page }) => {
  await restoring(page, async () => {
    await native(page, { agents: [] });
    const setup = await openSetup(page);
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    await expect(setup.getByTestId("setup-step-agents")).toContainText("None found");
    await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
    await setup.getByRole("button", { name: "Continue", exact: true }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is ready" })).toBeVisible();
  });
});

test("Add it to the shell's file puts the command on the PATH, and says to open a new terminal", async ({ page }) => {
  await restoring(page, async () => {
    const app = await native(page, { agents: [agent("codex", "Codex", true, "absent")], onPath: false });
    const setup = await openSetup(page);
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    const command = setup.getByTestId("setup-step-command");
    await expect(command).toContainText("Add this line to ~/.zshrc");
    await expect(command.getByRole("button", { name: "Copy the line" })).toBeVisible();

    await command.getByRole("button", { name: "Add it to ~/.zshrc" }).click();
    await expect(command).toContainText("Added to ~/.zshrc");
    await expect(command.getByRole("button")).toHaveCount(0);
    // the part is done, and the screen waits for the person to read what is left to do
    await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
    await expect(setup).toContainText("Open a new terminal to use the pinrail command.");
    expect((await app.calls()).filter((c) => c === "add_cli_to_path")).toHaveLength(1);
    await setup.getByRole("button", { name: "Continue", exact: true }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is ready" })).toBeVisible();
  });
});

const STARTABLE = [
  { id: "claude-code", name: "Claude Code", program: "claude", installed: true, version: "2.1.325", preferred: false },
  { id: "codex", name: "Codex", program: "codex", installed: true, version: "0.98.0", preferred: true },
  // Pinrail can start it, but the setup did not connect it
  { id: "pi", name: "Pi", program: "pi", installed: true, version: "1.0.0", preferred: false },
];
const FOLDERS = [
  {
    folder: "/work/api",
    repo: "acme/api",
    branch: "main",
    exists: true,
    used_at: "2026-11-14T10:00:00Z",
    agent: null as string | null,
  },
  {
    folder: "/work/web",
    repo: "acme/web",
    branch: "main",
    exists: true,
    used_at: "2026-11-10T10:00:00Z",
    agent: null as string | null,
  },
  {
    folder: "/work/old",
    repo: null,
    branch: null,
    exists: false,
    used_at: "2026-11-01T10:00:00Z",
    agent: null as string | null,
  },
];

/** Stands in for the runtime: the agents Pinrail can start, the folders it
 *  has seen, and the start itself, which answers with a session of the
 *  core's or refuses with `refuse`. It records each start. */
async function runtime(page: Page, options: { folders?: typeof FOLDERS; refuse?: Record<string, unknown> } = {}) {
  const name = `s-setup-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  const made = await page.request.post(`${core}/api/v1/sessions/claude-code/${name}/status`, {
    data: { kind: "start", text: "A first session", origin: { repo: "acme/api" } },
  });
  expect(made.status(), await made.text()).toBeLessThan(300);
  const session = { agent: "claude-code", id: await sessionId(page.request, "claude-code", name), agent_session: null };
  const run = {
    id: "run_1",
    host: "local",
    agent: "claude-code",
    state: "running",
    terminal: "local:t_1",
    folder: "/work/api",
    started_at: new Date().toISOString(),
    ended_at: null,
    end_reason: null,
    exit_code: null,
    session,
  };
  const started: Record<string, unknown>[] = [];
  await page.route(/\/api\/v1\/agents\/startable/, (route) => route.fulfill({ json: { agents: STARTABLE } }));
  await page.route(/\/api\/v1\/folders\/recent/, (route) =>
    route.fulfill({ json: { folders: options.folders ?? FOLDERS } }),
  );
  await page.route(/\/api\/v1\/runs(\?|$)/, async (route) => {
    if (route.request().method() !== "POST") return route.fulfill({ json: { runs: started.length ? [run] : [] } });
    if (options.refuse) return route.fulfill({ status: 409, json: options.refuse });
    started.push(route.request().postDataJSON());
    return route.fulfill({ status: 201, json: { run, session } });
  });
  await page.route(/\/api\/v1\/runs\/run_1$/, (route) => route.fulfill({ json: run }));
  await page.route(/\/api\/v1\/terminals\/[^/]+\/tickets/, (route) => route.fulfill({ json: { ticket: "t" } }));
  return { started, session };
}

/** The setup, run to its last screen with Claude Code and Codex connected. */
async function atLastScreen(page: Page, options: { pickFolder?: string } = {}) {
  await native(page, {
    agents: [agent("codex", "Codex", true, "absent"), agent("claude-code", "Claude Code", true, "absent")],
    ...options,
  });
  const setup = await openSetup(page);
  await setup.getByRole("button", { name: "Set up Pinrail" }).click();
  await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
  await setup.getByRole("button", { name: "Continue", exact: true }).click();
  await expect(setup.getByRole("heading", { name: "Pinrail is ready. Start your first session." })).toBeVisible();
  return setup;
}

test("a request on the last screen starts a session of the first agent, and ends the setup", async ({ page }) => {
  await restoring(page, async () => {
    const { started, session } = await runtime(page);
    const setup = await atLastScreen(page);
    // the first agent in the welcome's order that Pinrail can start, and the
    // folder used last
    await expect(setup.getByRole("button", { name: /^Agent:/ })).toHaveAccessibleName("Agent: Claude Code");
    await expect(setup.getByRole("button", { name: /^Folder:/ })).toHaveAccessibleName("Folder: /work/api");
    // a request for each plugin the setup installed
    await expect(setup.getByTestId("setup-request-list")).toContainText("which TODOs in this repository");
    await expect(setup.getByTestId("setup-request-feedback")).toContainText("what you need to know");
    await expect(setup.getByRole("note")).toContainText("under Waiting on you");

    await setup
      .getByTestId("setup-request-list")
      .getByRole("button", { name: /^Start/ })
      .click();
    await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
    expect(started).toHaveLength(1);
    expect(started[0]).toMatchObject({
      agent: "claude-code",
      folder: "/work/api",
      task: "Ask me through Pinrail which TODOs in this repository to tackle first.",
    });
    // the new session takes the setup's place
    await expect(page).toHaveURL(new RegExp(`#/sessions/claude-code/${session.id}`));
    await expect(page.getByText("Setup is finished. What Claude Code asks appears in your inbox.")).toBeVisible();
  });
});

test("the last screen starts another agent on a request of your own", async ({ page }) => {
  await restoring(page, async () => {
    const { started } = await runtime(page);
    const setup = await atLastScreen(page);
    // the connected agents Pinrail can start, and no other
    await setup.getByRole("button", { name: /^Agent:/ }).click();
    const agents = page.getByRole("menu", { name: "Agents" });
    await expect(agents.getByRole("menuitem")).toHaveCount(2);
    await agents.getByRole("menuitem", { name: /Codex/ }).click();
    await expect(setup.getByRole("button", { name: /^Agent:/ })).toHaveAccessibleName("Agent: Codex");

    const own = setup.getByRole("textbox", { name: "Your own request" });
    await expect(setup.getByRole("button", { name: "Start with your request" })).toBeDisabled();
    await own.fill("Fix the login redirect");
    await own.press("Enter");
    await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
    expect(started[0]).toMatchObject({ agent: "codex", folder: "/work/api", task: "Fix the login redirect" });
  });
});

test("the folder is one Pinrail has seen, or one chosen in the system's dialog", async ({ page }) => {
  await restoring(page, async () => {
    const { started } = await runtime(page);
    const setup = await atLastScreen(page, { pickFolder: "/Users/you/code/shop" });
    const folder = setup.getByRole("button", { name: /^Folder:/ });
    await folder.click();
    const folders = page.getByRole("menu", { name: "Folders" });
    // the folders that are still there, the one used last first
    await expect(folders.getByRole("menuitem")).toHaveText([/api/, /web/, /Choose another folder…/]);
    await folders.getByRole("menuitem", { name: /web/ }).click();
    await expect(folder).toHaveAccessibleName("Folder: /work/web");

    await folder.click();
    await page.getByRole("menuitem", { name: "Choose another folder…" }).click();
    await expect(folder).toHaveAccessibleName("Folder: /Users/you/code/shop");
    await expect(folder).toContainText("shop");
    await setup
      .getByTestId("setup-request-feedback")
      .getByRole("button", { name: /^Start/ })
      .click();
    await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
    expect(started[0]).toMatchObject({ folder: "/Users/you/code/shop" });
  });
});

test("folders that all come from one agent's history say so", async ({ page }) => {
  await restoring(page, async () => {
    // a first run: Pinrail has seen no folder, and Claude Code worked in two
    await runtime(page, { folders: FOLDERS.slice(0, 2).map((f) => ({ ...f, agent: "claude-code" })) });
    const setup = await atLastScreen(page);
    const folder = setup.getByRole("button", { name: /^Folder:/ });
    await expect(folder).toHaveAccessibleName("Folder: /work/api");
    await folder.click();
    const folders = page.getByRole("menu", { name: "Folders" });
    await expect(folders).toContainText("Folders Claude Code worked in");
    await expect(folders.getByRole("menuitem")).toHaveText([/api/, /web/, /Choose another folder…/]);
  });
});

test("with no folder seen yet, the folder button asks for one", async ({ page }) => {
  await restoring(page, async () => {
    await runtime(page, { folders: [] });
    const setup = await atLastScreen(page, { pickFolder: "/Users/you/code/shop" });
    const folder = setup.getByRole("button", { name: /^Folder:/ });
    await expect(folder).toHaveText("Choose a folder");
    await folder.click();
    await expect(folder).toHaveAccessibleName("Folder: /Users/you/code/shop");
    await setup.getByRole("button", { name: "Skip", exact: true }).click();
    await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
  });
});

test("a start that fails says why, and the setup stays open", async ({ page }) => {
  await restoring(page, async () => {
    const { started } = await runtime(page, { refuse: { error: "folder_missing", message: "no such folder" } });
    const setup = await atLastScreen(page);
    await setup
      .getByTestId("setup-request-list")
      .getByRole("button", { name: /^Start/ })
      .click();
    await expect(setup.getByRole("alert")).toHaveText("The folder no longer exists.");
    await expect(setup.getByTestId("setup-request-list").getByRole("button", { name: /^Start/ })).toBeEnabled();
    expect(started).toHaveLength(0);
  });
});

test("when Pinrail can start none of the connected agents, the requests are there to copy", async ({ page }) => {
  await restoring(page, async () => {
    await native(page, { agents: [agent("cline", "Cline", true, "absent")] });
    const setup = await openSetup(page);
    await setup.getByRole("button", { name: "Set up Pinrail" }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is set up" })).toBeVisible();
    await setup.getByRole("button", { name: "Continue", exact: true }).click();
    await expect(setup.getByRole("heading", { name: "Pinrail is ready", exact: true })).toBeVisible();
    await expect(setup).toContainText("Paste one of these requests into your agent to try it.");
    const request = setup.getByTestId("setup-request-list");
    await expect(request.getByRole("button", { name: "Copy" })).toBeVisible();
    await expect(setup.getByRole("button", { name: /^Start/ })).toHaveCount(0);
    await expect(setup.getByRole("textbox")).toHaveCount(0);
    await setup.getByRole("button", { name: "Done", exact: true }).click();
    await expect(page.getByRole("dialog", { name: "Set up Pinrail" })).toHaveCount(0);
  });
});
