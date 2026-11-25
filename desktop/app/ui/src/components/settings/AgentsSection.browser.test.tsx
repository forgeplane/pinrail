// Settings › Agents at component level: the arguments each agent gets on
// every start, saved through the core, which says why it refuses a line.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { ApiError } from "../../api/client";
import { core } from "../../testing/core";
import { cleanup, render } from "../../testing/render";
import { AgentsSection, type AgentStatus } from "./AgentsSection";

const native = vi.hoisted(() => ({ agents: [] as unknown[], calls: [] as string[] }));
vi.mock("../../api/client", async (real) => ({
  ...(await import("../../testing/core")).fakeClient(await real()),
  inTauri: () => true,
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (command: string) => {
    native.calls.push(command);
    return native.agents;
  },
}));

afterEach(() => {
  cleanup();
  core.reset();
  native.calls = [];
});

const agent = (fields: Partial<AgentStatus>): AgentStatus => ({
  id: "claude-code",
  name: "Claude Code",
  found: true,
  skill: "/Users/you/.claude/skills/pinrail/SKILL.md",
  state: "connected",
  skill_state: "connected",
  integration: { kind: "hooks", file: "/Users/you/.claude/settings.json", state: "current" },
  starts: true,
  skips_checks: null,
  args_example: "--model opus",
  ...fields,
});

/** The section, with these agents and the arguments the settings hold. */
async function open(agents: AgentStatus[], args: Record<string, string> = {}) {
  native.agents = agents;
  core.answer("settings", {
    appearance: { theme: "system" },
    sidebar: { open: true },
    notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
    shortcut: { global: "", global_opens: "oldest" },
    plugins: {},
    port: 4747,
    agents: { auto_update: true, args },
  });
  await render(<AgentsSection open />);
}

const field = () => page.getByRole("textbox", { name: "Claude Code's arguments" });
/** Opens Claude Code's settings with More. */
const more = () => userEvent.click(page.getByRole("button", { name: "Claude Code's settings" }));

test("an agent's arguments are saved through the core when the person presses Enter", async () => {
  core.answer("patchSettings", {});
  await open([agent({})]);
  await more();
  // the example is the agent's own
  await expect.element(field()).toHaveAttribute("placeholder", "Such as --model opus");
  const looked = native.calls.filter((c) => c === "agents_status").length;
  await userEvent.fill(field(), "  --model opus ");
  await userEvent.keyboard("{Enter}");
  await expect
    .poll(() => core.calls("patchSettings").at(-1))
    .toEqual([{ agents: { args: { "claude-code": "--model opus" } } }]);
  // the agents are looked at again, for what the arguments say
  await expect.poll(() => native.calls.filter((c) => c === "agents_status").length).toBeGreaterThan(looked);
});

test("clearing the line clears the agent's arguments", async () => {
  core.answer("patchSettings", {});
  await open([agent({})], { "claude-code": "--model opus" });
  await more();
  await expect.element(field()).toHaveValue("--model opus");
  await userEvent.clear(field());
  await userEvent.keyboard("{Enter}");
  await expect.poll(() => core.calls("patchSettings").at(-1)).toEqual([{ agents: { args: { "claude-code": null } } }]);
});

test("a line the core refuses says why, and Esc puts back the saved one", async () => {
  core.answer("patchSettings", () => {
    throw new ApiError(422, {
      violations: [
        {
          path: "/agents/args/claude-code",
          message: "gives --session-id, which Pinrail sets itself to name or resume the session",
        },
      ],
    });
  });
  await open([agent({})], { "claude-code": "--model opus" });
  await more();
  await userEvent.fill(field(), "--session-id abc");
  await userEvent.keyboard("{Enter}");
  await expect
    .element(page.getByRole("alert"))
    .toHaveTextContent(
      "Pinrail did not save this line. It gives --session-id, which Pinrail sets itself to name or resume the session.",
    );
  await userEvent.click(field());
  await userEvent.keyboard("{Escape}");
  await expect.element(field()).toHaveValue("--model opus");
  await expect.element(page.getByRole("alert")).not.toBeInTheDocument();
});

test("arguments that turn the agent's checks off show on the closed card, and warn in its settings", async () => {
  await open([agent({ skips_checks: "--dangerously-skip-permissions" })], {
    "claude-code": "--dangerously-skip-permissions",
  });
  await expect.element(page.getByText("Permission checks off")).toBeVisible();
  await more();
  await expect
    .element(
      page.getByText(
        "With --dangerously-skip-permissions, Claude Code does not ask before it runs commands or changes files.",
      ),
    )
    .toBeVisible();
});

test("an agent Pinrail does not start has no settings, and one not connected has none yet", async () => {
  await open([
    agent({ id: "kiro", name: "Kiro CLI", skill: "/Users/you/.kiro/skills/pinrail/SKILL.md", starts: false }),
    agent({
      id: "codex",
      name: "Codex",
      state: "absent",
      skill_state: "absent",
      skill: "/Users/you/.codex/skills/pinrail/SKILL.md",
    }),
  ]);
  await expect.element(page.getByTestId("agent-kiro")).toBeVisible();
  expect(page.getByRole("button", { name: /settings$/ }).elements()).toHaveLength(0);
  // Remove is a button, as Connect is
  await expect.element(page.getByTestId("agent-kiro").getByRole("button", { name: "Remove" })).toBeVisible();
  await expect.element(page.getByTestId("agent-codex").getByRole("button", { name: "Connect" })).toBeVisible();
});
