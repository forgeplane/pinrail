// The settings dialog against the fake core: a switch saves its setting.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { ServerSettings } from "../../api/types";
import { core } from "../../testing/core";
import { cleanup, render } from "../../testing/render";
import { SettingsDialog } from "./SettingsDialog";

vi.mock("../../api/client", async (real) => (await import("../../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

const served = {
  appearance: {
    theme: "system",
    dark_theme: "dark",
    light_theme: "light",
    text_size: "default",
    tables: "bordered",
    table_width: "fit",
    element_tiles: false,
  },
  sidebar: { open: true },
  close_window: "hide",
  menu_bar_icon: true,
  notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
  shortcut: { global: "", global_opens: "oldest" },
  plugins: {},
  port: 4747,
} as unknown as ServerSettings;

/** The dialog on `section`, against a core that saves each change. */
async function open(section: "general" | "notifications" | "items") {
  core.answer("settings", served);
  // the core answers with the settings as they are after the change
  core.answer("patchSettings", (body: unknown) => ({ ...served, ...(body as object) }));
  core.answer("listReviews", { reviews: [], total: 0 });
  core.answer("plugins", { plugins: [] });
  core.answer("projects", {
    projects: [{ id: "p1", name: "Pinrail", repos: [{ repo: "forgeplane/pinrail" }] }],
    loose: [{ repo: "acme/site" }],
  });
  await render(<SettingsDialog open section={section} onSection={() => {}} onClose={() => {}} />);
}

const lastPatch = () => core.calls("patchSettings").at(-1)?.[0];

describe("the notifications section", () => {
  test("an inbox switch saves the inbox setting", async () => {
    await open("notifications");
    const comments = page.getByRole("switch", { name: "Comments in the inbox" });
    await expect.element(comments).toBeChecked();
    await expect.element(page.getByRole("switch", { name: "Other changes in the inbox" })).not.toBeChecked();
    await comments.click();
    await expect.element(comments).not.toBeChecked();
    await vi.waitFor(() => expect(lastPatch()).toEqual({ inbox: { show: { item_commented: false } } }));
    // a kind off in the inbox is never announced
    await expect.element(page.getByRole("switch", { name: "Comments as a system notification" })).toBeDisabled();
  });

  test("a system switch saves its announce setting, and what waits is always in the inbox", async () => {
    await open("notifications");
    await expect.element(page.getByRole("switch", { name: "Reviews as a system notification" })).toBeChecked();
    await expect.element(page.getByRole("switch", { name: "Reviews in the inbox" })).not.toBeInTheDocument();
    const finished = page.getByRole("switch", { name: "Agent finished as a system notification" });
    await expect.element(finished).not.toBeChecked();
    await finished.click();
    await vi.waitFor(() => expect(lastPatch()).toEqual({ notifications: { announce: { agent_finished: true } } }));
  });

  test("a project or repository is muted and unmuted", async () => {
    await open("notifications");
    await expect.element(page.getByText("None muted")).toBeVisible();
    await page.getByRole("button", { name: "Mute" }).click();
    await page.getByRole("menuitemcheckbox", { name: "acme/site" }).click();
    await vi.waitFor(() => expect(lastPatch()).toEqual({ inbox: { muted_repos: ["acme/site"] } }));
    // the menu stays open for more picks until Esc
    await userEvent.keyboard("{Escape}");
    const unmute = page.getByRole("button", { name: "Unmute acme/site" });
    await unmute.click();
    await vi.waitFor(() => expect(lastPatch()).toEqual({ inbox: { muted_repos: [] } }));
    await expect.element(page.getByText("None muted")).toBeVisible();
  });

  test("the mute menu closes on a click outside it, and Esc closes it and not Settings", async () => {
    const onClose = vi.fn();
    core.answer("settings", served);
    core.answer("projects", { projects: [], loose: [{ repo: "acme/site" }] });
    await render(<SettingsDialog open section="notifications" onSection={() => {}} onClose={onClose} />);
    const menu = page.getByRole("menu", { name: "Project or repository" });
    await page.getByRole("button", { name: "Mute" }).click();
    await expect.element(menu).toBeVisible();
    await page.getByRole("heading", { name: "Notifications" }).click();
    await expect.element(menu).not.toBeInTheDocument();

    await page.getByRole("button", { name: "Mute" }).click();
    await expect.element(menu).toBeVisible();
    await userEvent.keyboard("{Escape}");
    await expect.element(menu).not.toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  test("the pause, the quiet hours and the sound are here, not in General or Work items", async () => {
    await open("notifications");
    await expect.element(page.getByRole("button", { name: "Pause notifications" })).toBeVisible();
    await expect.element(page.getByRole("switch", { name: "Sound" })).toBeVisible();
    await page.getByRole("switch", { name: "Quiet hours" }).click();
    await vi.waitFor(() =>
      expect(lastPatch()).toEqual({ notifications: { quiet_hours: { from: "22:00", to: "07:00" } } }),
    );
    await expect.element(page.getByLabelText("Quiet from")).toHaveValue("22:00");
    cleanup();

    await open("general");
    await expect.element(page.getByRole("heading", { name: "General" })).toBeVisible();
    expect(page.getByRole("button", { name: "Pause notifications" }).query()).toBeNull();
    cleanup();

    await open("items");
    await expect.element(page.getByRole("heading", { name: "Work items" })).toBeVisible();
    expect(page.getByRole("switch", { name: /Comments/ }).query()).toBeNull();
  });
});

describe("quitting with agents running", () => {
  test("is chosen in General while agents run in Pinrail", async () => {
    await open("general");
    await expect.element(page.getByRole("heading", { name: "General" })).toBeVisible();
    expect(page.getByRole("radiogroup", { name: "Quitting with agents running" }).query()).toBeNull();
    cleanup();

    core.reset();
    const runtime = { ...served, labs: { runtime: true } } as ServerSettings;
    core.answer("settings", runtime);
    core.answer("patchSettings", (body: unknown) => ({ ...runtime, ...(body as object) }));
    await render(<SettingsDialog open section="general" onSection={() => {}} onClose={() => {}} />);
    const choices = page.getByRole("radiogroup", { name: "Quitting with agents running" });
    await expect.element(choices.getByRole("radio", { name: "Ask" })).toBeChecked();
    await choices.getByRole("radio", { name: "Stop them" }).click();
    await vi.waitFor(() => expect(lastPatch()).toEqual({ runtime: { on_quit: "stop" } }));
    await expect.element(page.getByText("Pinrail stops the agents that run in it, then quits.")).toBeVisible();
  });
});
