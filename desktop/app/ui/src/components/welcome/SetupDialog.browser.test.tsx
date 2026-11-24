// The setup dialog at component level, against the fake core.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { core } from "../../testing/core";
import { cleanup, render } from "../../testing/render";
import { SetupDialog } from "./SetupDialog";

vi.mock("../../api/client", async (real) => (await import("../../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

test("Tab and Shift+Tab go round the dialog's controls", async () => {
  core.answer("catalog", { plugins: [] });
  await render(
    <>
      <button type="button">Behind the dialog</button>
      <SetupDialog onClose={() => {}} />
    </>,
  );
  const dialog = page.getByRole("dialog");
  await expect.element(dialog).toBeVisible();
  const controls = [...dialog.element().querySelectorAll<HTMLElement>("button, input, a[href]")];
  const [first, last] = [controls[0], controls[controls.length - 1]];
  // Tab on the last control goes round to the first, and Shift+Tab back
  last.focus();
  await userEvent.keyboard("{Tab}");
  expect(document.activeElement).toBe(first);
  await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
  expect(document.activeElement).toBe(last);
});
