// The settings dialog against the fake core: a switch saves its setting.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
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
  },
  sidebar: { open: true },
  close_window: "hide",
  menu_bar_icon: true,
  notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
  shortcut: { global: "", global_opens: "oldest" },
  plugins: {},
  port: 4747,
} as unknown as ServerSettings;

describe("the settings dialog", () => {
  test("a work item notification switch saves the inbox setting", async () => {
    core.answer("settings", served);
    // the core answers with the settings as they are after the change
    core.answer("patchSettings", (body: unknown) => ({ ...served, ...(body as object) }));
    core.answer("listReviews", { reviews: [], total: 0 });
    core.answer("plugins", { plugins: [] });
    await render(<SettingsDialog open section="items" onSection={() => {}} onClose={() => {}} />);
    const comments = page.getByRole("switch", { name: "Comments" });
    await expect.element(comments).toBeChecked();
    await expect.element(page.getByRole("switch", { name: "Other changes" })).not.toBeChecked();
    await comments.click();
    await expect.element(comments).not.toBeChecked();
    await vi.waitFor(() =>
      expect(core.calls("patchSettings").at(-1)?.[0]).toEqual({ inbox: { show: { item_commented: false } } }),
    );
  });
});
