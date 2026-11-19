import { expect, test } from "../fixtures";
import { core, settingSaved, settingsDialog, settingsSection } from "../helpers";

const defaults = {
  runtime: {
    terminal: { renderer: ["auto"], option_as_meta: "off", screen_reader: false, clipboard_write: true },
    default_view: "last",
    new_run_view: "terminal",
  },
};

// the settings are the core's, shared by the worker: each test leaves the
// defaults for the next
test.afterEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, { data: defaults });
});

test("the Terminal section changes the terminal's settings and the session views", async ({ page, request }) => {
  // the Option keys are offered on macOS alone
  await page.addInitScript(() => Object.defineProperty(navigator, "platform", { get: () => "MacIntel" }));
  await request.patch(`${core}/api/v1/settings`, { data: defaults });
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Terminal").click();
  const dialog = settingsDialog(page);
  // before any terminal is opened, it says what Automatic does
  await expect(dialog).toContainText("Automatic uses libghostty, and xterm.js where libghostty cannot run.");

  const pick = async (control: string, option: string) => {
    await dialog.getByRole("button", { name: control, exact: true }).click();
    await page.getByRole("option", { name: option, exact: true }).click();
  };
  await pick("Renderer", "xterm.js");
  await settingSaved(request, (s) => s.runtime.terminal.renderer, ["xterm"]);
  await pick("Option as Meta", "Left Option");
  await settingSaved(request, (s) => s.runtime.terminal.option_as_meta, "left");

  await dialog.getByRole("switch", { name: "Screen reader" }).click();
  await settingSaved(request, (s) => s.runtime.terminal.screen_reader, true);
  await dialog.getByRole("switch", { name: "Let programs copy to the clipboard" }).click();
  await settingSaved(request, (s) => s.runtime.terminal.clipboard_write, false);

  const opens = dialog.getByRole("radiogroup", { name: "Open a session in" });
  await opens.getByRole("radio", { name: "Summary" }).click();
  await settingSaved(request, (s) => s.runtime.default_view, "summary");
  await expect(opens.getByRole("radio", { name: "Summary" })).toBeChecked();
  const started = dialog.getByRole("radiogroup", { name: "Open an agent you start in" });
  await started.getByRole("radio", { name: "Summary" }).click();
  await settingSaved(request, (s) => s.runtime.new_run_view, "summary");
  await expect(started.getByRole("radio", { name: "Summary" })).toBeChecked();
  await expect(dialog.getByRole("switch", { name: "Screen reader" })).toHaveAttribute("aria-checked", "true");
  await expect(dialog.getByRole("button", { name: "Renderer", exact: true })).toContainText("xterm.js");
  // the other settings of the terminal are kept
  await settingSaved(request, (s) => s.runtime.terminal.renderer, ["xterm"]);
});

test("a renderer list of the settings file's own shows as it is", async ({ page, request }) => {
  await request.patch(`${core}/api/v1/settings`, {
    data: { runtime: { terminal: { renderer: ["ghostty", "xterm"] } } },
  });
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Terminal").click();
  await expect(settingsDialog(page).getByRole("button", { name: "Renderer", exact: true })).toContainText(
    "ghostty, xterm",
  );
});
